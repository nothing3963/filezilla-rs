import { useEffect, useState } from "react";
import { File, Folder, Search } from "lucide-react";
import { api } from "../api/backend";
import { useT } from "../i18n";
import { formatBytes } from "../lib/format";
import { useAppStore } from "../store/app";
import type { RemoteEntry } from "../types";
import { Button } from "./ui/controls";
import { Modal } from "./ui/Modal";

function parentPath(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const index = trimmed.lastIndexOf("/");
  return index <= 0 ? "/" : trimmed.slice(0, index);
}

export function RemoteSearch() {
  const t = useT();
  const open = useAppStore((state) => state.ui.searchOpen);
  const tab = useAppStore((state) => state.tabs.find((item) => item.id === state.activeTabId) ?? null);
  const navigateRemote = useAppStore((state) => state.navigateRemote);
  const toggleUi = useAppStore((state) => state.toggleUi);

  const [query, setQuery] = useState("");
  const [results, setResults] = useState<RemoteEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setQuery("");
      setResults([]);
      setError(null);
    }
  }, [open]);

  const search = async () => {
    if (!tab || !query.trim()) return;
    setLoading(true);
    setError(null);
    try {
      setResults(await api.searchRemote(tab.id, tab.remote.path ?? "/", query.trim()));
    } catch (err) {
      setError(String((err as { message?: string }).message ?? err));
    } finally {
      setLoading(false);
    }
  };

  const openResult = (entry: RemoteEntry) => {
    if (!tab) return;
    if (entry.isDir) {
      void navigateRemote(tab.id, entry.path);
    } else {
      void navigateRemote(tab.id, parentPath(entry.path));
    }
    toggleUi("searchOpen");
  };

  return (
    <Modal
      open={open}
      onClose={() => toggleUi("searchOpen")}
      title={t("search.title")}
      subtitle={t("search.subtitle")}
      icon={<Search size={16} />}
      size="medium"
      footer={
        <Button variant="subtle" onClick={() => toggleUi("searchOpen")}>
          {t("dialog.cancel")}
        </Button>
      }
    >
      <div className="remote-search">
        <div className="remote-search-bar">
          <input
            className="input"
            autoFocus
            value={query}
            placeholder={t("search.placeholder")}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void search();
            }}
          />
          <Button onClick={() => void search()} disabled={!tab || !query.trim() || loading}>
            {loading ? t("search.searching") : t("search.run")}
          </Button>
        </div>
        <div className="remote-search-scope">
          {tab?.remote.path ?? "/"}
        </div>
        {error && <div className="file-error">{error}</div>}
        <div className="remote-search-results">
          {results.length === 0 ? (
            <div className="remote-search-empty">
              {loading ? t("search.searching") : t("search.empty")}
            </div>
          ) : (
            results.map((entry) => (
              <button
                key={entry.path}
                className="remote-search-result"
                onClick={() => openResult(entry)}
                title={entry.path}
              >
                <span className="remote-search-icon">
                  {entry.isDir ? <Folder size={14} /> : <File size={14} />}
                </span>
                <span className="remote-search-name">{entry.name}</span>
                <span className="remote-search-path">{entry.path}</span>
                <span className="remote-search-size">
                  {entry.isDir ? "—" : formatBytes(entry.size)}
                </span>
              </button>
            ))
          )}
        </div>
      </div>
    </Modal>
  );
}
