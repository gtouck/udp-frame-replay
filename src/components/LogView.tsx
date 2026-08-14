import { useEffect, useRef } from "react";
import { clearLog, formatClock, formatCount, type LogLevel } from "../api";
import { useStore } from "../store";
import OverlayScrollArea from "./OverlayScrollArea";

const LEVEL_TEXT: Record<LogLevel, string> = {
  info: "信息",
  warn: "警告",
  error: "错误",
};

export default function LogView() {
  const logs = useStore((s) => s.logs);
  const groups = useStore((s) => s.errorGroups);
  const filter = useStore((s) => s.logFilter);
  const setFilter = useStore((s) => s.setLogFilter);
  const setLogs = useStore((s) => s.setLogs);

  const bodyRef = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);

  const shown = filter === "all" ? logs : logs.filter((l) => l.level === filter);

  useEffect(() => {
    const el = bodyRef.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;
  }, [shown.length]);

  const onScroll = () => {
    const el = bodyRef.current;
    if (!el) return;
    pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
  };

  async function wipe() {
    await clearLog();
    setLogs(() => []);
  }

  return (
    <section className="screen">
      <header className="screen-head">
        <span className="screen-name">日志</span>

        <div className="segments segments-inline">
          {(["all", "info", "warn", "error"] as const).map((f) => (
            <button
              key={f}
              className="segment"
              aria-pressed={filter === f}
              onClick={() => setFilter(f)}
            >
              {f === "all" ? "全部" : LEVEL_TEXT[f]}
            </button>
          ))}
        </div>

        <span className="screen-note">{formatCount(shown.length)} 条</span>
        <button className="btn btn-slim" onClick={wipe}>
          清空
        </button>
      </header>

      <OverlayScrollArea
        className="screen-body"
        ref={bodyRef}
        onScroll={onScroll}
      >
        {/* 解析错误按类型聚合。坏文件会有几百万条同类错误，逐条列出会把界面拖死。 */}
        {groups.length > 0 && (
          <div className="errgroups">
            {groups.map((g) => (
              <div className="errgroup" key={g.kind}>
                <span className="errgroup-count">{formatCount(g.count)}</span>
                <span className="errgroup-msg">{g.message}</span>
                <span className="errgroup-lines">
                  行 {g.sampleLines.slice(0, 8).join("、")}
                  {g.count > g.sampleLines.length ? " …" : ""}
                </span>
              </div>
            ))}
          </div>
        )}

        {shown.length === 0 && groups.length === 0 ? (
          <div className="screen-empty">暂无日志</div>
        ) : (
          <div className="loglines">
            {shown.map((l) => (
              <div className="logline" data-level={l.level} key={l.seq}>
                <span className="log-time">{formatClock(l.at)}</span>
                <span className="log-level">{LEVEL_TEXT[l.level]}</span>
                <span className="log-text">{l.text}</span>
              </div>
            ))}
          </div>
        )}
      </OverlayScrollArea>
    </section>
  );
}
