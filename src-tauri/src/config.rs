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

// ── 筛选规则 ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextOp {
    Equals,
    Contains,
}

/// 一条筛选条件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Condition {
    /// 按行内字段的文本匹配。字段序号从行首算起，0-based。
    #[serde(rename_all = "camelCase")]
    Field {
        index: usize,
        op: TextOp,
        value: String,
    },

    /// 按数据体中的字节匹配
    #[serde(rename_all = "camelCase")]
    Bytes {
        /// 字节偏移。负数表示从帧尾倒数，-2 配两字节即匹配最后两字节。
        offset: i64,
        /// 期望的字节序列，十六进制文本
        value: String,
        /// 可选掩码，十六进制文本，长度须与 value 一致
        mask: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterRule {
    pub condition: Condition,
    /// 取反：满足条件的反而被排除
    pub negate: bool,
    pub enabled: bool,
}

/// 多条规则之间是「与」的关系：全部满足才发送。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FilterConfig {
    pub rules: Vec<FilterRule>,
}

// ── 修改规则 ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Endian {
    Big,
    Little,
}

impl Default for Endian {
    fn default() -> Self {
        Endian::Big
    }
}

/// 多字节值占用的字节数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Width {
    W1,
    W2,
    W4,
    W8,
}

impl Width {
    pub fn bytes(self) -> usize {
        match self {
            Width::W1 => 1,
            Width::W2 => 2,
            Width::W4 => 4,
            Width::W8 => 8,
        }
    }
}

