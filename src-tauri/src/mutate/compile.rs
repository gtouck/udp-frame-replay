//! 规则编译：把界面上的文本配置变成执行期只剩比较和写入的紧凑结构。
//!
//! 能在启动时发现的问题一律在这里报出来 —— 发到一半才炸是最糟的失败方式。

use thiserror::Error;

use crate::config::{
    ByteRange, ChecksumAlgo, Endian, MutationConfig, MutationOp, PrefixRule, TimeEpoch, TimeUnit,
    Width,
};
use crate::filter::{parse_hex, CompiledCondition, FieldSplit, FilterError};
use crate::mutate::checksum;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MutateError {
    #[error("第 {index} 条修改规则的字节值 “{text}” 不是合法的十六进制")]
    BadHex { index: usize, text: String },

    #[error("第 {index} 条修改规则的字节值不能为空")]
    EmptyValue { index: usize },

    #[error("第 {index} 条修改规则的删除长度不能为 0")]
    ZeroLength { index: usize },

    #[error("第 {index} 条修改规则的范围起点 {start} 在终点 {end} 之后")]
    BadRange { index: usize, start: i64, end: i64 },

    #[error("第 {a} 条和第 {b} 条修改规则改到了同一段字节，结果会取决于谁先执行，请先分开")]
    Overlap { a: usize, b: usize },

    #[error("第 {index} 条修改规则的条件有误：{source}")]
    Condition {
        index: usize,
        #[source]
        source: FilterError,
    },
}

/// 阶段一：改变帧结构的操作
#[derive(Debug)]
pub enum Structural {
    Insert(Vec<u8>),
    Replace(Vec<u8>),
    Delete(usize),
}

impl Structural {
    /// 该操作在**原始帧**上覆盖的字节数。插入是零宽的。
    pub fn span(&self) -> usize {
        match self {
            Structural::Insert(_) => 0,
            Structural::Replace(b) => b.len(),
            Structural::Delete(n) => *n,
        }
    }
}

#[derive(Debug)]
pub struct Stage1Op {
    /// 1-based 规则序号，用于报错
    pub index: usize,
    pub offset: i64,
    pub kind: Structural,
    pub cond: Option<CompiledCondition>,
}

/// 阶段二：写入计算值的操作
#[derive(Debug)]
pub enum Computed {
    Sequence {
        start: u64,
        step: u64,
        reset_each_loop: bool,
        /// 该规则专属的计数器下标
        counter: usize,
    },
    Timestamp {
        unit: TimeUnit,
        epoch: TimeEpoch,
    },
    Length {
        range: ByteRange,
        include_self: bool,
    },
    Checksum {
        algorithm: ChecksumAlgo,
        range: ByteRange,
    },
}

#[derive(Debug)]
pub struct Stage2Op {
    pub index: usize,
    pub offset: i64,
    pub width: usize,
    pub endian: Endian,
    pub kind: Computed,
    pub cond: Option<CompiledCondition>,
}

#[derive(Debug)]
pub struct CompiledMutations {
    pub stage1: Vec<Stage1Op>,
    pub stage2: Vec<Stage2Op>,
    pub split: FieldSplit,
    /// 有条件需要读行文本
    pub needs_text: bool,
    /// 需要多少个序号计数器
    pub counters: usize,
}

impl CompiledMutations {
    pub fn is_empty(&self) -> bool {
        self.stage1.is_empty() && self.stage2.is_empty()
    }

