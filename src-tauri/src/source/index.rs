//! 行索引。单遍扫描建立每行起始偏移，之后按行号 O(1) 定位。
//!
//! 用 `u32` 存偏移：文件上限 4GB，每行 256 字节的 1GB 文件约 400 万行，索引约 16MB。

/// 行起始偏移表。长度为 `行数 + 1`，末位是文件长度哨兵。
pub struct LineIndex {
    starts: Vec<u32>,
}

impl LineIndex {
    /// 单遍扫描建立索引。以 `\n` 分行，文件末尾的换行不产生空行。
    pub fn build(data: &[u8]) -> Self {
        let mut starts: Vec<u32> = Vec::new();

        if !data.is_empty() {
            starts.push(0);
            // 预估行数，减少扩容次数
            starts.reserve(data.len() / 128);

            let mut pos = 0usize;
            while let Some(rel) = memchr::memchr(b'\n', &data[pos..]) {
                let nl = pos + rel;
                pos = nl + 1;
                if pos < data.len() {
                    starts.push(pos as u32);
                } else {
                    break; // 末尾换行，不产生空行
                }
            }
        }

        starts.push(data.len() as u32); // 哨兵
        LineIndex { starts }
    }

    pub fn line_count(&self) -> usize {
        self.starts.len() - 1
    }

    /// 第 `i` 行（0-based）的字节区间，已剔除行尾的 `\r\n` / `\n`。
    pub fn line_range(&self, i: usize, data: &[u8]) -> Option<(usize, usize)> {
        if i >= self.line_count() {
            return None;
        }
        let start = self.starts[i] as usize;
        let mut end = self.starts[i + 1] as usize;

        if end > start && data[end - 1] == b'\n' {
            end -= 1;
        }
        if end > start && data[end - 1] == b'\r' {
            end -= 1;
        }
        Some((start, end))
    }

    /// 索引自身占用的内存字节数，用于界面展示
    pub fn memory_bytes(&self) -> usize {
        self.starts.capacity() * std::mem::size_of::<u32>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(s: &str) -> Vec<&str> {
        let data = s.as_bytes();
        let idx = LineIndex::build(data);
        (0..idx.line_count())
            .map(|i| {
                let (a, b) = idx.line_range(i, data).unwrap();
                std::str::from_utf8(&data[a..b]).unwrap()
            })
            .collect()
    }

    #[test]
    fn empty_file_has_no_lines() {
        assert_eq!(LineIndex::build(b"").line_count(), 0);
    }

    #[test]
    fn single_line_without_newline() {
        assert_eq!(ranges("01AA"), vec!["01AA"]);
    }

    #[test]
    fn trailing_newline_does_not_add_empty_line() {
        assert_eq!(ranges("a\nb\n"), vec!["a", "b"]);
    }

    #[test]
    fn missing_trailing_newline_keeps_last_line() {
        assert_eq!(ranges("a\nb"), vec!["a", "b"]);
    }

    #[test]
    fn strips_crlf() {
        assert_eq!(ranges("a\r\nb\r\n"), vec!["a", "b"]);
    }

    #[test]
    fn preserves_empty_interior_lines() {
        assert_eq!(ranges("a\n\nb\n"), vec!["a", "", "b"]);
    }

    #[test]
    fn out_of_range_returns_none() {
        let data = b"a\nb\n";
        let idx = LineIndex::build(data);
        assert!(idx.line_range(2, data).is_none());
    }
}
