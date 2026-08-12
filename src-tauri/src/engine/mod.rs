//! 发送引擎：解析线程 + 发送线程 + 共享状态。
//!
//! 分工严格：解析线程负责一切「想」的事（取行、剥前缀、解码、改字节），
//! 发送线程只负责「发」—— 定时、`sendto`、记录。
//! 发送线程里没有解析、没有加锁、没有堆分配，因为微秒级节拍下任何一项都会变成抖动。

pub mod clock;
pub mod pipeline;
pub mod ring;
pub mod sender;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use parking_lot::Mutex;
use serde::Serialize;
use thiserror::Error;

use crate::config::SendConfig;
use crate::log::LogSink;
use crate::net::{NetError, UdpSender};
use crate::source::DataSource;
use ring::Ring;

// 运行状态。用 u8 原子量表示，发送线程每帧都要读。
pub const S_IDLE: u8 = 0;
pub const S_RUNNING: u8 = 1;
pub const S_PAUSED: u8 = 2;
pub const S_STOPPING: u8 = 3;
pub const S_FINISHED: u8 = 4;

pub fn state_name(s: u8) -> &'static str {
    match s {
        S_RUNNING => "running",
        S_PAUSED => "paused",
        S_STOPPING => "stopping",
        S_FINISHED => "finished",
        _ => "idle",
    }
}

/// 发送视图里展示的最近帧数量。高频发送时界面本来就只能采样。
const RECENT_CAP: usize = 1000;

/// 单帧在快照里保留的字节数上限
const RECENT_BYTES: usize = 256;

/// 抖动统计的样本窗口
const JITTER_CAP: usize = 4096;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("尚未打开文件")]
    NoFile,

    #[error("正在发送，请先停止")]
    Busy,

    #[error(transparent)]
    Net(#[from] NetError),

    #[error("起始行 {start} 大于结束行 {end}")]
    BadRange { start: u64, end: u64 },

    #[error("起始行 {0} 超出文件行数")]
    StartBeyondEof(u64),

    #[error("文件在打开后被修改，请重新打开")]
    FileChanged,
}

#[derive(Default)]
pub struct Stats {
    pub sent_frames: AtomicU64,
    pub sent_bytes: AtomicU64,
    /// 内核发送缓冲满而丢弃的帧数。必须单独计数 ——
    /// UDP 不会告诉任何人这件事，不显式暴露使用者就会以为全发出去了。
    pub dropped_buffer_full: AtomicU64,
    pub refused: AtomicU64,
    pub io_errors: AtomicU64,
    /// 超过 UDP 单包上限而跳过的帧数
    pub oversize: AtomicU64,
    pub parsed_frames: AtomicU64,
    pub skipped_lines: AtomicU64,
    /// 解析线程当前读到的行号，1-based
    pub current_line: AtomicU64,
    pub loops_done: AtomicU64,
}

/// 发送视图用的单帧快照
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentFrame {
    pub line_no: u32,
    /// 实际发出的总字节数
    pub len: u32,
    /// 帧内容，超长时截断
    pub bytes: Vec<u8>,
    pub at: u64,
}

#[derive(Default)]
pub struct Jitter {
    samples: VecDeque<u64>,
}

impl Jitter {
    fn push(&mut self, us: u64) {
        if self.samples.len() >= JITTER_CAP {
            self.samples.pop_front();
        }
        self.samples.push_back(us);
    }

    /// 返回 (p50, p99)，单位微秒
    fn percentiles(&self) -> (u64, u64) {
        if self.samples.is_empty() {
            return (0, 0);
        }
        let mut v: Vec<u64> = self.samples.iter().copied().collect();
        v.sort_unstable();
        let at = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
        (at(0.50), at(0.99))
    }

    /// 时序带要画的最近样本
    fn tail(&self, n: usize) -> Vec<u64> {
        self.samples.iter().rev().take(n).rev().copied().collect()
    }
}

pub struct Shared {
    pub state: AtomicU8,
    /// 单步信用。暂停状态下每有一点信用，发送线程就放一帧出去。
    pub step_credits: AtomicI64,
    /// 解析线程已把该发的都发完了
    pub producer_done: AtomicBool,
    pub stats: Stats,
    pub ring: Ring,
    pub log: Arc<LogSink>,
    pub recent: Mutex<VecDeque<SentFrame>>,
    pub jitter: Mutex<Jitter>,
    pub target_desc: String,
    pub interval_us: u64,
}

impl Shared {
    #[inline]
    pub fn state(&self) -> u8 {
        self.state.load(Ordering::Acquire)
    }

    #[inline]
    pub fn stopping(&self) -> bool {
        self.state() == S_STOPPING
    }

    /// 记录一帧已发出。
    ///
    /// 快照用 `try_lock`：界面正在读的时候直接跳过这一帧，
    /// 绝不让发送线程为了记录而阻塞。反正高频下界面本来就是采样显示。
    /// `interval_us` 为 `None` 表示单步发送 —— 那种间隔不代表节拍能力，不计入抖动统计。
    pub fn record_sent(&self, line_no: u32, data: &[u8], at: u64, interval_us: Option<u64>) {
        self.stats.sent_frames.fetch_add(1, Ordering::Relaxed);
        self.stats
            .sent_bytes
            .fetch_add(data.len() as u64, Ordering::Relaxed);

        if let Some(mut r) = self.recent.try_lock() {
            if r.len() >= RECENT_CAP {
                r.pop_front();
            }
            let n = data.len().min(RECENT_BYTES);
            r.push_back(SentFrame {
                line_no,
                len: data.len() as u32,
                bytes: data[..n].to_vec(),
                at,
            });
        }

        if let Some(us) = interval_us {
            if let Some(mut j) = self.jitter.try_lock() {
                j.push(us);
            }
        }
    }
}

