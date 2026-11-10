import { useState } from "react";
import { Bookmark as BookmarkIcon, Folder, Plus, Server, Trash2 } from "lucide-react";
import { cn } from "../lib/cn";
import { useT } from "../i18n";
import { basename } from "../lib/format";
import { useAppStore } from "../store/app";
import type { Bookmark, PaneSort, SortKey } from "../types";
import { Button, IconButton } from "./ui/controls";
import { Modal } from "./ui/Modal";

const SORT_KEYS: SortKey[] = ["name", "size", "modified", "permissions"];

export function BookmarksDialog() {
  const t = useT();
  const open = useAppStore((state) => state.ui.bookmarksOpen);
  const toggleUi = useAppStore((state) => state.toggleUi);
  const tab = useAppStore((state) => state.tabs.find((item) => item.id === state.activeTabId) ?? null);
  const localPath = useAppStore((state) => state.local.path);
  const bookmarks = useAppStore((state) => state.settings.bookmarks);
  const sites = useAppStore((state) => state.sites.sites);
  const updateSettings = useAppStore((state) => state.updateSettings);
  const navigateLocal = useAppStore((state) => state.navigateLocal);
  const navigateRemote = useAppStore((state) => state.navigateRemote);
  const connectSite = useAppStore((state) => state.connectSite);
  const setActiveTab = useAppStore((state) => state.setActiveTab);
  const setLocalSort = useAppStore((state) => state.setLocalSort);
  const setRemoteSort = useAppStore((state) => state.setRemoteSort);
  const toast = useAppStore((state) => state.toast);
  const [side, setSide] = useState<"local" | "remote">("local");

  const localBookmarks = bookmarks.filter((bookmark) => bookmark.side === "local");
  const remoteBookmarks = bookmarks.filter((bookmark) => bookmark.side === "remote");
  const visibleBookmarks = side === "local" ? localBookmarks : remoteBookmarks;

  const addBookmark = (bookmarkSide: "local" | "remote") => {
    const state = useAppStore.getState();
    const activeTab = state.tabs.find((item) => item.id === state.activeTabId) ?? null;
    const path = bookmarkSide === "local" ? state.local.path : activeTab?.remote.path;
    if (!path) {
      toast("warning", t("bookmarks.noPath"));
      return;
    }
    const bookmark: Bookmark = {
      id: crypto.randomUUID(),
      name: basename(path) || path,
      side: bookmarkSide,
      path,
      siteId: bookmarkSide === "remote" ? activeTab?.siteId ?? null : null,
      sort: bookmarkSide === "local" ? state.local.sort : activeTab?.remote.sort ?? null,
    };
    updateSettings({ bookmarks: [...state.settings.bookmarks, bookmark] });
  };

  const removeBookmark = (id: string) => {
    const current = useAppStore.getState().settings.bookmarks;
    updateSettings({ bookmarks: current.filter((bookmark) => bookmark.id !== id) });
  };

  const bindBookmark = (id: string, siteId: string | null) => {
    const current = useAppStore.getState().settings.bookmarks;
    updateSettings({
      bookmarks: current.map((bookmark) =>
        bookmark.id === id ? { ...bookmark, siteId } : bookmark,
      ),
    });
  };

  const changeBookmarkSort = (id: string, sort: PaneSort | null) => {
    const current = useAppStore.getState().settings.bookmarks;
    updateSettings({
      bookmarks: current.map((bookmark) =>
        bookmark.id === id ? { ...bookmark, sort } : bookmark,
      ),
    });
  };

  const openRemoteBookmark = async (bookmark: Bookmark) => {
    const state = useAppStore.getState();
    const site = bookmark.siteId
      ? state.sites.sites.find((item) => item.id === bookmark.siteId)
      : undefined;
    if (bookmark.siteId && !site) {
      toast("warning", t("bookmarks.siteMissing"));
      return;
    }
    const existing = site ? state.tabs.find((item) => item.siteId === site.id) : undefined;
    let targetTabId: string | null = null;
    if (existing) {
      setActiveTab(existing.id);
      targetTabId = existing.id;
      await navigateRemote(existing.id, bookmark.path);
    } else if (site) {
      const before = new Set(state.tabs.map((item) => item.id));
      await connectSite(site);
      const created = useAppStore
        .getState()
        .tabs.find((item) => !before.has(item.id) && item.siteId === site.id);
      if (!created) return;
      targetTabId = created.id;
      await navigateRemote(created.id, bookmark.path);
    } else if (tab) {
      // Legacy bookmark without a site reference: reuse the active connection.
      targetTabId = tab.id;
      await navigateRemote(tab.id, bookmark.path);
    } else {
      toast("warning", t("bookmarks.noRemote"));
      return;
    }
    if (bookmark.sort && targetTabId) setRemoteSort(targetTabId, bookmark.sort);
    toggleUi("bookmarksOpen");
  };

  const openBookmark = (bookmark: Bookmark) => {
    if (bookmark.side === "local") {
      void navigateLocal(bookmark.path);
      if (bookmark.sort) setLocalSort(bookmark.sort);
      toggleUi("bookmarksOpen");
      return;
    }
    void openRemoteBookmark(bookmark);
  };

  return (
    <Modal
      open={open}
      onClose={() => toggleUi("bookmarksOpen")}
      title={t("bookmarks.title")}
      subtitle={t("bookmarks.subtitle")}
      icon={<BookmarkIcon size={16} />}
      size="medium"
      footer={
        <Button variant="subtle" onClick={() => toggleUi("bookmarksOpen")}>
          {t("dialog.cancel")}
        </Button>
      }
    >
      <div className="bookmarks-dialog">
        <div className="bookmarks-tabs">
          <div className="segmented">
            <button className={cn(side === "local" && "active")} onClick={() => setSide("local")}>
              {t("pane.local")}
              <span className="bookmarks-count">{localBookmarks.length}</span>
            </button>
            <button className={cn(side === "remote" && "active")} onClick={() => setSide("remote")}>
              {t("pane.remote")}
              <span className="bookmarks-count">{remoteBookmarks.length}</span>
            </button>
          </div>
          {side === "local" ? (
            <Button variant="subtle" onClick={() => addBookmark("local")} disabled={!localPath}>
              <Plus size={14} />
              {t("bookmarks.addLocal")}
            </Button>
          ) : (
            <Button variant="subtle" onClick={() => addBookmark("remote")} disabled={!tab}>
              <Plus size={14} />
              {t("bookmarks.addRemote")}
            </Button>
          )}
        </div>
        {visibleBookmarks.length === 0 ? (
          <div className="bookmarks-empty">{t("bookmarks.empty")}</div>
        ) : (
          <div className="bookmarks-list">
            {visibleBookmarks.map((bookmark) => {
              const boundSite = sites.find((site) => site.id === bookmark.siteId);
              return (
                <div className="bookmark-row" key={bookmark.id}>
                  <button className="bookmark-main" onClick={() => openBookmark(bookmark)}>
                    <span className="bookmark-icon">
                      {bookmark.side === "local" ? <Folder size={14} /> : <Server size={14} />}
                    </span>
                    <span className="bookmark-name">{bookmark.name}</span>
                    <span className="bookmark-path">{bookmark.path}</span>
                  </button>
                  <select
                    className="bookmark-sort"
                    value={bookmark.sort ? `${bookmark.sort.key}:${bookmark.sort.dir}` : ""}
                    onChange={(event) => {
                      const [key, dir] = event.target.value.split(":");
                      changeBookmarkSort(
                        bookmark.id,
                        key ? { key: key as SortKey, dir: Number(dir) === -1 ? -1 : 1 } : null,
                      );
                    }}
                  >
                    {!bookmark.sort && <option value="">—</option>}
                    {SORT_KEYS.flatMap((key) =>
                      ([1, -1] as const).map((dir) => (
                        <option key={`${key}:${dir}`} value={`${key}:${dir}`}>
                          {t(`columns.${key}`)} {dir === 1 ? "↑" : "↓"}
                        </option>
                      )),
                    )}
                  </select>
                  {bookmark.side === "remote" && (
                    <select
                      className="bookmark-site"
                      value={boundSite?.id ?? bookmark.siteId ?? ""}
                      onChange={(event) => bindBookmark(bookmark.id, event.target.value || null)}
                    >
                      {!boundSite && (
                        <option value={bookmark.siteId ?? ""}>
                          {bookmark.siteId ? t("bookmarks.siteMissing") : "—"}
                        </option>
                      )}
                      {sites.map((site) => (
                        <option key={site.id} value={site.id}>
                          {site.name}
                        </option>
                      ))}
                    </select>
                  )}
                  <IconButton
                    label={t("bookmarks.remove")}
                    onClick={() => removeBookmark(bookmark.id)}
                  >
                    <Trash2 size={14} />
                  </IconButton>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </Modal>
  );
}
