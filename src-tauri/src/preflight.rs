//! 启动前预检。
//!
//! 一次把所有问题列出来，而不是报第一个就停 —— 配置里往往不止一处要改，
//! 逐个报错会让人来回试好几轮。
//!
//! 界面在配置变动时随时调用它，所以问题在按下「开始发送」之前就已经摆在眼前了。

use serde::Serialize;

use crate::config::{MutationOp, SendConfig, TargetKind};
use crate::filter::CompiledFilter;
use crate::mutate::CompiledMutations;
use crate::net::UdpSender;
use crate::source::DataSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// 会阻止启动
    Error,
    /// 能启动，但多半不是想要的结果
    Warn,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub severity: Severity,
    /// 问题出在哪一块配置，供界面定位
    pub area: String,
    pub message: String,
}

impl Problem {
    fn error(area: &str, message: impl Into<String>) -> Self {
        Problem {
            severity: Severity::Error,
            area: area.into(),
            message: message.into(),
        }
    }

    fn warn(area: &str, message: impl Into<String>) -> Self {
        Problem {
            severity: Severity::Warn,
            area: area.into(),
            message: message.into(),
        }
    }
}

/// 检查整套配置。
///
/// `source` 为 `None` 表示尚未打开文件。
/// `engine_active` 为真时跳过试建套接字 —— 正在发送的任务已经占着那个本地端口，
/// 再建一次必然撞上「地址已被占用」，那是假错误。
pub fn check(cfg: &SendConfig, source: Option<&DataSource>, engine_active: bool) -> Vec<Problem> {
    let mut out = Vec::new();

    check_source(cfg, source, &mut out);
    check_filter(cfg, &mut out);
    check_mutations(cfg, &mut out);
    check_target(cfg, engine_active, &mut out);
    check_pacing(cfg, &mut out);

    out
}

fn check_source(cfg: &SendConfig, source: Option<&DataSource>, out: &mut Vec<Problem>) {
    let Some(src) = source else {
        out.push(Problem::error("文件", "尚未打开数据文件"));
        return;
    };

    // mmap 期间文件被外部截断会触发 SIGBUS，常规错误处理兜不住，只能提前拦
    if src.verify_unchanged().is_err() {
        out.push(Problem::error(
            "文件",
            "文件在打开后被改动过，请重新打开再发送",
        ));
    }

    let lines = src.line_count() as u64;
    let start = cfg.pacing.start_line.max(1);
    let end = if cfg.pacing.end_line == 0 {
        lines
    } else {
        cfg.pacing.end_line
    };

    if start > lines {
        out.push(Problem::error(
            "节奏控制",
            format!("起始行 {start} 超出文件的 {lines} 行"),
        ));
    }
    if cfg.pacing.end_line != 0 && cfg.pacing.end_line > lines {
        out.push(Problem::warn(
            "节奏控制",
            format!("结束行 {} 超出文件的 {lines} 行，将发到末尾", cfg.pacing.end_line),
        ));
    }
    if start > end {
        out.push(Problem::error(
            "节奏控制",
            format!("起始行 {start} 在结束行 {end} 之后"),
        ));
    }
}

fn check_filter(cfg: &SendConfig, out: &mut Vec<Problem>) {
    if let Err(e) = CompiledFilter::compile(&cfg.filter, &cfg.parse.prefix) {
        out.push(Problem::error("筛选规则", e.to_string()));
    }
}

fn check_mutations(cfg: &SendConfig, out: &mut Vec<Problem>) {
    if let Err(e) = CompiledMutations::compile(&cfg.mutate, &cfg.parse.prefix) {
        out.push(Problem::error("修改规则", e.to_string()));
        return;
    }

    // 阶段二按声明顺序执行，校验和之后再写别的计算值就白算了
    let stage2: Vec<&MutationOp> = cfg
        .mutate
        .rules
        .iter()
        .filter(|r| r.enabled && !r.op.is_structural())
        .map(|r| &r.op)
        .collect();

    if let Some(last_checksum) = stage2
        .iter()
        .rposition(|op| matches!(op, MutationOp::Checksum { .. }))
    {
        if last_checksum + 1 < stage2.len() {
            let after = stage2[last_checksum + 1].label();
            out.push(Problem::warn(
                "修改规则",
                format!(
                    "校验和后面还有「{after}」规则。它会改动已经算过校验的字节，把校验和移到最后才对"
                ),
            ));
        }
    }
}

