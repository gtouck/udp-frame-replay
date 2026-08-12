//! 解析线程与发送线程之间的有界环形缓冲。
//!
//! 两条无锁队列构成槽位对象池：解析线程从 `free` 取空槽、填好推进 `filled`；
//! 发送线程从 `filled` 取、发完把槽还回 `free`。稳态下不发生任何堆分配 ——
//! 槽里的 `Vec` 只清空不释放，容量一直留着复用。

use crossbeam_queue::ArrayQueue;

use crate::mutate::SpanSet;

/// 单槽预留字节数。绝大多数帧远小于此，超出的帧会让该槽的 Vec 自行增长。
pub const DEFAULT_SLOT_BYTES: usize = 2048;

/// 槽位数量。8192 × 2048 ≈ 16MB，预取深度对任何速率都绰绰有余。
pub const DEFAULT_SLOTS: usize = 8192;

/// 一个待发帧
pub struct Frame {
    pub data: Vec<u8>,
    /// 来源行号，1-based，用于界面对照与日志定位
    pub line_no: u32,
    /// 被修改规则改动过的字节区段，供发送视图着色。定长数组，不额外分配。
    pub spans: SpanSet,
}

pub struct Ring {
    filled: ArrayQueue<Frame>,
    free: ArrayQueue<Frame>,
}

impl Ring {
    pub fn new(slots: usize, slot_bytes: usize) -> Self {
        let free = ArrayQueue::new(slots);
        for _ in 0..slots {
            // push 只会在队列满时失败，这里恰好填满，不会失败
            let _ = free.push(Frame {
                data: Vec::with_capacity(slot_bytes),
                line_no: 0,
                spans: SpanSet::default(),
            });
        }
        Ring {
            filled: ArrayQueue::new(slots),
            free,
        }
    }

    /// 取一个空槽。返回 `None` 表示发送线程还没消费完，解析线程需要等。
    pub fn take_free(&self) -> Option<Frame> {
        self.free.pop()
    }

    /// 提交一个填好的帧。返回 `Err` 表示已满（正常情况下不会发生，
    /// 因为槽位总数固定，能取到空槽就一定有位置放回去）。
    pub fn push_filled(&self, frame: Frame) -> Result<(), Frame> {
        self.filled.push(frame)
    }

    pub fn pop_filled(&self) -> Option<Frame> {
        self.filled.pop()
    }

    /// 归还用完的槽
    pub fn recycle(&self, mut frame: Frame) {
        frame.data.clear();
        frame.line_no = 0;
        frame.spans.clear();
        let _ = self.free.push(frame);
    }

    /// 当前待发帧数，用于界面展示缓冲水位
    pub fn pending(&self) -> usize {
        self.filled.len()
    }

    /// 清空待发帧，把槽全部还回空闲池
    pub fn drain(&self) {
        while let Some(f) = self.filled.pop() {
            self.recycle(f);
        }
    }
}

impl Default for Ring {
    fn default() -> Self {
        Ring::new(DEFAULT_SLOTS, DEFAULT_SLOT_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_frame() {
        let ring = Ring::new(4, 64);
        let mut f = ring.take_free().unwrap();
        f.data.extend_from_slice(&[1, 2, 3]);
        f.line_no = 42;
        ring.push_filled(f).ok().unwrap();

        assert_eq!(ring.pending(), 1);
        let got = ring.pop_filled().unwrap();
        assert_eq!(got.data, vec![1, 2, 3]);
        assert_eq!(got.line_no, 42);
        ring.recycle(got);
        assert_eq!(ring.pending(), 0);
    }

    #[test]
    fn free_pool_is_exhaustible_and_refillable() {
        let ring = Ring::new(2, 16);
        let a = ring.take_free().unwrap();
        let b = ring.take_free().unwrap();
        assert!(ring.take_free().is_none(), "槽位用尽后必须返回 None");

        ring.recycle(a);
        assert!(ring.take_free().is_some());
        ring.recycle(b);
    }

    #[test]
    fn recycled_slot_keeps_capacity_but_clears_content() {
        let ring = Ring::new(1, 8);
        let mut f = ring.take_free().unwrap();
        f.data.extend_from_slice(&[0u8; 4096]); // 超长帧撑大容量
        let cap = f.data.capacity();
        ring.recycle(f);

        let again = ring.take_free().unwrap();
        assert!(again.data.is_empty(), "内容必须清空");
        assert_eq!(again.data.capacity(), cap, "容量必须保留，避免重复分配");
        assert_eq!(again.line_no, 0);
    }

    #[test]
    fn drain_returns_all_slots() {
        let ring = Ring::new(3, 16);
        for _ in 0..3 {
            let f = ring.take_free().unwrap();
            ring.push_filled(f).ok().unwrap();
        }
        assert!(ring.take_free().is_none());
        ring.drain();
        assert_eq!(ring.pending(), 0);
        assert!(ring.take_free().is_some());
    }
}
