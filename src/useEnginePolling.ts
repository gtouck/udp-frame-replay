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
 * 速率的测量窗口，同时也是它的刷新周期。
 *
 * 原先按单次轮询间隔（50ms）算，两头都不成立：窗口那么短，轮询自身的抖动
 * ——这一拍 47ms、下一拍 62ms——会被直接放大成速率误差，读数本身就是噪声；
 * 而且一秒推二十次，再准也没人读得出来。窗口拉到一秒，噪声降二十倍，
 * 刷新周期也正好落在人眼能跟上的节奏上。
 */
const RATE_WINDOW_MS = 1000;

/** 头一个读数用短窗口先给出来，免得刚开始发送的一秒里显示 0，看着像没发出去 */
const RATE_FIRST_MS = 200;

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
  /** 速率窗口的起点。null 表示还没开始计（未运行，或刚从暂停恢复） */
  const rateAnchor = useRef<{ at: number; frames: number } | null>(null);
  /** 本轮任务是否已经给出过读数 —— 决定用短窗口还是正常窗口 */
  const rateSettled = useRef(false);

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
      // 暂停、停止、结束都不该继续显示上一刻的速率
      if (!snap || snap.state !== "running") {
        rateAnchor.current = null;
        rateSettled.current = false;
        setRate(0);
        return;
      }

      const now = performance.now();
      const anchor = rateAnchor.current;
      if (!anchor) {
        rateAnchor.current = { at: now, frames: snap.sentFrames };
        return;
      }

      // 窗口没走满就什么都不做：读数保持上一次的值，不跟着轮询跳
      const dt = now - anchor.at;
      if (dt < (rateSettled.current ? RATE_WINDOW_MS : RATE_FIRST_MS)) return;

      setRate(Math.max(0, ((snap.sentFrames - anchor.frames) * 1000) / dt));
      rateSettled.current = true;
      rateAnchor.current = { at: now, frames: snap.sentFrames };
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
