//! Tauri 命令层：前端与引擎之间的边界。

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::config::{FilterConfig, ParseConfig, SendConfig};
use crate::engine::{Engine, EngineSnapshot, SentFrame};
use crate::filter::CompiledFilter;
use crate::log::{ErrorGroup, LogEntry};
use crate::net::{list_interfaces, InterfaceInfo};
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
    /// 被筛选规则排除（解析没问题，但不会发出去）
    pub filtered: bool,
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
    state.log.info(format!(
        "已打开 {} · {} 行 · 索引占用 {} KB",
        info.path,
        info.line_count,
        info.index_memory_bytes / 1024
    ));
    *state.source.write() = Some(Arc::new(src));
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
    filter: FilterConfig,
    state: State<'_, AppState>,
) -> Result<Vec<LinePreview>, String> {
    let guard = state.source.read();
    let src = guard.as_ref().ok_or("尚未打开文件")?;

    // 规则写到一半时会有非法的十六进制，那不是错误，只是还没写完 ——
    // 预览退回「不筛选」，等规则写完整了自然就生效了
    let compiled = CompiledFilter::compile(&filter, &config.prefix).ok();

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

        let filtered = err.is_none()
            && compiled
                .as_ref()
                .is_some_and(|f| !f.is_empty() && !f.accepts(&text, &buf));

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
            filtered,
            error: err,
            error_msg: err.map(|e| e.message().to_string()),
        });
    }

    Ok(out)
}

// ── 网络 ────────────────────────────────────────────────────

#[tauri::command]
pub fn network_interfaces() -> Vec<InterfaceInfo> {
    list_interfaces()
}

// ── 发送控制 ────────────────────────────────────────────────

#[tauri::command]
pub fn start_send(config: SendConfig, state: State<'_, AppState>) -> Result<(), String> {
    let mut slot = state.engine.lock();

    // 上一次跑完的引擎留在槽里，先收干净再启动新的
    if let Some(existing) = slot.as_ref() {
        if !existing.is_finished() {
            return Err("正在发送，请先停止".into());
        }
        if let Some(old) = slot.take() {
            old.shutdown();
        }
    }

    let source = state
        .source
        .read()
        .as_ref()
        .cloned()
        .ok_or("尚未打开文件")?;

    let engine = Engine::start(source, config, state.log.clone()).map_err(|e| {
        let msg = e.to_string();
        state.log.error(format!("启动失败：{msg}"));
        msg
    })?;

    *slot = Some(engine);
    Ok(())
}

#[tauri::command]
pub fn pause_send(state: State<'_, AppState>) {
    if let Some(e) = state.engine.lock().as_ref() {
        e.pause();
    }
}

#[tauri::command]
pub fn resume_send(state: State<'_, AppState>) {
    if let Some(e) = state.engine.lock().as_ref() {
        e.resume();
    }
}

/// 单步：暂停状态下放一帧出去，用于逐帧核对规则
#[tauri::command]
pub fn step_send(state: State<'_, AppState>) {
    if let Some(e) = state.engine.lock().as_ref() {
        e.step();
    }
}

#[tauri::command]
pub fn stop_send(state: State<'_, AppState>) {
    let engine = state.engine.lock().take();
    if let Some(e) = engine {
        e.shutdown();
    }
}

#[tauri::command]
pub fn engine_status(state: State<'_, AppState>) -> Option<EngineSnapshot> {
    state.engine.lock().as_ref().map(|e| e.snapshot())
}

#[tauri::command]
pub fn recent_frames(limit: usize, state: State<'_, AppState>) -> Vec<SentFrame> {
    state
        .engine
        .lock()
        .as_ref()
        .map(|e| e.recent_frames(limit.min(200)))
        .unwrap_or_default()
}

// ── 日志 ────────────────────────────────────────────────────

/// 按序号增量拉取，避免每次轮询都搬运整个日志
#[tauri::command]
pub fn log_entries(after: u64, limit: usize, state: State<'_, AppState>) -> Vec<LogEntry> {
    state.log.entries_after(after, limit.min(2000))
}

#[tauri::command]
pub fn error_groups(state: State<'_, AppState>) -> Vec<ErrorGroup> {
    state.log.error_groups()
}

#[tauri::command]
pub fn clear_log(state: State<'_, AppState>) {
    state.log.clear();
}
