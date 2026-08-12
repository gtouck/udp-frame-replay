//! 解析线程：把文件里的行变成待发帧，填进环形缓冲。
//!
//! 一切耗时的判断都在这条线程上完成，好让发送线程只剩下 `sendto`。
//! 这条线程跑得比发送线程快得多 —— hex 解码在 GB/s 量级，
//! 而 UDP 发送再快也就百万包/秒级，所以缓冲不会饿死。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::config::SendConfig;
use crate::engine::{Shared, S_FINISHED};
use crate::filter::CompiledFilter;
use crate::mutate::{CompiledMutations, Mutator};
use crate::net::udp::MAX_UDP_PAYLOAD;
use crate::parse::parse_line;
use crate::source::DataSource;

/// 环形缓冲满时的退避间隔
const BACKOFF: Duration = Duration::from_micros(50);

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    shared: Arc<Shared>,
    source: Arc<DataSource>,
    cfg: SendConfig,
    filter: CompiledFilter,
    mutations: CompiledMutations,
    start_line: u64,
    end_line: u64,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("frame-parser".into())
        .spawn(move || run(shared, source, cfg, filter, mutations, start_line, end_line))
        .expect("创建解析线程失败")
}

fn run(
    shared: Arc<Shared>,
    source: Arc<DataSource>,
    cfg: SendConfig,
    filter: CompiledFilter,
    mutations: CompiledMutations,
    start_line: u64,
    end_line: u64,
) {
    let enc = cfg.parse.encoding;
    let mut mutator = Mutator::new(mutations);
    // 修改后的帧写进这个缓冲，再和槽位交换 —— 交换只动指针，不拷贝字节
    let mut mutated: Vec<u8> = Vec::with_capacity(2048);
    let mut spans = crate::mutate::SpanSet::default();
    // 行号在内部一律用 0-based，配置里的 start/end 是 1-based 且含端点
    let first = start_line - 1;
    let last_exclusive = end_line;

    let mut line = first;
    let mut loops_done = 0u32;
    let mut oversize_logged = false;
    let mut mutation_issue_logged = false;

    'outer: loop {
        if shared.stopping() {
            break;
        }

        if line >= last_exclusive {
            loops_done += 1;
            shared
                .stats
                .loops_done
                .store(loops_done as u64, Ordering::Relaxed);

            let more = cfg.pacing.repeat
                && (cfg.pacing.repeat_count == 0 || loops_done < cfg.pacing.repeat_count);
            if !more {
                break;
            }
            line = first;
            continue;
        }

        // 取一个空槽。取不到说明发送线程还没跟上，等一下再来。
        let mut frame = loop {
            if shared.stopping() {
                break 'outer;
            }
            match shared.ring.take_free() {
                Some(f) => break f,
                None => thread::sleep(BACKOFF),
            }
        };

        shared.stats.current_line.store(line + 1, Ordering::Relaxed);

        let text = match source.line_text(line as usize, enc) {
            Some(t) => t,
            None => {
                shared.ring.recycle(frame);
                break;
            }
        };

        // 直接解码进槽位的缓冲区，省掉一次中间拷贝
        let (_, err) = parse_line(&text, &cfg.parse, &mut frame.data);

        if let Some(kind) = err {
            // 只累加聚合计数，不产生日志条目 —— 坏文件会有几百万条同类错误
            shared.log.parse_error(kind, (line + 1) as u32);
            shared.stats.skipped_lines.fetch_add(1, Ordering::Relaxed);
            shared.ring.recycle(frame);
            line += 1;
            continue;
        }

        if frame.data.len() > MAX_UDP_PAYLOAD {
            shared.stats.oversize.fetch_add(1, Ordering::Relaxed);
            if !oversize_logged {
                oversize_logged = true;
                shared.log.warn(format!(
                    "第 {} 行有 {} 字节，超过 UDP 单包上限 {MAX_UDP_PAYLOAD}，已跳过（后续同类不再单独记录）",
                    line + 1,
                    frame.data.len()
                ));
            }
            shared.ring.recycle(frame);
            line += 1;
            continue;
        }

        // 筛选放在解析之后：条件既可能看行内字段，也可能看解码出的字节。
        // 不匹配的行直接跳过，连缓冲槽位都不占。
        if !filter.is_empty() && !filter.accepts(&text, &frame.data) {
            shared.stats.filtered_out.fetch_add(1, Ordering::Relaxed);
            shared.ring.recycle(frame);
            line += 1;
            continue;
        }

        // 修改规则放在筛选之后：筛选看的是原始数据，改完再筛会让规则的含义变得难以预料
        if !mutator.is_empty() {
            let stats = mutator.apply(&text, &frame.data, &mut mutated, &mut spans, loops_done);
            std::mem::swap(&mut frame.data, &mut mutated);
            frame.spans = spans;

            if !stats.is_clean() {
                let n = (stats.out_of_range + stats.overlaps) as u64;
                shared.stats.mutation_issues.fetch_add(n, Ordering::Relaxed);
                if !mutation_issue_logged {
                    mutation_issue_logged = true;
                    shared.log.warn(format!(
                        "第 {} 行有修改规则未能生效：偏移越界 {} 次、区间冲突 {} 次（后续同类不再单独记录）",
                        line + 1,
                        stats.out_of_range,
                        stats.overlaps
                    ));
                }
            }

            // 改完可能超出 UDP 单包上限
            if frame.data.len() > MAX_UDP_PAYLOAD {
                shared.stats.oversize.fetch_add(1, Ordering::Relaxed);
                shared.ring.recycle(frame);
                line += 1;
                continue;
            }
        }

        frame.line_no = (line + 1) as u32;
        shared.stats.parsed_frames.fetch_add(1, Ordering::Relaxed);

        if shared.ring.push_filled(frame).is_err() {
            // 槽位总数固定，取得到空槽就一定放得回去；真到这里说明有 bug
            shared.log.error("内部错误：环形缓冲已满，发送中止");
            break;
        }

        line += 1;
    }

    // 告诉发送线程「不会再有新帧了」，它把剩下的发完就可以收尾
    shared.producer_done.store(true, Ordering::Release);

    // 正常跑完（不是被叫停）且已经没有待发帧时，直接置为完成
    if !shared.stopping() && shared.ring.pending() == 0 {
        let _ = shared.state.compare_exchange(
            crate::engine::S_RUNNING,
            S_FINISHED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}
