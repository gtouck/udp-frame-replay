import { create } from "zustand";
import {
  defaultPacingConfig,
  defaultParseConfig,
  defaultTargetConfig,
  type EngineSnapshot,
  type ErrorGroup,
  type FileInfo,
  type LogEntry,
  type LogLevel,
  type PacingConfig,
  type ParseConfig,
  type PrefixRule,
  type SentFrame,
  type TargetConfig,
} from "./api";

interface AppStore {
  file: FileInfo | null;
  setFile: (f: FileInfo | null) => void;

  parse: ParseConfig;
  /** 配置版本号。每次改动自增，用来作废预览缓存。 */
  parseVersion: number;
  setParse: (patch: Partial<ParseConfig>) => void;
  setPrefix: (patch: Partial<Extract<PrefixRule, { mode: "fields" }>>) => void;
  setPrefixMode: (mode: PrefixRule["mode"]) => void;
  setSkipChars: (n: number) => void;

  target: TargetConfig;
  setTarget: (patch: Partial<TargetConfig>) => void;
  setTargetMode: (mode: TargetConfig["mode"]) => void;

  pacing: PacingConfig;
  setPacing: (patch: Partial<PacingConfig>) => void;

  /** 引擎快照，由轮询写入。null 表示当前没有任务。 */
  engine: EngineSnapshot | null;
  setEngine: (s: EngineSnapshot | null) => void;

  frames: SentFrame[];
  setFrames: (f: SentFrame[]) => void;

  logs: LogEntry[];
  setLogs: (f: (prev: LogEntry[]) => LogEntry[]) => void;

  errorGroups: ErrorGroup[];
  setErrorGroups: (g: ErrorGroup[]) => void;

  /** 实测发送速率，帧/秒。由前端从相邻两次快照算出。 */
  rate: number;
  setRate: (r: number) => void;

  logFilter: LogLevel | "all";
  setLogFilter: (f: LogLevel | "all") => void;

  notice: string | null;
  setNotice: (m: string | null) => void;
}

export const useStore = create<AppStore>((set) => ({
  file: null,
  setFile: (file) => set({ file }),

  parse: defaultParseConfig(),
  parseVersion: 0,
  setParse: (patch) =>
    set((s) => ({
      parse: { ...s.parse, ...patch },
      parseVersion: s.parseVersion + 1,
    })),

  setPrefix: (patch) =>
    set((s) => {
      if (s.parse.prefix.mode !== "fields") return s;
      return {
        parse: { ...s.parse, prefix: { ...s.parse.prefix, ...patch } },
        parseVersion: s.parseVersion + 1,
      };
    }),

  setPrefixMode: (mode) =>
    set((s) => {
      if (s.parse.prefix.mode === mode) return s;
      const prefix: PrefixRule =
        mode === "fields"
          ? {
              mode: "fields",
              delimiter: { kind: "whitespace" },
              collapse: true,
              skipFields: 0,
            }
          : { mode: "chars", skipChars: 0 };
      return {
        parse: { ...s.parse, prefix },
        parseVersion: s.parseVersion + 1,
      };
    }),

  setSkipChars: (n) =>
    set((s) => {
      if (s.parse.prefix.mode !== "chars") return s;
      return {
        parse: { ...s.parse, prefix: { mode: "chars", skipChars: n } },
        parseVersion: s.parseVersion + 1,
      };
    }),

  target: defaultTargetConfig(),
  setTarget: (patch) =>
    set((s) => ({ target: { ...s.target, ...patch } as TargetConfig })),

  setTargetMode: (mode) =>
    set((s) => {
      if (s.target.mode === mode) return s;
      const common = {
        bindAddr: s.target.bindAddr,
        bindPort: s.target.bindPort,
        sendBufferBytes: s.target.sendBufferBytes,
      };
      const target: TargetConfig =
        mode === "unicast"
          ? { mode: "unicast", host: "127.0.0.1", port: s.target.port, ...common }
          : {
              mode: "multicast",
              group: "239.255.0.1",
              port: s.target.port,
              interface: null,
              ttl: 1,
              loopback: true,
              ...common,
            };
      return { target };
    }),

  pacing: defaultPacingConfig(),
  setPacing: (patch) => set((s) => ({ pacing: { ...s.pacing, ...patch } })),

  engine: null,
  setEngine: (engine) => set({ engine }),

  frames: [],
  setFrames: (frames) => set({ frames }),

  logs: [],
  setLogs: (fn) => set((s) => ({ logs: fn(s.logs) })),

  errorGroups: [],
  setErrorGroups: (errorGroups) => set({ errorGroups }),

  rate: 0,
  setRate: (rate) => set({ rate }),

  logFilter: "all",
  setLogFilter: (logFilter) => set({ logFilter }),

  notice: null,
  setNotice: (notice) => set({ notice }),
}));

/** 当前是否有正在运行或暂停的任务 */
export const isActive = (e: EngineSnapshot | null) =>
  e !== null && (e.state === "running" || e.state === "paused");
