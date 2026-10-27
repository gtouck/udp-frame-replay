import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent,
} from "react";
import { createPortal } from "react-dom";
import { copyText } from "../clipboard";
import { useStore } from "../store";

type RowAction = { label: string; run: () => void | Promise<void> };
type MenuState = { x: number; y: number; actions: RowAction[] };

export function useRowMenu(resetKey?: unknown) {
  const [selected, select] = useState<string | number | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const close = useCallback(() => setMenu(null), []);

  useEffect(() => {
    select(null);
    close();
  }, [resetKey, close]);

  const open = (
    event: MouseEvent,
    key: string | number,
    getText: () => string | Promise<string>,
    inspect?: RowAction,
  ) => {
    event.preventDefault();
    select(key);
    setMenu({
      x: event.clientX,
      y: event.clientY,
      actions: [
        {
          label: "复制选中行",
          run: async () => {
            await copyText(await getText());
            useStore.getState().setNotice("已复制选中行", "info");
          },
        },
        ...(inspect ? [inspect] : []),
      ],
    });
  };

  return {
    selected,
    select,
    open,
    menu: menu && <RowMenu {...menu} onClose={close} />,
  };
}

function RowMenu({ x, y, actions, onClose }: MenuState & { onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.left = `${Math.max(4, Math.min(x, window.innerWidth - el.offsetWidth - 4))}px`;
    el.style.top = `${Math.max(4, Math.min(y, window.innerHeight - el.offsetHeight - 4))}px`;
    el.querySelector("button")?.focus();
  }, [x, y]);

  useEffect(() => {
    const dismiss = (event: Event) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" || event.key === "Tab") onClose();
    };
    window.addEventListener("pointerdown", dismiss);
    window.addEventListener("scroll", dismiss, true);
    window.addEventListener("resize", onClose);
    window.addEventListener("blur", onClose);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", dismiss);
      window.removeEventListener("scroll", dismiss, true);
      window.removeEventListener("resize", onClose);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  return createPortal(
    <div
      className="row-menu"
      role="menu"
      aria-label="选中行操作"
      ref={ref}
      style={{ left: x, top: y }}
      onKeyDown={(event) => {
        if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
        event.preventDefault();
        const buttons = Array.from(event.currentTarget.querySelectorAll("button"));
        const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const direction = event.key === "ArrowDown" ? 1 : -1;
        buttons[(current + direction + buttons.length) % buttons.length]?.focus();
      }}
    >
      {actions.map((action) => (
        <button
          key={action.label}
          role="menuitem"
          onClick={() => {
            onClose();
            void Promise.resolve().then(action.run).catch((error) => {
              useStore.getState().setNotice(String(error));
            });
          }}
        >
          {action.label}
        </button>
      ))}
    </div>,
    document.body,
  );
}
