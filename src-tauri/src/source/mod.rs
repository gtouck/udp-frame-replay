//! 数据源：内存映射文件 + 行索引 + 编码解码。

pub mod index;
pub mod text;

use std::borrow::Cow;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use memmap2::Mmap;
use serde::Serialize;
use thiserror::Error;

use crate::config::TextEncoding;
use index::LineIndex;

/// 索引用 u32 存偏移，因此文件不能超过 4GB
const MAX_FILE_BYTES: u64 = u32::MAX as u64;

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("打开文件失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("文件为空")]
    Empty,

    #[error("文件过大（{0} 字节），上限 4GB")]
    TooLarge(u64),

    #[error("文件在打开后被修改（大小或修改时间已变），请重新打开")]
    Changed,
}

/// 文件基本信息，供界面展示
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub size_bytes: u64,
    pub line_count: usize,
    pub index_memory_bytes: usize,
}

pub struct DataSource {
    path: PathBuf,
    mmap: Mmap,
    index: LineIndex,
    size: u64,
    mtime: Option<SystemTime>,
}

impl DataSource {
    pub fn open(path: &Path) -> Result<Self, SourceError> {
        let file = File::open(path)?;
        let meta = file.metadata()?;
        let size = meta.len();

        if size == 0 {
            return Err(SourceError::Empty);
        }
        if size > MAX_FILE_BYTES {
            return Err(SourceError::TooLarge(size));
        }

        // SAFETY: 只读映射。文件在映射期间被外部截断会触发 SIGBUS，
        // 因此每次开始发送前都会调用 verify_unchanged() 复检。
        let mmap = unsafe { Mmap::map(&file)? };
        let index = LineIndex::build(&mmap);

        Ok(DataSource {
            path: path.to_path_buf(),
            mmap,
            index,
            size,
            mtime: meta.modified().ok(),
        })
    }

    pub fn info(&self) -> FileInfo {
        FileInfo {
            path: self.path.to_string_lossy().into_owned(),
            size_bytes: self.size,
            line_count: self.index.line_count(),
            index_memory_bytes: self.index.memory_bytes(),
        }
    }

    pub fn line_count(&self) -> usize {
        self.index.line_count()
    }

    /// 第 `i` 行的原始字节（0-based，已剔除行尾换行）
    pub fn line_bytes(&self, i: usize) -> Option<&[u8]> {
        let (a, b) = self.index.line_range(i, &self.mmap)?;
        Some(&self.mmap[a..b])
    }

    /// 第 `i` 行按指定编码解码后的文本。UTF-8 且合法时零拷贝。
    pub fn line_text(&self, i: usize, enc: TextEncoding) -> Option<Cow<'_, str>> {
        self.line_bytes(i).map(|b| text::decode(b, enc))
    }

    /// 复检文件是否仍与打开时一致。
    ///
    /// mmap 期间文件被外部截断会导致访问映射区触发 SIGBUS，进程直接崩溃，
    /// 无法用常规错误处理兜住 —— 只能在开始发送前主动检查。
    pub fn verify_unchanged(&self) -> Result<(), SourceError> {
        let meta = std::fs::metadata(&self.path)?;
        if meta.len() != self.size {
            return Err(SourceError::Changed);
        }
        if meta.modified().ok() != self.mtime {
            return Err(SourceError::Changed);
        }
        Ok(())
    }
}