impl Default for Width {
    fn default() -> Self {
        Width::W2
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimeUnit {
    Millis,
    Micros,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimeEpoch {
    /// Unix 纪元起的绝对时间
    Unix,
    /// 本次发送开始起的相对时间
    SinceStart,
}

/// 字节范围。起止都支持负数（从帧尾倒数）。
///
/// 区间是左闭右开 `[start, end)`；`end` 填 0 表示一直到帧尾 ——
/// 「第 2 字节到倒数第 2 字节」这种校验范围太常见了，必须能直接表达。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteRange {
    pub start: i64,
    pub end: i64,
}

impl Default for ByteRange {
    fn default() -> Self {
        ByteRange { start: 0, end: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChecksumAlgo {
    Sum8,
    Sum16,
    Xor8,
    /// CRC-16/IBM-3740，常称 CCITT-FALSE
    Crc16Ccitt,
    Crc16Modbus,
    Crc16Xmodem,
    /// CRC-32/ISO-HDLC，zip 与以太网用的那个
    Crc32,
}

/// 一次修改操作。
///
/// 前三种改变帧的结构，在阶段一执行，偏移一律基于**原始帧**；
/// 后四种写入计算值，在阶段二执行，偏移基于**阶段一之后的帧**。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MutationOp {
    #[serde(rename_all = "camelCase")]
    Insert { offset: i64, value: String },

    #[serde(rename_all = "camelCase")]
    Replace { offset: i64, value: String },

    #[serde(rename_all = "camelCase")]
    Delete { offset: i64, length: usize },

    #[serde(rename_all = "camelCase")]
    Sequence {
        offset: i64,
        width: Width,
        endian: Endian,
        start: u64,
        step: u64,
        /// 循环发送时是否把计数器归零
        reset_each_loop: bool,
    },

    #[serde(rename_all = "camelCase")]
    Timestamp {
        offset: i64,
        width: Width,
        endian: Endian,
        unit: TimeUnit,
        epoch: TimeEpoch,
    },

    #[serde(rename_all = "camelCase")]
    Length {
        offset: i64,
        width: Width,
        endian: Endian,
        range: ByteRange,
        /// 长度值是否把长度字段自身的字节也算进去
        include_self: bool,
    },

    #[serde(rename_all = "camelCase")]
    Checksum {
        offset: i64,
        algorithm: ChecksumAlgo,
        endian: Endian,
        range: ByteRange,
    },
}

impl MutationOp {
    /// 是否属于阶段一（改变帧结构）
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            MutationOp::Insert { .. } | MutationOp::Replace { .. } | MutationOp::Delete { .. }
        )
    }

    pub fn label(&self) -> &'static str {
        match self {
            MutationOp::Insert { .. } => "插入",
            MutationOp::Replace { .. } => "替换",
            MutationOp::Delete { .. } => "删除",
            MutationOp::Sequence { .. } => "序号",
            MutationOp::Timestamp { .. } => "时间戳",
            MutationOp::Length { .. } => "长度",
            MutationOp::Checksum { .. } => "校验和",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationRule {
    pub op: MutationOp,
    /// 仅当条件成立时才对该帧生效。留空表示对每一帧都生效。
    pub condition: Option<Condition>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MutationConfig {
    pub rules: Vec<MutationRule>,
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
    #[serde(default)]
    pub filter: FilterConfig,
    #[serde(default)]
    pub mutate: MutationConfig,
    pub target: TargetConfig,
    pub pacing: PacingConfig,
}

/// 存盘的配置档。
///
/// 手动规则一旦配好就该能复用 —— 换一种数据格式不该从头再配一遍。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// 档案格式版本，将来改结构时用来做兼容
    pub version: u32,
    pub name: String,
    pub config: SendConfig,
}

pub const PROFILE_VERSION: u32 = 1;

impl Profile {
    pub fn new(name: impl Into<String>, config: SendConfig) -> Self {
        Profile {
            version: PROFILE_VERSION,
            name: name.into(),
            config,
        }
    }
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
            mutate: MutationConfig {
                rules: vec![
                    MutationRule {
                        op: MutationOp::Insert {
                            offset: 0,
                            value: "5A A5".into(),
                        },
                        condition: None,
                        enabled: true,
                    },
                    MutationRule {
                        op: MutationOp::Checksum {
                            offset: -2,
                            algorithm: ChecksumAlgo::Crc16Ccitt,
                            endian: Endian::Little,
                            range: ByteRange { start: 2, end: -2 },
                        },
                        condition: Some(Condition::Field {
                            index: 0,
                            op: TextOp::Equals,
                            value: "[TX]".into(),
                        }),
                        enabled: true,
                    },
                ],
            },
            filter: FilterConfig {
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
                            offset: -2,
                            value: "3F 2B".into(),
                            mask: Some("FF F0".into()),
                        },
                        negate: true,
                        enabled: true,
                    },
                ],
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
    fn profile_round_trips_through_json_text() {
        let profile = Profile::new(
            "CAN 总线回放",
            SendConfig {
                pacing: PacingConfig {
                    interval_us: 500,
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let text = serde_json::to_string_pretty(&profile).unwrap();
        let back: Profile = serde_json::from_str(&text).unwrap();
        assert_eq!(back, profile);
        assert_eq!(back.version, PROFILE_VERSION);
        assert_eq!(back.name, "CAN 总线回放");
    }

    #[test]
    fn profile_without_optional_sections_still_loads() {
        // 旧档案没有 filter / mutate 两节，靠 serde default 补齐
        let text = r#"{
            "version": 1,
            "name": "旧档案",
            "config": {
                "parse": {
                    "encoding": "utf8",
                    "prefix": { "mode": "chars", "skipChars": 4 },
                    "hex": { "ignoreChars": "" }
                },
                "target": { "mode": "unicast", "host": "10.0.0.1", "port": 8000,
                            "bindAddr": null, "bindPort": null, "sendBufferBytes": null },
                "pacing": { "intervalUs": 100, "startLine": 1, "endLine": 0,
                            "repeat": false, "repeatCount": 0, "highPrecision": false }
            }
        }"#;
        let p: Profile = serde_json::from_str(text).expect("旧档案必须仍然可读");
        assert!(p.config.filter.rules.is_empty());
        assert!(p.config.mutate.rules.is_empty());
        assert_eq!(p.config.pacing.interval_us, 100);
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
