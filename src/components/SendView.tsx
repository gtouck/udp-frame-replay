import { useEffect, useRef } from "react";
import { formatCount, hex2 } from "../api";
import { useStore } from "../store";

/** 每帧在视图里展示的字节数上限，超出的部分省略 */
const SHOW_BYTES = 32;

const ascii = (b: number) =>
  b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : "·";

export default function SendView() {
  const frames = useStore((s) => s.frames);
  const engine = useStore((s) => s.engine);
  const bodyRef = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);

  // 跟随最新帧，但使用者往上翻看历史时不要把他拽回来
  useEffect(() => {
    const el = bodyRef.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;
  }, [frames]);

  const onScroll = () => {
    const el = bodyRef.current;
    if (!el) return;
    pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
  };

  const sent = engine?.sentFrames ?? 0;

  return (
    <section className="screen">
      <header className="screen-head">
        <span className="screen-name">发送数据</span>
        <span className="screen-note">
          {engine
            ? `采样显示 · 实际已发 ${formatCount(sent)} 帧`
            : ""}
        </span>
      </header>

      <div className="screen-body" ref={bodyRef} onScroll={onScroll}>
        {frames.length === 0 ? (
          <div className="screen-empty">
            {engine ? "等待第一帧…" : "开始发送后这里显示实际发出的字节"}
          </div>
        ) : (
          <div className="dump">
            {frames.map((f, i) => {
              const shown = f.bytes.slice(0, SHOW_BYTES);
              const clipped = f.len > shown.length;
              return (
                <div className="dump-row" key={`${f.at}-${f.lineNo}-${i}`}>
                  <span className="dump-line">{f.lineNo}</span>
                  <span className="dump-hex">
                    {shown.map((b) => hex2(b)).join(" ")}
                    {clipped ? " …" : ""}
                  </span>
                  <span className="dump-ascii">
                    {shown.map(ascii).join("")}
                    {clipped ? "…" : ""}
                  </span>
                  <span className="dump-len">{f.len} B</span>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
