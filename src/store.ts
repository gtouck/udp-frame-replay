import { create } from "zustand";
import {
  defaultParseConfig,
  type FileInfo,
  type ParseConfig,
  type PrefixRule,
} from "./api";

export type RunState = "idle" | "running" | "paused" | "error";

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

  runState: RunState;
  setRunState: (s: RunState) => void;

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

  runState: "idle",
  setRunState: (runState) => set({ runState }),

  notice: null,
  setNotice: (notice) => set({ notice }),
}));
