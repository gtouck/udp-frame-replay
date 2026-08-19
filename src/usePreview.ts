import { useCallback, useEffect, useReducer, useRef } from "react";
import {
  preview,
  type FilterConfig,
  type LinePreview,
  type ParseConfig,
} from "./api";

/** 每次向后端取多少行。太小会请求频繁，太大会拖慢首屏。 */
const CHUNK = 200;

/**
 * 按需拉取并缓存预览数据。
 *
 * 文件或配置一变（version 自增）整个缓存立即作废 —— 屏幕上每一行的内容与标注
 * 都跟着变，缓存旧结果只会让人看到已经不成立的东西。
 */
export function usePreview(
  config: ParseConfig,
  filter: FilterConfig,
  version: number,
) {
  const [, force] = useReducer((x: number) => x + 1, 0);
  const cache = useRef(new Map<number, LinePreview[]>());
  const pending = useRef(new Set<number>());
  const versionRef = useRef(version);

  useEffect(() => {
    versionRef.current = version;
    cache.current.clear();
    pending.current.clear();
    force();
  }, [version]);

  const requestRange = useCallback(
    (startLine: number, endLine: number) => {
      const first = Math.floor(startLine / CHUNK);
      const last = Math.floor(endLine / CHUNK);

      for (let c = first; c <= last; c++) {
        if (cache.current.has(c) || pending.current.has(c)) continue;
        pending.current.add(c);

        const at = versionRef.current;
        preview(c * CHUNK, CHUNK, config, filter)
          .then((rows) => {
            if (at !== versionRef.current) return; // 配置已变，结果作废
            cache.current.set(c, rows);
            force();
          })
          .catch(() => {
            /* 文件未打开或已关闭，静默忽略 */
          })
          .finally(() => pending.current.delete(c));
      }
    },
    // version 也算依赖：缓存作废后要靠它的身份变化把调用方的取数 effect 重新跑一遍，
    // 否则换文件时 config 没变，屏幕上就一直停在空行。
    [config, filter, version],
  );

  const getLine = useCallback((line: number): LinePreview | undefined => {
    return cache.current.get(Math.floor(line / CHUNK))?.[line % CHUNK];
  }, []);

  return { requestRange, getLine };
}
