import { invoke } from "@tauri-apps/api/core";

// ── 与 Rust 侧一一对应的类型 ────────────────────────────────

export type TextEncoding = "utf8" | "gbk" | "latin1";

export type Delimiter =
  | { kind: "whitespace" }
  | { kind: "comma" }
  | { kind: "tab" }
  | { kind: "custom"; value: string };

export type PrefixRule =
  | {
      mode: "fields";
      delimiter: Delimiter;
      collapse: boolean;
      skipFields: number;
    }
  | { mode: "chars"; skipChars: number };

export interface HexRule {
  ignoreChars: string;
}

export interface ParseConfig {
  encoding: TextEncoding;
  prefix: PrefixRule;
  hex: HexRule;
}

export type ParseErrorKind =
  | "emptyData"
  | "oddHexDigits"
  | "notEnoughFields"
  | "lineTooShort";

/** 一行的预览标注。后端返回切好的三段文本，前端直接渲染，不做索引换算。 */
export interface LinePreview {
  lineNo: number;
  prefix: string;
  data: string;
  trailing: string;
  byteLen: number;
  truncated: boolean;
  error: ParseErrorKind | null;
  errorMsg: string | null;
}

export interface FileInfo {
  path: string;
  sizeBytes: number;
  lineCount: number;
  indexMemoryBytes: number;
}

// ── 发送目标 ────────────────────────────────────────────────

/** 后端把 kind 用 serde flatten 摊平了，所以 mode/host/port 与 bindPort 平级。 */
export type TargetConfig = (
  | { mode: "unicast"; host: string; port: number }
  | {
      mode: "multicast";
      group: string;
      port: number;
      interface: string | null;
      ttl: number;
      loopback: boolean;
    }
) & {
  bindAddr: string | null;
  bindPort: number | null;
  sendBufferBytes: number | null;
};

export interface PacingConfig {
  intervalUs: number;
  startLine: number;
  /** 0 表示直到文件末尾 */
  endLine: number;
  repeat: boolean;
  /** 0 表示无限循环 */
  repeatCount: number;
  highPrecision: boolean;
}

export interface SendConfig {
  parse: ParseConfig;
  target: TargetConfig;
  pacing: PacingConfig;
}

export interface InterfaceInfo {
  name: string;
  ip: string;
  isLoopback: boolean;
}

// ── 引擎状态 ────────────────────────────────────────────────

export type EngineState =
  | "idle"
  | "running"
  | "paused"
  | "stopping"
  | "finished";

export interface EngineSnapshot {
  state: EngineState;
  sentFrames: number;
  sentBytes: number;
  /** 内核发送缓冲满而丢弃的帧数。UDP 不会告诉任何人这件事，必须单独看。 */
  droppedBufferFull: number;
  refused: number;
  ioErrors: number;
  oversize: number;
  parsedFrames: number;
  skippedLines: number;
  currentLine: number;
  loopsDone: number;
  pending: number;
  jitterP50Us: number;
  jitterP99Us: number;
  recentIntervals: number[];
  targetDesc: string;
  intervalUs: number;
}

export interface SentFrame {
  lineNo: number;
  len: number;
  bytes: number[];
  at: number;
}

export type LogLevel = "info" | "warn" | "error";

export interface LogEntry {
  seq: number;
  at: number;
  level: LogLevel;
  text: string;
}

export interface ErrorGroup {
  kind: ParseErrorKind;
  message: string;
  count: number;
  sampleLines: number[];
}

// ── 命令封装 ────────────────────────────────────────────────

export const openFile = (path: string) => invoke<FileInfo>("open_file", { path });

export const closeFile = () => invoke<void>("close_file");

export const fileInfo = () => invoke<FileInfo | null>("file_info");

export const preview = (start: number, count: number, config: ParseConfig) =>
  invoke<LinePreview[]>("preview", { start, count, config });

// ── 默认配置 ────────────────────────────────────────────────

export const networkInterfaces = () =>
  invoke<InterfaceInfo[]>("network_interfaces");

export const startSend = (config: SendConfig) =>
  invoke<void>("start_send", { config });

export const pauseSend = () => invoke<void>("pause_send");
export const resumeSend = () => invoke<void>("resume_send");
export const stepSend = () => invoke<void>("step_send");
export const stopSend = () => invoke<void>("stop_send");

export const engineStatus = () => invoke<EngineSnapshot | null>("engine_status");

export const recentFrames = (limit: number) =>
  invoke<SentFrame[]>("recent_frames", { limit });

export const logEntries = (after: number, limit: number) =>
  invoke<LogEntry[]>("log_entries", { after, limit });

export const errorGroups = () => invoke<ErrorGroup[]>("error_groups");
export const clearLog = () => invoke<void>("clear_log");

// ── 默认配置 ────────────────────────────────────────────────

export const defaultParseConfig = (): ParseConfig => ({
  encoding: "utf8",
  prefix: {
    mode: "fields",
    delimiter: { kind: "whitespace" },
    collapse: true,
    skipFields: 0,
  },
  hex: { ignoreChars: ":-," },
});

export const defaultTargetConfig = (): TargetConfig => ({
  mode: "unicast",
  host: "127.0.0.1",
  port: 9000,
  bindAddr: null,
  bindPort: null,
  sendBufferBytes: null,
});

export const defaultPacingConfig = (): PacingConfig => ({
  intervalUs: 1000,
  startLine: 1,
  endLine: 0,
  repeat: false,
  repeatCount: 0,
  highPrecision: false,
});

// ── 展示辅助 ────────────────────────────────────────────────

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1048576).toFixed(1)} MB`;
  return `${(n / 1073741824).toFixed(2)} GB`;
}

export const formatCount = (n: number) => n.toLocaleString("en-US");

export const hex2 = (b: number) =>
  b.toString(16).toUpperCase().padStart(2, "0");

/** 微秒转成人读的量纲，保持窄宽度便于放进读数栏 */
export function formatUs(us: number): string {
  if (us < 1000) return `${us}μs`;
  if (us < 1_000_000) return `${(us / 1000).toFixed(us < 10_000 ? 1 : 0)}ms`;
  return `${(us / 1_000_000).toFixed(2)}s`;
}

export function formatClock(unixMs: number): string {
  const d = new Date(unixMs);
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(
    d.getMilliseconds(),
    3,
  )}`;
}
