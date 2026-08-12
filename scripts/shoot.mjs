/**
 * 界面截图。用 dev-preview.html（内含 Tauri IPC 桩件）在无头浏览器里
 * 跑真实前端代码，不需要桌面截图权限，也不用每次重编 Rust。
 *
 *   node scripts/shoot.mjs <输出路径> [场景]
 *
 * 场景：idle（仅打开文件）| sending（开始发送后）| paused
 */
import { chromium } from "playwright";

const out = process.argv[2] ?? "shot.png";
const scene = process.argv[3] ?? "sending";

const browser = await chromium.launch();
const ctx = await browser.newContext({
  viewport: { width: 1400, height: 900 },
  deviceScaleFactor: 2,
});
const p = await ctx.newPage();

const problems = [];
p.on("console", (m) => {
  if (m.type() === "error") problems.push(`console: ${m.text()}`);
});
p.on("pageerror", (e) => problems.push(`pageerror: ${e.message}`));

await p.goto("http://localhost:1420/dev-preview.html", {
  waitUntil: "networkidle",
});

await p.getByRole("button", { name: "打开文件" }).click();
await p.waitForTimeout(400);

if (scene !== "idle") {
  await p.getByRole("button", { name: "开始发送" }).click();
  await p.waitForTimeout(1200); // 让计数器和时序带跑起来
}
if (scene === "paused") {
  await p.getByRole("button", { name: "暂停" }).click();
  await p.waitForTimeout(300);
}

await p.screenshot({ path: out });
await browser.close();

if (problems.length) {
  console.error("页面报错：\n" + problems.join("\n"));
  process.exit(1);
}
console.log(`已截图 ${out}（场景 ${scene}）`);