    pub fn compile(cfg: &MutationConfig, prefix: &PrefixRule) -> Result<Self, MutateError> {
        let split = FieldSplit::from_prefix(prefix);
        let mut stage1 = Vec::new();
        let mut stage2 = Vec::new();
        let mut needs_text = false;
        let mut counters = 0usize;

        for (i, rule) in cfg.rules.iter().enumerate() {
            if !rule.enabled {
                continue;
            }
            let index = i + 1;

            let cond = match &rule.condition {
                Some(c) => {
                    let compiled = CompiledCondition::compile(c, index)
                        .map_err(|source| MutateError::Condition { index, source })?;
                    needs_text |= compiled.needs_text();
                    Some(compiled)
                }
                None => None,
            };

            match &rule.op {
                MutationOp::Insert { offset, value } => {
                    let bytes = hex_or_err(value, index)?;
                    stage1.push(Stage1Op {
                        index,
                        offset: *offset,
                        kind: Structural::Insert(bytes),
                        cond,
                    });
                }

                MutationOp::Replace { offset, value } => {
                    let bytes = hex_or_err(value, index)?;
                    stage1.push(Stage1Op {
                        index,
                        offset: *offset,
                        kind: Structural::Replace(bytes),
                        cond,
                    });
                }

                MutationOp::Delete { offset, length } => {
                    if *length == 0 {
                        return Err(MutateError::ZeroLength { index });
                    }
                    stage1.push(Stage1Op {
                        index,
                        offset: *offset,
                        kind: Structural::Delete(*length),
                        cond,
                    });
                }

                MutationOp::Sequence {
                    offset,
                    width,
                    endian,
                    start,
                    step,
                    reset_each_loop,
                } => {
                    let counter = counters;
                    counters += 1;
                    stage2.push(Stage2Op {
                        index,
                        offset: *offset,
                        width: width.bytes(),
                        endian: *endian,
                        kind: Computed::Sequence {
                            start: *start,
                            step: *step,
                            reset_each_loop: *reset_each_loop,
                            counter,
                        },
                        cond,
                    });
                }

                MutationOp::Timestamp {
                    offset,
                    width,
                    endian,
                    unit,
                    epoch,
                } => stage2.push(Stage2Op {
                    index,
                    offset: *offset,
                    width: width.bytes(),
                    endian: *endian,
                    kind: Computed::Timestamp {
                        unit: *unit,
                        epoch: *epoch,
                    },
                    cond,
                }),

                MutationOp::Length {
                    offset,
                    width,
                    endian,
                    range,
                    include_self,
                } => {
                    check_range(*range, index)?;
                    stage2.push(Stage2Op {
                        index,
                        offset: *offset,
                        width: width.bytes(),
                        endian: *endian,
                        kind: Computed::Length {
                            range: *range,
                            include_self: *include_self,
                        },
                        cond,
                    });
                }

                MutationOp::Checksum {
                    offset,
                    algorithm,
                    endian,
                    range,
                } => {
                    check_range(*range, index)?;
                    stage2.push(Stage2Op {
                        index,
                        offset: *offset,
                        width: checksum::natural_width(*algorithm),
                        endian: *endian,
                        kind: Computed::Checksum {
                            algorithm: *algorithm,
                            range: *range,
                        },
                        cond,
                    });
                }
            }
        }

        detect_static_overlap(&stage1)?;

        Ok(CompiledMutations {
            stage1,
            stage2,
            split,
            needs_text,
            counters,
        })
    }
}

fn hex_or_err(text: &str, index: usize) -> Result<Vec<u8>, MutateError> {
    let bytes = parse_hex(text).ok_or_else(|| MutateError::BadHex {
        index,
        text: text.to_string(),
    })?;
    if bytes.is_empty() {
        return Err(MutateError::EmptyValue { index });
    }
    Ok(bytes)
}

fn check_range(r: ByteRange, index: usize) -> Result<(), MutateError> {
    // end 为 0 表示「到帧尾」，与 start 无从比较
    if r.end != 0 && r.start >= 0 && r.end > 0 && r.start > r.end {
        return Err(MutateError::BadRange {
            index,
            start: r.start,
            end: r.end,
        });
    }
    Ok(())
}

