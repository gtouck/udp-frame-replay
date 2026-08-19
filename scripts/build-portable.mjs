#!/usr/bin/env node
/**
 * 打免安装压缩包。
 *
 * `npm run tauri build` 出的是 MSI / NSIS 安装程序 —— 内网分发要走安装、要管理员权限、
 * 装完还散在几个目录里。这个脚本走另一条路：跳过打包器，只取那个自包含的 exe，
 * 连同说明和示例数据摆成一个文件夹，压成 zip。解压即用，删掉文件夹就算卸载干净。
 *
 * 配置不会跑到用户目录去 —— 那是程序里 `portable.rs` 保证的，不是这里。
 *
 * 用法：
 *   node scripts/build-portable.mjs               # 编译并打包
 *   node scripts/build-portable.mjs --skip-build  # 复用已有的 release 产物
 *   node scripts/build-portable.mjs -- --target x86_64-pc-windows-msvc
 *                                                 # `--` 之后的参数原样转给 tauri build
 */

import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const isWindows = process.platform === "win32";

// ── 参数 ────────────────────────────────────────────────────

const argv = process.argv.slice(2);
const skipBuild = argv.includes("--skip-build");
const passthrough = argv.includes("--") ? argv.slice(argv.indexOf("--") + 1) : [];

// ── 工具 ────────────────────────────────────────────────────

const log = (msg) => console.log(`\x1b[36m▸\x1b[0m ${msg}`);

/** 跑一条命令，失败就带着退出码停下 —— 半成品压缩包比没有压缩包更坑人。 */
function run(cmd, args, { cwd = root, shell = isWindows } = {}) {
  log(`${cmd} ${args.join(" ")}`);
  const r = spawnSync(cmd, args, { cwd, stdio: "inherit", shell });
  if (r.status !== 0) {
    console.error(`\n\x1b[31m✗\x1b[0m 命令失败（退出码 ${r.status ?? "信号 " + r.signal}）：${cmd}`);
    process.exit(r.status || 1);
  }
}

/**
 * cargo 的产物目录。
 *
 * 不能假定是 `src-tauri/target` —— `CARGO_TARGET_DIR` 或 `.cargo/config.toml`
 * 都可能把它挪到别处（本机就挪了），写死路径的结果是编译成功却找不到 exe。
 */
function targetDir() {
  const r = spawnSync("cargo", ["metadata", "--no-deps", "--format-version", "1"], {
    cwd: join(root, "src-tauri"),
    encoding: "utf8",
    shell: isWindows,
  });
  if (r.status === 0) {
    try {
      return JSON.parse(r.stdout).target_directory;
    } catch {
      /* 落到下面的默认值 */
    }
  }
  return join(root, "src-tauri/target");
}

const humanSize = (bytes) => `${(bytes / 1024 / 1024).toFixed(1)} MB`;

/**
 * 递归拷贝目录。
 *
 * 不用 `fs.cpSync` 是因为它在 Node 22 的 Windows 上会崩：目标路径里带中文
 * 且长到一定程度时直接 access violation（退出码 0xC0000005），
 * 而这个压缩包的目录名恰好就是「产品名（中文）+ 版本 + 平台」。
 * copyFileSync 没这个毛病。
 */
function copyDir(src, dest) {
  mkdirSync(dest, { recursive: true });
  for (const entry of readdirSync(src, { withFileTypes: true })) {
    const from = join(src, entry.name);
    const to = join(dest, entry.name);
    if (entry.isDirectory()) copyDir(from, to);
    else copyFileSync(from, to);
  }
}

// ── 读配置 ──────────────────────────────────────────────────

const tauriConf = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
const cargoName = /^name\s*=\s*"(.+)"/m.exec(
  readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8"),
)?.[1];

const productName = tauriConf.productName;
const version = tauriConf.version;
/** Tauri 构建完会把 cargo 产物改名成 productName，改名前的名字留作后备。 */
const binaryNames = [tauriConf.mainBinaryName, productName, cargoName].filter(Boolean);

/** 压缩包名里的平台标识，让人一眼看出这包该给谁 */
const platform = { win32: "windows", darwin: "macos", linux: "linux" }[process.platform] ?? process.platform;
const arch = process.arch === "x64" ? "x64" : process.arch;

const stageName = `${productName}-v${version}-${platform}-${arch}`;
const outDir = join(root, "release");
const stageDir = join(outDir, stageName);
const zipPath = join(outDir, `${stageName}.zip`);

