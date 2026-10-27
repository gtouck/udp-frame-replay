/** 先运行 npm run dev，再执行 node scripts/check-data-views.mjs [浏览器通道]。 */
import assert from "node:assert/strict";
import { chromium } from "playwright";

const browser = await chromium.launch({ channel: process.argv[2] ?? "msedge" });
try {
  const context = await browser.newContext({
    viewport: { width: 1400, height: 900 },
    permissions: ["clipboard-read", "clipboard-write"],
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("http://127.0.0.1:1420/dev-preview.html", { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "打开文件", exact: true }).click();
  await page.locator(".seg-data").first().waitFor();

  const clipboardEquals = (expected) => page.waitForFunction(
    async (text) => (await navigator.clipboard.readText()) === text,
    expected,
  );
  const longHex = Array.from({ length: 2048 }, (_, i) => (i % 256).toString(16).toUpperCase().padStart(2, "0")).join(" ");
  const source = page.locator(".rows .row").nth(4);
  assert.equal((await source.locator(".seg-data").textContent()).length, 4096);
  await source.click({ button: "right", position: { x: 110, y: 10 } });
  assert.equal(await source.getAttribute("data-selected"), "true");
  await page.getByRole("menuitem", { name: "复制选中行" }).click();
  await clipboardEquals(`[TX] 000128 发送 ${longHex}  # 长帧`);

  // 在窗口右下角弹出菜单，仍应完整可见，Esc 可关闭。
  await source.evaluate((el) => el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 1399, clientY: 899 })));
  const box = await page.getByRole("menu").boundingBox();
  assert.ok(box.x >= 0 && box.y >= 0 && box.x + box.width <= 1400 && box.y + box.height <= 900, JSON.stringify(box));
  await page.keyboard.press("Escape");
  assert.equal(await page.getByRole("menu").count(), 0);

  await page.evaluate(async () => {
    const { useStore } = await import("/src/store.ts");
    useStore.getState().addMutationRule("checksum");
  });
  await page.getByRole("button", { name: "开始发送", exact: true }).click();
  await page.locator(".dump-expand").first().waitFor();
  await page.getByRole("button", { name: "暂停", exact: true }).click();
  await page.getByRole("button", { name: "继续", exact: true }).waitFor();

  const row = page.locator(".dump-row").filter({ has: page.locator(".dump-expand") }).last();
  const id = Number(await row.locator(".dump-line").textContent());
  const frame = await page.evaluate((id) => window.__TAURI_INTERNALS__.invoke("sent_frame", { id }), id);
  const expectedHex = frame.bytes.map((b) => b.toString(16).toUpperCase().padStart(2, "0")).join(" ");
  assert.equal(frame.len, 2052);
  await row.click({ button: "right", position: { x: 110, y: 10 } });
  await page.getByRole("menuitem", { name: "复制选中行" }).click();
  await clipboardEquals(expectedHex);

  // 通过右键详情入口打开，核对末尾数据和超出摘要范围的高亮。
  await row.click({ button: "right", position: { x: 110, y: 10 } });
  await page.getByRole("menuitem", { name: "查看完整数据" }).click();
  const detail = page.getByRole("dialog", { name: /发送数据/ });
  await detail.waitFor();
  await detail.locator(".frame-row").first().waitFor();
  assert.ok(await detail.locator(".frame-row").count() < 64, "长帧应使用虚拟滚动");
  await detail.locator(".overlay-scroll-viewport").evaluate((el) => { el.scrollTop = el.scrollHeight; });
  await detail.locator(".frame-offset").filter({ hasText: /^0800$/ }).waitFor();
  assert.equal(await detail.locator(".frame-row").last().locator(".dump-hex").textContent(), "FE FF 1D 0F");
  assert.equal(await detail.locator(".frame-row").last().locator('[data-mark="computed"]').count(), 2);
  assert.equal(await detail.locator(".frame-body").evaluate((el) => getComputedStyle(el).userSelect), "text");
  await detail.getByRole("button", { name: "复制完整数据" }).click();
  await clipboardEquals(expectedHex);
  await page.screenshot({ path: "dist/data-views-dark.png" });
  await page.evaluate(() => { document.documentElement.dataset.theme = "light"; });
  await page.screenshot({ path: "dist/data-views-light.png" });
  await page.keyboard.press("Escape");
  assert.equal(await detail.count(), 0);

  // 停止后仍可读完整帧，并覆盖 Clipboard API 不可用时的兼容复制。
  await page.getByRole("button", { name: "停止", exact: true }).click();
  await page.getByRole("button", { name: "开始发送", exact: true }).waitFor();
  await page.evaluate(async () => {
    const { useStore } = await import("/src/store.ts");
    const frames = await window.__TAURI_INTERNALS__.invoke("recent_frames", { limit: 60 });
    useStore.getState().setFrames(frames);
  });
  await row.locator(".dump-expand").click();
  await detail.waitFor();
  await page.evaluate(async () => {
    await navigator.clipboard.writeText("待验证兼容复制");
    Object.defineProperty(navigator.clipboard, "writeText", {
      configurable: true,
      value: async () => { throw new Error("模拟 WebView 禁用 Clipboard API"); },
    });
  });
  await detail.getByRole("button", { name: "复制完整数据" }).click();
  await clipboardEquals(expectedHex);
  await detail.getByRole("button", { name: "关闭", exact: true }).click();
  await row.dblclick({ position: { x: 110, y: 10 } });
  await detail.waitFor();
  await page.keyboard.press("Escape");

  await page.evaluate(() => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = (cmd, args) => cmd === "sent_frame"
      ? Promise.reject("该帧已移出缓存，请暂停发送后重新选择")
      : invoke(cmd, args);
  });
  await row.click({ button: "right", position: { x: 110, y: 10 } });
  await page.getByRole("menuitem", { name: "复制选中行" }).click();
  await page.getByText("该帧已移出缓存，请暂停发送后重新选择", { exact: true }).waitFor();
  assert.equal(await detail.count(), 0);
  assert.deepEqual(errors, []);
  console.log("通过：完整原文复制、发送帧复制、停止后查看、兼容复制、详情入口、末尾字节和高亮、虚拟滚动、菜单边界、Esc 和缓存过期提示。");
} finally {
  await browser.close();
}
