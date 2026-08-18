import { create } from "zustand";
import {
  newMutationOp,
  type Condition,
  type EngineSnapshot,
  type ErrorGroup,
  type FileInfo,
  type FilterConfig,
  type FilterRule,
  type MutationConfig,
  type MutationOp,
  type MutationRule,
  type LogEntry,
  type LogLevel,
  type PacingConfig,
  type ParseConfig,
  type Problem,
  type SendConfig,
  type PrefixRule,
  type SentFrame,
  type TargetConfig,
} from "./api";
import { restoreConfig } from "./session";

interface AppStore {
  file: FileInfo | null;
  setFile: (f: FileInfo | null) => void;

  parse: ParseConfig;
  filter: FilterConfig;
  mutate: MutationConfig;

  /**
   * 预览版本号。解析规则或筛选规则一改就自增，用来作废预览缓存 ——
   * 规则变了，屏幕上每一行的标注都跟着变，留着旧标注只会让人看到已经不成立的结果。
   */
  previewVersion: number;

  setParse: (patch: Partial<ParseConfig>) => void;
  setPrefix: (patch: Partial<Extract<PrefixRule, { mode: "fields" }>>) => void;
  setPrefixMode: (mode: PrefixRule["mode"]) => void;
  setSkipChars: (n: number) => void;

  addFilterRule: (kind: Condition["kind"]) => void;
  updateFilterRule: (i: number, patch: Partial<FilterRule>) => void;
  updateFilterCondition: (i: number, patch: Partial<Condition>) => void;
  removeFilterRule: (i: number) => void;

  addMutationRule: (kind: MutationOp["kind"]) => void;
  updateMutationRule: (i: number, patch: Partial<MutationRule>) => void;
  updateMutationOp: (i: number, patch: Record<string, unknown>) => void;
  moveMutationRule: (i: number, delta: number) => void;
  removeMutationRule: (i: number) => void;

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

  problems: Problem[];
  setProblems: (p: Problem[]) => void;

  /** 载入配置档时整体替换五部分配置 */
  applyConfig: (c: SendConfig) => void;

  /** 状态栏上的一句话。默认按错误处理 —— 大多数调用点传的都是异常。 */
  notice: { text: string; level: "info" | "error" } | null;
  setNotice: (text: string | null, level?: "info" | "error") => void;
}

const newCondition = (kind: Condition["kind"]): Condition =>
  kind === "field"
    ? { kind: "field", index: 0, op: "equals", value: "" }
    : { kind: "bytes", offset: 0, value: "", mask: null };

/**
 * 上次退出时的配置。读一次就够 —— 之后一切以内存里的状态为准，
 * 由 `useSessionPersist` 单向写回。
 */
const restored = restoreConfig();

export const useStore = create<AppStore>((set) => ({
  file: null,
  setFile: (file) => set({ file }),

  parse: restored.parse,
  filter: restored.filter,
  mutate: restored.mutate,
  previewVersion: 0,

  setParse: (patch) =>
    set((s) => ({
      parse: { ...s.parse, ...patch },
      previewVersion: s.previewVersion + 1,
    })),

  setPrefix: (patch) =>
    set((s) => {
      if (s.parse.prefix.mode !== "fields") return s;
      return {
        parse: { ...s.parse, prefix: { ...s.parse.prefix, ...patch } },
        previewVersion: s.previewVersion + 1,
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
        previewVersion: s.previewVersion + 1,
      };
    }),

  setSkipChars: (n) =>
    set((s) => {
      if (s.parse.prefix.mode !== "chars") return s;
      return {
        parse: { ...s.parse, prefix: { mode: "chars", skipChars: n } },
        previewVersion: s.previewVersion + 1,
      };
    }),

  addFilterRule: (kind) =>
    set((s) => ({
      filter: {
        rules: [
          ...s.filter.rules,
          { condition: newCondition(kind), negate: false, enabled: true },
        ],
      },
      previewVersion: s.previewVersion + 1,
    })),

  updateFilterRule: (i, patch) =>
    set((s) => ({
      filter: {
        rules: s.filter.rules.map((r, j) => (i === j ? { ...r, ...patch } : r)),
      },
      previewVersion: s.previewVersion + 1,
    })),

  updateFilterCondition: (i, patch) =>
    set((s) => ({
      filter: {
        rules: s.filter.rules.map((r, j) =>
          i === j
            ? ({ ...r, condition: { ...r.condition, ...patch } } as FilterRule)
            : r,
        ),
      },
      previewVersion: s.previewVersion + 1,
    })),

  removeFilterRule: (i) =>
    set((s) => ({
      filter: { rules: s.filter.rules.filter((_, j) => j !== i) },
      previewVersion: s.previewVersion + 1,
    })),

  addMutationRule: (kind) =>
    set((s) => ({
      mutate: {
        rules: [
          ...s.mutate.rules,
          { op: newMutationOp(kind), condition: null, enabled: true },
        ],
      },
    })),

  updateMutationRule: (i, patch) =>
    set((s) => ({
      mutate: {
        rules: s.mutate.rules.map((r, j) => (i === j ? { ...r, ...patch } : r)),
      },
    })),

  updateMutationOp: (i, patch) =>
    set((s) => ({
      mutate: {
        rules: s.mutate.rules.map((r, j) =>
          i === j ? ({ ...r, op: { ...r.op, ...patch } } as MutationRule) : r,
        ),
      },
    })),

  /** 顺序有意义：阶段二严格按这个顺序执行，校验和必须排在最后 */
  moveMutationRule: (i, delta) =>
    set((s) => {
      const j = i + delta;
      if (j < 0 || j >= s.mutate.rules.length) return s;
      const rules = [...s.mutate.rules];
      [rules[i], rules[j]] = [rules[j], rules[i]];
      return { mutate: { rules } };
    }),

  removeMutationRule: (i) =>
    set((s) => ({ mutate: { rules: s.mutate.rules.filter((_, j) => j !== i) } })),

  target: restored.target,
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

  pacing: restored.pacing,
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

  problems: [],
  setProblems: (problems) => set({ problems }),

  applyConfig: (c) =>
    set((s) => ({
      parse: c.parse,
      filter: c.filter ?? { rules: [] },
      mutate: c.mutate ?? { rules: [] },
      target: c.target,
      pacing: c.pacing,
      // 解析与筛选都换了，预览缓存必须整体作废
      previewVersion: s.previewVersion + 1,
    })),

  notice: null,
  setNotice: (text, level = "error") =>
    set({ notice: text === null ? null : { text, level } }),
}));

/** 把散在 store 里的五部分配置凑成后端要的整体 */
export const configOf = (s: AppStore): SendConfig => ({
  parse: s.parse,
  filter: s.filter,
  mutate: s.mutate,
  target: s.target,
  pacing: s.pacing,
});

/** 有阻止启动的问题吗 */
export const hasBlockingProblem = (problems: Problem[]) =>
  problems.some((p) => p.severity === "error");

/** 当前是否有正在运行或暂停的任务 */
export const isActive = (e: EngineSnapshot | null) =>
  e !== null && (e.state === "running" || e.state === "paused");
