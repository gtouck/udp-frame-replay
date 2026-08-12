//! 发送节拍器。
//!
//! 两种模式：
//! - 毫秒模式用 `sleep` 驱动，CPU 几乎不占，误差取决于操作系统调度粒度
//! - 高精度模式先粗睡到目标前 1ms，再自旋等待，能把误差压到个位数微秒，
//!   代价是自旋期间占满一个 CPU 核心
//!
//! 无论哪种模式这都是**软实时**：操作系统调度仍会造成偶发抖动，无法根除。
//! 因此节拍器如实返回每拍的实际间隔，由界面把真实表现画出来。

use std::thread;
use std::time::{Duration, Instant};

/// 高精度模式下留给自旋的时间窗
const SPIN_GUARD: Duration = Duration::from_micros(1000);

pub struct Pacer {
    interval: Duration,
    high_precision: bool,
    /// 下一拍的目标时刻
    next: Instant,
    /// 上一次放行的时刻，用于算实际间隔
    last_release: Instant,
    /// 落后超过这个量就重新对齐，避免积压后的补发风暴
    max_lag: Duration,
    resyncs: u64,
}

impl Pacer {
    pub fn new(interval_us: u64, high_precision: bool) -> Self {
        let interval = Duration::from_micros(interval_us);
        let now = Instant::now();
        Pacer {
            interval,
            high_precision,
            next: now + interval,
            last_release: now,
            max_lag: (interval * 8).max(Duration::from_millis(50)),
            resyncs: 0,
        }
    }

    /// 重新开始计时。暂停恢复后必须调用，否则会把暂停时长当成欠账补发。
    pub fn arm(&mut self) {
        let now = Instant::now();
        self.next = now + self.interval;
        self.last_release = now;
    }

    /// 等到下一拍，返回距上次放行的实际间隔（微秒）。
    pub fn wait(&mut self) -> u64 {
        if self.interval.is_zero() {
            // 间隔为 0 表示全速发送，不做任何等待
            let released = Instant::now();
            let actual = released.duration_since(self.last_release).as_micros() as u64;
            self.last_release = released;
            return actual;
        }

        let target = self.next;
        let now = Instant::now();

        if now < target {
            let remaining = target - now;
            if self.high_precision {
                if remaining > SPIN_GUARD {
                    thread::sleep(remaining - SPIN_GUARD);
                }
                while Instant::now() < target {
                    std::hint::spin_loop();
                }
            } else {
                thread::sleep(remaining);
            }
        }

        let released = Instant::now();
        let actual = released.duration_since(self.last_release).as_micros() as u64;
        self.last_release = released;

        // 按目标时刻递推而非「当前时刻 + 间隔」，避免每拍的误差累积成漂移
        self.next += self.interval;

        // 但落后太多时必须重新对齐：否则一旦卡顿，
        // 后面会为了追回欠账连续爆发式发送，那比丢几拍更糟
        if self.next + self.max_lag < released {
            self.next = released + self.interval;
            self.resyncs += 1;
        }

        actual
    }

    /// 因落后过多而重新对齐的次数
    pub fn resyncs(&self) -> u64 {
        self.resyncs
    }

    pub fn interval_us(&self) -> u64 {
        self.interval.as_micros() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 节拍测试只断言统计特性。断言「精确 N 微秒」在任何共享 CI 上都必然不稳定。
    fn measure(interval_us: u64, high_precision: bool, ticks: usize) -> Vec<u64> {
        let mut p = Pacer::new(interval_us, high_precision);
        p.arm();
        (0..ticks).map(|_| p.wait()).collect()
    }

    #[test]
    fn millisecond_mode_holds_average_interval() {
        let target = 2000u64; // 2ms
        let samples = measure(target, false, 50);
        let mean = samples.iter().sum::<u64>() / samples.len() as u64;

        // 只要平均值落在合理区间即可：不能明显快于目标（那是没等），
        // 也不能慢一倍以上（那是节拍器坏了）
        assert!(
            mean >= target * 9 / 10,
            "平均间隔 {mean}μs 明显快于目标 {target}μs"
        );
        assert!(mean < target * 3, "平均间隔 {mean}μs 慢于目标 {target}μs 太多");
    }

    #[test]
    fn zero_interval_does_not_wait() {
        let start = Instant::now();
        let samples = measure(0, false, 1000);
        assert_eq!(samples.len(), 1000);
        // 全速模式下一千拍应当瞬间完成
        assert!(start.elapsed() < Duration::from_millis(200));
    }

    #[test]
    fn does_not_drift_over_many_ticks() {
        let target = 1000u64;
        let ticks = 100;
        let start = Instant::now();
        measure(target, false, ticks);
        let total = start.elapsed().as_micros() as u64;

        // 累计时长应接近 拍数 × 间隔。按目标时刻递推正是为了保证这点：
        // 若按「当前时刻 + 间隔」递推，每拍的调度延迟会累加成明显漂移。
        let expected = target * ticks as u64;
        assert!(
            total < expected * 3,
            "累计 {total}μs 远超预期 {expected}μs，说明发生了漂移"
        );
    }

    #[test]
    fn resyncs_after_a_long_stall() {
        let mut p = Pacer::new(1000, false);
        p.arm();
        p.wait();

        // 模拟一次长卡顿：远超 max_lag
        thread::sleep(Duration::from_millis(200));

        // 卡顿后的若干拍不应为了追账而连续零等待爆发
        let start = Instant::now();
        for _ in 0..5 {
            p.wait();
        }
        assert!(p.resyncs() >= 1, "长时间卡顿后应当重新对齐");
        assert!(
            start.elapsed() >= Duration::from_millis(3),
            "重新对齐后应恢复正常节拍，而不是补发风暴"
        );
    }

    #[test]
    fn arm_discards_pause_debt() {
        let mut p = Pacer::new(1000, false);
        p.arm();
        p.wait();

        thread::sleep(Duration::from_millis(100)); // 模拟暂停
        p.arm(); // 恢复时重新计时

        let start = Instant::now();
        p.wait();
        // 重新计时后第一拍应等满一个间隔，而不是立刻放行
        assert!(start.elapsed() >= Duration::from_micros(500));
    }
}
