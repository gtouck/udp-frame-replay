//! 配置模型。全部可序列化为 JSON，供 Profile 持久化与前端交互。

use serde::{Deserialize, Serialize};

/// 文本编码。带汉字标识的数据文件在 Windows 上常见 GBK，按 UTF-8 硬解会乱码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextEncoding {
    Utf8,
    Gbk,
    /// ISO-8859-1，把每个字节当作一个字符，用于纯 ASCII 或未知编码的兜底
    Latin1,
}

impl Default for TextEncoding {
    fn default() -> Self {
        TextEncoding::Utf8
    }
}

impl TextEncoding {
    pub fn as_encoding(&self) -> &'static encoding_rs::Encoding {
        match self {
            TextEncoding::Utf8 => encoding_rs::UTF_8,
            TextEncoding::Gbk => encoding_rs::GBK,
            TextEncoding::Latin1 => encoding_rs::WINDOWS_1252,
        }
    }
}

/// 字段分隔符
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum Delimiter {
    /// 任意空白字符（空格、Tab）
    Whitespace,
    Comma,
    Tab,
    /// 自定义字符集合，其中任一字符都作为分隔符
    Custom(String),
}

impl Default for Delimiter {
    fn default() -> Self {
        Delimiter::Whitespace
    }
}

impl Delimiter {
    /// 判断某个字符是否为分隔符
    pub fn is_delim(&self, c: char) -> bool {
        match self {
            Delimiter::Whitespace => c.is_whitespace(),
            Delimiter::Comma => c == ',',
            Delimiter::Tab => c == '\t',
            Delimiter::Custom(set) => set.chars().any(|d| d == c),
        }
    }
}

/// 前缀剥离规则
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum PrefixRule {
    /// 字段模式：按分隔符切分，丢弃前 N 个字段
    #[serde(rename_all = "camelCase")]
    Fields {
        delimiter: Delimiter,
        /// 连续分隔符视为一个（空白模式下建议开启）
        collapse: bool,
        /// 丢弃前 N 个字段
        skip_fields: usize,
    },
    /// 偏移模式：跳过前 N 个 Unicode 字符
    ///
    /// 按 char 计而非字节 —— 否则汉字前缀会被切碎。
    #[serde(rename_all = "camelCase")]
    Chars { skip_chars: usize },
}

impl Default for PrefixRule {
    fn default() -> Self {
        PrefixRule::Fields {
            delimiter: Delimiter::Whitespace,
            collapse: true,
            skip_fields: 0,
        }
    }
}

/// 十六进制解码规则
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HexRule {
    /// 除空白外还需忽略的分隔字符
    pub ignore_chars: String,
}

impl Default for HexRule {
    fn default() -> Self {
        HexRule {
            ignore_chars: ":-,".to_string(),
        }
    }
}

impl HexRule {
    #[inline]
    pub fn is_ignorable(&self, b: u8) -> bool {
        b.is_ascii_whitespace() || self.ignore_chars.as_bytes().contains(&b)
    }
}

/// 解析配置：文本编码 + 前缀剥离 + hex 解码
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ParseConfig {
    pub encoding: TextEncoding,
    pub prefix: PrefixRule,
    pub hex: HexRule,
}

// ── 发送目标 ────────────────────────────────────────────────

/// 发送方式
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum TargetKind {
    #[serde(rename_all = "camelCase")]
    Unicast { host: String, port: u16 },

    #[serde(rename_all = "camelCase")]
    Multicast {
        group: String,
        port: u16,
        /// 出站网卡的本机 IP。多网卡机器上不指定就会走系统默认路由，
        /// 组播包很可能从错误的网卡出去 —— 这是同类工具最常见的坑。
        interface: Option<String>,
        ttl: u32,
        /// 是否让本机也收到自己发的组播
        loopback: bool,
    },
}

impl Default for TargetKind {
    fn default() -> Self {
        TargetKind::Unicast {
            host: "127.0.0.1".into(),
            port: 9000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TargetConfig {
    #[serde(flatten)]
    pub kind: TargetKind,
    /// 本地绑定地址，留空为 0.0.0.0
    pub bind_addr: Option<String>,
    /// 本地绑定端口，留空为系统分配。有些接收端会校验源端口。
    pub bind_port: Option<u16>,
    /// 内核发送缓冲大小。高速发送时调大可减少 EWOULDBLOCK 丢弃。
    pub send_buffer_bytes: Option<usize>,
}

// ── 节奏控制 ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PacingConfig {
    /// 帧间隔，微秒
    pub interval_us: u64,
    /// 起始行，1-based，含
    pub start_line: u64,
    /// 结束行，1-based，含。0 表示直到文件末尾。
    pub end_line: u64,
    pub repeat: bool,
    /// 循环次数，0 表示无限
    pub repeat_count: u32,
    /// 高精度模式：自旋等待换取微秒级节拍，代价是占满一个 CPU 核心
    pub high_precision: bool,
}

impl Default for PacingConfig {
    fn default() -> Self {
        PacingConfig {
            interval_us: 1000,
            start_line: 1,
            end_line: 0,
            repeat: false,
            repeat_count: 0,
            high_precision: false,
        }
    }
}

/// 一次发送任务的完整配置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SendConfig {
    pub parse: ParseConfig,
    pub target: TargetConfig,
    pub pacing: PacingConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 这些测试钉住前后端的 JSON 契约。字段名或标签形状一变，
    /// 界面传过来的配置就会静默反序列化失败 —— 必须让它在这里先炸。
    fn round_trip<T>(v: &T) -> serde_json::Value
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_value(v).expect("序列化失败");
        let back: T = serde_json::from_value(json.clone()).expect("反序列化失败");
        assert_eq!(&back, v, "往返后不一致");
        json
    }

