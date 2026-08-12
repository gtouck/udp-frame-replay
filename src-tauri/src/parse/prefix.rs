//! 前缀剥离：定位数据体在行内的起始字节偏移。

use crate::config::{Delimiter, PrefixRule};
use crate::parse::ParseErrorKind;

/// 按分隔符切分出的字段迭代器，产出 `(起始字节偏移, 字段内容)`。
///
/// 前缀剥离和筛选规则都要按字段定位，共用同一套切分语义，
/// 免得两处对「连续分隔符」「空字段」的理解出现偏差。
pub struct Fields<'a, 'd> {
    text: &'a str,
    delim: &'d Delimiter,
    collapse: bool,
    pos: usize,
    finished: bool,
}

impl<'a, 'd> Fields<'a, 'd> {
    pub fn new(text: &'a str, delim: &'d Delimiter, collapse: bool) -> Self {
        Fields {
            text,
            delim,
            collapse,
            pos: 0,
            finished: false,
        }
    }

    /// 从 `self.pos` 开始，返回下一个分隔符的字节偏移与其字节长度
    fn next_delim(&self) -> Option<(usize, usize)> {
        self.text[self.pos..]
            .char_indices()
            .find(|(_, c)| self.delim.is_delim(*c))
            .map(|(i, c)| (self.pos + i, c.len_utf8()))
    }
}

impl<'a> Iterator for Fields<'a, '_> {
    type Item = (usize, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished || self.pos > self.text.len() {
            return None;
        }

        if self.collapse {
            // 连续分隔符算一个：先吃掉分隔符，字段是极长的非分隔符段
            while self.pos < self.text.len() {
                let c = self.text[self.pos..].chars().next()?;
                if self.delim.is_delim(c) {
                    self.pos += c.len_utf8();
                } else {
                    break;
                }
            }
            if self.pos >= self.text.len() {
                self.finished = true;
                return None;
            }
            let start = self.pos;
            match self.next_delim() {
                Some((d, _)) => {
                    self.pos = d;
                    Some((start, &self.text[start..d]))
                }
                None => {
                    self.pos = self.text.len();
                    self.finished = true;
                    Some((start, &self.text[start..]))
                }
            }
        } else {
            // 不折叠：每个分隔符都结束一个字段，`a,,b` 是三个字段
            let start = self.pos;
            match self.next_delim() {
                Some((d, len)) => {
                    self.pos = d + len;
                    Some((start, &self.text[start..d]))
                }
                None => {
                    self.finished = true;
                    Some((start, &self.text[start..]))
                }
            }
        }
    }
}

/// 取第 `n` 个字段的内容（0-based）
pub fn field_at<'a>(
    text: &'a str,
    delim: &Delimiter,
    collapse: bool,
    n: usize,
) -> Option<&'a str> {
    Fields::new(text, delim, collapse).nth(n).map(|(_, s)| s)
}

/// 返回数据体起始的字节偏移。
pub fn strip(text: &str, rule: &PrefixRule) -> Result<usize, ParseErrorKind> {
    match rule {
        PrefixRule::Fields {
            delimiter,
            collapse,
            skip_fields,
        } => field_start(text, delimiter, *collapse, *skip_fields),

        PrefixRule::Chars { skip_chars } => char_offset(text, *skip_chars),
    }
}

/// 定位第 `n` 个字段（0-based）的起始字节偏移。
///
/// `collapse` 为真时连续分隔符视为一个（不产生空字段）；为假时 `a,,b` 是三个字段。
fn field_start(
    text: &str,
    delimiter: &Delimiter,
    collapse: bool,
    n: usize,
) -> Result<usize, ParseErrorKind> {
    if n == 0 {
        // 第 0 个字段单独处理：整行都是分隔符时也要返回一个合法偏移，
        // 让后续判成「没有数据」而不是「字段不够」—— 前者更贴近实情
        if collapse {
            let off = text
                .char_indices()
                .find(|(_, c)| !delimiter.is_delim(*c))
                .map(|(i, _)| i)
                .unwrap_or(text.len());
            return Ok(off);
        }
        return Ok(0);
    }

    Fields::new(text, delimiter, collapse)
        .nth(n)
        .map(|(off, _)| off)
        .ok_or(ParseErrorKind::NotEnoughFields)
}

