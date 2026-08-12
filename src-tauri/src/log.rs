//! 运行日志：环形条目 + 解析错误聚合。
//!
//! 关键决定：解析错误**按类型聚合计数**，不是每行一条。
//! 一个格式不对的 1GB 文件会产生几百万条同类错误，逐条记录会瞬间把界面拖死，
//! 而使用者真正需要知道的只是「哪一类错误、出现多少次、头几行在哪」。

use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::Serialize;

use crate::parse::ParseErrorKind;

/// 内存中保留的日志条数上限，超出后淘汰最旧的
const MAX_ENTRIES: usize = 50_000;

/// 每类错误保留多少个示例行号
const SAMPLE_LINES: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub seq: u64,
    /// Unix 毫秒时间戳，交给前端格式化
    pub at: u64,
    pub level: Level,
    pub text: String,
}

/// 一类解析错误的聚合结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorGroup {
    pub kind: ParseErrorKind,
    pub message: String,
    pub count: u64,
    /// 头几个出错的行号，供跳转定位
    pub sample_lines: Vec<u32>,
}

#[derive(Default)]
struct Inner {
    entries: VecDeque<LogEntry>,
    next_seq: u64,
    groups: HashMap<ParseErrorKind, ErrorGroup>,
}

#[derive(Default)]
pub struct LogSink {
    inner: Mutex<Inner>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl LogSink {
    pub fn log(&self, level: Level, text: impl Into<String>) {
        let mut g = self.inner.lock();
        let seq = g.next_seq;
        g.next_seq += 1;
        g.entries.push_back(LogEntry {
            seq,
            at: now_ms(),
            level,
            text: text.into(),
        });
        while g.entries.len() > MAX_ENTRIES {
            g.entries.pop_front();
        }
    }

    pub fn info(&self, text: impl Into<String>) {
        self.log(Level::Info, text);
    }

    pub fn warn(&self, text: impl Into<String>) {
        self.log(Level::Warn, text);
    }

    pub fn error(&self, text: impl Into<String>) {
        self.log(Level::Error, text);
    }

    /// 记一次解析错误。只累加计数，不产生日志条目。
    pub fn parse_error(&self, kind: ParseErrorKind, line_no: u32) {
        let mut g = self.inner.lock();
        let group = g.groups.entry(kind).or_insert_with(|| ErrorGroup {
            kind,
            message: kind.message().to_string(),
            count: 0,
            sample_lines: Vec::new(),
        });
        group.count += 1;
        if group.sample_lines.len() < SAMPLE_LINES {
            group.sample_lines.push(line_no);
        }
    }

    /// 取序号大于 `after` 的日志条目，最多 `limit` 条。
    ///
    /// 前端按序号增量拉取，避免每次轮询都搬运整个日志。
    pub fn entries_after(&self, after: u64, limit: usize) -> Vec<LogEntry> {
        let g = self.inner.lock();
        g.entries
            .iter()
            .filter(|e| e.seq >= after)
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn error_groups(&self) -> Vec<ErrorGroup> {
        let g = self.inner.lock();
        let mut v: Vec<_> = g.groups.values().cloned().collect();
        v.sort_by(|a, b| b.count.cmp(&a.count));
        v
    }

    pub fn total_parse_errors(&self) -> u64 {
        self.inner.lock().groups.values().map(|g| g.count).sum()
    }

    pub fn clear(&self) {
        let mut g = self.inner.lock();
        g.entries.clear();
        g.groups.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_errors_by_kind_instead_of_per_line() {
        let sink = LogSink::default();
        for line in 1..=100_000u32 {
            sink.parse_error(ParseErrorKind::OddHexDigits, line);
        }
        sink.parse_error(ParseErrorKind::EmptyData, 7);

        // 十万次错误只产生两个聚合组，日志条目一条都没有
        let groups = sink.error_groups();
        assert_eq!(groups.len(), 2);
        assert!(sink.entries_after(0, 10).is_empty());

        assert_eq!(groups[0].kind, ParseErrorKind::OddHexDigits);
        assert_eq!(groups[0].count, 100_000);
        assert_eq!(groups[0].sample_lines.len(), SAMPLE_LINES);
        assert_eq!(groups[0].sample_lines[0], 1);

        assert_eq!(sink.total_parse_errors(), 100_001);
    }

    #[test]
    fn entries_are_capped_and_drop_oldest() {
        let sink = LogSink::default();
        for i in 0..MAX_ENTRIES + 500 {
            sink.info(format!("第 {i} 条"));
        }
        let all = sink.entries_after(0, usize::MAX);
        assert_eq!(all.len(), MAX_ENTRIES);
        // 最旧的被淘汰，序号从 500 开始
        assert_eq!(all[0].seq, 500);
    }

    #[test]
    fn incremental_fetch_by_sequence() {
        let sink = LogSink::default();
        sink.info("一");
        sink.warn("二");
        sink.error("三");

        let first = sink.entries_after(0, 10);
        assert_eq!(first.len(), 3);

        let next = sink.entries_after(first.last().unwrap().seq + 1, 10);
        assert!(next.is_empty());

        sink.info("四");
        let more = sink.entries_after(3, 10);
        assert_eq!(more.len(), 1);
        assert_eq!(more[0].text, "四");
    }

    #[test]
    fn levels_are_preserved() {
        let sink = LogSink::default();
        sink.info("i");
        sink.warn("w");
        sink.error("e");
        let all = sink.entries_after(0, 10);
        assert_eq!(all[0].level, Level::Info);
        assert_eq!(all[1].level, Level::Warn);
        assert_eq!(all[2].level, Level::Error);
    }
}
