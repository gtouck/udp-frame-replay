//! 应用全局状态。

use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

use crate::engine::Engine;
use crate::log::LogSink;
use crate::source::DataSource;

#[derive(Default)]
pub struct AppState {
    /// 用 `Arc` 持有：发送引擎需要在自己的线程上一直用着这份映射，
    /// 即使界面上把文件关掉了，正在跑的任务也不该被抽掉底下的数据。
    pub source: RwLock<Option<Arc<DataSource>>>,
    pub engine: Mutex<Option<Engine>>,
    pub log: Arc<LogSink>,
}
