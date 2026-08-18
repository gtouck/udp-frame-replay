import { formatBytes, formatCount, formatUs } from "../api";
import { useStore } from "../store";
import TimingTape from "./TimingTape";

const LAMP_TEXT: Record<string, string> = {
  idle: "空闲",
  running: "运行中",
  paused: "已暂停",
  stopping: "停止中",
  finished: "已完成",
};

/** 读数字段。定宽，数字跳动时布局绝不重排。 */
function Readout({
  label,
  value,
  width,
  alert,
  title,
  clip,
}: {
  label: string;
  value: string;
  width?: number;
  alert?: boolean;
  title?: string;
  /** 长度不可控的文本：截断显示，完整内容挂在 title 上 */
  clip?: boolean;
}) {
  return (
    <div className="readout" title={title}>
      <span className="readout-key">{label}</span>
      <span
        className="readout-val"
        data-alert={alert ? "true" : undefined}
        data-clip={clip ? "true" : undefined}
        style={
          width ? ({ "--w": `${width}ch` } as React.CSSProperties) : undefined
        }
      >
        {value}
      </span>
    </div>
  );
}

export default function StatusBar() {
  const file = useStore((s) => s.file);
  const engine = useStore((s) => s.engine);
  const rate = useStore((s) => s.rate);
  const notice = useStore((s) => s.notice);
  const groups = useStore((s) => s.errorGroups);

  const state = engine?.state ?? "idle";
  const parseErrors = groups.reduce((n, g) => n + g.count, 0);

  return (
    <footer className="status">
      {engine && (
        <TimingTape
          samples={engine.recentIntervals}
          targetUs={engine.intervalUs}
        />
      )}

      {engine ? (
        <>
          <Readout
            label="已发"
            value={formatCount(engine.sentFrames)}
            width={11}
          />
          <Readout
            label="速率"
            value={`${formatCount(Math.round(rate))}/s`}
            width={12}
          />
          <Readout
            label="抖动"
            value={`p50 ${formatUs(engine.jitterP50Us)} · p99 ${formatUs(
              engine.jitterP99Us,
            )}`}
            title="实际帧间隔相对目标的分布。软实时下抖动无法根除，这里显示真实表现。"
          />
          <Readout
            label="缓冲满丢弃"
            value={formatCount(engine.droppedBufferFull)}
            width={8}
            alert={engine.droppedBufferFull > 0}
            title="内核发送缓冲满导致的丢帧。UDP 不会报告这件事，所以单独列出来。"
          />
          <Readout
            label="跳过"
            value={formatCount(engine.skippedLines)}
            width={8}
            alert={engine.skippedLines > 0}
            title="解析失败被跳过的行"
          />
          {engine.filteredOut > 0 && (
            <Readout
              label="已筛掉"
              value={formatCount(engine.filteredOut)}
              width={9}
              title="解析没问题，但不满足筛选规则，没有发出"
            />
          )}
          {engine.mutationIssues > 0 && (
            <Readout
              label="修改未生效"
              value={formatCount(engine.mutationIssues)}
              width={8}
              alert
              title="偏移越界或区间冲突，该条规则被跳过；帧本身照常发出"
            />
          )}
          {engine.oversize > 0 && (
            <Readout
              label="超长"
              value={formatCount(engine.oversize)}
              width={6}
              alert
              title="超过 UDP 单包上限而跳过的帧"
            />
          )}
          {engine.refused > 0 && (
            <Readout
              label="端口不可达"
              value={formatCount(engine.refused)}
              width={6}
              alert
              title="对端多半没有程序在监听"
            />
          )}
        </>
      ) : file ? (
        <>
          <Readout label="行数" value={formatCount(file.lineCount)} width={11} />
          <Readout label="大小" value={formatBytes(file.sizeBytes)} width={9} />
          <Readout
            label="索引"
            value={formatBytes(file.indexMemoryBytes)}
            width={9}
          />
          {parseErrors > 0 && (
            <Readout
              label="解析错误"
              value={formatCount(parseErrors)}
              width={8}
              alert
            />
          )}
        </>
      ) : (
        <Readout label="文件" value="未打开" />
      )}

      {notice && (
        <Readout
          label="提示"
          value={notice.text}
          title={notice.text}
          alert={notice.level === "error"}
          clip
        />
      )}

      <span className="status-spacer" />

      {engine?.targetDesc && (
        <span className="status-target">{engine.targetDesc}</span>
      )}

      <div className="lamp" data-state={state}>
        <span className="lamp-dot" />
        <span className="lamp-text">{LAMP_TEXT[state] ?? state}</span>
      </div>
    </footer>
  );
}
