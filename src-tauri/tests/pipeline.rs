//! 端到端管线测试：真实文件 → mmap → 行索引 → 编码解码 → 剥前缀 → hex 解码。
//!
//! 走的是 `preview` 命令内部完全相同的代码路径，只是绕开 Tauri 的 IPC 外壳。

use std::path::PathBuf;

use data_perf_lib::config::{Delimiter, ParseConfig, PrefixRule, TextEncoding};
use data_perf_lib::parse::{parse_line, ParseErrorKind};
use data_perf_lib::source::DataSource;

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../testdata")
        .join(name)
}

/// 与界面默认值一致：空白分隔、折叠连续空白、丢弃前 3 个字段
fn cfg(encoding: TextEncoding) -> ParseConfig {
    ParseConfig {
        encoding,
        prefix: PrefixRule::Fields {
            delimiter: Delimiter::Whitespace,
            collapse: true,
            skip_fields: 3,
        },
        hex: Default::default(),
    }
}

/// 用来断言"普通行"的样本行号。
///
/// 不用第 1 行 —— 它和第 6 行被特意加长成几百字节，是留给界面横向滚动条用的
/// 样本。解析规则的断言绑在那两行上，界面一调宽度就会连带弄坏解析测试。
const NORMAL_LINE: usize = 2;

/// 解析第 `line_no` 行（1-based），返回三段文本与解码结果
fn parse_at(
    src: &DataSource,
    cfg: &ParseConfig,
    line_no: usize,
) -> (String, String, String, Vec<u8>, Option<ParseErrorKind>) {
    let text = src.line_text(line_no - 1, cfg.encoding).expect("行存在");
    let mut bytes = Vec::new();
    let (spans, err) = parse_line(&text, cfg, &mut bytes);
    (
        text[..spans.data_start].to_string(),
        text[spans.data_start..spans.data_end].to_string(),
        text[spans.data_end..].to_string(),
        bytes,
        err,
    )
}

#[test]
fn opens_file_and_counts_lines() {
    let src = DataSource::open(&testdata("sample-utf8.txt")).unwrap();
    assert_eq!(src.line_count(), 5000);

    let info = src.info();
    assert_eq!(info.line_count, 5000);
    assert!(info.size_bytes > 0);
}

#[test]
fn normal_line_strips_prefix_and_decodes() {
    let src = DataSource::open(&testdata("sample-utf8.txt")).unwrap();
    let c = cfg(TextEncoding::Utf8);

    let (prefix, data, trailing, bytes, err) = parse_at(&src, &c, NORMAL_LINE);

    assert_eq!(prefix, "[TX] 000002 接收 ");
    assert_eq!(data, "3F 72 1F CB 19 71 17 44");
    assert_eq!(trailing, "");
    assert_eq!(err, None);
    assert_eq!(
        bytes,
        vec![0x3F, 0x72, 0x1F, 0xCB, 0x19, 0x71, 0x17, 0x44]
    );
}

#[test]
fn odd_hex_digits_flagged_but_complete_bytes_kept() {
    let src = DataSource::open(&testdata("sample-utf8.txt")).unwrap();
    let c = cfg(TextEncoding::Utf8);

    // 第 11 行：…发送 01 A5 3
    let (_, data, _, bytes, err) = parse_at(&src, &c, 11);
    assert_eq!(data, "01 A5 3");
    assert_eq!(err, Some(ParseErrorKind::OddHexDigits));
    assert_eq!(bytes, vec![0x01, 0xA5]);
}

#[test]
fn missing_data_field_reports_not_enough_fields() {
    let src = DataSource::open(&testdata("sample-utf8.txt")).unwrap();
    let c = cfg(TextEncoding::Utf8);

    // 第 21 行只有 3 个字段，第 4 个字段（数据体）不存在
    let (_, _, _, bytes, err) = parse_at(&src, &c, 21);
    assert_eq!(err, Some(ParseErrorKind::NotEnoughFields));
    assert!(bytes.is_empty());

    // 第 41 行只有 2 个字段
    let (_, _, _, _, err) = parse_at(&src, &c, 41);
    assert_eq!(err, Some(ParseErrorKind::NotEnoughFields));
}

