import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X, Copy } from "lucide-react";
import { isTauri } from "../api/backend";
import { useT } from "../i18n";
import { useAppStore } from "../store/app";
import { Logo } from "./Logo";

export function TitleBar() {
  const t = useT();
  const tabs = useAppStore((state) => state.tabs);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    const win = getCurrentWindow();
    let disposed = false;
    void win.isMaximized().then((value) => !disposed && setMaximized(value));
    const unlisten = win.onResized(async () => {
      const value = await win.isMaximized().catch(() => false);
      if (!disposed) setMaximized(value);
    });
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
    };
  }, []);

  const minimize = () => {
    if (isTauri()) void getCurrentWindow().minimize();
  };
  const toggleMaximize = () => {
    if (!isTauri()) return;
    const win = getCurrentWindow();
    void win.toggleMaximize().then(() => win.isMaximized().then(setMaximized));
  };
  const close = () => {
    if (isTauri()) void getCurrentWindow().close();
  };

  return (
    <header
      className="titlebar"
      data-tauri-drag-region
      onDoubleClick={(event) => {
        if ((event.target as HTMLElement).closest(".window-controls")) return;
        toggleMaximize();
      }}
    >
      <Logo size={18} />
      <span className="titlebar-title" data-tauri-drag-region>
        FileZilla Modern
      </span>
      <span className="titlebar-badge">
        {tabs.length > 0 ? t("titlebar.connections", { count: tabs.length }) : t("titlebar.ready")}
      </span>
      <div className="titlebar-spacer" data-tauri-drag-region />
      <div className="window-controls">
        <button className="window-control" title="Minimize" onClick={minimize}>
          <Minus size={14} strokeWidth={1.6} />
        </button>
        <button className="window-control" title={maximized ? "Restore" : "Maximize"} onClick={toggleMaximize}>
          {maximized ? <Copy size={12} strokeWidth={1.6} /> : <Square size={11} strokeWidth={1.6} />}
        </button>
        <button className="window-control close" title="Close" onClick={close}>
          <X size={14} strokeWidth={1.6} />
        </button>
      </div>
    </header>
  );
}
