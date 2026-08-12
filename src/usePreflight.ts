import { useEffect } from "react";
import { preflightCheck } from "./api";
import { useStore } from "./store";

/** 配置改动后的静默期。边打字边校验会闪个不停，也没必要。 */
const DEBOUNCE_MS = 350;

/**
 * 配置变动时预检。
 *
 * 问题在按下「开始发送」之前就摆出来 —— 一次列全，而不是报第一个就停，
 * 免得来回改好几轮。
 */
export function usePreflight() {
  const parse = useStore((s) => s.parse);
  const filter = useStore((s) => s.filter);
  const mutate = useStore((s) => s.mutate);
  const target = useStore((s) => s.target);
  const pacing = useStore((s) => s.pacing);
  const file = useStore((s) => s.file);
  const setProblems = useStore((s) => s.setProblems);

  useEffect(() => {
    let alive = true;
    const id = setTimeout(() => {
      preflightCheck({ parse, filter, mutate, target, pacing })
        .then((p) => {
          if (alive) setProblems(p);
        })
        .catch(() => {
          /* 应用退出途中可能失败，忽略 */
        });
    }, DEBOUNCE_MS);

    return () => {
      alive = false;
      clearTimeout(id);
    };
  }, [parse, filter, mutate, target, pacing, file, setProblems]);
}
