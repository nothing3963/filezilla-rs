import { Activity, HardDrive, Lock, Server, Wifi, WifiOff } from "lucide-react";
import { useT } from "../i18n";
import { formatSpeed } from "../lib/format";
import { useAppStore } from "../store/app";
import { cn } from "../lib/cn";

export function StatusBar() {
  const t = useT();
  const tab = useAppStore((state) => state.tabs.find((item) => item.id === state.activeTabId) ?? null);
  const local = useAppStore((state) => state.local);
  const transfers = useAppStore((state) => state.transfers);
  const info = useAppStore((state) => state.info);
  const connectionCount = useAppStore((state) => state.tabs.length);
  const toggleUi = useAppStore((state) => state.toggleUi);

  const running = transfers.filter(
    (task) => task.status === "active" || task.status === "connecting",
  );
  const speed = running.reduce((sum, task) => sum + (task.status === "active" ? task.speedBps : 0), 0);

  return (
    <footer className="statusbar">
      <div className="status-item">
        <span className={cn("status-dot", tab && "connected")} />
        {tab ? (
          <>
            {tab.protocol === "sftp" || tab.protocol === "ftps" || tab.protocol === "ftpsi" ? (
              <Lock size={12} />
            ) : null}
            <span className="truncate">
              {tab.username ? `${tab.username}@` : ""}
              {tab.host}:{tab.port}
            </span>
          </>
        ) : (
          <span>{t("status.disconnected")}</span>
        )}
      </div>

      <div className="status-item">
        <Server size={12} />
        {tab ? <span>{t("status.remoteItems", { count: tab.remote.entries.length })}</span> : <span>—</span>}
      </div>

      <div className="status-item">
        <HardDrive size={12} />
        <span>{t("status.localItems", { count: local.entries.length })}</span>
      </div>

      <div className="status-item">
        {connectionCount > 0 ? <Wifi size={12} /> : <WifiOff size={12} />}
        <span>{t("status.sessions", { count: connectionCount })}</span>
      </div>

      <div className="status-spacer" />

      {running.length > 0 && (
        <div className="status-item text-[var(--accent)]">
          <Activity size={12} />
          <span>{t("status.transferring", { count: running.length, speed: formatSpeed(speed) })}</span>
        </div>
      )}

      <button className="status-item status-link" onClick={() => toggleUi("logOpen")}>
        {t("status.idle")}
      </button>

      <span className="status-item">{info ? t("status.version", { version: info.version }) : ""}</span>
    </footer>
  );
}
