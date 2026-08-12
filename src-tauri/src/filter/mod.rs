//! 筛选规则：决定一行要不要发出去。
//!
//! 规则在开始发送时**编译**一次 —— 十六进制文本转成字节、掩码校验都在这一步做完，
//! 之后每帧的判定只剩下比较，没有字符串解析。

pub mod condition;

pub use condition::{CompiledFilter, FilterError};
