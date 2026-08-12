import { useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { formatCount } from "../api";
import { useStore } from "../store";
import { usePreview } from "../usePreview";

const ROW_H = 22;

export default function SourceView() {
  const file = useStore((s) => s.file);
  const config = useStore((s) => s.parse);
  const version = useStore((s) => s.parseVersion);

  const lineCount = file?.lineCount ?? 0;
  const scrollRef = useRef<HTMLDivElement>(null);
  const { requestRange, getLine } = usePreview(config, version);

  const virt = useVirtualizer({
    count: lineCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_H,
    overscan: 24,
  });

  const items = virt.getVirtualItems();
  const firstIdx = items.length ? items[0].index : 0;
  const lastIdx = items.length ? items[items.length - 1].index : 0;

  useEffect(() => {
    if (lineCount > 0) requestRange(firstIdx, lastIdx);
  }, [firstIdx, lastIdx, lineCount, requestRange]);

  return (
    <section className="screen">
      <header className="screen-head">
        <span className="screen-name">数据原文</span>
        <span className="screen-note">
          {file ? `${formatCount(lineCount)} 行` : ""}
        </span>
      </header>

      <div className="screen-body" ref={scrollRef}>
        {!file ? (
          <div className="screen-empty">打开一个数据文件开始</div>
        ) : (
          <div className="rows" style={{ height: virt.getTotalSize() }}>
            {items.map((v) => {
              const row = getLine(v.index);
              return (
                <div
                  key={v.key}
                  className="row"
                  data-error={row?.error ? "true" : undefined}
                  style={{ transform: `translateY(${v.start}px)` }}
                >
                  <span className="row-no">{v.index + 1}</span>

                  {row ? (
                    <>
                      <span className="seg-prefix">{row.prefix}</span>
                      <span className="seg-data">{row.data}</span>
                      <span className="seg-trailing">{row.trailing}</span>
                      {/* 错误紧跟数据，就近说明是哪一段出的问题 */}
                      {row.error && <span className="row-err">{row.errorMsg}</span>}
                      <span className="row-len">
                        {row.error ? "" : `${row.byteLen} B`}
                      </span>
                    </>
                  ) : (
                    <span className="seg-trailing">…</span>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
