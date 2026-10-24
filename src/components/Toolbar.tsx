import {
  ArrowDownToLine,
  ArrowLeftRight,
  ArrowUpFromLine,
  Bookmark,
  FolderPlus,
  Link2,
  ListTodo,
  Plug,
  RefreshCw,
  ScrollText,
  Search,
  Server,
  Settings,
  Unplug,
  Zap,
} from "lucide-react";
import { useT } from "../i18n";
import { useAppStore } from "../store/app";
import { cn } from "../lib/cn";

export function Toolbar() {
  const t = useT();
  const activeTabId = useAppStore((state) => state.activeTabId);
  const tabs = useAppStore((state) => state.tabs);
  const connecting = useAppStore((state) => state.connecting);
  const queueOpen = useAppStore((state) => state.ui.queueOpen);
  const logOpen = useAppStore((state) => state.ui.logOpen);
  const quickOpen = useAppStore((state) => state.ui.quickConnectOpen);
  const compareEnabled = useAppStore((state) => state.ui.compareEnabled);
  const syncBrowsing = useAppStore((state) => state.settings.syncBrowsing);
  const toolbarLabels = useAppStore((state) => state.settings.toolbarLabels);
  const updateSettings = useAppStore((state) => state.updateSettings);
  const toggleCompare = useAppStore((state) => state.toggleCompare);

  const openSiteManager = useAppStore((state) => state.openSiteManager);
  const toggleUi = useAppStore((state) => state.toggleUi);
  const openSettings = useAppStore((state) => state.openSettings);
  const reconnectTab = useAppStore((state) => state.reconnectTab);
  const closeTab = useAppStore((state) => state.closeTab);
  const refreshRemote = useAppStore((state) => state.refreshRemote);
  const promptNewFolder = useAppStore((state) => state.promptNewFolder);
  const transferSelection = useAppStore((state) => state.transferSelection);

  const active = tabs.find((tab) => tab.id === activeTabId);
  const hasRemote = Boolean(active);

  return (
    <div className={cn("toolbar", !toolbarLabels && "icon-only")}>
      <div className="toolbar-group">
        <button className="tool-button" onClick={openSiteManager} title={t("toolbar.siteManager")}>
          <Server />
          <span>{t("toolbar.siteManager")}</span>
        </button>
        <button
          className={cn("tool-button", quickOpen && "active")}
          onClick={() => toggleUi("quickConnectOpen")}
          title={t("toolbar.quickConnect")}
        >
          <Zap />
          <span>{t("toolbar.quickConnect")}</span>
        </button>
      </div>

      <div className="toolbar-divider" />

      <div className="toolbar-group">
        <button
          className="tool-button"
          disabled={!active || connecting}
          onClick={() => active && void reconnectTab(active.id)}
          title={t("toolbar.reconnect")}
        >
          <Plug />
          <span>{t("toolbar.reconnect")}</span>
        </button>
        <button
          className="tool-button"
          disabled={!active}
          onClick={() => active && void closeTab(active.id)}
          title={t("toolbar.disconnect")}
        >
          <Unplug />
          <span>{t("toolbar.disconnect")}</span>
        </button>
        <button
          className="tool-button"
          disabled={!hasRemote}
          onClick={() => void refreshRemote()}
          title={t("toolbar.refresh")}
        >
          <RefreshCw />
          <span>{t("toolbar.refresh")}</span>
        </button>
        <button
          className={cn("tool-button", compareEnabled && "active")}
          disabled={!hasRemote}
          onClick={toggleCompare}
          title={t("toolbar.compare")}
        >
          <ArrowLeftRight />
          <span>{t("toolbar.compare")}</span>
        </button>
        <button
          className={cn("tool-button", syncBrowsing && "active")}
          disabled={!hasRemote}
          onClick={() => updateSettings({ syncBrowsing: !syncBrowsing })}
          title={t("toolbar.sync")}
        >
          <Link2 />
          <span>{t("toolbar.sync")}</span>
        </button>
        <button
          className="tool-button"
          disabled={!hasRemote}
          onClick={() => toggleUi("searchOpen")}
          title={t("toolbar.search")}
        >
          <Search />
          <span>{t("toolbar.search")}</span>
        </button>
        <button
          className="tool-button"
          onClick={() => toggleUi("bookmarksOpen")}
          title={t("toolbar.bookmarks")}
        >
          <Bookmark />
          <span>{t("toolbar.bookmarks")}</span>
        </button>
      </div>

      <div className="toolbar-divider" />

      <div className="toolbar-group">
        <button
          className="tool-button"
          onClick={() => promptNewFolder(hasRemote ? "remote" : "local")}
          title={t("toolbar.newFolder")}
        >
          <FolderPlus />
          <span>{t("toolbar.newFolder")}</span>
        </button>
        <button
          className="tool-button"
          onClick={() => void transferSelection("upload")}
          title={t("toolbar.upload")}
        >
          <ArrowUpFromLine />
          <span>{t("toolbar.upload")}</span>
        </button>
        <button
          className="tool-button"
          disabled={!hasRemote}
          onClick={() => void transferSelection("download")}
          title={t("toolbar.download")}
        >
          <ArrowDownToLine />
          <span>{t("toolbar.download")}</span>
        </button>
      </div>

      <div className="toolbar-divider" />

      <div className="toolbar-group">
        <button
          className={cn("tool-button", queueOpen && "active")}
          onClick={() => toggleUi("queueOpen")}
          title={t("toolbar.queue")}
        >
          <ListTodo />
          <span>{t("toolbar.queue")}</span>
        </button>
        <button
          className={cn("tool-button", logOpen && "active")}
          onClick={() => toggleUi("logOpen")}
          title={t("toolbar.logs")}
        >
          <ScrollText />
          <span>{t("toolbar.logs")}</span>
        </button>
      </div>

      <div className="toolbar-spacer" />

      <div className="toolbar-group">
        <button className="tool-button" onClick={() => void openSettings()} title={t("toolbar.settings")}>
          <Settings />
          <span>{t("toolbar.settings")}</span>
        </button>
      </div>
    </div>
  );
}
