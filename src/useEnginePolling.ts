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

/**
 * 一个窗口里至少要攒够多少帧，攒不够就把窗口拉长。
 *
 * 一秒窗口在高速下绰绰有余，到了秒级间隔就不够看了：1 帧/秒时窗口里只有一帧，
 * 多一帧少一帧就是 100% 的误差，读数在 0 和 1 之间来回跳。窗口里的帧数决定了
 * 量化误差（约 1/N），攒够几帧才谈得上"平均速率"。
 */
const RATE_MIN_FRAMES = 5;

/**
 * 窗口拉长的上限。
 *
 * 再稀也不能无限等 —— 读数太旧就不再反映"现在"了。到了上限就按实际帧数出数，
 * 哪怕只有一帧。
 */
const RATE_MAX_WINDOW_MS = 5000;

/** 头一个读数用短窗口先给出来，免得刚开始发送的头几秒里显示 0，看着像没发出去 */
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
  /** 最近一次观察到帧数变化的时刻。窗口的收尾对齐到它，理由见 updateRate。 */
  const rateLastChange = useRef<{ at: number; frames: number } | null>(null);

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
        rateLastChange.current = null;
        rateSettled.current = false;
        setRate(0);
        return;
      }

      const now = performance.now();

      if (rateLastChange.current?.frames !== snap.sentFrames) {
        rateLastChange.current = { at: now, frames: snap.sentFrames };
      }

      const anchor = rateAnchor.current;
      if (!anchor) {
        rateAnchor.current = { at: now, frames: snap.sentFrames };
        return;
      }

      // 窗口没走满就什么都不做：读数保持上一次的值，不跟着轮询跳
      const dt = now - anchor.at;
      const frames = snap.sentFrames - anchor.frames;

      if (!rateSettled.current) {
        // 还没出过读数：等到真有帧可算就先给一个。
        // 秒级间隔下第一帧要过一秒才来，这时给出的 1 帧/1 秒已经是对的估计。
        if (dt < RATE_FIRST_MS || frames === 0) return;
      } else {
        if (dt < RATE_WINDOW_MS) return;
        // 帧太稀就继续攒，直到够数或撞上窗口上限
        if (frames < RATE_MIN_FRAMES && dt < RATE_MAX_WINDOW_MS) return;
      }

      rateSettled.current = true;

      // 窗口里一帧都没有：确实就是 0，按墙上时间重新起窗
      const change = rateLastChange.current;
      if (frames <= 0 || !change || change.at <= anchor.at) {
        setRate(0);
        rateAnchor.current = { at: now, frames: snap.sentFrames };
        return;
      }

      // 收尾对齐到"最后一帧到达的时刻"而不是"现在"。
      //
      // 误差来自帧数是整数而时间是连续的：按墙上时间截断，2 秒一帧的配置在
      // 5 秒窗口里时而 2 帧时而 3 帧，读数在 0.4 和 0.6 之间来回跳。两端都落在
      // 帧到达的时刻上，窗口就总是跨整数帧，这项误差直接消失 —— 剩下的只有
      // 轮询本身 50ms 的观测精度。
      setRate((frames * 1000) / (change.at - anchor.at));
      rateAnchor.current = change;
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
