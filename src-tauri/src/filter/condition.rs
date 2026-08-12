//! 条件的编译与求值。

use thiserror::Error;

use crate::config::{Condition, Delimiter, FilterConfig, PrefixRule, TextOp};
use crate::parse::prefix::field_at;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FilterError {
    #[error("第 {index} 条规则的字节值 “{text}” 不是合法的十六进制")]
    BadHex { index: usize, text: String },

    #[error("第 {index} 条规则的字节值不能为空")]
    EmptyValue { index: usize },

    #[error("第 {index} 条规则的掩码有 {mask} 字节，与字节值的 {value} 字节对不上")]
    MaskLength {
        index: usize,
        value: usize,
        mask: usize,
    },
}

enum Compiled {
    Field {
        index: usize,
        op: TextOp,
        value: String,
    },
    Bytes {
        offset: i64,
        expect: Vec<u8>,
        mask: Option<Vec<u8>>,
    },
}

struct Rule {
    cond: Compiled,
    negate: bool,
}

pub struct CompiledFilter {
    rules: Vec<Rule>,
    delimiter: Delimiter,
    collapse: bool,
    /// 是否有规则需要按字段切分整行。没有的话就完全不碰行文本。
    needs_fields: bool,
}

impl std::fmt::Debug for CompiledFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledFilter")
            .field("rules", &self.rules.len())
            .field("needs_fields", &self.needs_fields)
            .finish()
    }
}

/// 解析十六进制文本，忽略空白与常见分隔符
fn parse_hex(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut hi: Option<u8> = None;

    for b in text.bytes() {
        let v = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ if b.is_ascii_whitespace() || b":-,".contains(&b) => continue,
            _ => return None,
        };
        match hi {
            None => hi = Some(v << 4),
            Some(h) => {
                out.push(h | v);
                hi = None;
            }
        }
    }

    // 落单的半个字节说明写漏了一位
    if hi.is_some() {
        return None;
    }
    Some(out)
}

