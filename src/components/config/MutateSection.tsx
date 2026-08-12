import {
  isStructural,
  OP_LABEL,
  type ChecksumAlgo,
  type Endian,
  type MutationOp,
  type TimeEpoch,
  type TimeUnit,
  type Width,
} from "../../api";
import { useStore } from "../../store";
import { Hint } from "./Field";

const WIDTHS: { v: Width; t: string }[] = [
  { v: "w1", t: "1 字节" },
  { v: "w2", t: "2 字节" },
  { v: "w4", t: "4 字节" },
  { v: "w8", t: "8 字节" },
];

const ALGOS: { v: ChecksumAlgo; t: string }[] = [
  { v: "sum8", t: "累加和 8" },
  { v: "sum16", t: "累加和 16" },
  { v: "xor8", t: "异或 8" },
  { v: "crc16Ccitt", t: "CRC16-CCITT" },
  { v: "crc16Modbus", t: "CRC16-MODBUS" },
  { v: "crc16Xmodem", t: "CRC16-XMODEM" },
  { v: "crc32", t: "CRC32" },
];

/** 偏移与字节序这类小控件重复出现，抽出来省得每处都写一遍 */
function Num({
  label,
  value,
  onChange,
  title,
  width = 56,
  min,
}: {
  label: string;
  value: number;
  onChange: (n: number) => void;
  title?: string;
  width?: number;
  min?: number;
}) {
  return (
    <label className="mini" title={title}>
      <span className="mini-label">{label}</span>
      <input
        className="input"
        type="number"
        min={min}
        style={{ width }}
        value={value}
        onChange={(e) => {
          const n = Number(e.target.value);
          if (!Number.isNaN(n)) onChange(min !== undefined ? Math.max(min, n) : n);
        }}
      />
    </label>
  );
}

function Pick<T extends string>({
  label,
  value,
  options,
  onChange,
  width = 88,
}: {
  label: string;
  value: T;
  options: { v: T; t: string }[];
  onChange: (v: T) => void;
  width?: number;
}) {
  return (
    <label className="mini">
      <span className="mini-label">{label}</span>
      <select
        className="select"
        style={{ width }}
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
      >
        {options.map((o) => (
          <option key={o.v} value={o.v}>
            {o.t}
          </option>
        ))}
      </select>
    </label>
  );
}

function Endianness({
  value,
  onChange,
}: {
  value: Endian;
  onChange: (v: Endian) => void;
}) {
  return (
    <Pick
      label="字节序"
      value={value}
      width={68}
      options={[
        { v: "big", t: "大端" },
        { v: "little", t: "小端" },
      ]}
      onChange={onChange}
    />
  );
}

function RangeFields({
  range,
  onChange,
}: {
  range: { start: number; end: number };
  onChange: (r: { start: number; end: number }) => void;
}) {
  return (
    <>
      <Num
        label="范围起"
        value={range.start}
        onChange={(start) => onChange({ ...range, start })}
        title="负数从帧尾倒数"
      />
      <Num
        label="止"
        value={range.end}
        onChange={(end) => onChange({ ...range, end })}
        title="不含。填 0 表示一直到帧尾；负数从帧尾倒数"
      />
    </>
  );
}

function OpFields({ index, op }: { index: number; op: MutationOp }) {
  const set = useStore((s) => s.updateMutationOp);
  const p = (patch: Record<string, unknown>) => set(index, patch);

  switch (op.kind) {
    case "insert":
    case "replace":
      return (
        <>
          <Num
            label="偏移"
            value={op.offset}
            onChange={(offset) => p({ offset })}
            title="基于原始帧。负数从帧尾倒数。"
          />
          <label className="mini mini-grow">
            <span className="mini-label">字节</span>
            <input
              className="input"
              value={op.value}
              placeholder="如 5A A5"
              onChange={(e) => p({ value: e.target.value })}
            />
          </label>
        </>
      );

    case "delete":
      return (
        <>
          <Num
            label="偏移"
            value={op.offset}
            onChange={(offset) => p({ offset })}
            title="基于原始帧。负数从帧尾倒数。"
          />
          <Num
            label="长度"
            value={op.length}
            min={1}
            onChange={(length) => p({ length })}
          />
        </>
      );

    case "sequence":
      return (
        <>
          <Num label="偏移" value={op.offset} onChange={(offset) => p({ offset })} />
          <Pick
            label="宽度"
            value={op.width}
            options={WIDTHS}
            width={76}
            onChange={(width) => p({ width })}
          />
          <Endianness value={op.endian} onChange={(endian) => p({ endian })} />
          <Num
            label="起始"
            value={op.start}
            min={0}
            onChange={(start) => p({ start })}
          />
          <Num label="步长" value={op.step} min={0} onChange={(step) => p({ step })} />
          <label className="check check-inline">
            <input
              type="checkbox"
              checked={op.resetEachLoop}
              onChange={(e) => p({ resetEachLoop: e.target.checked })}
            />
            循环时归零
          </label>
        </>
      );

    case "timestamp":
      return (
        <>
          <Num label="偏移" value={op.offset} onChange={(offset) => p({ offset })} />
          <Pick
            label="宽度"
            value={op.width}
            options={WIDTHS}
            width={76}
            onChange={(width) => p({ width })}
          />
          <Endianness value={op.endian} onChange={(endian) => p({ endian })} />
          <Pick
            label="单位"
            value={op.unit}
            width={64}
            options={[
              { v: "millis" as TimeUnit, t: "毫秒" },
              { v: "micros" as TimeUnit, t: "微秒" },
            ]}
            onChange={(unit) => p({ unit })}
          />
          <Pick
            label="基准"
            value={op.epoch}
            width={96}
            options={[
              { v: "unix" as TimeEpoch, t: "Unix 纪元" },
              { v: "sinceStart" as TimeEpoch, t: "本次发送起" },
            ]}
            onChange={(epoch) => p({ epoch })}
          />
        </>
      );

    case "length":
      return (
        <>
          <Num label="偏移" value={op.offset} onChange={(offset) => p({ offset })} />
          <Pick
            label="宽度"
            value={op.width}
            options={WIDTHS}
            width={76}
            onChange={(width) => p({ width })}
          />
          <Endianness value={op.endian} onChange={(endian) => p({ endian })} />
          <RangeFields range={op.range} onChange={(range) => p({ range })} />
          <label className="check check-inline">
            <input
              type="checkbox"
              checked={op.includeSelf}
              onChange={(e) => p({ includeSelf: e.target.checked })}
            />
            含长度字段自身
          </label>
        </>
      );

    case "checksum":
      return (
        <>
          <Num label="偏移" value={op.offset} onChange={(offset) => p({ offset })} />
          <Pick
            label="算法"
            value={op.algorithm}
            options={ALGOS}
            width={128}
            onChange={(algorithm) => p({ algorithm })}
          />
          <Endianness value={op.endian} onChange={(endian) => p({ endian })} />
          <RangeFields range={op.range} onChange={(range) => p({ range })} />
        </>
      );
  }
}

