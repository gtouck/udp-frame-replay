//! 端到端发送测试：起一个真实的 UDP 接收套接字，发已知文件，逐字节核对收到的内容。
//!
//! 这是性价比最高的一条测试线 —— 它同时覆盖行索引、编码解码、前缀剥离、
//! hex 解码、环形缓冲、节拍器和套接字，任何一环坏了都会在这里露出来。

use std::io::Write;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use data_perf_lib::config::{
    Condition, Delimiter, FilterConfig, FilterRule, PacingConfig, ParseConfig, PrefixRule,
    SendConfig, TargetConfig, TargetKind, TextEncoding, TextOp,
};
use data_perf_lib::engine::Engine;
use data_perf_lib::log::LogSink;
use data_perf_lib::source::DataSource;

/// 每个测试用独立文件名，避免并行跑测试时互相踩
static SEQ: AtomicU32 = AtomicU32::new(0);

fn temp_file(content: &str) -> std::path::PathBuf {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "data-perf-test-{}-{}.txt",
        std::process::id(),
        n
    ));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.sync_all().unwrap();
    path
}

const SAMPLE: &str = "\
[TX] 000001 发送 01 02 03
[TX] 000002 发送 AA BB
[TX] 000003 发送 FF
[TX] 000004 发送 10 20 30 40
[TX] 000005 发送 55
";

fn cfg(port: u16) -> SendConfig {
    SendConfig {
        parse: ParseConfig {
            encoding: TextEncoding::Utf8,
            prefix: PrefixRule::Fields {
                delimiter: Delimiter::Whitespace,
                collapse: true,
                skip_fields: 3,
            },
            hex: Default::default(),
        },
        filter: Default::default(),
        target: TargetConfig {
            kind: TargetKind::Unicast {
                host: "127.0.0.1".into(),
                port,
            },
            ..Default::default()
        },
        pacing: PacingConfig {
            interval_us: 0, // 全速，让测试尽快跑完
            ..Default::default()
        },
    }
}

/// 绑定一个接收套接字，返回 (socket, 端口)
fn receiver() -> (UdpSocket, u16) {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    let port = s.local_addr().unwrap().port();
    (s, port)
}

/// 收满 `want` 个包或超时为止
fn collect(sock: &UdpSocket, want: usize, budget: Duration) -> Vec<Vec<u8>> {
    let mut got = Vec::new();
    let deadline = Instant::now() + budget;
    let mut buf = [0u8; 65535];

    while got.len() < want && Instant::now() < deadline {
        match sock.recv(&mut buf) {
            Ok(n) => got.push(buf[..n].to_vec()),
            Err(_) => continue, // 读超时，继续等到预算用完
        }
    }
    got
}

fn run(content: &str, mut make: impl FnMut(&mut SendConfig)) -> (Vec<Vec<u8>>, Arc<LogSink>) {
    let path = temp_file(content);
    let (sock, port) = receiver();

    let mut c = cfg(port);
    make(&mut c);

    let src = Arc::new(DataSource::open(&path).unwrap());
    let log = Arc::new(LogSink::default());
    let engine = Engine::start(src, c, log.clone()).unwrap();

    // 期望帧数未知时多要一些，靠超时收尾
    let frames = collect(&sock, 64, Duration::from_millis(800));
    engine.shutdown();
    let _ = std::fs::remove_file(&path);

    (frames, log)
}

#[test]
fn sends_every_line_in_order_with_exact_bytes() {
    let (frames, _) = run(SAMPLE, |_| {});

    assert_eq!(frames.len(), 5, "五行应当发出五帧");
    assert_eq!(frames[0], vec![0x01, 0x02, 0x03]);
    assert_eq!(frames[1], vec![0xAA, 0xBB]);
    assert_eq!(frames[2], vec![0xFF]);
    assert_eq!(frames[3], vec![0x10, 0x20, 0x30, 0x40]);
    assert_eq!(frames[4], vec![0x55]);
}