// ── 编译 ────────────────────────────────────────────────────

if (skipBuild) {
  log("跳过编译，复用已有的 release 产物");
} else {
  // --no-bundle：只要那个二进制，不生成 MSI / NSIS / DMG。
  // 前端构建由 tauri.conf.json 的 beforeBuildCommand 带起来，这里不必重复。
  run("npm", ["run", "tauri", "build", "--", "--no-bundle", ...passthrough]);
}

// ── 找二进制 ────────────────────────────────────────────────

const ext = isWindows ? ".exe" : "";
// --target 会让产物多一层目录，这里跟着找
const triple = passthrough.includes("--target") ? passthrough[passthrough.indexOf("--target") + 1] : "";
const releaseDir = join(targetDir(), triple ?? "", "release");

const binary = binaryNames.map((n) => join(releaseDir, n + ext)).find(existsSync);
if (!binary) {
  console.error(
    `\x1b[31m✗\x1b[0m 没找到可执行文件，试过：\n` +
      binaryNames.map((n) => `    ${join(releaseDir, n + ext)}`).join("\n") +
      (skipBuild ? "\n  用了 --skip-build —— 先跑一次完整构建试试。" : ""),
  );
  process.exit(1);
}

// ── 摆文件 ──────────────────────────────────────────────────

log(`整理 ${relative(root, stageDir)}`);
rmSync(stageDir, { recursive: true, force: true });
mkdirSync(stageDir, { recursive: true });

copyFileSync(binary, join(stageDir, `${productName}${ext}`));
copyFileSync(join(root, "docs/USER_MANUAL.md"), join(stageDir, "使用说明.md"));
copyDir(join(root, "testdata"), join(stageDir, "示例数据"));

writeFileSync(
  join(stageDir, "便携版说明.txt"),
  `${productName} v${version}（免安装版）

怎么用
  双击 ${productName}${ext} 就行，不需要安装。
  整个文件夹可以随便挪位置、拷到 U 盘带走。

东西都放在哪
  配置、窗口状态等自动记忆    本文件夹下的 data 目录（首次运行时自动建）
  手动保存的配置档            默认存到本文件夹，也可以另存到别处
  示例数据                    示例数据 目录，可以拿来试手

  程序不往系统目录、注册表里写任何东西。不想要了直接删掉整个文件夹即可。

  提示：放到 C:\\Program Files 这类需要管理员权限的位置时，本文件夹不可写，
  配置会退回存到系统的用户目录 —— 想保持便携就放在普通目录下。

运行环境
  Windows 10 1803 及以上。
  依赖系统的 WebView2 运行时：Windows 11 自带；Windows 10 若提示缺少，
  到 https://developer.microsoft.com/microsoft-edge/webview2/ 装一次即可。

详细用法见 使用说明.md。
`.replace(/\n/g, "\r\n"),
);

// ── 压缩 ────────────────────────────────────────────────────

rmSync(zipPath, { force: true });
log(`压缩 ${relative(root, zipPath)}`);

if (isWindows) {
  // 走 .NET 的 ZipFile，不用自带的 tar：bsdtar 写 zip 时不置 UTF-8 标志位，
  // 文件名的中文到了资源管理器里就是一片乱码（这包里从 exe 到说明全是中文名）。
  // ZipFile 遇到非 ASCII 名字会置上那个标志位。
  const script =
    "Add-Type -AssemblyName System.IO.Compression.FileSystem; " +
    `[System.IO.Compression.ZipFile]::CreateFromDirectory('${stageDir}','${zipPath}','Optimal',$true)`;
  // 优先 pwsh（.NET Core）：它写的目录分隔符是 zip 规范要求的 `/`，
  // Windows PowerShell 5.1 那套 .NET Framework 写的是 `\`，非 Windows 的解压工具会当成文件名的一部分。
  const shellExe = spawnSync("pwsh", ["-NoProfile", "-Command", "exit 0"], { shell: true }).status === 0
    ? "pwsh"
    : "powershell";
  run(shellExe, ["-NoProfile", "-NonInteractive", "-Command", script], { shell: false });
} else {
  run("zip", ["-r", "-q", `${stageName}.zip`, stageName], { cwd: outDir });
}

// ── 收尾 ────────────────────────────────────────────────────

console.log(
  `\n\x1b[32m✓\x1b[0m ${relative(root, zipPath)}  ${humanSize(statSync(zipPath).size)}\n` +
    `  解压后的目录也留着了：${relative(root, stageDir)}\n`,
);
