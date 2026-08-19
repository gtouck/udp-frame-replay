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

## 免安装压缩包

```bash
npm run build:portable
```

产物在 `release/`：一个 `产品名-v版本-平台-架构.zip`，外加解压后的同名目录（方便直接跑一下看看）。
里面是 exe、`使用说明.md`、`示例数据/`、`便携版说明.txt` —— 解压即用，不需要安装程序。

常用参数：

```bash
npm run build:portable -- --skip-build                          # 复用已有的 release 产物，只重新打包
npm run build:portable -- -- --target x86_64-pc-windows-msvc    # 第二个 -- 之后原样转给 tauri build
```

脚本做的事：`tauri build --no-bundle`（只编译，不生成 MSI/NSIS）→ 摆好目录 → 压缩。
target 目录问 `cargo metadata` 要，不写死 `src-tauri/target`（`CARGO_TARGET_DIR` 可能把它挪走）。

### 配置为什么不会跑到用户目录去

免安装的前提是「删掉文件夹就等于卸载干净」，这不是打包脚本能保证的，靠的是 `src-tauri/src/portable.rs`：

- Tauri 在 Windows/Linux 上默认把 webview 数据目录指向 `%LOCALAPPDATA%\<identifier>`，
  自动记忆的配置存在 localStorage 里，也就一并落到了用户目录。
- 所以窗口改成在 `lib.rs` 的 `setup` 里建（`tauri.conf.json` 里那个窗口配了 `"create": false`），
  建的时候把 `data_directory` 指到 `<程序目录>/data`。
- 程序目录不可写时（装进了 `Program Files`、或从只读介质运行）退回系统目录：
  便携性没了，但配置还记得住，比启动直接失败强。
- 手动存的配置档，存取对话框的默认位置也是程序所在目录（`app_dir` 命令）。

### 打包脚本里两个 Windows 的坑

改这个脚本时别踩回去：

- **不能用 `fs.cpSync` 拷目录**。Node 22 在 Windows 上，目标路径带中文且长到一定程度时它会直接
  access violation（退出码 `0xC0000005`），而压缩包的目录名恰好是「中文产品名 + 版本 + 平台」。
  脚本里用 `copyFileSync` 自己递归。
- **不能用自带的 `tar` 压 zip**。bsdtar 不置 UTF-8 标志位，中文文件名到资源管理器里就是乱码。
  脚本走 .NET 的 `ZipFile`，并且优先用 `pwsh` —— Windows PowerShell 5.1 那套 .NET Framework
  写出来的目录分隔符是 `\`，不合 zip 规范。

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

### macOS 的本地网络权限

macOS 15 起有「本地网络隐私」：未授权的 App 发往同一局域网的流量**全部**被拦，
组播、广播、局域网单播一律返回 `EHOSTUNREACH`（errno 65）。能上公网却发不到
网关，就是这个原因 —— 不是路由问题，也不是代码问题。

`src-tauri/Info.plist` 声明了 `NSLocalNetworkUsageDescription`，Tauri 打包时会
自动发现 `tauri.conf.json` 同目录下的这个文件并合并（不需要在 `tauri.conf.json`
里配置）。有了它，系统才会弹授权框，App 才会出现在
系统设置 → 隐私与安全性 → 本地网络 的列表里。

验证是否合并成功：

```bash
plutil -p "src-tauri/target/release/bundle/macos/数据帧回放工具.app/Contents/Info.plist" \
  | grep NSLocalNetwork
```

两个坑：

- **`tauri dev` 下拿不到这个权限**。dev 模式跑的是裸二进制，没有 app bundle
  也就没有 Info.plist。要验证组播必须 `tauri build` 后运行 `.app`。
- **ad-hoc 签名的 App 权限不持久**。默认构建出来是 `Signature=adhoc`
  （`codesign -dvvv` 可查），系统按 cdhash 认 App，每次重新构建都会变，
  已授予的权限随之失效。要权限稳定就得用 Developer ID 证书签名，见下面「签名」。

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
