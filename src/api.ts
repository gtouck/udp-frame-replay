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
  /** 被筛选规则排除：解析没问题，但不会发出去 */
  filtered: boolean;
  error: ParseErrorKind | null;
  errorMsg: string | null;
}

export interface FileInfo {
  path: string;
  sizeBytes: number;
  lineCount: number;
  indexMemoryBytes: number;
}

// ── 筛选规则 ────────────────────────────────────────────────

export type TextOp = "equals" | "contains";

export type Condition =
  | { kind: "field"; index: number; op: TextOp; value: string }
  | { kind: "bytes"; offset: number; value: string; mask: string | null };

export interface FilterRule {
  condition: Condition;
  /** 取反：满足条件的反而被排除 */
  negate: boolean;
  enabled: boolean;
}

/** 多条规则之间是「与」的关系：全部满足才发送。 */
export interface FilterConfig {
  rules: FilterRule[];
}

// ── 修改规则 ────────────────────────────────────────────────

export type Endian = "big" | "little";
export type Width = "w1" | "w2" | "w4" | "w8";
export type TimeUnit = "millis" | "micros";
export type TimeEpoch = "unix" | "sinceStart";

export type ChecksumAlgo =
  | "sum8"
  | "sum16"
  | "xor8"
  | "crc16Ccitt"
  | "crc16Modbus"
  | "crc16Xmodem"
  | "crc32";

/** 左闭右开 [start, end)，起止都可为负（从帧尾倒数）；end 填 0 表示到帧尾。 */
export interface ByteRange {
  start: number;
  end: number;
}

export type MutationOp =
  | { kind: "insert"; offset: number; value: string }
  | { kind: "replace"; offset: number; value: string }
  | { kind: "delete"; offset: number; length: number }
  | {
      kind: "sequence";
      offset: number;
      width: Width;
      endian: Endian;
      start: number;
      step: number;
      resetEachLoop: boolean;
    }
  | {
      kind: "timestamp";
      offset: number;
      width: Width;
      endian: Endian;
      unit: TimeUnit;
      epoch: TimeEpoch;
    }
  | {
      kind: "length";
      offset: number;
      width: Width;
      endian: Endian;
      range: ByteRange;
      includeSelf: boolean;
    }
  | {
      kind: "checksum";
      offset: number;
      algorithm: ChecksumAlgo;
      endian: Endian;
      range: ByteRange;
    };

export interface MutationRule {
  op: MutationOp;
  /** 留空表示对每一帧都生效 */
  condition: Condition | null;
  enabled: boolean;
}

export interface MutationConfig {
  rules: MutationRule[];
}

/** 前三种改结构（阶段一），后四种写计算值（阶段二） */
export const STRUCTURAL_KINDS: MutationOp["kind"][] = [
  "insert",
  "replace",
  "delete",
];

export const isStructural = (k: MutationOp["kind"]) =>
  STRUCTURAL_KINDS.includes(k);

export const OP_LABEL: Record<MutationOp["kind"], string> = {
  insert: "插入",
  replace: "替换",
  delete: "删除",
  sequence: "序号",
  timestamp: "时间戳",
  length: "长度",
  checksum: "校验和",
};

/** 一段被改动过的字节，位置是改完之后的帧内偏移 */
export interface Span {
  start: number;
  len: number;
  kind: "insert" | "replace" | "computed";
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
  filter: FilterConfig;
  mutate: MutationConfig;
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
  /** 解析成功但被筛选规则排除的行数 */
  filteredOut: number;
  /** 修改规则在执行期遇到的问题次数（偏移越界、区间冲突） */
  mutationIssues: number;
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
  spans: Span[];
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

// ── 预检 ────────────────────────────────────────────────────

export type Severity = "error" | "warn";

export interface Problem {
  severity: Severity;
  /** 问题出在哪一块配置 */
  area: string;
  message: string;
}

// ── 解析规则推测 ────────────────────────────────────────────

export interface Guess {
  config: ParseConfig;
  /** 说给使用者听的一句话，讲清楚软件替他做了什么决定 */
  summary: string;
}

// ── 配置档 ──────────────────────────────────────────────────

export interface Profile {
  version: number;
  name: string;
  config: SendConfig;
}

// ── 命令封装 ────────────────────────────────────────────────

export const openFile = (path: string) => invoke<FileInfo>("open_file", { path });

export const closeFile = () => invoke<void>("close_file");

export const fileInfo = () => invoke<FileInfo | null>("file_info");

export const preview = (
  start: number,
  count: number,
  config: ParseConfig,
  filter: FilterConfig,
) => invoke<LinePreview[]>("preview", { start, count, config, filter });

/** 按已打开文件的实际内容推测解析规则。推不出来时返回 null。 */
export const guessParse = (config: ParseConfig) =>
  invoke<Guess | null>("guess_parse", { config });

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

export const preflightCheck = (config: SendConfig) =>
  invoke<Problem[]>("preflight_check", { config });

export const saveProfile = (path: string, name: string, config: SendConfig) =>
  invoke<void>("save_profile", { path, name, config });

export const loadProfile = (path: string) =>
  invoke<Profile>("load_profile", { path });

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

export const defaultFilterConfig = (): FilterConfig => ({ rules: [] });

export const defaultMutationConfig = (): MutationConfig => ({ rules: [] });

/** 新增一条修改规则时的初始形态 */
export function newMutationOp(kind: MutationOp["kind"]): MutationOp {
  switch (kind) {
    case "insert":
      return { kind, offset: 0, value: "" };
    case "replace":
      return { kind, offset: 0, value: "" };
    case "delete":
      return { kind, offset: 0, length: 1 };
    case "sequence":
      return {
        kind,
        offset: 0,
        width: "w2",
        endian: "big",
        start: 0,
        step: 1,
        resetEachLoop: false,
      };
    case "timestamp":
      return {
        kind,
        offset: 0,
        width: "w4",
        endian: "big",
        unit: "millis",
        epoch: "unix",
      };
    case "length":
      return {
        kind,
        offset: 0,
        width: "w2",
        endian: "big",
        range: { start: 0, end: 0 },
        includeSelf: false,
      };
    case "checksum":
      return {
        kind,
        offset: -2,
        algorithm: "crc16Ccitt",
        endian: "big",
        range: { start: 0, end: -2 },
      };
  }
}

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

/**
 * 发送速率，帧/秒。
 *
 * 低速段保留一位小数：2 秒一帧是 0.5 帧/秒，四舍五入成整数就成了 0 ——
 * 明明在发，读数却说没发。高速段小数位没有意义，整数加千位分隔更好认。
 */
export function formatRate(r: number): string {
  if (r === 0) return "0";
  if (r < 10) return r.toFixed(1);
  return formatCount(Math.round(r));
}

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