#[test]
fn respects_start_and_end_line() {
    let (frames, _) = run(SAMPLE, |c| {
        c.pacing.start_line = 2;
        c.pacing.end_line = 4;
    });

    assert_eq!(frames.len(), 3, "第 2~4 行共三帧");
    assert_eq!(frames[0], vec![0xAA, 0xBB]);
    assert_eq!(frames[2], vec![0x10, 0x20, 0x30, 0x40]);
}

#[test]
fn repeats_the_configured_number_of_times() {
    let (frames, _) = run(SAMPLE, |c| {
        c.pacing.start_line = 1;
        c.pacing.end_line = 2;
        c.pacing.repeat = true;
        c.pacing.repeat_count = 3;
    });

    assert_eq!(frames.len(), 6, "两行循环三轮共六帧");
    // 每轮内容一致
    for round in 0..3 {
        assert_eq!(frames[round * 2], vec![0x01, 0x02, 0x03]);
        assert_eq!(frames[round * 2 + 1], vec![0xAA, 0xBB]);
    }
}

#[test]
fn malformed_lines_are_skipped_not_fatal() {
    let content = "\
[TX] 000001 发送 01 02 03
[TX] 000002 发送 AA B
[TX] 000003 发送
[TX] 000004 发送 10 20
";
    let (frames, log) = run(content, |_| {});

    // 第 2 行 hex 个数为奇数、第 3 行没有数据，都跳过，好行照发
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0], vec![0x01, 0x02, 0x03]);
    assert_eq!(frames[1], vec![0x10, 0x20]);

    assert_eq!(log.total_parse_errors(), 2);
    // 错误按类型聚合，不是每行一条
    assert_eq!(log.error_groups().len(), 2);
}

#[test]
fn gbk_encoded_file_sends_the_same_bytes() {
    let path = {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir()
            .join(format!("data-perf-gbk-{}-{}.txt", std::process::id(), n));
        let (bytes, _, _) = encoding_rs::GBK.encode(SAMPLE);
        std::fs::write(&p, &bytes).unwrap();
        p
    };

    let (sock, port) = receiver();
    let mut c = cfg(port);
    c.parse.encoding = TextEncoding::Gbk;

    let src = Arc::new(DataSource::open(&path).unwrap());
    let engine = Engine::start(src, c, Arc::new(LogSink::default())).unwrap();
    let frames = collect(&sock, 64, Duration::from_millis(800));
    engine.shutdown();
    let _ = std::fs::remove_file(&path);

    assert_eq!(frames.len(), 5);
    assert_eq!(frames[0], vec![0x01, 0x02, 0x03]);
    assert_eq!(frames[3], vec![0x10, 0x20, 0x30, 0x40]);
}

#[test]
fn reports_stats_matching_what_was_received() {
    let path = temp_file(SAMPLE);
    let (sock, port) = receiver();

    let src = Arc::new(DataSource::open(&path).unwrap());
    let engine = Engine::start(src, cfg(port), Arc::new(LogSink::default())).unwrap();

    let frames = collect(&sock, 5, Duration::from_millis(800));
    // 给发送线程一点时间把状态落定
    std::thread::sleep(Duration::from_millis(50));
    let snap = engine.snapshot();
    engine.shutdown();
    let _ = std::fs::remove_file(&path);

    assert_eq!(frames.len(), 5);
    assert_eq!(snap.sent_frames, 5);
    assert_eq!(snap.parsed_frames, 5);
    assert_eq!(snap.skipped_lines, 0);
    assert_eq!(snap.dropped_buffer_full, 0);
    assert_eq!(snap.sent_bytes, 3 + 2 + 1 + 4 + 1);
}

