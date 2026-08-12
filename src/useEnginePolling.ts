import { useEffect, useRef } from "react";
import {
  engineStatus,
  errorGroups,
  logEntries,
  recentFrames,
  type EngineSnapshot,
} from "./api";
import { useStore } from "./store";

/** 状态与帧快照的轮询间隔。高频发送时界面本来就是采样显示。 */
const FAST_MS = 50;

/** 日志与错误聚合的轮询间隔，没必要跟状态一样勤 */
const SLOW_EVERY = 6;

/** 界面上最多保留多少条日志 */
const LOG_CAP = 2000;

/**
 * 引擎状态轮询。
 *
 * 由前端定时拉取而不是后端推送：发送线程每秒可能发十万帧，
 * 让它往 IPC 里推事件会直接把通道淹掉。拉取的频率由界面说了算。
 */
export function useEnginePolling() {
  const setEngine = useStore((s) => s.setEngine);
  const setFrames = useStore((s) => s.setFrames);
  const setLogs = useStore((s) => s.setLogs);
  const setErrorGroups = useStore((s) => s.setErrorGroups);
  const setRate = useStore((s) => s.setRate);

  // 用 ref 保存跨轮次的状态，避免把 effect 依赖搞复杂
  const busy = useRef(false);
  const tick = useRef(0);
  const lastSeq = useRef(0);
  const lastSample = useRef<{ at: number; frames: number } | null>(null);

  useEffect(() => {
    let alive = true;

    const poll = async () => {
      if (busy.current) return; // 上一轮还没回来，跳过，绝不排队堆积
      busy.current = true;

      try {
        const snap = await engineStatus();
        if (!alive) return;
        setEngine(snap);
        updateRate(snap);

        if (snap) {
          setFrames(await recentFrames(60));
        }

        if (tick.current % SLOW_EVERY === 0) {
          const fresh = await logEntries(lastSeq.current, 500);
          if (fresh.length) {
            lastSeq.current = fresh[fresh.length - 1].seq + 1;
            setLogs((prev) => [...prev, ...fresh].slice(-LOG_CAP));
          }
          setErrorGroups(await errorGroups());
        }
      } catch {
        /* 命令在应用退出途中可能失败，忽略即可 */
      } finally {
        busy.current = false;
        tick.current++;
      }
    };

    const updateRate = (snap: EngineSnapshot | null) => {
      if (!snap || snap.state !== "running") {
        lastSample.current = null;
        setRate(0);
        return;
      }
      const now = performance.now();
      const prev = lastSample.current;
      lastSample.current = { at: now, frames: snap.sentFrames };
      if (!prev) return;

      const dt = (now - prev.at) / 1000;
      if (dt > 0.02) {
        setRate(Math.max(0, (snap.sentFrames - prev.frames) / dt));
      }
    };

    const id = setInterval(poll, FAST_MS);
    void poll();

    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [setEngine, setFrames, setLogs, setErrorGroups, setRate]);

  /** 停止任务后重置日志游标，下次启动从头拉 */
  return {
    resetLogCursor: () => {
      lastSeq.current = 0;
      setLogs(() => []);
    },
  };
}
