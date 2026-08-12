//! 阶段一：结构变更（插入 / 替换 / 删除）。
//!
//! **所有偏移都基于原始帧。** 在 offset 5 插了两个字节，不会让另一条
//! 「offset 10 替换」的规则漂到 12 —— 规则之间互不影响，写规则的人
//! 只需要照着原始数据数位置，不用在脑子里模拟前面几条的累积效果。
//!
//! 实现上是把所有编辑按起点排好后正向扫一遍原始帧、边走边拼输出。
//! 这和「从后往前逐条就地应用」等价，但省掉了反复搬移字节。

use crate::filter::{resolve_offset, FieldSplit};
use crate::mutate::compile::{Stage1Op, Structural};
use crate::mutate::{MutStats, SpanKind, SpanSet};

/// 已定位的一次编辑：原始帧上的 `[start, end)` 区间
pub struct Edit {
    start: usize,
    end: usize,
    op: usize,
}

/// 把 `src` 按规则变换后写入 `dst`（会先清空）。
///
/// `scratch` 是复用的编辑列表缓冲，避免每帧分配。
pub fn apply(
    src: &[u8],
    ops: &[Stage1Op],
    line_text: &str,
    split: &FieldSplit,
    dst: &mut Vec<u8>,
    scratch: &mut Vec<Edit>,
    spans: &mut SpanSet,
) -> MutStats {
    let mut stats = MutStats::default();
    dst.clear();
    scratch.clear();

    for (i, op) in ops.iter().enumerate() {
        if let Some(c) = &op.cond {
            if !c.eval(line_text, src, split) {
                continue;
            }
        }

        let span = op.kind.span();
        match resolve_offset(op.offset, src.len(), span) {
            Some(start) => scratch.push(Edit {
                start,
                end: start + span,
                op: i,
            }),
            None => stats.out_of_range += 1,
        }
    }

    if scratch.is_empty() {
        dst.extend_from_slice(src);
        return stats;
    }

    // 稳定排序：同一位置的多次插入保持书写顺序
    scratch.sort_by_key(|e| e.start);

    let mut pos = 0usize;
    for edit in scratch.iter() {
        if edit.start < pos {
            // 与前一条编辑撞上了。负偏移到执行期才知道位置，编译期拦不住。
            stats.overlaps += 1;
            continue;
        }
        dst.extend_from_slice(&src[pos..edit.start]);

        match &ops[edit.op].kind {
            Structural::Insert(bytes) => {
                spans.push(dst.len(), bytes.len(), SpanKind::Insert);
                dst.extend_from_slice(bytes);
                // 插入是零宽的：原来 start 处的字节还要照常输出
                pos = edit.start;
            }
            Structural::Replace(bytes) => {
                spans.push(dst.len(), bytes.len(), SpanKind::Replace);
                dst.extend_from_slice(bytes);
                pos = edit.end;
            }
            Structural::Delete(_) => {
                pos = edit.end;
            }
        }
    }
    dst.extend_from_slice(&src[pos..]);

    stats
}

