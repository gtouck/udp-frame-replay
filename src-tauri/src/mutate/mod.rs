//! 转发时的字节修改。
//!
//! 两阶段执行，这是整个模块的立身之本：
//!
//! ```text
//! 原始帧 ──阶段一(结构变更)──▶ 中间帧 ──阶段二(计算写回)──▶ 待发帧
//!         插入 / 删除 / 静态替换       长度 / 序号 / 时间戳 / 校验和
//!         偏移基于【原始帧】            偏移基于【中间帧】,按声明顺序
//! ```
//!
//! 分开的理由很实在：一旦插入或删除了字节，原报文里的长度字段和校验和就全废了。
//! 先把结构改完，再算这些派生值，改完的帧才依然是合法报文。

pub mod checksum;
pub mod compile;
pub mod stage1;
pub mod stage2;

use std::time::Instant;

pub use compile::{CompiledMutations, MutateError};

/// 一段被修改规则改动过的字节，用于界面对照着色。
///
/// 位置是**改完之后**的帧内偏移，前端直接按它上色，不做任何换算。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start: u32,
    pub len: u32,
    pub kind: SpanKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SpanKind {
    #[default]
    Insert,
    Replace,
    /// 计算值：序号 / 时间戳 / 长度 / 校验和
    Computed,
}

/// 一帧最多记录多少段改动。定长数组，发送路径上不产生任何堆分配。
pub const MAX_SPANS: usize = 12;

#[derive(Debug, Clone, Copy)]
pub struct SpanSet {
    items: [Span; MAX_SPANS],
    count: u8,
}

impl Default for SpanSet {
    fn default() -> Self {
        SpanSet {
            items: [Span::default(); MAX_SPANS],
            count: 0,
        }
    }
}

impl SpanSet {
    pub fn clear(&mut self) {
        self.count = 0;
    }

    pub fn push(&mut self, start: usize, len: usize, kind: SpanKind) {
        if (self.count as usize) < MAX_SPANS && len > 0 {
            self.items[self.count as usize] = Span {
                start: start as u32,
                len: len as u32,
                kind,
            };
            self.count += 1;
        }
    }

