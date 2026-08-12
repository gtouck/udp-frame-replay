# 构建与打包

## 依赖

| 工具 | 版本 |
|---|---|
| Rust | 1.77 以上 |
| Node.js | 20 以上 |

各平台还需要 Tauri 的系统依赖，见 <https://tauri.app/start/prerequisites/>。
Linux 上通常是 `webkit2gtk-4.1`、`librsvg2-dev`、`build-essential`；
Windows 上是 WebView2 运行时（Win11 自带）与 MSVC 生成工具。

## 开发

```bash
npm install
npm run tauri dev
```

界面改动也可以不启动 Rust 端单独调：

```bash
npm run dev
# 浏览器打开 http://localhost:1420/dev-preview.html
```

`dev-preview.html` 里装了一套 Tauri IPC 桩件，跑的是 `src/` 下的真实前端代码。
应用代码不为此改动任何一行。

## 测试

```bash
cd src-tauri && cargo test        # 后端：单元 + 端到端
npm run build                     # 前端类型检查 + 构建
```

端到端测试会绑定本机 UDP 端口并真实收发，其中组播回环一项依赖本机允许组播回环。

## 打包

```bash
npm run tauri build
```

产物在 `src-tauri/target/release/bundle/`：

| 平台 | 产物 |
|---|---|
| macOS | `macos/*.app`、`dmg/*.dmg` |
| Windows | `msi/*.msi`、`nsis/*-setup.exe` |
| Linux | `deb/*.deb`、`appimage/*.AppImage`、`rpm/*.rpm` |

### macOS 上的 DMG 打包

`.app` 在任何环境下都能打出来。`.dmg` 那一步会调 `bundle_dmg.sh`，
它用 AppleScript 让 Finder 设置磁盘映像的窗口外观 —— 在没有图形会话
或未授予自动化权限的环境（CI、SSH、容器）里会失败，报：

```
failed to bundle project: error running bundle_dmg.sh
```

这时 `.app` 已经在 `bundle/macos/` 里了，可以直接用或自行压缩分发。
要出 `.dmg` 就在有图形会话的机器上跑，并在
系统设置 → 隐私与安全性 → 自动化 里允许终端控制 Finder。

只要 `.app` 的话可以跳过 DMG：

```bash
npm run tauri build -- --bundles app
```

### 跨平台构建

Tauri 不支持跨平台交叉编译 —— 每个平台的包都要在对应系统上构建。
可行的做法：

- 在三台机器（或虚拟机）上分别执行 `npm run tauri build`
- 用 CI 的多平台矩阵，例如 GitHub Actions 的
  `runs-on: [macos-latest, windows-latest, ubuntu-22.04]`

Apple Silicon 与 Intel 的 macOS 包需要分别构建，或用
`npm run tauri build -- --target universal-apple-darwin` 出通用包。

## 签名

发布给别人用时需要签名，否则 macOS 会拦下来、Windows 会弹 SmartScreen 警告：

- macOS：需要 Apple Developer ID 证书与公证（notarization）
- Windows：需要代码签名证书

配置方式见 <https://tauri.app/distribute/>。自用或内网分发可以跳过。
