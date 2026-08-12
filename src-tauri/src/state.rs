//! 应用全局状态。

use parking_lot::RwLock;

use crate::source::DataSource;

#[derive(Default)]
pub struct AppState {
    pub source: RwLock<Option<DataSource>>,
}