    #[test]
    fn delimiter_uses_kind_and_value() {
        let j = round_trip(&Delimiter::Whitespace);
        assert_eq!(j, serde_json::json!({ "kind": "whitespace" }));

        let j = round_trip(&Delimiter::Custom("|;".into()));
        assert_eq!(j, serde_json::json!({ "kind": "custom", "value": "|;" }));
    }

    #[test]
    fn prefix_rule_is_tagged_by_mode() {
        let j = round_trip(&PrefixRule::Fields {
            delimiter: Delimiter::Comma,
            collapse: false,
            skip_fields: 2,
        });
        assert_eq!(
            j,
            serde_json::json!({
                "mode": "fields",
                "delimiter": { "kind": "comma" },
                "collapse": false,
                "skipFields": 2
            })
        );

        let j = round_trip(&PrefixRule::Chars { skip_chars: 15 });
        assert_eq!(j, serde_json::json!({ "mode": "chars", "skipChars": 15 }));
    }

    #[test]
    fn target_config_flattens_kind_to_top_level() {
        let cfg = TargetConfig {
            kind: TargetKind::Unicast {
                host: "192.168.1.10".into(),
                port: 9000,
            },
            bind_addr: None,
            bind_port: Some(5000),
            send_buffer_bytes: None,
        };
        let j = round_trip(&cfg);

        // mode/host/port 必须和 bindPort 平级，前端才好组装
        assert_eq!(j["mode"], "unicast");
        assert_eq!(j["host"], "192.168.1.10");
        assert_eq!(j["port"], 9000);
        assert_eq!(j["bindPort"], 5000);
    }

    #[test]
    fn multicast_target_round_trips() {
        let cfg = TargetConfig {
            kind: TargetKind::Multicast {
                group: "239.255.0.1".into(),
                port: 5000,
                interface: Some("192.168.1.5".into()),
                ttl: 8,
                loopback: true,
            },
            ..Default::default()
        };
        let j = round_trip(&cfg);
        assert_eq!(j["mode"], "multicast");
        assert_eq!(j["interface"], "192.168.1.5");
        assert_eq!(j["ttl"], 8);
        assert_eq!(j["loopback"], true);
    }

    #[test]
    fn pacing_uses_camel_case() {
        let j = round_trip(&PacingConfig::default());
        for key in [
            "intervalUs",
            "startLine",
            "endLine",
            "repeat",
            "repeatCount",
            "highPrecision",
        ] {
            assert!(j.get(key).is_some(), "缺少字段 {key}");
        }
    }

    #[test]
    fn full_send_config_round_trips() {
        let cfg = SendConfig {
            parse: ParseConfig {
                encoding: TextEncoding::Gbk,
                prefix: PrefixRule::Fields {
                    delimiter: Delimiter::Whitespace,
                    collapse: true,
                    skip_fields: 3,
                },
                hex: HexRule {
                    ignore_chars: ":-,".into(),
                },
            },
            target: TargetConfig {
                kind: TargetKind::Multicast {
                    group: "224.0.1.1".into(),
                    port: 6000,
                    interface: None,
                    ttl: 1,
                    loopback: false,
                },
                bind_addr: Some("0.0.0.0".into()),
                bind_port: None,
                send_buffer_bytes: Some(1 << 20),
            },
            pacing: PacingConfig {
                interval_us: 250,
                start_line: 10,
                end_line: 2000,
                repeat: true,
                repeat_count: 5,
                high_precision: true,
            },
        };
        round_trip(&cfg);
    }

    #[test]
    fn defaults_deserialize_from_frontend_shape() {
        // 前端 defaultSendConfig() 生成的形状，必须能被后端直接吃下
        let json = serde_json::json!({
            "parse": {
                "encoding": "utf8",
                "prefix": { "mode": "fields", "delimiter": { "kind": "whitespace" },
                            "collapse": true, "skipFields": 0 },
                "hex": { "ignoreChars": ":-," }
            },
            "target": {
                "mode": "unicast", "host": "127.0.0.1", "port": 9000,
                "bindAddr": null, "bindPort": null, "sendBufferBytes": null
            },
            "pacing": {
                "intervalUs": 1000, "startLine": 1, "endLine": 0,
                "repeat": false, "repeatCount": 0, "highPrecision": false
            }
        });
        let cfg: SendConfig = serde_json::from_value(json).expect("前端默认配置必须可解析");
        assert_eq!(cfg.pacing.interval_us, 1000);
        assert!(matches!(cfg.target.kind, TargetKind::Unicast { .. }));
    }
}
