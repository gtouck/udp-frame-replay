//! 解析管线：把一行原文切成「丢弃的前缀 / 采用的数据体 / 尾部忽略内容」，并解码数据体。
//!
//! 输入是已按配置编码解码好的 `&str`（见 [`crate::source::text`]），
//! 所有偏移都是该 `&str` 内的字节偏移。

pub mod hex;
pub mod prefix;

use serde::{Deserialize, Serialize};

use crate::config::ParseConfig;

/// 解析错误类型。用于日志聚合，所以是有限枚举而非自由字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParseErrorKind {
    /// 剥离前缀后没有任何十六进制字符
    EmptyData,
    /// 十六进制字符数为奇数，无法凑成完整字节
    OddHexDigits,
    /// 按字段模式切分时字段数不足，无法定位数据体
    NotEnoughFields,
    /// 按字符模式跳过时行长度不足
    LineTooShort,
}

impl ParseErrorKind {
    pub fn message(&self) -> &'static str {
        match self {
            ParseErrorKind::EmptyData => "剥离前缀后没有十六进制数据",
            ParseErrorKind::OddHexDigits => "十六进制字符数为奇数",
            ParseErrorKind::NotEnoughFields => "字段数不足，无法定位数据体",
            ParseErrorKind::LineTooShort => "行长度不足，无法跳过指定字符数",
        }
    }
}

/// 一行的切分结果，以字节偏移表示（相对于整行文本）。
///
/// `[0, data_start)` 是被丢弃的前缀，
/// `[data_start, data_end)` 是被采用的数据体，
/// `[data_end, len)` 是尾部被忽略的内容（如注释）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineSpans {
    pub data_start: usize,
    pub data_end: usize,
}

impl LineSpans {
    /// 整行都算作前缀（解析在剥离阶段就失败了）
    fn all_prefix(len: usize) -> Self {
        LineSpans {
            data_start: len,
            data_end: len,
        }
    }
}

/// 完整解析一行：剥前缀 + 解码 hex。解码结果写入 `out`（调用前会被清空）。
///
/// 即使发生错误也返回切分位置，供界面标注展示。
pub fn parse_line(
    text: &str,
    cfg: &ParseConfig,
    out: &mut Vec<u8>,
) -> (LineSpans, Option<ParseErrorKind>) {
    out.clear();

    let data_start = match prefix::strip(text, &cfg.prefix) {
        Ok(start) => start,
        Err(kind) => return (LineSpans::all_prefix(text.len()), Some(kind)),
    };

    let scan = hex::decode_into(&text[data_start..], &cfg.hex, out);

    let spans = LineSpans {
        data_start,
        data_end: data_start + scan.data_len,
    };

    (spans, scan.error)
}
