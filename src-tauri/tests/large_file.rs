//! 大文件测试。
//!
//! 生成一个接近 1GB 的合成文件，验证行索引的构建耗时与内存占用符合设计预期。
//!
//! 默认不跑（要写将近 1GB 到磁盘、耗时数十秒）。手动执行：
//!
//! ```bash
//! cargo test --test large_file -- --ignored --nocapture
//! ```

use std::io::{BufWriter, Write};
use std::time::Instant;

use data_perf_lib::config::{Delimiter, ParseConfig, PrefixRule, TextEncoding};
use data_perf_lib::parse::parse_line;
use data_perf_lib::source::DataSource;

/// 目标文件大小，接近使用者说的上限
const TARGET_BYTES: u64 = 1_000_000_000;

/// 每行 256 字节起步，与真实数据的量级一致
const PAYLOAD_BYTES: usize = 96;

fn synth_path() -> std::path::PathBuf {
    std::env::temp_dir().join("data-perf-1gb.big.txt")
}

/// 生成合成文件。已存在且大小合适就直接复用，免得每次都写一遍。
fn ensure_file() -> std::path::PathBuf {
    let path = synth_path();
    if let Ok(m) = std::fs::metadata(&path) {
        if m.len() > TARGET_BYTES - 10_000_000 {
            return path;
        }
    }

    eprintln!("正在生成约 1GB 的合成文件，请稍候…");
    let started = Instant::now();
    let f = std::fs::File::create(&path).unwrap();
    let mut w = BufWriter::with_capacity(1 << 20, f);

    let mut line = String::with_capacity(320);
    let mut written = 0u64;
    let mut n: u64 = 0;

    while written < TARGET_BYTES {
        n += 1;
        line.clear();
        line.push_str(if n % 2 == 0 { "[TX] " } else { "[RX] " });
        line.push_str(&format!("{n:09} "));
        line.push_str(if n % 2 == 0 { "发送 " } else { "接收 " });
        for i in 0..PAYLOAD_BYTES {
            line.push_str(&format!("{:02X} ", (n as usize + i) as u8));
        }
        line.push('\n');
        w.write_all(line.as_bytes()).unwrap();
        written += line.len() as u64;
    }
    w.flush().unwrap();

    eprintln!(
        "生成完毕：{} 行 · {:.2} GB · 耗时 {:.1}s",
        n,
        written as f64 / 1e9,
        started.elapsed().as_secs_f64()
    );
    path
}

#[test]
#[ignore = "要写将近 1GB 到磁盘，手动执行：cargo test --test large_file -- --ignored"]
fn one_gigabyte_file_indexes_quickly_and_cheaply() {
    let path = ensure_file();
    let size = std::fs::metadata(&path).unwrap().len();

    let started = Instant::now();
    let src = DataSource::open(&path).unwrap();
    let elapsed = started.elapsed();

    let info = src.info();
    eprintln!(
        "映射 + 建索引：{:.0}ms · {} 行 · 索引 {:.1} MB",
        elapsed.as_secs_f64() * 1000.0,
        info.line_count,
        info.index_memory_bytes as f64 / 1e6
    );

    assert_eq!(info.size_bytes, size);
    assert!(info.line_count > 1_000_000, "1GB 文件应当有百万行以上");

    // 设计里说 1GB 约 0.3 秒。给足余量，慢一个数量级就说明实现退化了。
    assert!(
        elapsed.as_secs_f64() < 5.0,
        "建索引耗时 {:.1}s，远超预期",
        elapsed.as_secs_f64()
    );

    // 每行一个 u32。索引内存必须与行数成正比，而不是与文件大小成正比。
    let expected = info.line_count * 4;
    assert!(
        info.index_memory_bytes < expected * 2,
        "索引占用 {} 字节，超出 {} 行 × 4 字节的合理范围",
        info.index_memory_bytes,
        info.line_count
    );
}

#[test]
#[ignore = "依赖上一个用例生成的大文件"]
fn random_access_into_a_large_file_is_constant_time() {
    let path = ensure_file();
    let src = DataSource::open(&path).unwrap();
    let count = src.line_count();

    let cfg = ParseConfig {
        encoding: TextEncoding::Utf8,
        prefix: PrefixRule::Fields {
            delimiter: Delimiter::Whitespace,
            collapse: true,
            skip_fields: 3,
        },
        hex: Default::default(),
    };

    // 跳到文件各处取行，耗时不该随位置变化
    let probes = [0, count / 4, count / 2, count * 3 / 4, count - 1];
    let mut buf = Vec::new();

    for &i in &probes {
        let started = Instant::now();
        let text = src.line_text(i, cfg.encoding).expect("行应当存在");
        let (spans, err) = parse_line(&text, &cfg, &mut buf);
        let took = started.elapsed();

        assert_eq!(err, None, "第 {} 行解析失败", i + 1);
        assert_eq!(buf.len(), 96, "第 {} 行应当解出 96 字节", i + 1);
        assert!(text.is_char_boundary(spans.data_start));
        assert!(
            took.as_micros() < 5_000,
            "第 {} 行取用耗时 {took:?}，不像 O(1)",
            i + 1
        );
    }
}

#[test]
#[ignore = "依赖上一个用例生成的大文件"]
fn full_scan_throughput_is_far_above_any_send_rate() {
    let path = ensure_file();
    let src = DataSource::open(&path).unwrap();

    let cfg = ParseConfig {
        encoding: TextEncoding::Utf8,
        prefix: PrefixRule::Fields {
            delimiter: Delimiter::Whitespace,
            collapse: true,
            skip_fields: 3,
        },
        hex: Default::default(),
    };

    // 只扫前 20 万行，够算出速率了
    let n = src.line_count().min(200_000);
    let mut buf = Vec::new();
    let started = Instant::now();

    for i in 0..n {
        let text = src.line_text(i, cfg.encoding).unwrap();
        parse_line(&text, &cfg, &mut buf);
    }

    let rate = n as f64 / started.elapsed().as_secs_f64();
    eprintln!("解析速率：{:.0} 帧/秒", rate);

    // 流水线预取成立的前提就是这条：解析必须跑得比发送快得多。
    // UDP 发送再快也就百万包/秒级，解析低于 50 万帧/秒就该重新审视架构了。
    assert!(rate > 500_000.0, "解析速率仅 {rate:.0} 帧/秒，慢于预期");
}
