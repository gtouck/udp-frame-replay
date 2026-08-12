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
 * 配置一改（version 自增）整个缓存立即作废 —— 解析规则变了，屏幕上每一行的
 * 标注都跟着变，缓存旧标注只会让人看到已经不成立的结果。
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
    [config, filter],
  );

  const getLine = useCallback((line: number): LinePreview | undefined => {
    return cache.current.get(Math.floor(line / CHUNK))?.[line % CHUNK];
  }, []);

  return { requestRange, getLine };
}
