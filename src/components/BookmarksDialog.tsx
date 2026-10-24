import { Bookmark as BookmarkIcon, Folder, Plus, Server, Trash2 } from "lucide-react";
import { useT } from "../i18n";
import { basename } from "../lib/format";
import { useAppStore } from "../store/app";
import type { Bookmark } from "../types";
import { Button, IconButton } from "./ui/controls";
import { Modal } from "./ui/Modal";

export function BookmarksDialog() {
  const t = useT();
  const open = useAppStore((state) => state.ui.bookmarksOpen);
  const toggleUi = useAppStore((state) => state.toggleUi);
  const tab = useAppStore((state) => state.tabs.find((item) => item.id === state.activeTabId) ?? null);
  const localPath = useAppStore((state) => state.local.path);
  const bookmarks = useAppStore((state) => state.settings.bookmarks);
  const updateSettings = useAppStore((state) => state.updateSettings);
  const navigateLocal = useAppStore((state) => state.navigateLocal);
  const navigateRemote = useAppStore((state) => state.navigateRemote);
  const toast = useAppStore((state) => state.toast);

  const addBookmark = (side: "local" | "remote") => {
    const path = side === "local" ? localPath : tab?.remote.path;
    if (!path) {
      toast("warning", t("bookmarks.noPath"));
      return;
    }
    const bookmark: Bookmark = {
      id: crypto.randomUUID(),
      name: basename(path) || path,
      side,
      path,
    };
    const current = useAppStore.getState().settings.bookmarks;
    updateSettings({ bookmarks: [...current, bookmark] });
  };

  const removeBookmark = (id: string) => {
    const current = useAppStore.getState().settings.bookmarks;
    updateSettings({ bookmarks: current.filter((bookmark) => bookmark.id !== id) });
  };

  const openBookmark = (bookmark: Bookmark) => {
    if (bookmark.side === "local") {
      void navigateLocal(bookmark.path);
    } else if (tab) {
      void navigateRemote(tab.id, bookmark.path);
    } else {
      toast("warning", t("bookmarks.noRemote"));
      return;
    }
    toggleUi("bookmarksOpen");
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
        <div className="bookmarks-add">
          <Button variant="subtle" onClick={() => addBookmark("local")} disabled={!localPath}>
            <Plus size={14} />
            {t("bookmarks.addLocal")}
          </Button>
          <Button variant="subtle" onClick={() => addBookmark("remote")} disabled={!tab}>
            <Plus size={14} />
            {t("bookmarks.addRemote")}
          </Button>
        </div>
        {bookmarks.length === 0 ? (
          <div className="bookmarks-empty">{t("bookmarks.empty")}</div>
        ) : (
          <div className="bookmarks-list">
            {bookmarks.map((bookmark) => (
              <div className="bookmark-row" key={bookmark.id}>
                <button className="bookmark-main" onClick={() => openBookmark(bookmark)}>
                  <span className="bookmark-icon">
                    {bookmark.side === "local" ? <Folder size={14} /> : <Server size={14} />}
                  </span>
                  <span className="bookmark-name">{bookmark.name}</span>
                  <span className="bookmark-path">{bookmark.path}</span>
                </button>
                <IconButton
                  label={t("bookmarks.remove")}
                  onClick={() => removeBookmark(bookmark.id)}
                >
                  <Trash2 size={14} />
                </IconButton>
              </div>
            ))}
          </div>
        )}
      </div>
    </Modal>
  );
}