/// 编译期就能判定的区间重叠。
///
/// 只有全部用非负偏移时才能在这里下结论 —— 负偏移要等到知道帧长才能定位，
/// 那种情况留给执行期检测并计数。
fn detect_static_overlap(ops: &[Stage1Op]) -> Result<(), MutateError> {
    let mut spans: Vec<(usize, usize, usize)> = Vec::new(); // (start, end, 规则序号)

    for op in ops {
        // 有条件的规则不一定每帧都生效，不能据此判定必然冲突
        if op.cond.is_some() {
            continue;
        }
        let span = op.kind.span();
        if op.offset < 0 || span == 0 {
            continue; // 负偏移无法静态定位；插入是零宽的，不构成覆盖冲突
        }
        let start = op.offset as usize;
        spans.push((start, start + span, op.index));
    }

    spans.sort_by_key(|(s, _, _)| *s);
    for w in spans.windows(2) {
        let (_, prev_end, a) = w[0];
        let (start, _, b) = w[1];
        if prev_end > start {
            return Err(MutateError::Overlap { a, b });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Condition, Delimiter, MutationRule, TextOp};

    fn prefix() -> PrefixRule {
        PrefixRule::Fields {
            delimiter: Delimiter::Whitespace,
            collapse: true,
            skip_fields: 3,
        }
    }

    fn rules(ops: Vec<MutationOp>) -> MutationConfig {
        MutationConfig {
            rules: ops
                .into_iter()
                .map(|op| MutationRule {
                    op,
                    condition: None,
                    enabled: true,
                })
                .collect(),
        }
    }

    fn compile(ops: Vec<MutationOp>) -> Result<CompiledMutations, MutateError> {
        CompiledMutations::compile(&rules(ops), &prefix())
    }

    #[test]
    fn splits_operations_into_two_stages() {
        let c = compile(vec![
            MutationOp::Insert {
                offset: 0,
                value: "AA".into(),
            },
            MutationOp::Checksum {
                offset: -2,
                algorithm: ChecksumAlgo::Crc16Ccitt,
                endian: Endian::Big,
                range: ByteRange { start: 0, end: -2 },
            },
        ])
        .unwrap();

        assert_eq!(c.stage1.len(), 1, "插入属于阶段一");
        assert_eq!(c.stage2.len(), 1, "校验和属于阶段二");
        assert!(!c.is_empty());
    }

    #[test]
    fn disabled_rules_are_dropped() {
        let cfg = MutationConfig {
            rules: vec![MutationRule {
                op: MutationOp::Insert {
                    offset: 0,
                    value: "AA".into(),
                },
                condition: None,
                enabled: false,
            }],
        };
        let c = CompiledMutations::compile(&cfg, &prefix()).unwrap();
        assert!(c.is_empty());
    }

    #[test]
    fn each_sequence_rule_gets_its_own_counter() {
        let seq = |offset| MutationOp::Sequence {
            offset,
            width: Width::W2,
            endian: Endian::Big,
            start: 0,
            step: 1,
            reset_each_loop: false,
        };
        let c = compile(vec![seq(0), seq(4)]).unwrap();
        assert_eq!(c.counters, 2);
    }

    #[test]
    fn checksum_width_follows_the_algorithm() {
        let mk = |algorithm| {
            compile(vec![MutationOp::Checksum {
                offset: 0,
                algorithm,
                endian: Endian::Big,
                range: ByteRange::default(),
            }])
            .unwrap()
            .stage2[0]
                .width
        };
        assert_eq!(mk(ChecksumAlgo::Xor8), 1);
        assert_eq!(mk(ChecksumAlgo::Crc16Modbus), 2);
        assert_eq!(mk(ChecksumAlgo::Crc32), 4);
    }

    #[test]
    fn rejects_bad_hex_and_empty_values() {
        assert!(matches!(
            compile(vec![MutationOp::Insert {
                offset: 0,
                value: "ZZ".into()
            }]),
            Err(MutateError::BadHex { index: 1, .. })
        ));
        assert!(matches!(
            compile(vec![MutationOp::Replace {
                offset: 0,
                value: "".into()
            }]),
            Err(MutateError::EmptyValue { index: 1 })
        ));
    }

    #[test]
    fn rejects_zero_length_delete() {
        assert_eq!(
            compile(vec![MutationOp::Delete {
                offset: 0,
                length: 0
            }])
            .unwrap_err(),
            MutateError::ZeroLength { index: 1 }
        );
    }

    #[test]
    fn rejects_inverted_range() {
        assert!(matches!(
            compile(vec![MutationOp::Length {
                offset: 0,
                width: Width::W2,
                endian: Endian::Big,
                range: ByteRange { start: 8, end: 4 },
                include_self: false,
            }]),
            Err(MutateError::BadRange { index: 1, .. })
        ));
    }

    #[test]
    fn detects_overlapping_structural_rules_at_compile_time() {
        // 替换 [2,5) 与删除 [4,7) 重叠
        let err = compile(vec![
            MutationOp::Replace {
                offset: 2,
                value: "01 02 03".into(),
            },
            MutationOp::Delete {
                offset: 4,
                length: 3,
            },
        ])
        .unwrap_err();
        assert_eq!(err, MutateError::Overlap { a: 1, b: 2 });
    }

    #[test]
    fn adjacent_rules_do_not_count_as_overlap() {
        // 替换 [2,4) 紧挨着删除 [4,6)，不冲突
        assert!(compile(vec![
            MutationOp::Replace {
                offset: 2,
                value: "01 02".into(),
            },
            MutationOp::Delete {
                offset: 4,
                length: 2,
            },
        ])
        .is_ok());
    }

    #[test]
    fn inserts_never_conflict_with_each_other() {
        // 插入是零宽的，同一位置插两次只是顺序问题，不是冲突
        assert!(compile(vec![
            MutationOp::Insert {
                offset: 3,
                value: "AA".into(),
            },
            MutationOp::Insert {
                offset: 3,
                value: "BB".into(),
            },
        ])
        .is_ok());
    }

    #[test]
    fn conditional_rules_are_not_judged_as_static_conflicts() {
        // 带条件的规则未必每帧都生效，不能断言它们必然冲突
        let cfg = MutationConfig {
            rules: vec![
                MutationRule {
                    op: MutationOp::Replace {
                        offset: 0,
                        value: "01 02".into(),
                    },
                    condition: Some(Condition::Field {
                        index: 0,
                        op: TextOp::Equals,
                        value: "[TX]".into(),
                    }),
                    enabled: true,
                },
                MutationRule {
                    op: MutationOp::Replace {
                        offset: 1,
                        value: "03 04".into(),
                    },
                    condition: Some(Condition::Field {
                        index: 0,
                        op: TextOp::Equals,
                        value: "[RX]".into(),
                    }),
                    enabled: true,
                },
            ],
        };
        assert!(CompiledMutations::compile(&cfg, &prefix()).is_ok());
    }

    #[test]
    fn negative_offsets_defer_overlap_check_to_runtime() {
        // 负偏移要知道帧长才能定位，编译期不下结论
        assert!(compile(vec![
            MutationOp::Replace {
                offset: -4,
                value: "01 02 03".into(),
            },
            MutationOp::Delete {
                offset: -2,
                length: 2,
            },
        ])
        .is_ok());
    }

    #[test]
    fn condition_errors_carry_the_rule_number() {
        let cfg = MutationConfig {
            rules: vec![MutationRule {
                op: MutationOp::Insert {
                    offset: 0,
                    value: "AA".into(),
                },
                condition: Some(Condition::Bytes {
                    offset: 0,
                    value: "ZZ".into(),
                    mask: None,
                }),
                enabled: true,
            }],
        };
        assert!(matches!(
            CompiledMutations::compile(&cfg, &prefix()),
            Err(MutateError::Condition { index: 1, .. })
        ));
    }
}
