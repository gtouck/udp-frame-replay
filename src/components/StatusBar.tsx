import { formatBytes, formatCount } from "../api";
import { useStore } from "../store";
import TimingTape from "./TimingTape";

const LAMP_TEXT = {
  idle: "空闲",
  running: "运行中",
  paused: "已暂停",
  error: "错误",
} as const;

/** 读数字段。定宽，数字跳动时布局绝不重排。 */
function Readout({
  label,
  value,
  width,
  alert,
}: {
  label: string;
  value: string;
  width?: number;
  alert?: boolean;
}) {
  return (
    <div className="readout">
      <span className="readout-key">{label}</span>
      <span
        className="readout-val"
        data-alert={alert ? "true" : undefined}
        style={width ? ({ "--w": `${width}ch` } as React.CSSProperties) : undefined}
      >
        {value}
      </span>
    </div>
  );
}

export default function StatusBar() {
  const file = useStore((s) => s.file);
  const runState = useStore((s) => s.runState);
  const notice = useStore((s) => s.notice);

  return (
    <footer className="status">
      <TimingTape samples={[]} targetUs={1000} />

      {file ? (
        <>
          <Readout label="行数" value={formatCount(file.lineCount)} width={11} />
          <Readout label="大小" value={formatBytes(file.sizeBytes)} width={9} />
          <Readout
            label="索引"
            value={formatBytes(file.indexMemoryBytes)}
            width={9}
          />
        </>
      ) : (
        <Readout label="文件" value="未打开" />
      )}

      {notice && <Readout label="提示" value={notice} alert />}

      <span className="status-spacer" />

      <div className="lamp" data-state={runState}>
        <span className="lamp-dot" />
        <span className="lamp-text">{LAMP_TEXT[runState]}</span>
      </div>
    </footer>
  );
}
