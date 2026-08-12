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

// ── 命令封装 ────────────────────────────────────────────────

export const openFile = (path: string) => invoke<FileInfo>("open_file", { path });

export const closeFile = () => invoke<void>("close_file");

export const fileInfo = () => invoke<FileInfo | null>("file_info");

export const preview = (start: number, count: number, config: ParseConfig) =>
  invoke<LinePreview[]>("preview", { start, count, config });

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

// ── 展示辅助 ────────────────────────────────────────────────

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1048576).toFixed(1)} MB`;
  return `${(n / 1073741824).toFixed(2)} GB`;
}

export const formatCount = (n: number) => n.toLocaleString("en-US");
