//! 解析规则自动推测：拿真实数据文件验，不用构造出来的理想样本。
//!
//! 这里断言的是「新人打开文件的第一眼」—— 推错了他看到的就是满屏红色，
//! 会以为软件读不了自己的文件。

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use data_perf_lib::config::{Delimiter, ParseConfig, PrefixRule, TextEncoding};
use data_perf_lib::guess::guess_parse;
use data_perf_lib::source::DataSource;

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../testdata")
        .join(name)
}

/// 每个测试用独立文件名，避免并行跑测试时互相踩
static SEQ: AtomicU32 = AtomicU32::new(0);

/// 把内容写进临时文件再按 `DataSource` 打开 —— 走的是和真实使用一样的路径
fn source_of(bytes: &[u8]) -> DataSource {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "data-perf-guess-{}-{}.txt",
        std::process::id(),
        n
    ));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(bytes).unwrap();
    f.sync_all().unwrap();
    DataSource::open(&path).unwrap()
}

fn fields_of(cfg: &ParseConfig) -> (Delimiter, usize) {
    match &cfg.prefix {
        PrefixRule::Fields {
            delimiter,
            skip_fields,
            ..
        } => (delimiter.clone(), *skip_fields),
        PrefixRule::Chars { .. } => panic!("推测结果应当是字段模式"),
    }
}

#[test]
fn guesses_prefix_and_encoding_for_utf8_sample() {
    let src = DataSource::open(&testdata("sample-utf8.txt")).unwrap();
    let g = guess_parse(&src, &ParseConfig::default()).expect("应当推得出规则");

    assert_eq!(g.config.encoding, TextEncoding::Utf8);
    assert_eq!(fields_of(&g.config), (Delimiter::Whitespace, 3));
}

/// 同样的内容换成 GBK，必须自己认出编码 —— 带汉字的日志在 Windows 上就是这样
#[test]
fn guesses_gbk_encoding_for_gbk_sample() {
    let src = DataSource::open(&testdata("sample-gbk.txt")).unwrap();
    let g = guess_parse(&src, &ParseConfig::default()).expect("应当推得出规则");

    assert_eq!(g.config.encoding, TextEncoding::Gbk);
    assert_eq!(fields_of(&g.config), (Delimiter::Whitespace, 3));
}

/// 回归：别被「丢得少反而行行成功」骗过去。
///
/// `[TX] 000021 发送` 这类只有三个字段的行，正确的「丢 3 个」必然报错，
/// 而「丢 1 个」会把序号 `000021` 当成三个字节收下。按成功行数排名会选错，
/// 按字节数排名才对。
#[test]
fn hex_looking_counter_prefix_does_not_win() {
    let mut text = String::new();
    for i in 1..=40 {
        if i % 10 == 0 {
            // 掺进没有数据体的行，正确规则在这些行上一定失败
            text.push_str(&format!("[TX] {i:06} 发送\n"));
        } else {
            text.push_str(&format!("[TX] {i:06} 发送 01 A5 3F 2B 5A\n"));
        }
    }
    let src = source_of(text.as_bytes());

    let g = guess_parse(&src, &ParseConfig::default()).unwrap();
    assert_eq!(fields_of(&g.config), (Delimiter::Whitespace, 3));
}

/// 带时间戳的 CSV。
///
/// 忽略字符里有 `:` 和 `-`，所以「整行都是数据」会把 `20240501` 当成四个字节
/// 收下 —— 和正确切法解出的字节数正好打平。平局要落在剥掉更多前缀的那一边。
#[test]
fn guesses_comma_delimiter() {
    let mut text = String::new();
    for i in 1..=40 {
        text.push_str(&format!("2024-05-01T00:00:{i:02},CH1,01 A5 3F 2B\n"));
    }
    let src = source_of(text.as_bytes());

    let g = guess_parse(&src, &ParseConfig::default()).unwrap();
    assert_eq!(fields_of(&g.config), (Delimiter::Comma, 2));
}

/// 没有前缀的裸十六进制文件不该被凭空剥掉一层
#[test]
fn bare_hex_file_keeps_whole_line() {
    let text = "01 A5 3F 2B\n5A A5 01 02\nDE AD BE EF\n".repeat(10);
    let src = source_of(text.as_bytes());

    let g = guess_parse(&src, &ParseConfig::default()).unwrap();
    assert_eq!(fields_of(&g.config).1, 0, "整行都是数据");
}

/// 压根不是十六进制的文件：老实返回 None，让界面保持原配置不动
#[test]
fn gives_up_on_non_hex_file() {
    let text = "这是一份说明文档\n完全没有十六进制数据\n只有中文句子\n".repeat(10);
    let src = source_of(text.as_bytes());

    assert!(guess_parse(&src, &ParseConfig::default()).is_none());
}
