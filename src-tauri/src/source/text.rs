//! 文本解码。带汉字标识的数据文件在 Windows 上常见 GBK，按 UTF-8 硬解会乱码。

use std::borrow::Cow;

use crate::config::TextEncoding;

/// 按指定编码解码一行。UTF-8 且字节合法时返回借用，零拷贝。
pub fn decode(bytes: &[u8], enc: TextEncoding) -> Cow<'_, str> {
    let (cow, _had_errors) = enc
        .as_encoding()
        .decode_without_bom_handling(bytes);
    cow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_is_zero_copy() {
        let s = "事件 01AA";
        let cow = decode(s.as_bytes(), TextEncoding::Utf8);
        assert!(matches!(cow, Cow::Borrowed(_)));
        assert_eq!(cow, s);
    }

    #[test]
    fn gbk_chinese_decodes_correctly() {
        // "发送" 的 GBK 编码
        let gbk = [0xB7u8, 0xA2, 0xCB, 0xCD, b' ', b'0', b'1', b'A', b'A'];
        let cow = decode(&gbk, TextEncoding::Gbk);
        assert_eq!(cow, "发送 01AA");
    }

    #[test]
    fn gbk_bytes_read_as_utf8_do_not_panic() {
        // 同样的字节按 UTF-8 解会产生替换字符，但不能 panic
        let gbk = [0xB7u8, 0xA2, b' ', b'0', b'1'];
        let cow = decode(&gbk, TextEncoding::Utf8);
        assert!(cow.ends_with(" 01"));
    }

    #[test]
    fn ascii_is_identical_across_encodings() {
        let b = b"01 A5 3F";
        assert_eq!(decode(b, TextEncoding::Utf8), "01 A5 3F");
        assert_eq!(decode(b, TextEncoding::Gbk), "01 A5 3F");
        assert_eq!(decode(b, TextEncoding::Latin1), "01 A5 3F");
    }
}