/// 供 `Mutator` 预分配复用缓冲
pub fn new_scratch() -> Vec<Edit> {
    Vec::with_capacity(16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ByteRange, Condition, Delimiter, MutationConfig, MutationOp, MutationRule, PrefixRule,
        TextOp,
    };
    use crate::mutate::compile::CompiledMutations;

    const SRC: &[u8] = &[0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
    const LINE: &str = "[TX] 1 发送 00 11 22 33 44 55";

    fn prefix() -> PrefixRule {
        PrefixRule::Fields {
            delimiter: Delimiter::Whitespace,
            collapse: true,
            skip_fields: 3,
        }
    }

    fn run(ops: Vec<MutationOp>) -> (Vec<u8>, MutStats) {
        run_with(ops.into_iter().map(|op| (op, None)).collect())
    }

    fn run_with(ops: Vec<(MutationOp, Option<Condition>)>) -> (Vec<u8>, MutStats) {
        let cfg = MutationConfig {
            rules: ops
                .into_iter()
                .map(|(op, condition)| MutationRule {
                    op,
                    condition,
                    enabled: true,
                })
                .collect(),
        };
        let c = CompiledMutations::compile(&cfg, &prefix()).unwrap();
        let mut dst = Vec::new();
        let mut scratch = new_scratch();
        let stats = apply(
            SRC,
            &c.stage1,
            LINE,
            &c.split,
            &mut dst,
            &mut scratch,
            &mut crate::mutate::SpanSet::default(),
        );
        (dst, stats)
    }

    #[test]
    fn no_rules_copies_frame_unchanged() {
        let (out, s) = run(vec![]);
        assert_eq!(out, SRC);
        assert_eq!(s.out_of_range, 0);
    }

    #[test]
    fn insert_puts_bytes_before_the_offset() {
        let (out, _) = run(vec![MutationOp::Insert {
            offset: 2,
            value: "AA BB".into(),
        }]);
        assert_eq!(out, vec![0x00, 0x11, 0xAA, 0xBB, 0x22, 0x33, 0x44, 0x55]);
    }

    #[test]
    fn insert_at_zero_prepends() {
        let (out, _) = run(vec![MutationOp::Insert {
            offset: 0,
            value: "FF".into(),
        }]);
        assert_eq!(out[0], 0xFF);
        assert_eq!(&out[1..], SRC);
    }

    #[test]
    fn insert_at_end_appends() {
        let (out, _) = run(vec![MutationOp::Insert {
            offset: 6,
            value: "FF".into(),
        }]);
        assert_eq!(out, vec![0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0xFF]);
    }

    #[test]
    fn replace_overwrites_in_place() {
        let (out, _) = run(vec![MutationOp::Replace {
            offset: 1,
            value: "AA BB".into(),
        }]);
        assert_eq!(out, vec![0x00, 0xAA, 0xBB, 0x33, 0x44, 0x55]);
        assert_eq!(out.len(), SRC.len(), "替换不改变帧长");
    }

    #[test]
    fn delete_removes_the_span() {
        let (out, _) = run(vec![MutationOp::Delete {
            offset: 2,
            length: 2,
        }]);
        assert_eq!(out, vec![0x00, 0x11, 0x44, 0x55]);
    }

    #[test]
    fn negative_offset_counts_from_the_end() {
        let (out, _) = run(vec![MutationOp::Replace {
            offset: -2,
            value: "EE FF".into(),
        }]);
        assert_eq!(out, vec![0x00, 0x11, 0x22, 0x33, 0xEE, 0xFF]);
    }

    /// 这是阶段一的核心保证 —— 独立测一遍
    #[test]
    fn offsets_stay_relative_to_the_original_frame() {
        // 在 0 处插 2 字节，同时替换原始帧的 offset 3。
        // 若偏移会随插入漂移，替换就会落到错误的位置。
        let (out, _) = run(vec![
            MutationOp::Insert {
                offset: 0,
                value: "AA AA".into(),
            },
            MutationOp::Replace {
                offset: 3,
                value: "EE".into(),
            },
        ]);
        // 原始帧的 offset 3 是 0x33，必须被换成 EE
        assert_eq!(out, vec![0xAA, 0xAA, 0x00, 0x11, 0x22, 0xEE, 0x44, 0x55]);
    }

    #[test]
    fn delete_and_replace_do_not_shift_each_other() {
        let (out, _) = run(vec![
            MutationOp::Delete {
                offset: 1,
                length: 2,
            },
            MutationOp::Replace {
                offset: 4,
                value: "EE".into(),
            },
        ]);
        // 删掉 11 22，原始 offset 4（0x44）换成 EE
        assert_eq!(out, vec![0x00, 0x33, 0xEE, 0x55]);
    }

    #[test]
    fn multiple_inserts_at_same_offset_keep_written_order() {
        let (out, _) = run(vec![
            MutationOp::Insert {
                offset: 2,
                value: "AA".into(),
            },
            MutationOp::Insert {
                offset: 2,
                value: "BB".into(),
            },
        ]);
        assert_eq!(out, vec![0x00, 0x11, 0xAA, 0xBB, 0x22, 0x33, 0x44, 0x55]);
    }

    #[test]
    fn out_of_range_operation_is_skipped_and_counted() {
        let (out, s) = run(vec![MutationOp::Replace {
            offset: 99,
            value: "EE".into(),
        }]);
        assert_eq!(out, SRC, "越界的规则不该改动帧");
        assert_eq!(s.out_of_range, 1);
    }

    #[test]
    fn runtime_overlap_from_negative_offsets_is_counted() {
        // 编译期放行（负偏移），执行期才发现 [2,5) 与 [4,6) 重叠
        let (_, s) = run(vec![
            MutationOp::Replace {
                offset: -4,
                value: "01 02 03".into(),
            },
            MutationOp::Delete {
                offset: -2,
                length: 2,
            },
        ]);
        assert_eq!(s.overlaps, 1);
    }

    #[test]
    fn condition_gates_a_rule() {
        let matching = Condition::Field {
            index: 0,
            op: TextOp::Equals,
            value: "[TX]".into(),
        };
        let (out, _) = run_with(vec![(
            MutationOp::Replace {
                offset: 0,
                value: "EE".into(),
            },
            Some(matching),
        )]);
        assert_eq!(out[0], 0xEE, "条件成立时应当生效");

        let other = Condition::Field {
            index: 0,
            op: TextOp::Equals,
            value: "[RX]".into(),
        };
        let (out, _) = run_with(vec![(
            MutationOp::Replace {
                offset: 0,
                value: "EE".into(),
            },
            Some(other),
        )]);
        assert_eq!(out, SRC, "条件不成立时该帧不该被改动");
    }

    #[test]
    fn stage1_ignores_stage2_operations() {
        let (out, _) = run(vec![MutationOp::Checksum {
            offset: 0,
            algorithm: crate::config::ChecksumAlgo::Crc16Ccitt,
            endian: crate::config::Endian::Big,
            range: ByteRange::default(),
        }]);
        assert_eq!(out, SRC, "校验和属于阶段二，阶段一不该碰它");
    }
}
