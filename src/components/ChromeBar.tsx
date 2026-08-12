import { open } from "@tauri-apps/plugin-dialog";
import {
  closeFile,
  openFile,
  pauseSend,
  resumeSend,
  startSend,
  stepSend,
  stopSend,
} from "../api";
import { hasBlockingProblem, isActive, useStore } from "../store";

/** 路径拆成目录与文件名两段：目录可以省略，文件名永远完整可见。 */
const cut = (p: string) => Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
const dirOf = (p: string) => (cut(p) < 0 ? "" : p.slice(0, cut(p) + 1));
const baseOf = (p: string) => (cut(p) < 0 ? p : p.slice(cut(p) + 1));

export default function ChromeBar() {
  const file = useStore((s) => s.file);
  const setFile = useStore((s) => s.setFile);
  const setNotice = useStore((s) => s.setNotice);
  const engine = useStore((s) => s.engine);
  const parse = useStore((s) => s.parse);
  const filter = useStore((s) => s.filter);
  const mutate = useStore((s) => s.mutate);
  const target = useStore((s) => s.target);
  const pacing = useStore((s) => s.pacing);
  const problems = useStore((s) => s.problems);

  const active = isActive(engine);
  const blocked = hasBlockingProblem(problems);
  const paused = engine?.state === "paused";
  const finished = engine?.state === "finished";

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

    try {
      setFile(await openFile(picked));
      setNotice(null);
    } catch (e) {
      setFile(null);
      setNotice(String(e));
    }
  }

  async function unload() {
    await closeFile();
    setFile(null);
    setNotice(null);
  }

  async function start() {
    try {
      await startSend({ parse, filter, mutate, target, pacing });
      setNotice(null);
    } catch (e) {
      setNotice(String(e));
    }
  }

  return (
    <header className="bar">
      <span className="bar-mark" aria-hidden />
      <span className="bar-title">数据帧回放</span>

      <span className="bar-path" data-empty={file ? undefined : "true"}>
        {file ? (
          <>
            <span className="bar-dir">{dirOf(file.path)}</span>
            <span className="bar-file">{baseOf(file.path)}</span>
          </>
        ) : (
          "未打开文件"
        )}
      </span>

      <div className="bar-group">
        <button className="btn" onClick={pickFile} disabled={active}>
          打开文件
        </button>
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
    </header>
  );
}
