//! 前缀剥离：定位数据体在行内的起始字节偏移。

use crate::config::PrefixRule;
use crate::parse::ParseErrorKind;

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
    delimiter: &crate::config::Delimiter,
    collapse: bool,
    n: usize,
) -> Result<usize, ParseErrorKind> {
    if n == 0 {
        // 第 0 个字段：collapse 模式下要跳过行首的分隔符
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

    let mut field_idx = 0usize;
    let mut in_field = false;
    let mut chars = text.char_indices().peekable();

    // collapse 模式下先跳过行首分隔符，避免把它算成一个空字段
    if collapse {
        while let Some((_, c)) = chars.peek() {
            if delimiter.is_delim(*c) {
                chars.next();
            } else {
                break;
            }
        }
    }

    while let Some((i, c)) = chars.next() {
        if delimiter.is_delim(c) {
            if in_field || !collapse {
                // 一个字段到此结束
                field_idx += 1;
                in_field = false;

                if collapse {
                    // 吞掉后续连续分隔符
                    while let Some((_, nc)) = chars.peek() {
                        if delimiter.is_delim(*nc) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }

                if field_idx == n {
                    // 下一个字符即为目标字段起点
                    return Ok(chars.peek().map(|(j, _)| *j).unwrap_or(text.len()));
                }
            }
            let _ = i;
        } else {
            in_field = true;
        }
    }

    Err(ParseErrorKind::NotEnoughFields)
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