impl CompiledFilter {
    /// 按解析配置里的分隔符编译规则 —— 筛选用的字段切分必须和前缀剥离一致，
    /// 否则界面上数出来的字段序号在筛选里会对不上。
    pub fn compile(cfg: &FilterConfig, prefix: &PrefixRule) -> Result<Self, FilterError> {
        let (delimiter, collapse) = match prefix {
            PrefixRule::Fields {
                delimiter,
                collapse,
                ..
            } => (delimiter.clone(), *collapse),
            // 字符模式下没有分隔符概念，字段筛选退回按空白切
            PrefixRule::Chars { .. } => (Delimiter::Whitespace, true),
        };

        let mut rules = Vec::new();
        let mut needs_fields = false;

        for (index, r) in cfg.rules.iter().enumerate() {
            if !r.enabled {
                continue;
            }

            let cond = match &r.condition {
                Condition::Field { index: i, op, value } => {
                    needs_fields = true;
                    Compiled::Field {
                        index: *i,
                        op: *op,
                        value: value.clone(),
                    }
                }

                Condition::Bytes {
                    offset,
                    value,
                    mask,
                } => {
                    let expect = parse_hex(value).ok_or_else(|| FilterError::BadHex {
                        index: index + 1,
                        text: value.clone(),
                    })?;
                    if expect.is_empty() {
                        return Err(FilterError::EmptyValue { index: index + 1 });
                    }

                    let mask = match mask {
                        Some(m) if !m.trim().is_empty() => {
                            let bytes = parse_hex(m).ok_or_else(|| FilterError::BadHex {
                                index: index + 1,
                                text: m.clone(),
                            })?;
                            if bytes.len() != expect.len() {
                                return Err(FilterError::MaskLength {
                                    index: index + 1,
                                    value: expect.len(),
                                    mask: bytes.len(),
                                });
                            }
                            Some(bytes)
                        }
                        _ => None,
                    };

                    Compiled::Bytes {
                        offset: *offset,
                        expect,
                        mask,
                    }
                }
            };

            rules.push(Rule {
                cond,
                negate: r.negate,
            });
        }

        Ok(CompiledFilter {
            rules,
            delimiter,
            collapse,
            needs_fields,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// 是否有规则要读行文本。没有就不必为筛选去解码整行。
    pub fn needs_line_text(&self) -> bool {
        self.needs_fields
    }

    /// 全部规则都满足才放行。
    pub fn accepts(&self, line_text: &str, data: &[u8]) -> bool {
        for r in &self.rules {
            let hit = match &r.cond {
                Compiled::Field { index, op, value } => {
                    match field_at(line_text, &self.delimiter, self.collapse, *index) {
                        Some(f) => match op {
                            TextOp::Equals => f == value,
                            TextOp::Contains => f.contains(value.as_str()),
                        },
                        // 字段不存在就是不匹配
                        None => false,
                    }
                }

                Compiled::Bytes {
                    offset,
                    expect,
                    mask,
                } => match resolve(*offset, data.len(), expect.len()) {
                    Some(start) => {
                        let slice = &data[start..start + expect.len()];
                        match mask {
                            None => slice == expect.as_slice(),
                            Some(m) => slice
                                .iter()
                                .zip(expect)
                                .zip(m)
                                .all(|((got, want), mk)| got & mk == want & mk),
                        }
                    }
                    // 帧太短，够不到要看的位置
                    None => false,
                },
            };

            // negate 为真时，条件成立反而应当排除
            if hit == r.negate {
                return false;
            }
        }
        true
    }
}

/// 把可能为负的偏移换算成实际下标。越界返回 `None`。
fn resolve(offset: i64, data_len: usize, need: usize) -> Option<usize> {
    let start = if offset >= 0 {
        offset as usize
    } else {
        let back = offset.unsigned_abs() as usize;
        if back > data_len {
            return None;
        }
        data_len - back
    };
    if start.checked_add(need)? > data_len {
        return None;
    }
    Some(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FilterRule;

    fn prefix() -> PrefixRule {
        PrefixRule::Fields {
            delimiter: Delimiter::Whitespace,
            collapse: true,
            skip_fields: 3,
        }
    }

    fn build(conds: Vec<(Condition, bool)>) -> CompiledFilter {
        let cfg = FilterConfig {
            rules: conds
                .into_iter()
                .map(|(condition, negate)| FilterRule {
                    condition,
                    negate,
                    enabled: true,
                })
                .collect(),
        };
        CompiledFilter::compile(&cfg, &prefix()).unwrap()
    }

    fn bytes(offset: i64, value: &str, mask: Option<&str>) -> Condition {
        Condition::Bytes {
            offset,
            value: value.into(),
            mask: mask.map(|s| s.into()),
        }
    }

    fn field(index: usize, op: TextOp, value: &str) -> Condition {
        Condition::Field {
            index,
            op,
            value: value.into(),
        }
    }

    const LINE: &str = "[TX] 000123 发送 01 A5 3F 2B";
    const DATA: &[u8] = &[0x01, 0xA5, 0x3F, 0x2B];

    #[test]
    fn empty_filter_accepts_everything() {
        let f = build(vec![]);
        assert!(f.is_empty());
        assert!(f.accepts(LINE, DATA));
        assert!(f.accepts("", &[]));
    }

    #[test]
    fn byte_at_offset_matches() {
        assert!(build(vec![(bytes(0, "01", None), false)]).accepts(LINE, DATA));
        assert!(!build(vec![(bytes(0, "02", None), false)]).accepts(LINE, DATA));
        assert!(build(vec![(bytes(1, "A5", None), false)]).accepts(LINE, DATA));
    }

    #[test]
    fn multi_byte_sequence_matches() {
        assert!(build(vec![(bytes(0, "01 A5", None), false)]).accepts(LINE, DATA));
        assert!(build(vec![(bytes(2, "3F2B", None), false)]).accepts(LINE, DATA));
        assert!(!build(vec![(bytes(0, "01 A6", None), false)]).accepts(LINE, DATA));
    }

    #[test]
    fn negative_offset_counts_from_the_end() {
        // -2 配两字节即匹配最后两字节
        assert!(build(vec![(bytes(-2, "3F 2B", None), false)]).accepts(LINE, DATA));
        assert!(build(vec![(bytes(-1, "2B", None), false)]).accepts(LINE, DATA));
        assert!(!build(vec![(bytes(-1, "3F", None), false)]).accepts(LINE, DATA));
    }

    #[test]
    fn mask_compares_only_selected_bits() {
        // A5 = 1010_0101，高四位为 A
        assert!(build(vec![(bytes(1, "A0", Some("F0")), false)]).accepts(LINE, DATA));
        assert!(build(vec![(bytes(1, "05", Some("0F")), false)]).accepts(LINE, DATA));
        assert!(!build(vec![(bytes(1, "B0", Some("F0")), false)]).accepts(LINE, DATA));
    }

    #[test]
    fn out_of_range_offset_does_not_match() {
        assert!(!build(vec![(bytes(10, "01", None), false)]).accepts(LINE, DATA));
        assert!(!build(vec![(bytes(-99, "01", None), false)]).accepts(LINE, DATA));
        // 偏移合法但序列超出帧尾
        assert!(!build(vec![(bytes(3, "2B 00", None), false)]).accepts(LINE, DATA));
    }

    #[test]
    fn field_equals_and_contains() {
        assert!(build(vec![(field(0, TextOp::Equals, "[TX]"), false)]).accepts(LINE, DATA));
        assert!(!build(vec![(field(0, TextOp::Equals, "[RX]"), false)]).accepts(LINE, DATA));
        assert!(build(vec![(field(2, TextOp::Equals, "发送"), false)]).accepts(LINE, DATA));
        assert!(build(vec![(field(1, TextOp::Contains, "0123"), false)]).accepts(LINE, DATA));
    }

    #[test]
    fn missing_field_does_not_match() {
        assert!(!build(vec![(field(99, TextOp::Equals, "x"), false)]).accepts(LINE, DATA));
    }

    #[test]
    fn negate_inverts_a_rule() {
        assert!(!build(vec![(field(0, TextOp::Equals, "[TX]"), true)]).accepts(LINE, DATA));
        assert!(build(vec![(field(0, TextOp::Equals, "[RX]"), true)]).accepts(LINE, DATA));
    }

    #[test]
    fn rules_combine_with_and() {
        // 两条都成立才放行
        let f = build(vec![
            (field(0, TextOp::Equals, "[TX]"), false),
            (bytes(0, "01", None), false),
        ]);
        assert!(f.accepts(LINE, DATA));

        let f = build(vec![
            (field(0, TextOp::Equals, "[TX]"), false),
            (bytes(0, "99", None), false),
        ]);
        assert!(!f.accepts(LINE, DATA), "有一条不满足就该被排除");
    }

    #[test]
    fn disabled_rules_are_skipped() {
        let cfg = FilterConfig {
            rules: vec![FilterRule {
                condition: bytes(0, "99", None),
                negate: false,
                enabled: false,
            }],
        };
        let f = CompiledFilter::compile(&cfg, &prefix()).unwrap();
        assert!(f.is_empty());
        assert!(f.accepts(LINE, DATA));
    }

    #[test]
    fn needs_line_text_only_when_a_field_rule_exists() {
        assert!(!build(vec![(bytes(0, "01", None), false)]).needs_line_text());
        assert!(build(vec![(field(0, TextOp::Equals, "x"), false)]).needs_line_text());
    }

    #[test]
    fn compile_rejects_bad_hex() {
        let cfg = FilterConfig {
            rules: vec![FilterRule {
                condition: bytes(0, "ZZ", None),
                negate: false,
                enabled: true,
            }],
        };
        assert!(matches!(
            CompiledFilter::compile(&cfg, &prefix()),
            Err(FilterError::BadHex { index: 1, .. })
        ));
    }

    #[test]
    fn compile_rejects_odd_hex_digits() {
        let cfg = FilterConfig {
            rules: vec![FilterRule {
                condition: bytes(0, "A5F", None),
                negate: false,
                enabled: true,
            }],
        };
        assert!(matches!(
            CompiledFilter::compile(&cfg, &prefix()),
            Err(FilterError::BadHex { .. })
        ));
    }

    #[test]
    fn compile_rejects_mask_length_mismatch() {
        let cfg = FilterConfig {
            rules: vec![FilterRule {
                condition: bytes(0, "01 A5", Some("FF")),
                negate: false,
                enabled: true,
            }],
        };
        assert_eq!(
            CompiledFilter::compile(&cfg, &prefix()).unwrap_err(),
            FilterError::MaskLength {
                index: 1,
                value: 2,
                mask: 1,
            }
        );
    }

    #[test]
    fn compile_rejects_empty_value() {
        let cfg = FilterConfig {
            rules: vec![FilterRule {
                condition: bytes(0, "  ", None),
                negate: false,
                enabled: true,
            }],
        };
        assert_eq!(
            CompiledFilter::compile(&cfg, &prefix()).unwrap_err(),
            FilterError::EmptyValue { index: 1 }
        );
    }
}