#[test]
fn trailing_comment_is_separated_not_error() {
    let src = DataSource::open(&testdata("sample-utf8.txt")).unwrap();
    let c = cfg(TextEncoding::Utf8);

    // 第 31 行：…发送 01 A5 3F  # 尾注
    let (_, data, trailing, bytes, err) = parse_at(&src, &c, 31);
    assert_eq!(data, "01 A5 3F");
    assert_eq!(trailing, "  # 尾注");
    assert_eq!(err, None, "尾部注释是可容纳的，不该判为错误");
    assert_eq!(bytes, vec![0x01, 0xA5, 0x3F]);
}

#[test]
fn gbk_file_decodes_identically_to_utf8() {
    let utf8 = DataSource::open(&testdata("sample-utf8.txt")).unwrap();
    let gbk = DataSource::open(&testdata("sample-gbk.txt")).unwrap();

    assert_eq!(utf8.line_count(), gbk.line_count());

    let cu = cfg(TextEncoding::Utf8);
    let cg = cfg(TextEncoding::Gbk);

    // 两个文件内容相同、编码不同，按各自编码解析后必须逐字节一致
    for line in [1usize, 2, 31, 500, 5000] {
        let (pu, du, tu, bu, eu) = parse_at(&utf8, &cu, line);
        let (pg, dg, tg, bg, eg) = parse_at(&gbk, &cg, line);
        assert_eq!(bu, bg, "第 {line} 行字节不一致");
        assert_eq!((pu, du, tu, eu), (pg, dg, tg, eg), "第 {line} 行标注不一致");
    }
}

#[test]
fn gbk_file_read_as_utf8_produces_garbage_but_does_not_panic() {
    let gbk = DataSource::open(&testdata("sample-gbk.txt")).unwrap();
    let wrong = cfg(TextEncoding::Utf8);

    // 编码选错时汉字变成替换字符，但字段数不变，数据体仍能正确取出
    let (_, data, _, bytes, err) = parse_at(&gbk, &wrong, NORMAL_LINE);
    assert_eq!(err, None);
    assert_eq!(data, "3F 72 1F CB 19 71 17 44");
    assert_eq!(bytes.len(), 8);
}

#[test]
fn char_mode_offset_lands_on_character_boundary() {
    let src = DataSource::open(&testdata("sample-utf8.txt")).unwrap();
    let c = ParseConfig {
        encoding: TextEncoding::Utf8,
        // "[TX] 000002 接收 " 共 15 个字符（两个汉字各算一个）
        prefix: PrefixRule::Chars { skip_chars: 15 },
        hex: Default::default(),
    };

    let (prefix, data, _, bytes, err) = parse_at(&src, &c, NORMAL_LINE);
    assert_eq!(prefix, "[TX] 000002 接收 ");
    assert_eq!(prefix.chars().count(), 15);
    // 15 个字符对应 19 个字节 —— 按字节跳过会把汉字切碎
    assert_eq!(prefix.len(), 19);
    assert_eq!(err, None);
    assert_eq!(data, "3F 72 1F CB 19 71 17 44");
    assert_eq!(bytes.len(), 8);
}

#[test]
fn every_line_parses_without_panic() {
    let src = DataSource::open(&testdata("sample-utf8.txt")).unwrap();
    let c = cfg(TextEncoding::Utf8);
    let mut buf = Vec::new();
    let mut errors = 0usize;

    for i in 0..src.line_count() {
        let text = src.line_text(i, c.encoding).unwrap();
        let (spans, err) = parse_line(&text, &c, &mut buf);

        // 切分位置必须始终落在字符边界上，否则界面切片会 panic
        assert!(text.is_char_boundary(spans.data_start), "第 {} 行", i + 1);
        assert!(text.is_char_boundary(spans.data_end), "第 {} 行", i + 1);
        assert!(spans.data_start <= spans.data_end);
        assert!(spans.data_end <= text.len());

        if err.is_some() {
            errors += 1;
        }
    }

    // 生成数据时刻意掺了 3 条坏行（11、21、41），第 31 行的尾注不算错误
    assert_eq!(errors, 3);
}

#[test]
fn verify_unchanged_passes_for_untouched_file() {
    let src = DataSource::open(&testdata("sample-utf8.txt")).unwrap();
    assert!(src.verify_unchanged().is_ok());
}
