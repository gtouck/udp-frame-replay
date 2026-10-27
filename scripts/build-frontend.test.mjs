import assert from "node:assert/strict";
import { test } from "node:test";
import {
  existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, rmdirSync,
  statSync, symlinkSync, utimesSync, writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { syncBuildOutput } from "./build-frontend.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const temporaryRoot = join(root, "src-tauri/target/frontend-tests");
mkdirSync(temporaryRoot, { recursive: true });

const fixture = (t) => {
  const directory = mkdtempSync(join(temporaryRoot, "case-"));
  t.after(() => {
    assert.equal(dirname(resolve(directory)), temporaryRoot);
    rmSync(directory, { recursive: true, force: true });
  });
  const source = join(directory, "source");
  const output = join(directory, "output");
  mkdirSync(source);
  mkdirSync(output);
  return { source, output, directory };
};

test("相同产物保留原时间戳，缺失或损坏产物可修复", (t) => {
  const { source, output } = fixture(t);
  writeFileSync(join(source, "index.html"), "页面");
  writeFileSync(join(source, "app.js"), "脚本");
  assert.equal(syncBuildOutput(source, output).updated, 2);
  utimesSync(join(output, "app.js"), 1000000000, 1000000000);
  const before = statSync(join(output, "app.js")).mtimeMs;
  assert.deepEqual(syncBuildOutput(source, output), { updated: 0, reused: 2, removed: 0 });
  assert.equal(statSync(join(output, "app.js")).mtimeMs, before);
  writeFileSync(join(output, "app.js"), "损坏");
  rmSync(join(output, "index.html"));
  assert.equal(syncBuildOutput(source, output).updated, 2);
  assert.equal(readFileSync(join(output, "app.js"), "utf8"), "脚本");
});

test("真实变化更新资源和入口，并清除旧哈希资源", (t) => {
  const { source, output } = fixture(t);
  mkdirSync(join(source, "assets"));
  mkdirSync(join(output, "assets"));
  writeFileSync(join(source, "index.html"), "引用 new.js");
  writeFileSync(join(source, "assets/new.js"), "新资源");
  writeFileSync(join(output, "index.html"), "引用 old.js");
  writeFileSync(join(output, "assets/old.js"), "旧资源");
  assert.deepEqual(syncBuildOutput(source, output), { updated: 2, reused: 0, removed: 1 });
  assert.equal(readFileSync(join(output, "index.html"), "utf8"), "引用 new.js");
  assert.equal(existsSync(join(output, "assets/old.js")), false);
});

test("不完整构建和非生成目录不会覆盖现有文件", (t) => {
  const { source, output } = fixture(t);
  writeFileSync(join(output, "index.html"), "可用页面");
  assert.throws(() => syncBuildOutput(source, output), /缺少 index.html/);
  assert.equal(readFileSync(join(output, "index.html"), "utf8"), "可用页面");
  assert.throws(() => syncBuildOutput(source, join(root, "src")), /生成目录之外/);
});

test("拒绝目标目录中的链接，防止越界写入或清理", (t) => {
  const { source, output, directory } = fixture(t);
  const target = join(directory, "keep");
  const link = join(output, "assets");
  mkdirSync(target);
  writeFileSync(join(target, "keep.txt"), "保留");
  writeFileSync(join(source, "index.html"), "新页面");
  symlinkSync(target, link, process.platform === "win32" ? "junction" : "dir");
  try {
    assert.throws(() => syncBuildOutput(source, output), /不能包含链接/);
    assert.equal(readFileSync(join(target, "keep.txt"), "utf8"), "保留");
  } finally {
    if (process.platform === "win32") rmdirSync(link);
    else rmSync(link);
  }
});
