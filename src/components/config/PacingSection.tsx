import { useState } from "react";
import { formatCount } from "../../api";
import { useStore } from "../../store";
import { Check, Field, Hint, NumberField, Segments } from "./Field";

type Unit = "us" | "ms" | "s";
const SCALE: Record<Unit, number> = { us: 1, ms: 1000, s: 1_000_000 };

export default function PacingSection() {
  const pacing = useStore((s) => s.pacing);
  const setPacing = useStore((s) => s.setPacing);
  const file = useStore((s) => s.file);

  // 使用者按 ms 还是 μs 思考取决于场景，存储一律用微秒
  const [unit, setUnit] = useState<Unit>("ms");
  const shown = pacing.intervalUs / SCALE[unit];

  const rate = pacing.intervalUs > 0 ? 1_000_000 / pacing.intervalUs : 0;
  const lineCount = file?.lineCount ?? 0;

  return (
    <>
      <Field label="间隔" htmlFor="cfg-interval">
        <div className="input-group">
          <input
            id="cfg-interval"
            className="input"
            type="number"
            min={0}
            step={unit === "us" ? 10 : 1}
            value={Number.isInteger(shown) ? shown : shown.toFixed(3)}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (Number.isNaN(v) || v < 0) return;
              setPacing({ intervalUs: Math.round(v * SCALE[unit]) });
            }}
          />
          <select
            className="select unit-select"
            value={unit}
            onChange={(e) => setUnit(e.target.value as Unit)}
            aria-label="间隔单位"
          >
            <option value="us">μs</option>
            <option value="ms">ms</option>
            <option value="s">s</option>
          </select>
        </div>
      </Field>

      <Hint>
        {pacing.intervalUs === 0
          ? "间隔 0 表示全速发送，速率由网卡和内核决定。"
          : `约 ${formatCount(Math.round(rate))} 帧/秒。`}
      </Hint>

      <NumberField
        label="起始行"
        value={pacing.startLine}
        min={1}
        max={lineCount || undefined}
        onChange={(startLine) => setPacing({ startLine })}
      />

      <NumberField
        label="结束行"
        value={pacing.endLine}
        min={0}
        max={lineCount || undefined}
        onChange={(endLine) => setPacing({ endLine })}
      />

      <Hint>
        结束行填 0 表示发到文件末尾
        {lineCount > 0 ? `（共 ${formatCount(lineCount)} 行）` : ""}。
      </Hint>

      <Check
        label="循环发送"
        checked={pacing.repeat}
        onChange={(repeat) => setPacing({ repeat })}
      />

      {pacing.repeat && (
        <>
          <NumberField
            label="循环"
            value={pacing.repeatCount}
            min={0}
            onChange={(repeatCount) => setPacing({ repeatCount })}
            suffix="次"
          />
          <Hint>填 0 表示一直循环，直到手动停止。</Hint>
        </>
      )}

      <Segments
        value={pacing.highPrecision ? "high" : "normal"}
        onChange={(v) => setPacing({ highPrecision: v === "high" })}
        options={[
          { value: "normal", label: "普通节拍" },
          { value: "high", label: "高精度" },
        ]}
      />

      <Hint>
        {pacing.highPrecision
          ? "自旋等待，误差可压到个位数微秒，但会占满一个 CPU 核心。抖动统计在状态栏。"
          : "睡眠驱动，几乎不占 CPU，误差取决于系统调度粒度（通常 1ms 级）。"}
      </Hint>
    </>
  );
}
