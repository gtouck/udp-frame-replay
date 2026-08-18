/**
 * 会话记忆：把配置和最近打开的文件存在本地，下次启动直接接着用。
 *
 * 为什么要有这个：配置档（ProfileSection）是「显式、可携带」的存档，适合把一套
 * 规则带到另一台机器；但它要求使用者主动想起来去存。日常使用里更常见的是
 * 「调了半天规则，关掉软件，明天接着调」 —— 这种场景不该逼人去点保存。
 *
 * 两者不冲突：这里做的是自动快照，配置档做的是命名存档。
 */

import {
  defaultFilterConfig,
  defaultMutationConfig,
  defaultPacingConfig,
  defaultParseConfig,
  defaultTargetConfig,
  type SendConfig,
} from "./api";

const CONFIG_KEY = "frame-replay.config";
const RECENT_KEY = "frame-replay.recent";

/** 存档格式版本。改动配置结构时加一，旧快照直接丢弃而不是硬塞。 */
const VERSION = 1;

/** 最近文件保留几条。多了没人翻，少了不够用。 */
const RECENT_MAX = 8;

export const defaultConfig = (): SendConfig => ({
  parse: defaultParseConfig(),
  filter: defaultFilterConfig(),
  mutate: defaultMutationConfig(),
  target: defaultTargetConfig(),
  pacing: defaultPacingConfig(),
});

/**
 * 读回上次的配置。
 *
 * 任何一步出问题都退回默认值 —— 一份坏掉的快照绝不能让软件打不开，
 * 那比丢配置严重得多。
 */
export function restoreConfig(): SendConfig {
  const fallback = defaultConfig();
  try {
    const raw = localStorage.getItem(CONFIG_KEY);
    if (!raw) return fallback;

    const saved = JSON.parse(raw) as { version?: number; config?: SendConfig };
    if (saved.version !== VERSION || !saved.config) return fallback;

    // 逐部分合并：快照里缺的部分用默认值补，多的部分忽略。
    // 这样新增一个配置项不必立刻升版本号作废所有人的快照。
    const c = saved.config;
    return {
      parse: { ...fallback.parse, ...c.parse },
      filter: { ...fallback.filter, ...c.filter },
      mutate: { ...fallback.mutate, ...c.mutate },
      target: { ...fallback.target, ...c.target },
      pacing: { ...fallback.pacing, ...c.pacing },
    };
  } catch {
    return fallback;
  }
}

export function persistConfig(config: SendConfig) {
  try {
    localStorage.setItem(CONFIG_KEY, JSON.stringify({ version: VERSION, config }));
  } catch {
    /* 隐私模式或配额满：记不住而已，不影响本次使用 */
  }
}

export function readRecentFiles(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    const list = raw ? (JSON.parse(raw) as unknown) : null;
    if (!Array.isArray(list)) return [];
    return list.filter((p): p is string => typeof p === "string").slice(0, RECENT_MAX);
  } catch {
    return [];
  }
}

/** 把 path 挪到最前；已存在则去重，不产生重复条目。 */
export function pushRecentFile(path: string): string[] {
  const next = [path, ...readRecentFiles().filter((p) => p !== path)].slice(
    0,
    RECENT_MAX,
  );
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    /* 同上 */
  }
  return next;
}

/** 文件打不开了（被移走、改名）就从列表里剔掉，免得反复点到死链。 */
export function dropRecentFile(path: string): string[] {
  const next = readRecentFiles().filter((p) => p !== path);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    /* 同上 */
  }
  return next;
}
