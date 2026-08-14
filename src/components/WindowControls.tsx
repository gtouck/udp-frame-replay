import { getCurrentWindow } from "@tauri-apps/api/window";

type WindowAction = "close" | "maximize" | "minimize";

const isMac = /Macintosh|Mac OS X/.test(navigator.userAgent);

/**
 * 无边框窗口的系统操作区。保持原生按钮的摆放习惯，但视觉上属于应用顶栏。
 */
export default function WindowControls() {
  const run = (action: WindowAction) => {
    const appWindow = getCurrentWindow();
    const command = {
      close: () => appWindow.close(),
      maximize: () => appWindow.toggleMaximize(),
      minimize: () => appWindow.minimize(),
    }[action];

    void command();
  };

  const controls: Array<{ action: WindowAction; label: string }> = isMac
    ? [
        { action: "close", label: "关闭窗口" },
        { action: "minimize", label: "最小化窗口" },
        { action: "maximize", label: "最大化或还原窗口" },
      ]
    : [
        { action: "minimize", label: "最小化窗口" },
        { action: "maximize", label: "最大化或还原窗口" },
        { action: "close", label: "关闭窗口" },
      ];

  return (
    <div className="window-controls" data-platform={isMac ? "mac" : "desktop"}>
      {controls.map(({ action, label }) => (
        <button
          key={action}
          className="window-control"
          data-action={action}
          type="button"
          aria-label={label}
          title={label}
          onClick={() => run(action)}
        >
          <span className="window-control-icon" aria-hidden />
        </button>
      ))}
    </div>
  );
}
