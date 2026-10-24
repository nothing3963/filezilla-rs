import { Globe, Lock, Plus, X } from "lucide-react";
import { useAppStore } from "../store/app";
import { protocolLabel } from "../lib/format";
import { cn } from "../lib/cn";
import { useT } from "../i18n";

export function TabBar() {
  const t = useT();
  const tabs = useAppStore((state) => state.tabs);
  const activeTabId = useAppStore((state) => state.activeTabId);
  const setActiveTab = useAppStore((state) => state.setActiveTab);
  const closeTab = useAppStore((state) => state.closeTab);
  const openSiteManager = useAppStore((state) => state.openSiteManager);

  return (
    <div className="tabbar" role="tablist">
      {tabs.map((tab) => {
        const secure = tab.protocol === "sftp" || tab.protocol === "ftps" || tab.protocol === "ftpsi";
        return (
          <div
            key={tab.id}
            role="tab"
            aria-selected={tab.id === activeTabId}
            className={cn("tab", tab.id === activeTabId && "active")}
            onClick={() => setActiveTab(tab.id)}
            onAuxClick={(event) => {
              if (event.button === 1) void closeTab(tab.id);
            }}
            title={`${tab.name} · ${tab.host}:${tab.port}`}
          >
            {secure ? <Lock className="tab-icon" /> : <Globe className="tab-icon" />}
            <span className="tab-title">
              {tab.name}
              <span className="ml-1.5 text-[9.5px] uppercase tracking-wide text-[var(--text-faint)]">
                {protocolLabel(tab.protocol)}
              </span>
            </span>
            <button
              className="tab-close"
              aria-label={t("common.close")}
              onClick={(event) => {
                event.stopPropagation();
                void closeTab(tab.id);
              }}
            >
              <X size={12} />
            </button>
          </div>
        );
      })}
      <button className="tab-add" onClick={openSiteManager} title={t("siteManager.title")}>
        <Plus size={15} />
      </button>
    </div>
  );
}
