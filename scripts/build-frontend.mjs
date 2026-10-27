/** 发布构建先写暂存目录，仅同步内容有变化的文件，保留 Cargo 依赖的 mtime。 */
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmdirSync,
  unlinkSync,
} from "node:fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), ".."));
const generatedRoot = join(root, "src-tauri/target");
const dist = join(root, "dist");

const isInside = (parent, path) => {
  const part = relative(parent, path);
  return part !== "" && part !== ".." && !part.startsWith(`..${sep}`) && !isAbsolute(part);
};

/** 只操作明确的生成目录；拒绝目录链接，防止清理越过项目边界。 */
function checkOutputPath(path) {
  const absolute = resolve(path);
  if (absolute !== dist && !isInside(generatedRoot, absolute)) {
    throw new Error(`拒绝操作生成目录之外的路径：${absolute}`);
  }
  for (let at = absolute; at !== root; at = dirname(at)) {
    if (lstatSync(at, { throwIfNoEntry: false })?.isSymbolicLink()) {
      throw new Error(`生成路径不能包含目录链接：${at}`);
    }
  }
  return absolute;
}

function listFiles(directory, prefix = "") {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const name = join(prefix, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`生成目录不能包含链接：${join(directory, entry.name)}`);
    return entry.isDirectory()
      ? listFiles(join(directory, entry.name), name)
      : [name];
  });
}

/** 构建成功后才发布；先补齐资源，再更新入口，最后清理旧哈希文件。 */
export function syncBuildOutput(source, destination = dist) {
  const output = checkOutputPath(destination);
  const files = listFiles(source);
  if (!files.includes("index.html")) throw new Error("前端构建缺少 index.html，保留原有产物");
  const previous = listFiles(output);
  const expected = new Set(files);
  const ordered = files.filter((name) => name !== "index.html").concat("index.html");
  let updated = 0;
  for (const name of ordered) {
    const from = join(source, name);
    const to = join(output, name);
    if (existsSync(to) && readFileSync(from).equals(readFileSync(to))) continue;
    mkdirSync(dirname(to), { recursive: true });
    copyFileSync(from, to);
    updated++;
  }
  const stale = previous.filter((name) => !expected.has(name));
  for (const name of stale) unlinkSync(join(output, name));
  return { updated, reused: files.length - updated, removed: stale.length };
}

/** 文件逐个删除，空目录最后删除；不会递归跟随目录链接。 */
function removeStaging(directory) {
  const safe = checkOutputPath(directory);
  for (const entry of readdirSync(safe, { withFileTypes: true })) {
    const path = join(safe, entry.name);
    if (entry.isDirectory() && !entry.isSymbolicLink()) removeStaging(path);
    else unlinkSync(path);
  }
  rmdirSync(safe);
}

async function main() {
  const check = spawnSync(process.execPath, [join(root, "node_modules/typescript/bin/tsc")], {
    cwd: root,
    stdio: "inherit",
  });
  if (check.status !== 0) throw new Error(`类型检查失败（退出码 ${check.status}），保留原有 dist`);
  const temporaryRoot = checkOutputPath(join(generatedRoot, "frontend-build"));
  mkdirSync(temporaryRoot, { recursive: true });
  const staging = mkdtempSync(join(temporaryRoot, "stage-"));
  try {
    const { build } = await import("vite");
    await build({ root, build: { outDir: staging, emptyOutDir: true } });
    const result = syncBuildOutput(staging);
    console.log(`前端产物：更新 ${result.updated}，复用 ${result.reused}，清理 ${result.removed}`);
  } finally {
    removeStaging(staging);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
