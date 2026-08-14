export type Theme = "dark" | "light";

export const THEME_CHANGE_EVENT = "app-theme-change";

const STORAGE_KEY = "udp-frame-replay.theme";

export function readTheme(): Theme {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === "light" || stored === "dark" ? stored : "dark";
  } catch {
    return "dark";
  }
}

export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  window.dispatchEvent(new Event(THEME_CHANGE_EVENT));
}

export function persistTheme(theme: Theme) {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // WebView 禁用存储时仍允许本次会话切换主题。
  }
}
