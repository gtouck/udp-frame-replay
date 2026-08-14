import { useEffect, useRef, useState } from "react";
import { THEME_CHANGE_EVENT } from "../theme";

interface Props {
  /** 最近若干次的实际帧间隔，单位微秒。空数组表示尚未发送。 */
  samples: number[];
  /** 配置的目标间隔，单位微秒。作为基线。 */
  targetUs: number;
}

/**
 * 时序带：把真实的帧间隔画成刻度。
 *
 * 这是工具的立场 —— 微秒级定时在普通操作系统上是软实时，抖动无法根除。
 * 与其把界面做得像精准，不如把真实表现直接画出来：
 * 刻度贴着基线就是稳，冒出来就是那一帧被系统调度耽误了。
 */
export default function TimingTape({ samples, targetUs }: Props) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [themeRevision, setThemeRevision] = useState(0);

  useEffect(() => {
    const redraw = () => setThemeRevision((revision) => revision + 1);
    window.addEventListener(THEME_CHANGE_EVENT, redraw);
    return () => window.removeEventListener(THEME_CHANGE_EVENT, redraw);
  }, []);

  useEffect(() => {
    const cv = ref.current;
    if (!cv) return;
    const ctx = cv.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const w = cv.clientWidth;
    const h = cv.clientHeight;
    cv.width = w * dpr;
    cv.height = h * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const css = getComputedStyle(document.documentElement);
    const baseline = css.getPropertyValue("--ink-faint").trim() || "#454d59";
    const good = css.getPropertyValue("--sig-live").trim() || "#7de0c4";
    const bad = css.getPropertyValue("--sig-replace").trim() || "#e8b54a";

    // 基线：目标间隔
    const baseY = h - 2.5;
    ctx.strokeStyle = baseline;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, baseY + 0.5);
    ctx.lineTo(w, baseY + 0.5);
    ctx.stroke();

    if (samples.length === 0 || targetUs <= 0) return;

    // 纵轴按「偏离目标的比例」缩放，满格 = 偏离一倍
    const n = Math.min(samples.length, w);
    const step = w / n;
    const start = samples.length - n;

    // 每个样本至少画一格：齐整的梳齿本身就说明「稳」，
    // 全空的带子只会让人以为坏了。
    const MIN_TICK = 2;

    for (let i = 0; i < n; i++) {
      const dev = Math.abs(samples[start + i] - targetUs) / targetUs;
      const tick = MIN_TICK + Math.min(dev, 1) * (h - 4 - MIN_TICK);
      ctx.strokeStyle = dev > 0.25 ? bad : good;
      const x = Math.floor(i * step) + 0.5;
      ctx.beginPath();
      ctx.moveTo(x, baseY);
      ctx.lineTo(x, baseY - tick);
      ctx.stroke();
    }
  }, [samples, targetUs, themeRevision]);

  return (
    <canvas
      className="tape"
      ref={ref}
      role="img"
      aria-label="发送时序抖动"
      title="发送时序：刻度越高，该帧偏离目标间隔越多"
    />
  );
}