    pub fn as_slice(&self) -> &[Span] {
        &self.items[..self.count as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// 一帧修改过程中遇到的问题计数
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MutStats {
    /// 偏移或范围落在帧外，该条规则被跳过
    pub out_of_range: u32,
    /// 两条结构性规则改到了同一段字节（只可能是负偏移，编译期拦不住）
    pub overlaps: u32,
}

impl MutStats {
    pub fn is_clean(&self) -> bool {
        self.out_of_range == 0 && self.overlaps == 0
    }

    pub fn merge(&mut self, other: MutStats) {
        self.out_of_range += other.out_of_range;
        self.overlaps += other.overlaps;
    }
}

/// 序号计数器的初值与循环行为
struct CounterSpec {
    index: usize,
    start: u64,
    reset_each_loop: bool,
}

pub struct Mutator {
    compiled: CompiledMutations,
    counters: Vec<u64>,
    specs: Vec<CounterSpec>,
    started: Instant,
    edits: Vec<stage1::Edit>,
    last_loop: u32,
}

impl Mutator {
    pub fn new(compiled: CompiledMutations) -> Self {
        let mut counters = vec![0u64; compiled.counters];
        let mut specs = Vec::new();

        for op in &compiled.stage2 {
            if let compile::Computed::Sequence {
                start,
                reset_each_loop,
                counter,
                ..
            } = &op.kind
            {
                counters[*counter] = *start;
                specs.push(CounterSpec {
                    index: *counter,
                    start: *start,
                    reset_each_loop: *reset_each_loop,
                });
            }
        }

        Mutator {
            compiled,
            counters,
            specs,
            started: Instant::now(),
            edits: stage1::new_scratch(),
            last_loop: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.compiled.is_empty()
    }

    /// 是否有条件需要读行文本
    pub fn needs_line_text(&self) -> bool {
        self.compiled.needs_text
    }

    /// 把 `src` 按规则变换后写入 `dst`（会先清空）。
    ///
    /// `loop_index` 是当前第几轮循环，用于处理「循环时重置序号」。
    pub fn apply(
        &mut self,
        line_text: &str,
        src: &[u8],
        dst: &mut Vec<u8>,
        spans: &mut SpanSet,
        loop_index: u32,
    ) -> MutStats {
        spans.clear();
        if loop_index != self.last_loop {
            self.last_loop = loop_index;
            for s in &self.specs {
                if s.reset_each_loop {
                    self.counters[s.index] = s.start;
                }
            }
        }

        let mut stats = stage1::apply(
            src,
            &self.compiled.stage1,
            line_text,
            &self.compiled.split,
            dst,
            &mut self.edits,
            spans,
        );

        if !self.compiled.stage2.is_empty() {
            let mut ctx = stage2::Ctx {
                counters: &mut self.counters,
                started: self.started,
            };
            stats.merge(stage2::apply(
                dst,
                &self.compiled.stage2,
                line_text,
                &self.compiled.split,
                &mut ctx,
                spans,
            ));
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ByteRange, ChecksumAlgo, Delimiter, Endian, MutationConfig, MutationOp, MutationRule,
        PrefixRule, Width,
    };

    fn prefix() -> PrefixRule {
        PrefixRule::Fields {
            delimiter: Delimiter::Whitespace,
            collapse: true,
            skip_fields: 3,
        }
    }

    fn mutator(ops: Vec<MutationOp>) -> Mutator {
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
        Mutator::new(CompiledMutations::compile(&cfg, &prefix()).unwrap())
    }

    /// 这是整个模块存在的理由：插入字节之后，长度和校验和仍然对得上。
    #[test]
    fn length_and_checksum_survive_an_insertion() {
        // 原始帧：4 字节负载 + 2 字节校验和占位
        let src = vec![0xAA, 0xBB, 0xCC, 0xDD, 0x00, 0x00];

        let mut m = mutator(vec![
            // 阶段一：在最前面插入 2 字节头
            MutationOp::Insert {
                offset: 0,
                value: "5A A5".into(),
            },
            // 阶段二：写长度（不含头与校验和），再算校验和
            MutationOp::Length {
                offset: 2,
                width: Width::W1,
                endian: Endian::Big,
                range: ByteRange { start: 3, end: -2 },
                include_self: false,
            },
            MutationOp::Checksum {
                offset: -2,
                algorithm: ChecksumAlgo::Crc16Ccitt,
                endian: Endian::Big,
                range: ByteRange { start: 0, end: -2 },
            },
        ]);

        let mut out = Vec::new();
        let stats = m.apply("", &src, &mut out, &mut SpanSet::default(), 0);
        assert!(stats.is_clean(), "不该有越界或冲突：{stats:?}");

        // 头 2 字节 + 原 6 字节 = 8
        assert_eq!(out.len(), 8);
        assert_eq!(&out[0..2], &[0x5A, 0xA5], "插入的头");

        // 长度字段写在 offset 2，统计 [3, len-2) = [3,6) 共 3 字节
        assert_eq!(out[2], 3);

        // 校验和覆盖 [0, len-2)，且必须与手工计算一致
        let want = checksum::compute(ChecksumAlgo::Crc16Ccitt, &out[0..6]);
        assert_eq!(&out[6..8], &(want as u16).to_be_bytes());
    }

    #[test]
    fn checksum_is_computed_after_the_length_field_is_written() {
        // 长度字段落在校验范围内。若顺序颠倒，校验和算的是长度写入前的旧值。
        let src = vec![0x00, 0x11, 0x22, 0x33, 0x00, 0x00];
        let mut m = mutator(vec![
            MutationOp::Length {
                offset: 0,
                width: Width::W1,
                endian: Endian::Big,
                range: ByteRange { start: 1, end: -2 },
                include_self: false,
            },
            MutationOp::Checksum {
                offset: -2,
                algorithm: ChecksumAlgo::Sum8,
                endian: Endian::Big,
                range: ByteRange { start: 0, end: -2 },
            },
        ]);

        let mut out = Vec::new();
        m.apply("", &src, &mut out, &mut SpanSet::default(), 0);

        // 长度 = [1,4) = 3
        assert_eq!(out[0], 3);
        // 校验和应当基于已经写好长度的前 4 字节
        let want = checksum::compute(ChecksumAlgo::Sum8, &[3, 0x11, 0x22, 0x33]);
        assert_eq!(out[4], want as u8);
    }

    #[test]
    fn deletion_also_keeps_length_correct() {
        let src = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x00];
        let mut m = mutator(vec![
            MutationOp::Delete {
                offset: 1,
                length: 2,
            },
            MutationOp::Length {
                offset: 0,
                width: Width::W1,
                endian: Endian::Big,
                range: ByteRange { start: 1, end: 0 },
                include_self: false,
            },
        ]);

        let mut out = Vec::new();
        m.apply("", &src, &mut out, &mut SpanSet::default(), 0);
        assert_eq!(out.len(), 4, "删掉两字节");
        assert_eq!(out[0], 3, "长度应当是删除之后的 [1,4)");
    }

    #[test]
    fn empty_mutator_copies_frame_through() {
        let mut m = mutator(vec![]);
        assert!(m.is_empty());
        let src = vec![1, 2, 3];
        let mut out = Vec::new();
        let stats = m.apply("", &src, &mut out, &mut SpanSet::default(), 0);
        assert_eq!(out, src);
        assert!(stats.is_clean());
    }

    #[test]
    fn sequence_advances_across_frames() {
        let mut m = mutator(vec![MutationOp::Sequence {
            offset: 0,
            width: Width::W2,
            endian: Endian::Big,
            start: 1,
            step: 1,
            reset_each_loop: false,
        }]);

        let src = vec![0u8; 4];
        let mut seen = Vec::new();
        for _ in 0..3 {
            let mut out = Vec::new();
            m.apply("", &src, &mut out, &mut SpanSet::default(), 0);
            seen.push(u16::from_be_bytes([out[0], out[1]]));
        }
        assert_eq!(seen, vec![1, 2, 3]);
    }

    #[test]
    fn sequence_resets_on_new_loop_when_configured() {
        let mut m = mutator(vec![MutationOp::Sequence {
            offset: 0,
            width: Width::W2,
            endian: Endian::Big,
            start: 10,
            step: 1,
            reset_each_loop: true,
        }]);

        let src = vec![0u8; 4];
        let read = |m: &mut Mutator, loop_idx: u32| {
            let mut out = Vec::new();
            m.apply("", &src, &mut out, &mut SpanSet::default(), loop_idx);
            u16::from_be_bytes([out[0], out[1]])
        };

        assert_eq!(read(&mut m, 0), 10);
        assert_eq!(read(&mut m, 0), 11);
        assert_eq!(read(&mut m, 1), 10, "新一轮循环应当归零");
        assert_eq!(read(&mut m, 1), 11);
    }

    #[test]
    fn sequence_continues_across_loops_by_default() {
        let mut m = mutator(vec![MutationOp::Sequence {
            offset: 0,
            width: Width::W2,
            endian: Endian::Big,
            start: 10,
            step: 1,
            reset_each_loop: false,
        }]);

        let src = vec![0u8; 4];
        let read = |m: &mut Mutator, loop_idx: u32| {
            let mut out = Vec::new();
            m.apply("", &src, &mut out, &mut SpanSet::default(), loop_idx);
            u16::from_be_bytes([out[0], out[1]])
        };

        assert_eq!(read(&mut m, 0), 10);
        assert_eq!(read(&mut m, 1), 11, "默认应当连续递增");
    }

    #[test]
    fn replace_then_checksum_reflects_the_replacement() {
        let src = vec![0x01, 0x02, 0x03, 0x00];
        let mut m = mutator(vec![
            MutationOp::Replace {
                offset: 1,
                value: "FF".into(),
            },
            MutationOp::Checksum {
                offset: 3,
                algorithm: ChecksumAlgo::Sum8,
                endian: Endian::Big,
                range: ByteRange { start: 0, end: 3 },
            },
        ]);

        let mut out = Vec::new();
        m.apply("", &src, &mut out, &mut SpanSet::default(), 0);
        assert_eq!(out[1], 0xFF);
        assert_eq!(out[3], 0x01u8.wrapping_add(0xFF).wrapping_add(0x03));
    }
}