/// 跳过 `n` 个 Unicode 字符后的字节偏移。
///
/// 按 char 计而非字节 —— 否则汉字前缀会被从中间切开。
fn char_offset(text: &str, n: usize) -> Result<usize, ParseErrorKind> {
    if n == 0 {
        return Ok(0);
    }
    let mut count = 0usize;
    for (i, _) in text.char_indices() {
        if count == n {
            return Ok(i);
        }
        count += 1;
    }
    // 恰好跳完整行也算合法（数据体为空，交给后续 EmptyData 判定）
    if count == n {
        Ok(text.len())
    } else {
        Err(ParseErrorKind::LineTooShort)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Delimiter;

    fn fields(delim: Delimiter, collapse: bool, skip: usize) -> PrefixRule {
        PrefixRule::Fields {
            delimiter: delim,
            collapse,
            skip_fields: skip,
        }
    }

    #[test]
    fn skip_zero_fields_returns_start() {
        let r = fields(Delimiter::Whitespace, true, 0);
        assert_eq!(strip("01 A5 3F", &r), Ok(0));
    }

    #[test]
    fn skip_zero_fields_collapses_leading_whitespace() {
        let r = fields(Delimiter::Whitespace, true, 0);
        assert_eq!(strip("   01 A5", &r), Ok(3));
    }

    #[test]
    fn skip_leading_fields_whitespace() {
        // [TX] 000123 发送 01 A5 3F 2B  → 丢弃前 3 个字段
        let line = "[TX] 000123 发送 01 A5 3F 2B";
        let r = fields(Delimiter::Whitespace, true, 3);
        let off = strip(line, &r).unwrap();
        assert_eq!(&line[off..], "01 A5 3F 2B");
    }

    #[test]
    fn chinese_prefix_not_split_mid_character() {
        let line = "事件 序号 接收 AA BB";
        let r = fields(Delimiter::Whitespace, true, 3);
        let off = strip(line, &r).unwrap();
        // 偏移必须落在字符边界上，否则切片会 panic
        assert!(line.is_char_boundary(off));
        assert_eq!(&line[off..], "AA BB");
    }

    #[test]
    fn collapse_treats_runs_as_one() {
        let line = "a    b   01AA";
        let r = fields(Delimiter::Whitespace, true, 2);
        let off = strip(line, &r).unwrap();
        assert_eq!(&line[off..], "01AA");
    }

    #[test]
    fn no_collapse_counts_empty_fields() {
        let line = "a,,01AA";
        let r = fields(Delimiter::Comma, false, 2);
        let off = strip(line, &r).unwrap();
        assert_eq!(&line[off..], "01AA");
    }

    #[test]
    fn collapse_skips_empty_fields() {
        let line = "a,,01AA";
        let r = fields(Delimiter::Comma, true, 1);
        let off = strip(line, &r).unwrap();
        assert_eq!(&line[off..], "01AA");
    }

    #[test]
    fn custom_delimiter_set() {
        let line = "a|b;01AA";
        let r = fields(Delimiter::Custom("|;".into()), true, 2);
        let off = strip(line, &r).unwrap();
        assert_eq!(&line[off..], "01AA");
    }

    #[test]
    fn not_enough_fields_is_error() {
        let r = fields(Delimiter::Whitespace, true, 5);
        assert_eq!(strip("a b c", &r), Err(ParseErrorKind::NotEnoughFields));
    }

    fn collect<'a>(text: &'a str, d: &Delimiter, collapse: bool) -> Vec<&'a str> {
        Fields::new(text, d, collapse).map(|(_, s)| s).collect()
    }

    #[test]
    fn fields_collapse_runs_and_ignore_edges() {
        assert_eq!(
            collect("  a   b  c ", &Delimiter::Whitespace, true),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn fields_without_collapse_keep_empty_slots() {
        assert_eq!(
            collect("a,,b", &Delimiter::Comma, false),
            vec!["a", "", "b"]
        );
        assert_eq!(
            collect(",a", &Delimiter::Comma, false),
            vec!["", "a"],
            "行首分隔符应产生一个空字段"
        );
        assert_eq!(
            collect("a,", &Delimiter::Comma, false),
            vec!["a", ""],
            "行尾分隔符应产生一个空字段"
        );
    }

    #[test]
    fn fields_offsets_land_on_char_boundaries() {
        let line = "事件 序号 AA BB";
        for (off, _) in Fields::new(line, &Delimiter::Whitespace, true) {
            assert!(line.is_char_boundary(off), "偏移 {off} 切碎了汉字");
        }
    }

    #[test]
    fn field_at_reads_the_requested_field() {
        let line = "[TX] 000123 发送 01 A5";
        let d = Delimiter::Whitespace;
        assert_eq!(field_at(line, &d, true, 0), Some("[TX]"));
        assert_eq!(field_at(line, &d, true, 2), Some("发送"));
        assert_eq!(field_at(line, &d, true, 9), None);
    }

    #[test]
    fn all_delimiter_line_reports_no_data_not_missing_fields() {
        let r = fields(Delimiter::Whitespace, true, 0);
        // 整行空白时第 0 个字段的偏移落在行尾，交由 hex 阶段判成「没有数据」
        assert_eq!(strip("    ", &r), Ok(4));
    }

    #[test]
    fn char_mode_counts_characters_not_bytes() {
        // 每个汉字 3 字节，跳 2 个字符应落在第 6 字节
        let line = "标识01AA";
        let r = PrefixRule::Chars { skip_chars: 2 };
        let off = strip(line, &r).unwrap();
        assert_eq!(off, 6);
        assert_eq!(&line[off..], "01AA");
    }

    #[test]
    fn char_mode_exact_length_is_ok() {
        let r = PrefixRule::Chars { skip_chars: 3 };
        assert_eq!(strip("abc", &r), Ok(3));
    }

    #[test]
    fn char_mode_beyond_length_is_error() {
        let r = PrefixRule::Chars { skip_chars: 10 };
        assert_eq!(strip("abc", &r), Err(ParseErrorKind::LineTooShort));
    }
}
