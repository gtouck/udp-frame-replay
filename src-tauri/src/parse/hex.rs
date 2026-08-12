//! 十六进制解码。
//!
//! 忽略空白与配置的分隔字符；遇到既非 hex 也非分隔符的字符即停止，其后视为尾部注释。
//! 这样既能容纳行尾注释，又不会静默吞掉真正的格式错误 —— 尾部被忽略的内容会在界面上标出来。

use crate::config::HexRule;
use crate::parse::ParseErrorKind;

pub struct HexScan {
    /// 数据体在输入中占用的字节数（到最后一个 hex 字符为止），其后为尾部忽略内容
    pub data_len: usize,
    pub error: Option<ParseErrorKind>,
}

#[inline]
fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 解码 `text` 中的十六进制数据，追加写入 `out`。
pub fn decode_into(text: &str, rule: &HexRule, out: &mut Vec<u8>) -> HexScan {
    let bytes = text.as_bytes();

    let mut hex_count = 0usize;
    let mut pending: u8 = 0;
    let mut last_hex_end = 0usize;

    for (i, &b) in bytes.iter().enumerate() {
        if let Some(v) = hex_val(b) {
            if hex_count % 2 == 0 {
                pending = v << 4;
            } else {
                out.push(pending | v);
            }
            hex_count += 1;
            last_hex_end = i + 1;
        } else if rule.is_ignorable(b) {
            continue;
        } else {
            // 非 hex 非分隔符：数据体到此为止，其后是尾部注释
            break;
        }
    }

    let error = if hex_count == 0 {
        Some(ParseErrorKind::EmptyData)
    } else if hex_count % 2 != 0 {
        Some(ParseErrorKind::OddHexDigits)
    } else {
        None
    };

    HexScan {
        data_len: last_hex_end,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(s: &str) -> (Vec<u8>, HexScan) {
        let mut out = Vec::new();
        let sc = decode_into(s, &HexRule::default(), &mut out);
        (out, sc)
    }

    #[test]
    fn decodes_space_separated() {
        let (bytes, sc) = scan("01 A5 3F 2B");
        assert_eq!(bytes, vec![0x01, 0xA5, 0x3F, 0x2B]);
        assert!(sc.error.is_none());
        assert_eq!(sc.data_len, 11);
    }

    #[test]
    fn decodes_contiguous() {
        let (bytes, sc) = scan("01A53F2B");
        assert_eq!(bytes, vec![0x01, 0xA5, 0x3F, 0x2B]);
        assert!(sc.error.is_none());
    }

    #[test]
    fn accepts_lowercase() {
        let (bytes, _) = scan("deadbeef");
        assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn ignores_configured_separators() {
        let (bytes, _) = scan("01:A5-3F,2B");
        assert_eq!(bytes, vec![0x01, 0xA5, 0x3F, 0x2B]);
    }

    #[test]
    fn stops_at_trailing_comment_and_reports_span() {
        let (bytes, sc) = scan("01 A5   # 这是注释");
        assert_eq!(bytes, vec![0x01, 0xA5]);
        assert!(sc.error.is_none());
        // 数据段止于最后一个 hex 字符，注释前的空格不计入
        assert_eq!(sc.data_len, 5);
    }

    #[test]
    fn odd_digit_count_is_error() {
        let (bytes, sc) = scan("01 A5 3");
        assert_eq!(sc.error, Some(ParseErrorKind::OddHexDigits));
        // 完整的字节仍然解出来了，便于界面展示
        assert_eq!(bytes, vec![0x01, 0xA5]);
    }

    #[test]
    fn empty_input_is_error() {
        let (_, sc) = scan("");
        assert_eq!(sc.error, Some(ParseErrorKind::EmptyData));
    }

    #[test]
    fn only_separators_is_empty_error() {
        let (_, sc) = scan("  :-, ");
        assert_eq!(sc.error, Some(ParseErrorKind::EmptyData));
    }

    #[test]
    fn non_hex_leading_text_yields_empty() {
        let (_, sc) = scan("hello");
        // 'h' 既非 hex 也非分隔符，立即停止，一个 hex 字符都没取到
        assert_eq!(sc.error, Some(ParseErrorKind::EmptyData));
    }

    #[test]
    fn multibyte_char_stops_scan() {
        let (bytes, sc) = scan("01A5发送");
        assert_eq!(bytes, vec![0x01, 0xA5]);
        assert_eq!(sc.data_len, 4);
        assert!(sc.error.is_none());
    }
}
