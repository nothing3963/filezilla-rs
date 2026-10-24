import { useEffect, useMemo, useRef, useState } from "react";
import { ScrollText, Search, Trash2, X } from "lucide-react";
import { useT } from "../i18n";
import { formatClock } from "../lib/format";
import { cn } from "../lib/cn";
import { useAppStore } from "../store/app";
import { IconButton, Switch } from "./ui/controls";
import { ContextMenu } from "./ui/ContextMenu";

export function LogPanel() {
  const t = useT();
  const logs = useAppStore((state) => state.logs);
  const settings = useAppStore((state) => state.settings);
  const clearLogs = useAppStore((state) => state.clearLogs);
  const updateSettings = useAppStore((state) => state.updateSettings);
  const [level, setLevel] = useState("all");
  const [search, setSearch] = useState("");
  const [autoScroll, setAutoScroll] = useState(true);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    return logs.filter((entry) => {
      if (level !== "all" && entry.level !== level) return false;
      if (query && !entry.message.toLowerCase().includes(query)) return false;
      return true;
    });
  }, [logs, level, search]);

  useEffect(() => {
    if (!autoScroll) return;
    const element = listRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [filtered.length, autoScroll]);

  return (
    <aside className="log-panel">
      <div className="log-header">
        <div className="log-title">
          <ScrollText size={14} />
          {t("log.title")}
          <span className="transfer-count">{logs.length}</span>
        </div>
        <div className="ml-auto flex items-center gap-1">
          <IconButton label={t("log.clear")} onClick={() => void clearLogs()}>
            <Trash2 />
          </IconButton>
          <IconButton
            label={t("common.close")}
            onClick={() => updateSettings({ showLogPanel: false })}
          >
            <X />
          </IconButton>
        </div>
      </div>

      <div className="log-toolbar">
        <select className="select h-[26px] text-[11px]" value={level} onChange={(event) => setLevel(event.target.value)}>
          <option value="all">{t("log.all")}</option>
          <option value="info">Info</option>
          <option value="warn">Warn</option>
          <option value="error">Error</option>
          <option value="debug">Debug</option>
        </select>
        <div className="relative flex-1">
          <Search
            size={12}
            className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-[var(--text-faint)]"
          />
          <input
            className="log-search pl-6"
            placeholder={t("log.search")}
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </div>
        <div className="flex items-center gap-1.5" title={t("log.autoScroll")}>
          <Switch
            checked={autoScroll}
            onChange={setAutoScroll}
            label={t("log.autoScroll")}
          />
        </div>
      </div>

      <div
        className="log-list"
        ref={listRef}
        onContextMenu={(event) => {
          event.preventDefault();
          setMenu({ x: event.clientX, y: event.clientY });
        }}
      >
        {filtered.length === 0 ? (
          <div className="p-4 text-center text-[11px] text-[var(--text-faint)]">{t("log.empty")}</div>
        ) : (
          filtered.map((entry) => (
            <div
              key={entry.id}
              className={cn(
                "log-row",
                entry.level === "error" && "error",
                entry.level === "warn" && "warn",
                entry.level === "debug" && "debug",
              )}
            >
              <span className="log-time">{formatClock(entry.timestamp)}</span>
              <span className={cn("log-scope", `log-level-${entry.level}`)}>{entry.scope}</span>
              <span className="log-message">{entry.message}</span>
            </div>
          ))
        )}
      </div>

      <div className="flex items-center justify-between border-t border-[var(--border)] px-3 py-1.5 text-[10px] text-[var(--text-faint)]">
        <span>{settings.logLevel.toUpperCase()}</span>
        <span>{filtered.length} / {logs.length}</span>
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={[
            {
              label: t("log.clear"),
              icon: <Trash2 size={14} />,
              disabled: logs.length === 0,
              onClick: () => void clearLogs(),
            },
          ]}
          onClose={() => setMenu(null)}
        />
      )}
    </aside>
  );
}
