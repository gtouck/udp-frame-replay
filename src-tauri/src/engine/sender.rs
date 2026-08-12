//! 发送线程：定时、`sendto`、记录。
//!
//! 这条线程上没有解析、没有加锁、没有堆分配 ——
//! 微秒级节拍下任何一项都会直接变成时序抖动。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::PacingConfig;
use crate::engine::clock::Pacer;
use crate::engine::ring::Frame;
use crate::engine::{Shared, S_FINISHED, S_PAUSED, S_RUNNING, S_STOPPING};
use crate::net::{SendFail, UdpSender};

/// 缓冲满时的重试次数。再多也没意义，内核不会因为多问几次就腾出空间。
const SEND_RETRIES: u32 = 3;

/// 待发队列为空时的等待间隔
const IDLE_WAIT: Duration = Duration::from_micros(50);

/// 暂停时的轮询间隔
const PAUSE_WAIT: Duration = Duration::from_millis(5);

pub fn spawn(shared: Arc<Shared>, udp: UdpSender, pacing: PacingConfig) -> JoinHandle<()> {
    thread::Builder::new()
        .name("frame-sender".into())
        .spawn(move || run(shared, udp, pacing))
        .expect("创建发送线程失败")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

struct Warned {
    refused: bool,
    io: bool,
    buffer_full: bool,
}

fn run(shared: Arc<Shared>, udp: UdpSender, pacing: PacingConfig) {
    // 提升优先级能明显改善定时稳定性，但普通用户权限下常常失败 —— 失败不致命
    if let Err(e) = thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Max)
    {
        shared.log.warn(format!(
            "无法提升发送线程优先级（{e:?}），高负载下时序抖动会变大"
        ));
    }

    let mut pacer = Pacer::new(pacing.interval_us, pacing.high_precision);
    pacer.arm();

    let mut warned = Warned {
        refused: false,
        io: false,
        buffer_full: false,
    };
    let mut was_paused = false;

    loop {
        match shared.state() {
            S_STOPPING => break,

            S_PAUSED => {
                was_paused = true;
                // 单步：有信用就放一帧，不受节拍约束
                if shared.step_credits.load(Ordering::Acquire) > 0 {
                    shared.step_credits.fetch_sub(1, Ordering::AcqRel);
                    if let Some(frame) = shared.ring.pop_filled() {
                        deliver(&shared, &udp, &frame, None, &mut warned);
                        shared.ring.recycle(frame);
                    }
                } else {
                    thread::sleep(PAUSE_WAIT);
                }
                continue;
            }

            _ => {
                if was_paused {
                    // 恢复时重新计时，否则会把暂停时长当成欠账补发
                    was_paused = false;
                    pacer.arm();
                }
            }
        }

        // 先取帧再等节拍：间隔应当落在两次实际发送之间
        let frame = match shared.ring.pop_filled() {
            Some(f) => f,
            None => {
                if shared.producer_done.load(Ordering::Acquire) {
                    let _ = shared.state.compare_exchange(
                        S_RUNNING,
                        S_FINISHED,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                    break;
                }
                thread::sleep(IDLE_WAIT);
                continue;
            }
        };

        let interval_us = pacer.wait();

        // 等待期间可能被叫停，此时这一帧不再发出
        if shared.stopping() {
            shared.ring.recycle(frame);
            break;
        }

        deliver(&shared, &udp, &frame, Some(interval_us), &mut warned);
        shared.ring.recycle(frame);
    }

    if pacer.resyncs() > 0 {
        shared.log.warn(format!(
            "有 {} 次因落后过多而重新对齐节拍 —— 目标间隔可能超出本机能力",
            pacer.resyncs()
        ));
    }
}

fn deliver(
    shared: &Shared,
    udp: &UdpSender,
    frame: &Frame,
    interval_us: Option<u64>,
    warned: &mut Warned,
) {
    match udp.send(&frame.data, SEND_RETRIES) {
        Ok(_) => {
            shared.record_sent(frame.line_no, &frame.data, &frame.spans, now_ms(), interval_us);
        }

        Err(SendFail::BufferFull) => {
            shared
                .stats
                .dropped_buffer_full
                .fetch_add(1, Ordering::Relaxed);
            if !warned.buffer_full {
                warned.buffer_full = true;
                shared.log.warn(
                    "内核发送缓冲已满，开始丢帧。可以调大发送缓冲或放宽发送间隔（后续同类不再单独记录）",
                );
            }
        }

        Err(SendFail::Refused) => {
            shared.stats.refused.fetch_add(1, Ordering::Relaxed);
            if !warned.refused {
                warned.refused = true;
                shared.log.warn(format!(
                    "收到端口不可达，{} 多半没有程序在监听。UDP 无连接，发送继续（后续同类不再单独记录）",
                    shared.target_desc
                ));
            }
        }

        Err(SendFail::Io(kind)) => {
            shared.stats.io_errors.fetch_add(1, Ordering::Relaxed);
            if !warned.io {
                warned.io = true;
                shared
                    .log
                    .error(format!("发送失败：{kind:?}（后续同类不再单独记录）"));
            }
        }
    }
}
