import { useEffect, useRef } from "react";
import { formatCount, hex2, type Span } from "../api";
import { useStore } from "../store";
import OverlayScrollArea from "./OverlayScrollArea";

/** 每帧在视图里展示的字节数上限，超出的部分省略 */
const SHOW_BYTES = 32;

const ascii = (b: number) =>
  b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : "·";

/**
 * 把改动区段摊成「每个字节属于哪一类」。
 *
 * 后端给的是改完之后的帧内偏移，所以这里不做任何换算，直接铺开。
 * 后写的规则盖住先写的，与执行顺序一致。
 */
function paint(len: number, spans: Span[]): (Span["kind"] | null)[] {
  const marks: (Span["kind"] | null)[] = new Array(len).fill(null);
  for (const s of spans) {
    for (let i = s.start; i < s.start + s.len && i < len; i++) {
      marks[i] = s.kind;
    }
  }
  return marks;
}

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
  // 没有修改规则时不占地方摆图例
  const hasMarks = frames.some((f) => (f.spans?.length ?? 0) > 0);

  return (
    <section className="screen">
      <header className="screen-head">
        <span className="screen-name">发送数据</span>

        {hasMarks && (
          <div className="legend">
            <span className="legend-item" data-mark="insert">
              插入
            </span>
            <span className="legend-item" data-mark="replace">
              替换
            </span>
            <span className="legend-item" data-mark="computed">
              计算值
            </span>
          </div>
        )}

        <span className="screen-note">
          {engine ? `采样显示 · 实际已发 ${formatCount(sent)} 帧` : ""}
        </span>
      </header>

      <OverlayScrollArea
        className="screen-body"
        ref={bodyRef}
        onScroll={onScroll}
      >
        {frames.length === 0 ? (
          <div className="screen-empty">
            {engine ? "等待第一帧…" : "开始发送后这里显示实际发出的字节"}
          </div>
        ) : (
          <div className="dump">
            {frames.map((f, i) => {
              const shown = f.bytes.slice(0, SHOW_BYTES);
              const clipped = f.len > shown.length;
              const marks = paint(shown.length, f.spans ?? []);
              return (
                <div className="dump-row" key={`${f.at}-${f.lineNo}-${i}`}>
                  <span className="dump-line">{f.lineNo}</span>
                  <span className="dump-hex">
                    {shown.map((b, j) => (
                      <span key={j} data-mark={marks[j] ?? undefined}>
                        {hex2(b)}
                        {j < shown.length - 1 ? " " : ""}
                      </span>
                    ))}
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
      </OverlayScrollArea>
    </section>
  );
}
