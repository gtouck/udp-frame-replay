//! 阶段二：写入计算值（序号 / 时间戳 / 长度 / 校验和）。
//!
//! 偏移基于**阶段一之后**的帧，并且严格按使用者排定的顺序执行 ——
//! 因为校验和必须最后算：长度字段通常也在校验范围内，先算校验和就白算了。
//!
//! 这两条合起来正是「插入或删除字节后长度和校验和依然正确」的由来。

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::config::{ByteRange, Endian, TimeEpoch, TimeUnit};
use crate::filter::{resolve_offset, FieldSplit};
use crate::mutate::compile::{Computed, Stage2Op};
use crate::mutate::{checksum, MutStats, SpanKind, SpanSet};

pub struct Ctx<'a> {
    pub counters: &'a mut [u64],
    pub started: Instant,
}

/// 就地改写 `frame`。
pub fn apply(
    frame: &mut [u8],
    ops: &[Stage2Op],
    line_text: &str,
    split: &FieldSplit,
    ctx: &mut Ctx<'_>,
    spans: &mut SpanSet,
) -> MutStats {
    let mut stats = MutStats::default();
    let len = frame.len();

    for op in ops {
        if let Some(c) = &op.cond {
            if !c.eval(line_text, frame, split) {
                continue;
            }
        }

        let Some(at) = resolve_offset(op.offset, len, op.width) else {
            stats.out_of_range += 1;
            continue;
        };

        let value = match &op.kind {
            Computed::Sequence { step, counter, .. } => {
                let v = ctx.counters[*counter];
                ctx.counters[*counter] = v.wrapping_add(*step);
                v
            }

            Computed::Timestamp { unit, epoch } => {
                let d = match epoch {
                    TimeEpoch::Unix => SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default(),
                    TimeEpoch::SinceStart => ctx.started.elapsed(),
                };
                match unit {
                    TimeUnit::Millis => d.as_millis() as u64,
                    TimeUnit::Micros => d.as_micros() as u64,
                }
            }

            Computed::Length {
                range,
                include_self,
            } => {
                let Some((a, b)) = resolve_range(*range, len) else {
                    stats.out_of_range += 1;
                    continue;
                };
                let mut n = (b - a) as u64;
                if *include_self {
                    n += op.width as u64;
                }
                n
            }

            Computed::Checksum { algorithm, range } => {
                let Some((a, b)) = resolve_range(*range, len) else {
                    stats.out_of_range += 1;
                    continue;
                };
                // 校验和字段若落在计算范围内，先清零再算 —— 这是绝大多数协议的约定，
                // 否则算进去的是上一次的残值，结果每次都不一样。
                if at < b && a < at + op.width {
                    frame[at..at + op.width].fill(0);
                }
                checksum::compute(*algorithm, &frame[a..b])
            }
        };

        write_value(&mut frame[at..at + op.width], value, op.endian);
        spans.push(at, op.width, SpanKind::Computed);
    }

    stats
}

/// 把可能为负的范围换算成 `[start, end)`。`end` 为 0 表示到帧尾。
fn resolve_range(r: ByteRange, len: usize) -> Option<(usize, usize)> {
    let start = if r.start >= 0 {
        r.start as usize
    } else {
        len.checked_sub(r.start.unsigned_abs() as usize)?
    };

    let end = if r.end == 0 {
        len
    } else if r.end > 0 {
        r.end as usize
    } else {
        len.checked_sub(r.end.unsigned_abs() as usize)?
    };

    if start > end || end > len {
        return None;
    }
    Some((start, end))
}

