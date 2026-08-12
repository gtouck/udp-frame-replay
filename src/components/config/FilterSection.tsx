import type { Condition, TextOp } from "../../api";
import { useStore } from "../../store";
import { Hint } from "./Field";

/** 一条规则的编辑器 */
function RuleCard({ index }: { index: number }) {
  const rule = useStore((s) => s.filter.rules[index]);
  const update = useStore((s) => s.updateFilterRule);
  const updateCond = useStore((s) => s.updateFilterCondition);
  const remove = useStore((s) => s.removeFilterRule);

  if (!rule) return null;
  const c = rule.condition;

  return (
    <div className="rule" data-off={rule.enabled ? undefined : "true"}>
      <div className="rule-head">
        <label className="rule-toggle" title="停用后这条规则不参与判定">
          <input
            type="checkbox"
            checked={rule.enabled}
            onChange={(e) => update(index, { enabled: e.target.checked })}
          />
          <span className="rule-name">
            {c.kind === "field" ? "行字段" : "数据字节"}
          </span>
        </label>

        <button
          className="rule-negate"
          aria-pressed={rule.negate}
          onClick={() => update(index, { negate: !rule.negate })}
          title="取反：满足条件的反而被排除"
        >
          取反
        </button>

        <button
          className="rule-remove"
          onClick={() => remove(index)}
          aria-label={`删除第 ${index + 1} 条规则`}
        >
          ✕
        </button>
      </div>

      {c.kind === "field" ? (
        <div className="rule-body">
          <input
            className="input rule-idx"
            type="number"
            min={0}
            value={c.index}
            aria-label="字段序号"
            onChange={(e) =>
              updateCond(index, {
                index: Math.max(0, +e.target.value || 0),
              } as Partial<Condition>)
            }
          />
          <select
            className="select rule-op"
            value={c.op}
            aria-label="比较方式"
            onChange={(e) =>
              updateCond(index, {
                op: e.target.value as TextOp,
              } as Partial<Condition>)
            }
          >
            <option value="equals">等于</option>
            <option value="contains">包含</option>
          </select>
          <input
            className="input"
            value={c.value}
            placeholder="值"
            aria-label="匹配值"
            onChange={(e) =>
              updateCond(index, { value: e.target.value } as Partial<Condition>)
            }
          />
        </div>
      ) : (
        <div className="rule-body">
          <input
            className="input rule-idx"
            type="number"
            value={c.offset}
            aria-label="字节偏移"
            title="负数从帧尾倒数：-2 配两字节即匹配最后两字节"
            onChange={(e) =>
              updateCond(index, {
                offset: +e.target.value || 0,
              } as Partial<Condition>)
            }
          />
          <input
            className="input"
            value={c.value}
            placeholder="字节 如 01 A5"
            aria-label="期望字节"
            onChange={(e) =>
              updateCond(index, { value: e.target.value } as Partial<Condition>)
            }
          />
          <input
            className="input rule-mask"
            value={c.mask ?? ""}
            placeholder="掩码"
            aria-label="掩码"
            title="只比较掩码中为 1 的位，长度须与字节值一致"
            onChange={(e) =>
              updateCond(index, {
                mask: e.target.value || null,
              } as Partial<Condition>)
            }
          />
        </div>
      )}
    </div>
  );
}

export default function FilterSection() {
  const rules = useStore((s) => s.filter.rules);
  const add = useStore((s) => s.addFilterRule);

  return (
    <>
      {rules.length === 0 ? (
        <Hint>还没有规则，所有行都会发送。</Hint>
      ) : (
        <>
          {rules.map((_, i) => (
            <RuleCard key={i} index={i} />
          ))}
          <Hint>
            所有启用的规则都满足才发送。不满足的行在上方原文里会变暗。
          </Hint>
        </>
      )}

      <div className="rule-add">
        <button className="btn btn-slim" onClick={() => add("field")}>
          + 行字段
        </button>
        <button className="btn btn-slim" onClick={() => add("bytes")}>
          + 数据字节
        </button>
      </div>
    </>
  );
}
