import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import {
  closeFile,
  guessParse,
  openFile,
  pauseSend,
  resumeSend,
  startSend,
  stepSend,
  stopSend,
} from "../api";
import { dropRecentFile, pushRecentFile, readRecentFiles } from "../session";
import { configOf, hasBlockingProblem, isActive, useStore } from "../store";
import HelpOverlay from "./HelpOverlay";
import ThemeToggle from "./ThemeToggle";
import WindowControls from "./WindowControls";

const appIcon = new URL("../../src-tauri/icons/32x32.png", import.meta.url).href;

/** 路径拆成目录与文件名两段：目录可以省略，文件名永远完整可见。 */
const cut = (p: string) => Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
const dirOf = (p: string) => (cut(p) < 0 ? "" : p.slice(0, cut(p) + 1));
const baseOf = (p: string) => (cut(p) < 0 ? p : p.slice(cut(p) + 1));

export default function ChromeBar() {
  const file = useStore((s) => s.file);
  const setFile = useStore((s) => s.setFile);
  const setNotice = useStore((s) => s.setNotice);
  const setParse = useStore((s) => s.setParse);
  const engine = useStore((s) => s.engine);
  const parse = useStore((s) => s.parse);
  const problems = useStore((s) => s.problems);

  const [recent, setRecent] = useState(readRecentFiles);
  const [menuOpen, setMenuOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);

  const active = isActive(engine);
  const blocked = hasBlockingProblem(problems);
  const paused = engine?.state === "paused";
  const finished = engine?.state === "finished";

  /**
   * 解析规则还停在「整行都是数据」上，说明使用者根本没配过 ——
   * 这种时候替他推一把；一旦他自己调过，就不再擅自改动。
   */
  const parseUntouched =
    parse.prefix.mode === "fields" && parse.prefix.skipFields === 0;

  async function load(path: string) {
    try {
      setFile(await openFile(path));
      setRecent(pushRecentFile(path));
      setNotice(null);
    } catch (e) {
      setFile(null);
      setRecent(dropRecentFile(path));
      setNotice(String(e));
      return;
    }

    // 推测失败不该影响「文件已经打开」这个事实，所以单独兜错
    if (!parseUntouched) return;
    try {
      const guess = await guessParse(parse);
      if (guess) {
        setParse(guess.config);
        setNotice(guess.summary, "info");
      }
    } catch {
      /* 文件刚被关掉之类，静默放弃 */
    }
  }

  async function pickFile() {
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [
        { name: "数据文件", extensions: ["txt", "log", "dat", "asc", "csv"] },
        { name: "全部文件", extensions: ["*"] },
      ],
    });
    if (typeof picked !== "string") return;
    await load(picked);
  }

  async function unload() {
    await closeFile();
    setFile(null);
    setNotice(null);
  }

  async function start() {
    try {
      await startSend(configOf(useStore.getState()));
      setNotice(null);
    } catch (e) {
      setNotice(String(e));
    }
  }

  return (
    <header className="bar" data-tauri-drag-region>
      <button
        className="help-btn"
        type="button"
        aria-label="快速上手"
        title="快速上手"
        onClick={() => setHelpOpen(true)}
      >
        ?
      </button>
      <ThemeToggle />
      <WindowControls />
      <img
        className="bar-icon"
        src={appIcon}
        alt=""
        draggable={false}
        data-tauri-drag-region
      />
      <span className="bar-title" data-tauri-drag-region>
        数据帧回放
      </span>

      <span
        className="bar-path"
        data-empty={file ? undefined : "true"}
        data-tauri-drag-region
      >
        {file ? (
          <>
            <span className="bar-dir" data-tauri-drag-region>
              {dirOf(file.path)}
            </span>
            <span className="bar-file" data-tauri-drag-region>
              {baseOf(file.path)}
            </span>
          </>
        ) : (
          "未打开文件"
        )}
      </span>

      <div className="bar-group">
        <button className="btn" onClick={pickFile} disabled={active}>
          打开文件
        </button>

        {/* 同一批数据往往要反复回放，省掉每次翻目录 */}
        {recent.length > 0 && (
          <div className="recent">
            <button
              className="btn recent-btn"
              onClick={() => setMenuOpen((o) => !o)}
              disabled={active}
              aria-expanded={menuOpen}
              aria-label="最近打开的文件"
              title="最近打开的文件"
            >
              <svg viewBox="0 0 20 20" aria-hidden>
                <path d="M2.5 10a7.5 7.5 0 1 0 7.5-7.5 8.1 8.1 0 0 0-5.62 2.28L2.5 6.67" />
                <path d="M2.5 2.5v4.17h4.17" />
                <path d="M10 5.83V10l3.33 1.67" />
              </svg>
            </button>

            {menuOpen && (
              <>
                <div className="recent-shade" onClick={() => setMenuOpen(false)} />
                <ul className="recent-menu">
                  {recent.map((p) => (
                    <li key={p}>
                      <button
                        className="recent-item"
                        title={p}
                        onClick={() => {
                          setMenuOpen(false);
                          void load(p);
                        }}
                      >
                        <span className="recent-name">{baseOf(p)}</span>
                        <span className="recent-dir">{dirOf(p)}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        )}

        {file && (
          <button className="btn" onClick={unload} disabled={active}>
            关闭
          </button>
        )}
      </div>

      <span className="bar-sep" aria-hidden />

      <div className="bar-group">
        {!active ? (
          <button
            className="btn"
            data-primary="true"
            onClick={start}
            disabled={!file || blocked}
            title={
              !file
                ? "先打开一个数据文件"
                : blocked
                  ? "左侧配置里还有需要修正的问题"
                  : undefined
            }
          >
            开始发送
          </button>
        ) : paused ? (
          <button className="btn" data-primary="true" onClick={() => resumeSend()}>
            继续
          </button>
        ) : (
          <button className="btn" onClick={() => pauseSend()}>
            暂停
          </button>
        )}

        <button
          className="btn"
          onClick={() => stepSend()}
          disabled={!paused}
          title="暂停时逐帧放行，用来核对规则"
        >
          单步
        </button>

        <button
          className="btn"
          data-danger="true"
          onClick={() => stopSend()}
          disabled={!active && !finished}
        >
          停止
        </button>
      </div>

      {helpOpen && <HelpOverlay onClose={() => setHelpOpen(false)} />}
    </header>
  );
}
