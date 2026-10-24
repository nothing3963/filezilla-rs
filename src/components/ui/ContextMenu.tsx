import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/cn";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onClick?: () => void;
  danger?: boolean;
  disabled?: boolean;
  shortcut?: string;
  separator?: boolean;
  section?: string;
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [position, setPosition] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const rect = element.getBoundingClientRect();
    const left = Math.min(x, window.innerWidth - rect.width - 8);
    const top = Math.min(y, window.innerHeight - rect.height - 8);
    setPosition({ left: Math.max(8, left), top: Math.max(8, top) });
  }, [x, y]);

  useEffect(() => {
    const close = (event: Event) => {
      if (event.target instanceof Node && ref.current?.contains(event.target)) return;
      onClose();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", close, true);
    window.addEventListener("contextmenu", close, true);
    window.addEventListener("resize", onClose);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close, true);
      window.removeEventListener("contextmenu", close, true);
      window.removeEventListener("resize", onClose);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  return createPortal(
    <div
      ref={ref}
      className="context-menu"
      style={{ left: position.left, top: position.top }}
      onContextMenu={(event) => event.preventDefault()}
    >
      {items.map((item, index) =>
        item.separator ? (
          <div key={`sep-${index}`} className="menu-separator" />
        ) : item.section ? (
          <div key={`section-${index}`} className="menu-label">
            {item.section}
          </div>
        ) : (
          <button
            key={`${item.label}-${index}`}
            className={cn("menu-item", item.danger && "danger")}
            disabled={item.disabled}
            onClick={() => {
              item.onClick?.();
              onClose();
            }}
          >
            {item.icon}
            <span className="flex-1 truncate">{item.label}</span>
            {item.shortcut && <span className="text-[10px] text-[var(--text-faint)]">{item.shortcut}</span>}
          </button>
        ),
      )}
    </div>,
    document.body,
  );
}
