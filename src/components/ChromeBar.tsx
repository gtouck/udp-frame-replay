import { open } from "@tauri-apps/plugin-dialog";
import { closeFile, openFile } from "../api";
import { useStore } from "../store";

export default function ChromeBar() {
  const file = useStore((s) => s.file);
  const setFile = useStore((s) => s.setFile);
  const setNotice = useStore((s) => s.setNotice);
  const runState = useStore((s) => s.runState);

  const running = runState === "running" || runState === "paused";

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

  return (
    <header className="bar">
      <span className="bar-mark" aria-hidden />
      <span className="bar-title">数据帧回放</span>

      <span className="bar-path" data-empty={file ? undefined : "true"}>
        {file ? file.path : "未打开文件"}
      </span>

      <div className="bar-group">
        <button className="btn" onClick={pickFile} disabled={running}>
          打开文件
        </button>
        {file && (
          <button className="btn" onClick={unload} disabled={running}>
            关闭
          </button>
        )}
      </div>
    </header>
  );
}