function RuleCard({ index }: { index: number }) {
  const rule = useStore((s) => s.mutate.rules[index]);
  const total = useStore((s) => s.mutate.rules.length);
  const update = useStore((s) => s.updateMutationRule);
  const move = useStore((s) => s.moveMutationRule);
  const remove = useStore((s) => s.removeMutationRule);

  if (!rule) return null;
  const stage = isStructural(rule.op.kind) ? 1 : 2;

  return (
    <div className="rule" data-off={rule.enabled ? undefined : "true"}>
      <div className="rule-head">
        <label className="rule-toggle">
          <input
            type="checkbox"
            checked={rule.enabled}
            onChange={(e) => update(index, { enabled: e.target.checked })}
          />
          <span className="rule-name">{OP_LABEL[rule.op.kind]}</span>
        </label>

        <span
          className="stage-tag"
          data-stage={stage}
          title={
            stage === 1
              ? "阶段一：改变帧结构，偏移基于原始帧"
              : "阶段二：写入计算值，偏移基于改完结构之后的帧"
          }
        >
          阶段 {stage}
        </span>

        <button
          className="rule-move"
          onClick={() => move(index, -1)}
          disabled={index === 0}
          aria-label="上移"
        >
          ↑
        </button>
        <button
          className="rule-move"
          onClick={() => move(index, 1)}
          disabled={index === total - 1}
          aria-label="下移"
        >
          ↓
        </button>
        <button
          className="rule-remove"
          onClick={() => remove(index)}
          aria-label={`删除第 ${index + 1} 条修改规则`}
        >
          ✕
        </button>
      </div>

      <div className="rule-body rule-body-wrap">
        <OpFields index={index} op={rule.op} />
      </div>
    </div>
  );
}

export default function MutateSection() {
  const rules = useStore((s) => s.mutate.rules);
  const add = useStore((s) => s.addMutationRule);

  // 阶段二按顺序执行，校验和必须最后 —— 长度字段通常也在校验范围内
  const stage2 = rules
    .map((r, i) => ({ r, i }))
    .filter(({ r }) => r.enabled && !isStructural(r.op.kind));
  let lastChecksum = -1;
  stage2.forEach(({ r }, i) => {
    if (r.op.kind === "checksum") lastChecksum = i;
  });
  const checksumNotLast =
    lastChecksum >= 0 && lastChecksum !== stage2.length - 1;

  return (
    <>
      {rules.length === 0 ? (
        <Hint>还没有规则，数据原样发出。</Hint>
      ) : (
        <>
          {rules.map((_, i) => (
            <RuleCard key={i} index={i} />
          ))}

          {checksumNotLast && (
            <p className="warn-note">
              校验和后面还有别的计算值规则。长度字段通常也在校验范围内，
              校验和排在最后才算得对 —— 用 ↓ 把它移到末尾。
            </p>
          )}

          <Hint>
            阶段一先改结构，偏移都按原始数据数；阶段二再写计算值，
            按上面的先后顺序执行。这样插入或删除字节之后，长度和校验和依然是对的。
          </Hint>
        </>
      )}

      <div className="rule-add rule-add-grid">
        {(
          [
            "insert",
            "replace",
            "delete",
            "sequence",
            "timestamp",
            "length",
            "checksum",
          ] as MutationOp["kind"][]
        ).map((k) => (
          <button key={k} className="btn btn-slim" onClick={() => add(k)}>
            + {OP_LABEL[k]}
          </button>
        ))}
      </div>
    </>
  );
}
