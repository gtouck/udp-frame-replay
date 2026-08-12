import { useState, type ReactNode } from "react";

interface Props {
  name: string;
  defaultOpen?: boolean;
  children: ReactNode;
}

/** 配置面板分组。标题用条压体大写，仿仪器前面板的丝印标签。 */
export default function Group({ name, defaultOpen = true, children }: Props) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <div className="group">
      <button
        className="group-head"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="group-caret">▶</span>
        {name}
      </button>
      {open && <div className="group-body">{children}</div>}
    </div>
  );
}
