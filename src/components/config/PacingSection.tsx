import { useState } from "react";
import { formatCount } from "../../api";
import { useStore } from "../../store";
import { Check, Field, FieldPair, Hint, NumberField, Segments } from "./Field";

type Unit = "us" | "ms" | "s";
const SCALE: Record<Unit, number> = { us: 1, ms: 1000, s: 1_000_000 };

export default function PacingSection() {
  const pacing = useStore((s) => s.pacing);
  const setPacing = useStore((s) => s.setPacing);
  const file = useStore((s) => s.file);

  // 使用者按 ms 还是 μs 思考取决于场景，存储一律用微秒
  const [unit, setUnit] = useState<Unit>("ms");
  const shown = pacing.intervalUs / SCALE[unit];

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

      <FieldPair>
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
      </FieldPair>

      <Hint>
        结束行填 0 表示发到文件末尾
        {lineCount > 0 ? `（共 ${formatCount(lineCount)} 行）` : ""}。
      </Hint>
      <FieldPair>
        <Check
          label="循环发送"
          checked={pacing.repeat}
          onChange={(repeat) => setPacing({ repeat })}
        />
        <NumberField
          label="循环"
          value={pacing.repeatCount}
          min={0}
          disabled={!pacing.repeat}
          onChange={(repeatCount) => setPacing({ repeatCount })}
          suffix="次"
        />
      </FieldPair>
      <Hint>填 0 表示一直循环，直到手动停止。</Hint>


      <Segments
        value={pacing.highPrecision ? "high" : "normal"}
        onChange={(v) => setPacing({ highPrecision: v === "high" })}
        options={[
          { value: "normal", label: "普通节拍" },
          { value: "high", label: "高精度" },
        ]}
      />
    </>
  );
}