#[test]
fn rejects_start_line_beyond_end_of_file() {
    let path = temp_file(SAMPLE);
    let src = Arc::new(DataSource::open(&path).unwrap());

    let mut c = cfg(19999);
    c.pacing.start_line = 9999;

    let err = Engine::start(src, c, Arc::new(LogSink::default())).unwrap_err();
    assert!(err.to_string().contains("超出文件行数"), "实际：{err}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rejects_inverted_line_range() {
    let path = temp_file(SAMPLE);
    let src = Arc::new(DataSource::open(&path).unwrap());

    let mut c = cfg(19998);
    c.pacing.start_line = 4;
    c.pacing.end_line = 2;

    let err = Engine::start(src, c, Arc::new(LogSink::default())).unwrap_err();
    assert!(err.to_string().contains("大于结束行"), "实际：{err}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn pause_halts_sending_and_step_releases_one_frame() {
    let path = temp_file(SAMPLE);
    let (sock, port) = receiver();

    let mut c = cfg(port);
    c.pacing.interval_us = 50_000; // 50ms 一帧，方便在帧间插入暂停

    let src = Arc::new(DataSource::open(&path).unwrap());
    let engine = Engine::start(src, c, Arc::new(LogSink::default())).unwrap();

    engine.pause();
    std::thread::sleep(Duration::from_millis(120));
    let after_pause = engine.snapshot().sent_frames;

    // 暂停期间不该再有新帧
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(
        engine.snapshot().sent_frames,
        after_pause,
        "暂停后不应继续发送"
    );

    // 单步放行一帧
    engine.step();
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(
        engine.snapshot().sent_frames,
        after_pause + 1,
        "单步应当恰好放出一帧"
    );

    engine.shutdown();
    let _ = collect(&sock, 1, Duration::from_millis(10));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn multicast_loopback_reaches_a_joined_receiver() {
    use std::net::{Ipv4Addr, SocketAddrV4};

    let group: Ipv4Addr = "239.255.42.99".parse().unwrap();

    let sock = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)).unwrap();
    let port = sock.local_addr().unwrap().port();
    sock.join_multicast_v4(&group, &Ipv4Addr::LOCALHOST).unwrap();
    sock.set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();

    let path = temp_file(SAMPLE);
    let mut c = cfg(port);
    c.target.kind = TargetKind::Multicast {
        group: group.to_string(),
        port,
        interface: Some("127.0.0.1".into()),
        ttl: 1,
        loopback: true,
    };

    let src = Arc::new(DataSource::open(&path).unwrap());
    let engine = Engine::start(src, c, Arc::new(LogSink::default())).unwrap();
    let frames = collect(&sock, 5, Duration::from_millis(1000));
    engine.shutdown();
    let _ = std::fs::remove_file(&path);

    assert!(
        !frames.is_empty(),
        "组播回环未收到任何数据；若本机禁用了组播回环，此项会失败"
    );
    assert_eq!(frames[0], vec![0x01, 0x02, 0x03]);
}

// ── 筛选规则 ────────────────────────────────────────────────

const MIXED: &str = "\
[TX] 000001 发送 01 02 03
[RX] 000002 接收 AA BB
[TX] 000003 发送 01 FF FF
[RX] 000004 接收 CC DD
[TX] 000005 发送 02 03 04
";

fn only(condition: Condition, negate: bool) -> FilterConfig {
    FilterConfig {
        rules: vec![FilterRule {
            condition,
            negate,
            enabled: true,
        }],
    }
}

#[test]
fn field_filter_sends_only_matching_lines() {
    let (frames, _) = run(MIXED, |c| {
        c.filter = only(
            Condition::Field {
                index: 0,
                op: TextOp::Equals,
                value: "[TX]".into(),
            },
            false,
        );
    });

    assert_eq!(frames.len(), 3, "只有三行以 [TX] 开头");
    assert_eq!(frames[0], vec![0x01, 0x02, 0x03]);
    assert_eq!(frames[1], vec![0x01, 0xFF, 0xFF]);
    assert_eq!(frames[2], vec![0x02, 0x03, 0x04]);
}

#[test]
fn negated_field_filter_excludes_matching_lines() {
    let (frames, _) = run(MIXED, |c| {
        c.filter = only(
            Condition::Field {
                index: 0,
                op: TextOp::Equals,
                value: "[TX]".into(),
            },
            true,
        );
    });

    assert_eq!(frames.len(), 2, "取反后只剩 [RX] 两行");
    assert_eq!(frames[0], vec![0xAA, 0xBB]);
    assert_eq!(frames[1], vec![0xCC, 0xDD]);
}

#[test]
fn byte_filter_matches_on_decoded_data() {
    let (frames, _) = run(MIXED, |c| {
        c.filter = only(
            Condition::Bytes {
                offset: 0,
                value: "01".into(),
                mask: None,
            },
            false,
        );
    });

    assert_eq!(frames.len(), 2, "首字节为 01 的有两行");
    assert_eq!(frames[0], vec![0x01, 0x02, 0x03]);
    assert_eq!(frames[1], vec![0x01, 0xFF, 0xFF]);
}

#[test]
fn negative_offset_filter_matches_from_frame_end() {
    let (frames, _) = run(MIXED, |c| {
        c.filter = only(
            Condition::Bytes {
                offset: -2,
                value: "FF FF".into(),
                mask: None,
            },
            false,
        );
    });

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0], vec![0x01, 0xFF, 0xFF]);
}

