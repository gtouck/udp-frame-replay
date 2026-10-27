import { useEffect, useRef, useState } from "react";
import { formatCount, hex2, sentFrame, type SentFrame } from "../api";
import { ascii, frameHex, paint } from "../frameData";
import { useStore } from "../store";
import OverlayScrollArea from "./OverlayScrollArea";
import FrameDetail from "./FrameDetail";
import { useRowMenu } from "./RowMenu";

/** 列表保留紧凑摘要，完整内容按需在详情中查看。 */
const SHOW_BYTES = 32;

const loadFrame = (frame: SentFrame) =>
  frame.bytes.length === frame.len ? Promise.resolve(frame) : sentFrame(frame.id);

export default function SendView() {
  const frames = useStore((s) => s.frames);
  const engine = useStore((s) => s.engine);
  const bodyRef = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const rowMenu = useRowMenu();
  const [detail, setDetail] = useState<SentFrame | null>(null);

  const inspect = async (pending: Promise<SentFrame>) => {
    try {
      setDetail(await pending);
    } catch (error) {
      useStore.getState().setNotice(String(error));
    }
  };

  // 跟随最新帧，但使用者往上翻看历史时不要把他拽回来
  useEffect(() => {
    const el = bodyRef.current;
    if (el && pinned.current && !rowMenu.menu && !detail) {
      el.scrollTop = el.scrollHeight;
    }
  }, [frames, rowMenu.menu, detail]);

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
          {engine ? `采样显示 · 实际已发 ${formatCount(sent)} 帧 · 右键复制 / 双击查看全部` : ""}
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
            {frames.map((f) => {
              const shown = f.bytes.slice(0, SHOW_BYTES);
              const clipped = f.len > shown.length;
              const marks = paint(shown.length, f.spans ?? []);
              return (
                <div
                  className="dump-row"
                  key={f.id}
                  data-selected={rowMenu.selected === f.id ? "true" : undefined}
                  onClick={() => rowMenu.select(f.id)}
                  onDoubleClick={() => void inspect(loadFrame(f))}
                  onContextMenu={(event) => {
                    // 菜单打开时立即留住完整快照，避免等待操作期间被后续发送覆盖。
                    const pending = loadFrame(f);
                    void pending.catch(() => {});
                    rowMenu.open(event, f.id, async () => frameHex((await pending).bytes), {
                      label: "查看完整数据",
                      run: () => inspect(pending),
                    });
                  }}
                >
                  <span className="dump-line">{f.lineNo}</span>
                  <span className="dump-hex">
                    {shown.map((b, j) => (
                      <span key={j} data-mark={marks[j] ?? undefined}>
                        {hex2(b)}
                        {j < shown.length - 1 ? " " : ""}
                      </span>
                    ))}
                    {clipped && (
                      <button
                        className="dump-expand"
                        title="查看完整数据"
                        onClick={() => void inspect(loadFrame(f))}
                      >
                        … 查看全部
                      </button>
                    )}
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
      {rowMenu.menu}
      {detail && <FrameDetail frame={detail} onClose={() => setDetail(null)} />}
    </section>
  );
}
