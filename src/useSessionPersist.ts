import { useEffect } from "react";
import { persistConfig } from "./session";
import { configOf, useStore } from "./store";

/** 写回间隔。输入框每敲一个字符都会改状态，不缓一下会写得过于频繁。 */
const DEBOUNCE_MS = 400;

/**
 * 把配置的每次改动写回本地，下次启动自动接上。
 *
 * 订阅整个 store 而不是逐项挂 effect —— 配置项有二十来个，
 * 漏挂一个的后果是「某个设置莫名其妙记不住」，这种 bug 很难被发现。
 *
 * 但整体订阅会被引擎轮询连带唤醒（运行时每秒十次），所以先比一次序列化结果：
 * 配置真的没变就直接返回，不去重置防抖计时器 —— 否则一旦开始发送，
 * 计时器被轮询无限推后，配置反而永远写不下去。
 */
export function useSessionPersist() {
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let last = JSON.stringify(configOf(useStore.getState()));

    const flush = () => {
      clearTimeout(timer);
      persistConfig(configOf(useStore.getState()));
    };

    const unsubscribe = useStore.subscribe((state) => {
      const json = JSON.stringify(configOf(state));
      if (json === last) return;
      last = json;
      clearTimeout(timer);
      timer = setTimeout(flush, DEBOUNCE_MS);
    });

    // 关窗口时 effect 的清理函数不一定跑得到，补一道
    window.addEventListener("beforeunload", flush);

    return () => {
      window.removeEventListener("beforeunload", flush);
      unsubscribe();
      flush();
    };
  }, []);
}