#[test]
fn multiple_rules_combine_with_and() {
    let (frames, _) = run(MIXED, |c| {
        c.filter = FilterConfig {
            rules: vec![
                FilterRule {
                    condition: Condition::Field {
                        index: 0,
                        op: TextOp::Equals,
                        value: "[TX]".into(),
                    },
                    negate: false,
                    enabled: true,
                },
                FilterRule {
                    condition: Condition::Bytes {
                        offset: 0,
                        value: "01".into(),
                        mask: None,
                    },
                    negate: false,
                    enabled: true,
                },
            ],
        };
    });

    assert_eq!(frames.len(), 2, "同时满足两条的行");
    assert_eq!(frames[0], vec![0x01, 0x02, 0x03]);
    assert_eq!(frames[1], vec![0x01, 0xFF, 0xFF]);
}

#[test]
fn filtered_out_lines_are_counted_separately_from_errors() {
    let path = temp_file(MIXED);
    let (sock, port) = receiver();

    let mut c = cfg(port);
    c.filter = only(
        Condition::Field {
            index: 0,
            op: TextOp::Equals,
            value: "[TX]".into(),
        },
        false,
    );

    let src = Arc::new(DataSource::open(&path).unwrap());
    let engine = Engine::start(src, c, Arc::new(LogSink::default())).unwrap();
    let frames = collect(&sock, 3, Duration::from_millis(800));
    std::thread::sleep(Duration::from_millis(50));
    let snap = engine.snapshot();
    engine.shutdown();
    let _ = std::fs::remove_file(&path);

    assert_eq!(frames.len(), 3);
    assert_eq!(snap.sent_frames, 3);
    assert_eq!(snap.filtered_out, 2, "被筛掉的行要单独计数");
    assert_eq!(snap.skipped_lines, 0, "筛掉不是解析错误");
}

#[test]
fn invalid_filter_is_rejected_before_starting() {
    let path = temp_file(MIXED);
    let src = Arc::new(DataSource::open(&path).unwrap());

    let mut c = cfg(19997);
    c.filter = only(
        Condition::Bytes {
            offset: 0,
            value: "ZZ".into(),
            mask: None,
        },
        false,
    );

    let err = Engine::start(src, c, Arc::new(LogSink::default())).unwrap_err();
    assert!(err.to_string().contains("筛选规则有误"), "实际：{err}");
    let _ = std::fs::remove_file(&path);
}
