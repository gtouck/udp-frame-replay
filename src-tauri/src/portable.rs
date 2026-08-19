//! 便携模式：可写数据一律落在程序同级目录，不往用户目录里写东西。
//!
//! 「解压即用、删掉文件夹就算卸载干净、拷到 U 盘换台机器配置还在」这三条
//! 全靠这个模块。默认行为不是这样：Tauri 在 Windows/Linux 上会强制把 webview
//! 的数据目录指到 `%LOCALAPPDATA%\<identifier>`（见 tauri 的 manager/webview.rs），
//! 自动记忆的配置存在 localStorage 里，也就一并落到了用户目录。

use std::path::{Path, PathBuf};

/// 可写数据的子目录名。
///
/// 不平铺到 exe 旁边是因为 webview 会在里面铺一层自己的缓存文件（`EBWebView/` 等），
/// 十几个文件混在程序目录里太脏，收进一个目录反而更像「同级目录下的配置」。
const DATA_DIR: &str = "data";

/// 程序所在目录。取不到 exe 路径（几乎不可能）就退回当前工作目录。
pub fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 便携数据目录，即 `<程序目录>/data`。
///
/// 程序目录不可写时返回 `None` —— 被放进 `Program Files`、或从只读介质运行都会这样。
/// 这时交给调用方退回系统目录：便携性没了，但配置还记得住，比启动直接失败强。
pub fn data_dir() -> Option<PathBuf> {
    let dir = app_dir().join(DATA_DIR);
    writable(&dir).then_some(dir)
}

/// 探一次真实写入。只看目录存不存在不够 —— 只读介质上 `create_dir_all`
/// 对已存在的目录同样返回 Ok，真正的失败要到 webview 落盘时才炸。
fn writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_dir_is_absolute() {
        assert!(app_dir().is_absolute());
    }

    #[test]
    fn writable_detects_a_real_directory() {
        assert!(writable(&std::env::temp_dir().join("data-perf-portable-probe")));
    }

    #[test]
    fn writable_rejects_a_path_under_a_file() {
        let file = std::env::temp_dir().join("data-perf-portable-probe.txt");
        std::fs::write(&file, b"x").unwrap();
        assert!(!writable(&file.join("sub")));
        let _ = std::fs::remove_file(&file);
    }
}
