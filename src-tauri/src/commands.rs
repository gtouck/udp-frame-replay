//! Tauri 命令层：前端与引擎之间的边界。

use serde::Serialize;
use tauri::State;

use crate::config::ParseConfig;
use crate::parse::{self, ParseErrorKind};
use crate::source::{DataSource, FileInfo};
use crate::state::AppState;

/// 单次预览请求的行数上限，避免一次拉取拖垮 IPC
const MAX_PREVIEW_LINES: usize = 1000;

/// 单个片段的展示字符数上限，防止病态超长行拖死渲染
const MAX_SEGMENT_CHARS: usize = 4096;

/// 一行的预览标注。
///
/// 返回切好的三段文本而非字节偏移 —— Rust 用 UTF-8 字节偏移、JS 用 UTF-16 码元索引，
/// 传偏移过去必然在汉字上错位。传片段则前端直接渲染三个 span，没有索引换算。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinePreview {
    /// 1-based 行号
    pub line_no: u32,
    /// 被丢弃的前缀
    pub prefix: String,
    /// 被采用的数据体
    pub data: String,
    /// 尾部被忽略的内容（如注释）
    pub trailing: String,
    /// 解码后的字节数
    pub byte_len: u32,
    /// 展示内容是否被截断
    pub truncated: bool,
    pub error: Option<ParseErrorKind>,
    pub error_msg: Option<String>,
}

/// 按字符边界安全截断
fn clip(s: &str) -> (String, bool) {
    if s.chars().count() <= MAX_SEGMENT_CHARS {
        return (s.to_string(), false);
    }
    let end = s
        .char_indices()
        .nth(MAX_SEGMENT_CHARS)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    (s[..end].to_string(), true)
}

#[tauri::command]
pub fn open_file(path: String, state: State<'_, AppState>) -> Result<FileInfo, String> {
    let src = DataSource::open(std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    let info = src.info();
    *state.source.write() = Some(src);
    Ok(info)
}

#[tauri::command]
pub fn close_file(state: State<'_, AppState>) {
    *state.source.write() = None;
}

#[tauri::command]
pub fn file_info(state: State<'_, AppState>) -> Option<FileInfo> {
    state.source.read().as_ref().map(|s| s.info())
}

/// 取 `[start, start+count)` 行的解析预览（`start` 为 0-based 行号）。
#[tauri::command]
pub fn preview(
    start: usize,
    count: usize,
    config: ParseConfig,
    state: State<'_, AppState>,
) -> Result<Vec<LinePreview>, String> {
    let guard = state.source.read();
    let src = guard.as_ref().ok_or("尚未打开文件")?;

    let count = count.min(MAX_PREVIEW_LINES);
    let end = (start + count).min(src.line_count());
    if start >= end {
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(end - start);
    let mut buf: Vec<u8> = Vec::with_capacity(1024);

    for i in start..end {
        let text = match src.line_text(i, config.encoding) {
            Some(t) => t,
            None => break,
        };

        let (spans, err) = parse::parse_line(&text, &config, &mut buf);

        let (prefix, t1) = clip(&text[..spans.data_start]);
        let (data, t2) = clip(&text[spans.data_start..spans.data_end]);
        let (trailing, t3) = clip(&text[spans.data_end..]);

        out.push(LinePreview {
            line_no: (i + 1) as u32,
            prefix,
            data,
            trailing,
            byte_len: buf.len() as u32,
            truncated: t1 || t2 || t3,
            error: err,
            error_msg: err.map(|e| e.message().to_string()),
        });
    }

    Ok(out)
}
