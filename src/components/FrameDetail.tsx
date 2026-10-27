import { useEffect, useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { hex2, type SentFrame } from "../api";
import { copyText } from "../clipboard";
import { ascii, frameHex, paint } from "../frameData";
import { useStore } from "../store";
import OverlayScrollArea from "./OverlayScrollArea";

const BYTES_PER_ROW = 16;
const ROW_HEIGHT = 24;

export default function FrameDetail({
  frame,
  onClose,
}: {
  frame: SentFrame;
  onClose: () => void;
}) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const marks = useMemo(() => paint(frame.bytes.length, frame.spans), [frame]);
  const virt = useVirtualizer({
    count: Math.ceil(frame.bytes.length / BYTES_PER_ROW),
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });

  useEffect(() => {
    const dialog = dialogRef.current;
    dialog?.showModal();
    return () => dialog?.close();
  }, []);

  const copy = async () => {
    try {
      await copyText(frameHex(frame.bytes));
      useStore.getState().setNotice(`已复制完整帧 · ${frame.len} B`, "info");
    } catch (error) {
      useStore.getState().setNotice(String(error));
    }
  };

  return (
    <dialog
      className="frame-dialog"
      ref={dialogRef}
      aria-labelledby="frame-title"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <header className="help-head">
        <span className="help-title" id="frame-title">
          发送数据 · 第 {frame.lineNo} 行 · {frame.len} B
        </span>
        <button className="btn btn-slim" onClick={() => void copy()}>
          复制完整数据
        </button>
        <button className="btn btn-slim" onClick={onClose} autoFocus>
          关闭
        </button>
      </header>
      <div className="frame-columns">
        <span>偏移（HEX）</span>
        <span>十六进制 · 每行 16 字节</span>
        <span>ASCII</span>
      </div>
      <OverlayScrollArea className="frame-body" ref={scrollRef}>
        <div className="frame-rows" style={{ height: virt.getTotalSize() }}>
          {virt.getVirtualItems().map((v) => {
            const offset = v.index * BYTES_PER_ROW;
            const bytes = frame.bytes.slice(offset, offset + BYTES_PER_ROW);
            return (
              <div
                className="frame-row"
                key={v.key}
                style={{ transform: `translateY(${v.start}px)` }}
              >
                <span className="frame-offset">
                  {offset.toString(16).toUpperCase().padStart(4, "0")}
                </span>
                <span className="dump-hex">
                  {bytes.map((b, i) => (
                    <span key={i} data-mark={marks[offset + i] ?? undefined}>
                      {hex2(b)}{i < bytes.length - 1 ? " " : ""}
                    </span>
                  ))}
                </span>
                <span className="frame-ascii">{bytes.map(ascii).join("")}</span>
              </div>
            );
          })}
        </div>
      </OverlayScrollArea>
      <footer className="frame-foot">
        完整显示 {frame.bytes.length} 字节 · 可滚动查看和选择文本
      </footer>
    </dialog>
  );
}