/// 按字节序把数值写进定长字段
fn write_value(dst: &mut [u8], value: u64, endian: Endian) {
    let w = dst.len();
    for (i, slot) in dst.iter_mut().enumerate() {
        let shift = match endian {
            Endian::Big => 8 * (w - 1 - i),
            Endian::Little => 8 * i,
        };
        *slot = ((value >> shift) & 0xFF) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ChecksumAlgo, Delimiter, MutationConfig, MutationOp, MutationRule, PrefixRule, Width,
    };
    use crate::mutate::compile::CompiledMutations;

    fn prefix() -> PrefixRule {
        PrefixRule::Fields {
            delimiter: Delimiter::Whitespace,
            collapse: true,
            skip_fields: 3,
        }
    }

    /// 对 `frame` 跑一遍阶段二，返回改完的帧
    fn run(frame: &[u8], ops: Vec<MutationOp>) -> (Vec<u8>, MutStats) {
        let cfg = MutationConfig {
            rules: ops
                .into_iter()
                .map(|op| MutationRule {
                    op,
                    condition: None,
                    enabled: true,
                })
                .collect(),
        };
        let c = CompiledMutations::compile(&cfg, &prefix()).unwrap();
        let mut counters: Vec<u64> = c
            .stage2
            .iter()
            .filter_map(|o| match &o.kind {
                Computed::Sequence { start, counter, .. } => Some((*counter, *start)),
                _ => None,
            })
            .fold(vec![0u64; c.counters], |mut acc, (i, v)| {
                acc[i] = v;
                acc
            });

        let mut buf = frame.to_vec();
        let mut ctx = Ctx {
            counters: &mut counters,
            started: Instant::now(),
        };
        let stats = apply(
            &mut buf,
            &c.stage2,
            "[TX] 1 发送",
            &c.split,
            &mut ctx,
            &mut SpanSet::default(),
        );
        (buf, stats)
    }

    #[test]
    fn writes_length_big_endian() {
        let frame = vec![0u8; 8];
        let (out, _) = run(
            &frame,
            vec![MutationOp::Length {
                offset: 0,
                width: Width::W2,
                endian: Endian::Big,
                range: ByteRange { start: 2, end: 0 },
                include_self: false,
            }],
        );
        // [2,8) 共 6 字节
        assert_eq!(&out[0..2], &[0x00, 0x06]);
    }

    #[test]
    fn writes_length_little_endian() {
        let frame = vec![0u8; 300];
        let (out, _) = run(
            &frame,
            vec![MutationOp::Length {
                offset: 0,
                width: Width::W2,
                endian: Endian::Little,
                range: ByteRange { start: 2, end: 0 },
                include_self: false,
            }],
        );
        // 298 = 0x012A，小端写作 2A 01
        assert_eq!(&out[0..2], &[0x2A, 0x01]);
    }

    #[test]
    fn include_self_adds_the_field_width() {
        let frame = vec![0u8; 8];
        let (out, _) = run(
            &frame,
            vec![MutationOp::Length {
                offset: 0,
                width: Width::W2,
                endian: Endian::Big,
                range: ByteRange { start: 2, end: 0 },
                include_self: true,
            }],
        );
        assert_eq!(&out[0..2], &[0x00, 0x08], "6 字节 + 2 字节长度字段自身");
    }

    #[test]
    fn sequence_increments_each_frame() {
        let cfg = MutationConfig {
            rules: vec![MutationRule {
                op: MutationOp::Sequence {
                    offset: 0,
                    width: Width::W2,
                    endian: Endian::Big,
                    start: 100,
                    step: 5,
                    reset_each_loop: false,
                },
                condition: None,
                enabled: true,
            }],
        };
        let c = CompiledMutations::compile(&cfg, &prefix()).unwrap();
        let mut counters = vec![100u64];
        let mut got = Vec::new();

        for _ in 0..3 {
            let mut buf = vec![0u8; 4];
            let mut ctx = Ctx {
                counters: &mut counters,
                started: Instant::now(),
            };
            apply(
                &mut buf,
                &c.stage2,
                "",
                &c.split,
                &mut ctx,
                &mut SpanSet::default(),
            );
            got.push(u16::from_be_bytes([buf[0], buf[1]]));
        }
        assert_eq!(got, vec![100, 105, 110]);
    }

    #[test]
    fn checksum_matches_manual_computation() {
        let frame = vec![0x01, 0x02, 0x03, 0x04, 0x00, 0x00];
        let (out, _) = run(
            &frame,
            vec![MutationOp::Checksum {
                offset: -2,
                algorithm: ChecksumAlgo::Crc16Ccitt,
                endian: Endian::Big,
                range: ByteRange { start: 0, end: -2 },
            }],
        );
        let want = checksum::compute(ChecksumAlgo::Crc16Ccitt, &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(&out[4..6], &(want as u16).to_be_bytes());
    }

    #[test]
    fn checksum_field_is_zeroed_before_being_included_in_its_own_range() {
        // 范围盖住整帧（含校验和字段自身）。字段里的残值必须先清零，
        // 否则同一份数据算两次会得到不同结果。
        let ops = || {
            vec![MutationOp::Checksum {
                offset: -2,
                algorithm: ChecksumAlgo::Crc16Ccitt,
                endian: Endian::Big,
                range: ByteRange { start: 0, end: 0 },
            }]
        };

        let (first, _) = run(&[0x01, 0x02, 0x00, 0x00], ops());
        // 拿上一轮的结果再算一次，结果必须一样
        let (second, _) = run(&first, ops());
        assert_eq!(first, second, "校验和必须可重复计算");
    }

    #[test]
    fn xor8_checksum_over_explicit_range() {
        let frame = vec![0x0F, 0xF0, 0xAA, 0x00];
        let (out, _) = run(
            &frame,
            vec![MutationOp::Checksum {
                offset: 3,
                algorithm: ChecksumAlgo::Xor8,
                endian: Endian::Big,
                range: ByteRange { start: 0, end: 3 },
            }],
        );
        assert_eq!(out[3], 0x0F ^ 0xF0 ^ 0xAA);
    }

    #[test]
    fn timestamp_since_start_is_small_and_increases() {
        let frame = vec![0u8; 8];
        let (out, _) = run(
            &frame,
            vec![MutationOp::Timestamp {
                offset: 0,
                width: Width::W8,
                endian: Endian::Big,
                unit: TimeUnit::Micros,
                epoch: TimeEpoch::SinceStart,
            }],
        );
        let v = u64::from_be_bytes(out[0..8].try_into().unwrap());
        assert!(v < 1_000_000, "相对启动的时间戳不该是个大数：{v}");
    }

    #[test]
    fn unix_timestamp_is_plausible() {
        let frame = vec![0u8; 8];
        let (out, _) = run(
            &frame,
            vec![MutationOp::Timestamp {
                offset: 0,
                width: Width::W8,
                endian: Endian::Big,
                unit: TimeUnit::Millis,
                epoch: TimeEpoch::Unix,
            }],
        );
        let v = u64::from_be_bytes(out[0..8].try_into().unwrap());
        // 2020-01-01 之后、2100 之前
        assert!(v > 1_577_836_800_000 && v < 4_102_444_800_000, "{v}");
    }

    #[test]
    fn value_wider_than_field_is_truncated_to_low_bytes() {
        let frame = vec![0u8; 4];
        let cfg = MutationConfig {
            rules: vec![MutationRule {
                op: MutationOp::Sequence {
                    offset: 0,
                    width: Width::W1,
                    endian: Endian::Big,
                    start: 0x1234,
                    step: 1,
                    reset_each_loop: false,
                },
                condition: None,
                enabled: true,
            }],
        };
        let c = CompiledMutations::compile(&cfg, &prefix()).unwrap();
        let mut counters = vec![0x1234u64];
        let mut buf = frame.clone();
        let mut ctx = Ctx {
            counters: &mut counters,
            started: Instant::now(),
        };
        apply(
                &mut buf,
                &c.stage2,
                "",
                &c.split,
                &mut ctx,
                &mut SpanSet::default(),
            );
        assert_eq!(buf[0], 0x34, "只保留低位字节");
    }

    #[test]
    fn out_of_range_field_is_skipped_and_counted() {
        let frame = vec![0u8; 2];
        let (out, s) = run(
            &frame,
            vec![MutationOp::Length {
                offset: 10,
                width: Width::W2,
                endian: Endian::Big,
                range: ByteRange::default(),
                include_self: false,
            }],
        );
        assert_eq!(out, frame);
        assert_eq!(s.out_of_range, 1);
    }

    #[test]
    fn resolve_range_handles_negatives_and_zero_end() {
        assert_eq!(resolve_range(ByteRange { start: 0, end: 0 }, 10), Some((0, 10)));
        assert_eq!(resolve_range(ByteRange { start: 2, end: 8 }, 10), Some((2, 8)));
        assert_eq!(
            resolve_range(ByteRange { start: 1, end: -1 }, 10),
            Some((1, 9)),
            "第 2 字节到倒数第 2 字节"
        );
        assert_eq!(resolve_range(ByteRange { start: -4, end: 0 }, 10), Some((6, 10)));
        assert_eq!(resolve_range(ByteRange { start: 8, end: 2 }, 10), None);
        assert_eq!(resolve_range(ByteRange { start: 0, end: 99 }, 10), None);
        assert_eq!(resolve_range(ByteRange { start: -99, end: 0 }, 10), None);
    }

    #[test]
    fn write_value_respects_endianness() {
        let mut b = [0u8; 4];
        write_value(&mut b, 0x0102_0304, Endian::Big);
        assert_eq!(b, [0x01, 0x02, 0x03, 0x04]);

        write_value(&mut b, 0x0102_0304, Endian::Little);
        assert_eq!(b, [0x04, 0x03, 0x02, 0x01]);
    }
}