/// 一次轮询返回给界面的完整状态
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineSnapshot {
    pub state: String,
    pub sent_frames: u64,
    pub sent_bytes: u64,
    pub dropped_buffer_full: u64,
    pub refused: u64,
    pub io_errors: u64,
    pub oversize: u64,
    pub parsed_frames: u64,
    pub skipped_lines: u64,
    pub current_line: u64,
    pub loops_done: u64,
    pub pending: usize,
    pub jitter_p50_us: u64,
    pub jitter_p99_us: u64,
    /// 时序带要画的最近帧间隔样本
    pub recent_intervals: Vec<u64>,
    pub target_desc: String,
    pub interval_us: u64,
}

pub struct Engine {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl Engine {
    /// 启动一次发送任务。
    pub fn start(
        source: Arc<DataSource>,
        cfg: SendConfig,
        log: Arc<LogSink>,
    ) -> Result<Self, EngineError> {
        // mmap 期间文件被外部截断会触发 SIGBUS，进程直接崩溃，
        // 常规错误处理兜不住，只能在开始前主动复检。
        source.verify_unchanged().map_err(|_| EngineError::FileChanged)?;

        let line_count = source.line_count() as u64;
        let start = cfg.pacing.start_line.max(1);
        let end = if cfg.pacing.end_line == 0 {
            line_count
        } else {
            cfg.pacing.end_line.min(line_count)
        };

        if start > line_count {
            return Err(EngineError::StartBeyondEof(start));
        }
        if start > end {
            return Err(EngineError::BadRange { start, end });
        }

        let udp = UdpSender::build(&cfg.target)?;
        let target_desc = udp.description.clone();

        log.info(format!("开始发送 · {target_desc}"));
        if let Some(local) = udp.local_addr() {
            log.info(format!("本地端口 {local}"));
        }
        log.info(format!(
            "行范围 {start} ~ {end}，间隔 {}μs{}",
            cfg.pacing.interval_us,
            if cfg.pacing.high_precision {
                "，高精度模式"
            } else {
                ""
            }
        ));

        let shared = Arc::new(Shared {
            state: AtomicU8::new(S_RUNNING),
            step_credits: AtomicI64::new(0),
            producer_done: AtomicBool::new(false),
            stats: Stats::default(),
            ring: Ring::default(),
            log: log.clone(),
            recent: Mutex::new(VecDeque::with_capacity(RECENT_CAP)),
            jitter: Mutex::new(Jitter::default()),
            target_desc,
            interval_us: cfg.pacing.interval_us,
        });

        let mut threads = Vec::with_capacity(2);
        threads.push(pipeline::spawn(shared.clone(), source, cfg.clone(), start, end));
        threads.push(sender::spawn(shared.clone(), udp, cfg.pacing.clone()));

        Ok(Engine { shared, threads })
    }

    pub fn pause(&self) {
        let _ = self.shared.state.compare_exchange(
            S_RUNNING,
            S_PAUSED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub fn resume(&self) {
        let _ = self.shared.state.compare_exchange(
            S_PAUSED,
            S_RUNNING,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// 单步：暂停状态下放一帧出去
    pub fn step(&self) {
        self.shared.step_credits.fetch_add(1, Ordering::AcqRel);
    }

    pub fn stop(&self) {
        self.shared.state.store(S_STOPPING, Ordering::Release);
    }

    /// 停止并等待两个线程收尾
    pub fn shutdown(mut self) {
        self.stop();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        self.shared.ring.drain();
        self.shared.log.info(format!(
            "停止发送 · 共发出 {} 帧",
            self.shared.stats.sent_frames.load(Ordering::Relaxed)
        ));
    }

    pub fn snapshot(&self) -> EngineSnapshot {
        let s = &self.shared.stats;
        let (p50, p99) = {
            let j = self.shared.jitter.lock();
            j.percentiles()
        };
        let recent_intervals = {
            let j = self.shared.jitter.lock();
            j.tail(200)
        };

        EngineSnapshot {
            state: state_name(self.shared.state()).to_string(),
            sent_frames: s.sent_frames.load(Ordering::Relaxed),
            sent_bytes: s.sent_bytes.load(Ordering::Relaxed),
            dropped_buffer_full: s.dropped_buffer_full.load(Ordering::Relaxed),
            refused: s.refused.load(Ordering::Relaxed),
            io_errors: s.io_errors.load(Ordering::Relaxed),
            oversize: s.oversize.load(Ordering::Relaxed),
            parsed_frames: s.parsed_frames.load(Ordering::Relaxed),
            skipped_lines: s.skipped_lines.load(Ordering::Relaxed),
            current_line: s.current_line.load(Ordering::Relaxed),
            loops_done: s.loops_done.load(Ordering::Relaxed),
            pending: self.shared.ring.pending(),
            jitter_p50_us: p50,
            jitter_p99_us: p99,
            recent_intervals,
            target_desc: self.shared.target_desc.clone(),
            interval_us: self.shared.interval_us,
        }
    }

    /// 最近发出的若干帧，供发送视图展示
    pub fn recent_frames(&self, limit: usize) -> Vec<SentFrame> {
        let r = self.shared.recent.lock();
        r.iter().rev().take(limit).rev().cloned().collect()
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.shared.state(), S_FINISHED | S_IDLE)
    }
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("state", &state_name(self.shared.state()))
            .field(
                "sent_frames",
                &self.shared.stats.sent_frames.load(Ordering::Relaxed),
            )
            .field("target", &self.shared.target_desc)
            .finish()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}