fn check_target(cfg: &SendConfig, engine_active: bool, out: &mut Vec<Problem>) {
    // 直接试着建一遍套接字：地址格式、组播地址范围、绑定权限一次全验到。
    // 但任务正在跑时不能建 —— 本地端口已经被它占着，会报出假错误。
    if !engine_active {
        if let Err(e) = UdpSender::build(&cfg.target) {
            out.push(Problem::error("发送目标", e.to_string()));
        }
    }

    if let TargetKind::Multicast { interface, ttl, .. } = &cfg.target.kind {
        // 不用 is_none_or：那是 1.82 才稳定的 API，crate 声明的 MSRV 是 1.77
        let unset = interface.as_deref().map_or(true, |s| s.trim().is_empty());
        if unset && crate::net::list_interfaces().iter().filter(|i| !i.is_loopback).count() > 1 {
            out.push(Problem::warn(
                "发送目标",
                "本机有多张网卡但没指定出站网卡，组播可能从错误的网卡发出去",
            ));
        }
        if *ttl == 0 {
            out.push(Problem::warn(
                "发送目标",
                "TTL 为 0，组播包不会离开本机",
            ));
        }
    }
}

fn check_pacing(cfg: &SendConfig, out: &mut Vec<Problem>) {
    let p = &cfg.pacing;

    if p.interval_us == 0 {
        out.push(Problem::warn(
            "节奏控制",
            "间隔为 0 表示全速发送，实际速率取决于网卡和内核，很可能出现缓冲满丢帧",
        ));
    } else if p.interval_us < 50 {
        out.push(Problem::warn(
            "节奏控制",
            format!(
                "间隔 {}μs 约合 {} 帧/秒，这个量级的瓶颈通常在网卡和协议栈，不在定时器",
                p.interval_us,
                1_000_000 / p.interval_us
            ),
        ));
    }

    if p.high_precision && p.interval_us >= 5_000 {
        out.push(Problem::warn(
            "节奏控制",
            "间隔已在毫秒量级，高精度模式带来的收益有限，却会占满一个 CPU 核心",
        ));
    }

    if p.repeat && p.repeat_count == 0 && p.interval_us == 0 {
        out.push(Problem::warn(
            "节奏控制",
            "无限循环 + 全速发送，启动后会一直满速发到手动停止",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ByteRange, ChecksumAlgo, Condition, Endian, FilterConfig, FilterRule, MutationConfig,
        MutationRule, PacingConfig, TargetConfig, Width,
    };

    fn base() -> SendConfig {
        SendConfig {
            target: TargetConfig {
                kind: TargetKind::Unicast {
                    host: "127.0.0.1".into(),
                    port: 19500,
                },
                ..Default::default()
            },
            pacing: PacingConfig {
                interval_us: 1000,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn run(cfg: &SendConfig) -> Vec<Problem> {
        check(cfg, None, false)
    }

    fn errors(p: &[Problem]) -> Vec<&str> {
        p.iter()
            .filter(|x| x.severity == Severity::Error)
            .map(|x| x.message.as_str())
            .collect()
    }

    fn warns(p: &[Problem]) -> Vec<&str> {
        p.iter()
            .filter(|x| x.severity == Severity::Warn)
            .map(|x| x.message.as_str())
            .collect()
    }

    #[test]
    fn missing_file_is_an_error() {
        let p = run(&base());
        assert!(errors(&p).iter().any(|m| m.contains("尚未打开")));
    }

    #[test]
    fn reports_every_problem_at_once_not_just_the_first() {
        let mut cfg = base();
        // 同时埋三处错：筛选、修改、目标
        cfg.filter = FilterConfig {
            rules: vec![FilterRule {
                condition: Condition::Bytes {
                    offset: 0,
                    value: "ZZ".into(),
                    mask: None,
                },
                negate: false,
                enabled: true,
            }],
        };
        cfg.mutate = MutationConfig {
            rules: vec![MutationRule {
                op: MutationOp::Delete {
                    offset: 0,
                    length: 0,
                },
                condition: None,
                enabled: true,
            }],
        };
        cfg.target.kind = TargetKind::Multicast {
            group: "192.168.1.1".into(), // 不是组播地址
            port: 19501,
            interface: None,
            ttl: 1,
            loopback: true,
        };

        let p = run(&cfg);
        let e = errors(&p);
        assert!(e.iter().any(|m| m.contains("十六进制")), "缺筛选规则报错");
        assert!(e.iter().any(|m| m.contains("删除长度")), "缺修改规则报错");
        assert!(e.iter().any(|m| m.contains("组播地址")), "缺目标报错");
    }

    #[test]
    fn warns_when_checksum_is_not_the_last_computed_rule() {
        let mut cfg = base();
        cfg.mutate = MutationConfig {
            rules: vec![
                MutationRule {
                    op: MutationOp::Checksum {
                        offset: -2,
                        algorithm: ChecksumAlgo::Crc16Ccitt,
                        endian: Endian::Big,
                        range: ByteRange { start: 0, end: -2 },
                    },
                    condition: None,
                    enabled: true,
                },
                MutationRule {
                    op: MutationOp::Length {
                        offset: 0,
                        width: Width::W2,
                        endian: Endian::Big,
                        range: ByteRange::default(),
                        include_self: false,
                    },
                    condition: None,
                    enabled: true,
                },
            ],
        };

        let p = run(&cfg);
        assert!(
            warns(&p).iter().any(|m| m.contains("校验和")),
            "校验和排在长度之前应当告警"
        );
    }

    #[test]
    fn checksum_last_produces_no_ordering_warning() {
        let mut cfg = base();
        cfg.mutate = MutationConfig {
            rules: vec![
                MutationRule {
                    op: MutationOp::Length {
                        offset: 0,
                        width: Width::W2,
                        endian: Endian::Big,
                        range: ByteRange::default(),
                        include_self: false,
                    },
                    condition: None,
                    enabled: true,
                },
                MutationRule {
                    op: MutationOp::Checksum {
                        offset: -2,
                        algorithm: ChecksumAlgo::Crc16Ccitt,
                        endian: Endian::Big,
                        range: ByteRange { start: 0, end: -2 },
                    },
                    condition: None,
                    enabled: true,
                },
            ],
        };

        let p = run(&cfg);
        assert!(!warns(&p).iter().any(|m| m.contains("校验和")));
    }

    #[test]
    fn zero_interval_warns_about_full_speed() {
        let mut cfg = base();
        cfg.pacing.interval_us = 0;
        assert!(warns(&run(&cfg)).iter().any(|m| m.contains("全速")));
    }

    #[test]
    fn high_precision_with_millisecond_interval_warns() {
        let mut cfg = base();
        cfg.pacing.high_precision = true;
        cfg.pacing.interval_us = 10_000;
        assert!(warns(&run(&cfg))
            .iter()
            .any(|m| m.contains("高精度")));
    }

    #[test]
    fn zero_ttl_multicast_warns() {
        let mut cfg = base();
        cfg.target.kind = TargetKind::Multicast {
            group: "239.255.0.9".into(),
            port: 19502,
            interface: Some("127.0.0.1".into()),
            ttl: 0,
            loopback: true,
        };
        assert!(warns(&run(&cfg))
            .iter()
            .any(|m| m.contains("不会离开本机")));
    }

    #[test]
    fn clean_config_has_no_errors_beyond_the_missing_file() {
        let p = run(&base());
        let e = errors(&p);
        assert_eq!(e.len(), 1, "只该报「尚未打开文件」，实际：{e:?}");
    }
}
