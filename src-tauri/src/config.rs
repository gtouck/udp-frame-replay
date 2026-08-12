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
