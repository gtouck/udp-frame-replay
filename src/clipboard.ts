/** WebView 未开放 Clipboard API 时，退回同一次用户操作中的文本复制。 */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    const focused = document.activeElement;
    const input = document.createElement("textarea");
    input.value = text;
    input.readOnly = true;
    input.style.cssText = "position:fixed;left:-9999px;top:0";
    document.body.appendChild(input);
    try {
      input.select();
      if (!document.execCommand("copy")) throw new Error("无法写入剪贴板，请重试");
    } finally {
      input.remove();
      if (focused instanceof HTMLElement) focused.focus({ preventScroll: true });
    }
  }
}
