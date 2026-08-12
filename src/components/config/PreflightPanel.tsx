import { useStore } from "../../store";

/**
 * 预检结果。
 *
 * 放在配置面板顶部 —— 问题就挨着造成它的那些控件，改起来不用来回找。
 */
export default function PreflightPanel() {
  const problems = useStore((s) => s.problems);
  const file = useStore((s) => s.file);

  // 没打开文件时只会报「尚未打开文件」，那是废话，不必占地方
  const shown = file ? problems : problems.filter((p) => p.area !== "文件");
  if (shown.length === 0) return null;

  return (
    <div className="preflight">
      {shown.map((p, i) => (
        <div className="problem" data-severity={p.severity} key={i}>
          <span className="problem-area">{p.area}</span>
          <span className="problem-msg">{p.message}</span>
        </div>
      ))}
    </div>
  );
}
