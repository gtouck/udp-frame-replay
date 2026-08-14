import { useState } from "react";
import { applyTheme, persistTheme, readTheme, type Theme } from "../theme";

const nextTheme = (theme: Theme): Theme => (theme === "dark" ? "light" : "dark");

export default function ThemeToggle() {
  const [theme, setTheme] = useState(readTheme);
  const next = nextTheme(theme);
  const label = next === "light" ? "切换到亮色主题" : "切换到深色主题";

  function toggle() {
    applyTheme(next);
    persistTheme(next);
    setTheme(next);
  }

  return (
    <button
      className="theme-toggle"
      type="button"
      aria-label={label}
      title={label}
      onClick={toggle}
    >
      {next === "light" ? (
        <svg viewBox="0 0 20 20" aria-hidden>
          <circle cx="10" cy="10" r="3.25" />
          <path d="M10 1.5v2M10 16.5v2M1.5 10h2M16.5 10h2M4 4l1.4 1.4M14.6 14.6 16 16M16 4l-1.4 1.4M5.4 14.6 4 16" />
        </svg>
      ) : (
        <svg viewBox="0 0 20 20" aria-hidden>
          <path d="M16.8 12.6A7.2 7.2 0 0 1 7.4 3.2a7.2 7.2 0 1 0 9.4 9.4Z" />
        </svg>
      )}
    </button>
  );
}
