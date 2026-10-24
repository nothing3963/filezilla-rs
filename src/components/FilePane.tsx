import { useMemo, useState, type DragEvent, type MouseEvent } from "react";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  Download,
  FolderPlus,
  Globe,
  HardDrive,
  Lock,
  RefreshCw,
  Search,
  Server,
  Upload,
  Unplug,
} from "lucide-react";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, isTauri } from "../api/backend";
import { DRAG_MIME, parseDragPayload, type DragPayload } from "../lib/drag";
import { basename, joinLocal, joinRemote, protocolLabel, protocolPort, protocolUsesTls } from "../lib/format";
import { cn } from "../lib/cn";
import { useT } from "../i18n";
import { DEFAULT_SORT, useAppStore } from "../store/app";
import type { LocalEntry, RemoteEntry } from "../types";
import { Button, IconButton } from "./ui/controls";
import { ContextMenu, type MenuItem } from "./ui/ContextMenu";
import { PathBar } from "./PathBar";
import { FileTable, type FileRowData } from "./FileTable";
import { Logo } from "./Logo";

type CompareStatus = "missing" | "different" | "same";

function compareEntry(
  entry: LocalEntry | RemoteEntry,
  other: LocalEntry | RemoteEntry | undefined,
): CompareStatus {
  if (!other) return "missing";
  if (entry.isDir !== other.isDir) return "different";
  if (entry.isDir) return "same";
  if (entry.size !== other.size) return "different";
  const left = entry.modified ? Date.parse(entry.modified) : NaN;
  const right = other.modified ? Date.parse(other.modified) : NaN;
  if (Number.isFinite(left) && Number.isFinite(right) && Math.abs(left - right) > 2000) {
    return "different";
  }
  return "same";
}

export function FilePane({ side }: { side: "local" | "remote" }) {
  const t = useT();
  const tab = useAppStore((state) => state.tabs.find((item) => item.id === state.activeTabId) ?? null);
  const local = useAppStore((state) => state.local);
  const settings = useAppStore((state) => state.settings);
  const compareEnabled = useAppStore((state) => state.ui.compareEnabled);
  const navigateLocal = useAppStore((state) => state.navigateLocal);
  const navigateRemote = useAppStore((state) => state.navigateRemote);
  const localBack = useAppStore((state) => state.localBack);
  const localForward = useAppStore((state) => state.localForward);
  const localUp = useAppStore((state) => state.localUp);
  const remoteBack = useAppStore((state) => state.remoteBack);
  const remoteForward = useAppStore((state) => state.remoteForward);
  const remoteUp = useAppStore((state) => state.remoteUp);
  const refreshLocal = useAppStore((state) => state.refreshLocal);
  const refreshRemote = useAppStore((state) => state.refreshRemote);
  const setLocalSelection = useAppStore((state) => state.setLocalSelection);
  const setRemoteSelection = useAppStore((state) => state.setRemoteSelection);
  const toggleLocalSort = useAppStore((state) => state.toggleLocalSort);
  const toggleRemoteSort = useAppStore((state) => state.toggleRemoteSort);
  const promptNewFolder = useAppStore((state) => state.promptNewFolder);
  const promptRename = useAppStore((state) => state.promptRename);
  const promptChmod = useAppStore((state) => state.promptChmod);
  const deleteSelection = useAppStore((state) => state.deleteSelection);
  const copySelectionPath = useAppStore((state) => state.copySelectionPath);
  const queueTransferItems = useAppStore((state) => state.queueTransferItems);
  const handleDrop = useAppStore((state) => state.handleDrop);
  const openSiteManager = useAppStore((state) => state.openSiteManager);
  const toast = useAppStore((state) => state.toast);

  const [filter, setFilter] = useState("");
  const [dragOver, setDragOver] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; row: FileRowData | null } | null>(null);

  const remote = tab?.remote;
  const connected = side === "local" || Boolean(tab);
  const loading = side === "local" ? local.loading : remote?.loading ?? false;
  const error = side === "local" ? local.error : remote?.error;
  const selected = side === "local" ? local.selected : remote?.selected ?? [];
  const entries = side === "local" ? local.entries : remote?.entries ?? [];
  const currentPath = side === "local" ? local.path : remote?.path ?? null;

  const otherEntries: (LocalEntry | RemoteEntry)[] =
    side === "local" ? remote?.entries ?? [] : local.entries;
  const comparison = useMemo(() => {
    const map = new Map<string, LocalEntry | RemoteEntry>();
    for (const entry of otherEntries) map.set(entry.name.toLowerCase(), entry);
    return map;
  }, [otherEntries]);

  const rows: FileRowData[] = useMemo(
    () =>
      (entries as (LocalEntry | RemoteEntry)[]).map((entry) => ({
        key: entry.path,
        name: entry.name,
        path: entry.path,
        isDir: entry.isDir,
        isSymlink: entry.isSymlink,
        size: entry.size,
        modified: entry.modified,
        permissions: entry.permissions,
        hidden: "hidden" in entry ? entry.hidden : false,
        extension: "extension" in entry ? entry.extension : undefined,
        compare: compareEnabled
          ? compareEntry(entry, comparison.get(entry.name.toLowerCase()))
          : undefined,
      })),
    [entries, compareEnabled, comparison],
  );

  const filteredRows = useMemo(() => {
    const query = filter.trim().toLowerCase();
    if (!query) return rows;
    return rows.filter((row) => row.name.toLowerCase().includes(query));
  }, [rows, filter]);

  const canGoBack =
    side === "local" ? local.historyIndex > 0 : Boolean(remote && remote.historyIndex > 0);
  const canGoForward =
    side === "local"
      ? local.historyIndex < local.history.length - 1
      : Boolean(remote && remote.historyIndex < remote.history.length - 1);

  const select = (keys: string[]) => {
    if (side === "local") setLocalSelection(keys);
    else if (tab) setRemoteSelection(tab.id, keys);
  };

  const openFile = async (path: string) => {
    if (!isTauri()) {
      toast("info", "Opening files is available in the desktop app");
      return;
    }
    try {
      await openPath(path);
    } catch (err) {
      toast("error", String(err));
    }
  };

  const editRemote = async (row: FileRowData) => {
    if (!tab || !isTauri()) return;
    try {
      const edit = await api.remoteEditStart(tab.id, row.path);
      await openPath(edit.localPath);
      toast("success", t("toast.remoteEditing", { name: row.name }));
    } catch (err) {
      toast("error", String((err as { message?: string }).message ?? err));
    }
  };
  const reveal = async (path: string) => {
    if (!isTauri()) return;
    try {
      await revealItemInDir(path);
    } catch (err) {
      toast("error", String(err));
    }
  };

  const uploadPaths = (items: FileRowData[]) => {
    if (!tab) {
      toast("warning", t("toast.noRemoteTarget"));
      return;
    }
    if (!items.length) return;
    void queueTransferItems(
      items.map((item) => ({
        direction: "upload",
        localPath: item.path,
        remotePath: joinRemote(tab.remote.path, item.name),
        size: item.size,
        isDir: item.isDir,
      })),
    );
  };

  const downloadPaths = (items: FileRowData[]) => {
    if (!local.path) {
      toast("warning", t("toast.noLocalTarget"));
      return;
    }
    if (!items.length) return;
    void queueTransferItems(
      items.map((item) => ({
        direction: "download",
        localPath: joinLocal(local.path as string, item.name),
        remotePath: item.path,
        size: item.size,
        isDir: item.isDir,
      })),
    );
  };

  const openRow = (row: FileRowData) => {
    if (side === "local") {
      if (settings.doubleClickAction === "open") void openFile(row.path);
      else uploadPaths([row]);
    } else if (settings.doubleClickAction === "open") {
      void editRemote(row);
    } else {
      downloadPaths([row]);
    }
  };

  const navigateRow = (row: FileRowData) => {
    if (side === "local") void navigateLocal(row.path);
    else if (tab) void navigateRemote(tab.id, row.path);
  };

  const onDropPayload = (payload: DragPayload, targetDir: string) => {
    if (payload.side === side) return;
    void handleDrop(payload, side, targetDir);
  };

  const paneDrop = (event: DragEvent) => {
    event.preventDefault();
    setDragOver(false);
    const payload = parseDragPayload(event.dataTransfer);
    if (!payload) return;
    const targetDir = side === "local" ? local.path : remote?.path;
    if (!targetDir) return;
    onDropPayload(payload, targetDir);
  };

  const selectedRows = rows.filter((row) => selected.includes(row.path));

  const menuItems: MenuItem[] = menu
    ? side === "local"
      ? [
          {
            label: t("context.open"),
            icon: <FolderPlus size={14} />,
            disabled: selectedRows.length !== 1,
            onClick: () => {
              if (menu.row?.isDir) void navigateLocal(menu.row.path);
              else if (menu.row) void openFile(menu.row.path);
            },
          },
          {
            label: t("context.upload"),
            icon: <Upload size={14} />,
            disabled: !tab || selectedRows.length === 0,
            onClick: () => uploadPaths(selectedRows),
          },
          { separator: true, label: "" },
          {
            label: t("context.rename"),
            disabled: selectedRows.length !== 1,
            onClick: () => {
              const target = local.entries.find((entry) => entry.path === menu.row?.path);
              if (target) promptRename("local", target);
            },
          },
          {
            label: t("context.delete"),
            danger: true,
            disabled: selectedRows.length === 0,
            onClick: () => deleteSelection("local"),
          },
          { separator: true, label: "" },
          {
            label: t("context.newFolder"),
            icon: <FolderPlus size={14} />,
            onClick: () => promptNewFolder("local"),
            disabled: !local.path,
          },
          { label: t("context.copyPath"), disabled: selectedRows.length === 0, onClick: () => copySelectionPath("local") },
          { label: t("context.refresh"), onClick: () => void refreshLocal() },
          {
            label: t("context.reveal"),
            disabled: selectedRows.length !== 1,
            onClick: () => menu.row && void reveal(menu.row.path),
          },
        ]
      : [
          {
            label: t("context.edit"),
            disabled: selectedRows.length !== 1 || selectedRows[0]?.isDir,
            onClick: () => menu.row && void editRemote(menu.row),
          },
          {
            label: t("context.download"),
            icon: <Download size={14} />,
            disabled: selectedRows.length === 0 || !local.path,
            onClick: () => downloadPaths(selectedRows),
          },
          { separator: true, label: "" },
          {
            label: t("context.rename"),
            disabled: selectedRows.length !== 1,
            onClick: () => {
              const target = tab?.remote.entries.find((entry) => entry.path === menu.row?.path);
              if (target) promptRename("remote", target);
            },
          },
          {
            label: t("context.permissions"),
            disabled: selectedRows.length !== 1 || selectedRows[0]?.isDir,
            onClick: () => {
              const target = tab?.remote.entries.find((entry) => entry.path === menu.row?.path);
              if (target) promptChmod(target);
            },
          },
          {
            label: t("context.delete"),
            danger: true,
            disabled: selectedRows.length === 0,
            onClick: () => deleteSelection("remote"),
          },
          { separator: true, label: "" },
          {
            label: t("context.newFolder"),
            icon: <FolderPlus size={14} />,
            onClick: () => promptNewFolder("remote"),
            disabled: !tab,
          },
          { label: t("context.copyPath"), disabled: selectedRows.length === 0, onClick: () => copySelectionPath("remote") },          {
            label: t("context.copyUrl"),
            disabled: selectedRows.length !== 1,
            onClick: () => {
              if (!tab || !menu.row) return;
              const scheme =
                tab.protocol === "sftp"
                  ? "sftp"
                  : tab.protocol === "ftp"
                    ? "ftp"
                    : "ftps";
              const defaultPort = protocolPort(tab.protocol);
              const port = tab.port && tab.port !== defaultPort ? `:${tab.port}` : "";
              const path = menu.row.path.startsWith("/") ? menu.row.path : `/${menu.row.path}`;
              const suffix = menu.row.isDir && !path.endsWith("/") ? "/" : "";
              const url = `${scheme}://${tab.host}${port}${encodeURI(path)}${suffix}`;
              void navigator.clipboard.writeText(url).then(
                () => toast("success", t("toast.urlCopied")),
                () => toast("error", t("toast.error")),
              );
            },
          },
          { label: t("context.refresh"), onClick: () => void refreshRemote() },
        ]
    : [];

  const secure = tab ? tab.protocol === "sftp" || protocolUsesTls(tab.protocol) : false;

  return (
    <section
      className="pane"
      data-pane-side={side}
      data-pane-path={currentPath ?? ""}
      onDragOver={(event) => {
        if (!event.dataTransfer.types.includes(DRAG_MIME)) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
        setDragOver(true);
      }}
      onDragLeave={(event) => {
        if (event.currentTarget.contains(event.relatedTarget as Node)) return;
        setDragOver(false);
      }}
      onDrop={paneDrop}
    >
      <div className="pane-header">
        <div className="pane-title">
          {side === "local" ? <HardDrive /> : <Server />}
          {side === "local" ? t("pane.local") : t("pane.remote")}
        </div>

        {side === "local" ? (
          <div className="pane-connection">
            {local.path ? basename(local.path) : t("pane.thisPc")}
          </div>
        ) : tab ? (
          <div className="pane-connection">
            <span className="truncate">{tab.name}</span>
            <span className="text-[var(--text-faint)]">·</span>
            <span className="truncate text-[var(--text-muted)]">
              {tab.username ? `${tab.username}@` : ""}
              {tab.host}:{tab.port}
            </span>
            <span className="pane-badge">{protocolLabel(tab.protocol)}</span>
            <span className={cn("pane-badge", secure && "secure")}>
              {secure ? <Lock size={10} /> : <Globe size={10} />}
              {secure ? t("status.secure") : t("status.insecure")}
            </span>
          </div>
        ) : (
          <div className="pane-connection text-[var(--text-faint)]">{t("pane.disconnected")}</div>
        )}

        <div className="pane-header-actions">
          {connected && (
            <>
              <div className="relative">
                <Search
                  size={12}
                  className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-[var(--text-faint)]"
                />
                <input
                  className="pane-search"
                  placeholder={t("pane.filter")}
                  value={filter}
                  onChange={(event) => setFilter(event.target.value)}
                />
              </div>
              <IconButton label={t("toolbar.newFolder")} onClick={() => promptNewFolder(side)}>
                <FolderPlus />
              </IconButton>
              {side === "local" ? (
                <IconButton
                  label={t("toolbar.upload")}
                  disabled={!tab || selectedRows.length === 0}
                  onClick={() => uploadPaths(selectedRows)}
                >
                  <ArrowUpFromLine />
                </IconButton>
              ) : (
                <IconButton
                  label={t("toolbar.download")}
                  disabled={selectedRows.length === 0 || !local.path}
                  onClick={() => downloadPaths(selectedRows)}
                >
                  <ArrowDownToLine />
                </IconButton>
              )}
              <IconButton
                label={t("toolbar.refresh")}
                onClick={() => (side === "local" ? void refreshLocal() : void refreshRemote())}
              >
                <RefreshCw />
              </IconButton>
            </>
          )}
        </div>
      </div>

      {side === "remote" && !tab ? (
        <div className="pane-body">
          <div className="file-empty">
            <Logo size={52} />
            <h3>{t("pane.disconnected")}</h3>
            <p>{t("pane.disconnectedHint")}</p>
            <Button variant="primary" onClick={openSiteManager}>
              <Server size={14} />
              {t("pane.connect")}
            </Button>
          </div>
        </div>
      ) : (
        <>
          <PathBar
            path={currentPath}
            side={side}
            canBack={canGoBack}
            canForward={canGoForward}
            onBack={() => (side === "local" ? void localBack() : tab && void remoteBack(tab.id))}
            onForward={() => (side === "local" ? void localForward() : tab && void remoteForward(tab.id))}
            onUp={() => (side === "local" ? void localUp() : tab && void remoteUp(tab.id))}
            onNavigate={(path) => {
              if (side === "local") void navigateLocal(path || null);
              else if (tab) void navigateRemote(tab.id, path);
            }}
            onRefresh={() => (side === "local" ? void refreshLocal() : void refreshRemote())}
          />
          <div
            className={cn("pane-body", dragOver && "dragover")}
            data-drop-label={side === "remote" ? t("pane.dropHere") : t("pane.dropLocal")}
          >
            {loading ? (
              <div className="pt-2">
                {Array.from({ length: 9 }).map((_, index) => (
                  <div className="skeleton-row" key={index}>
                    <div className="skeleton w-[16px]" style={{ height: 14 }} />
                    <div className="skeleton" style={{ width: `${30 + ((index * 13) % 45)}%` }} />
                    <div className="skeleton ml-auto w-[60px]" />
                    <div className="skeleton w-[100px]" />
                  </div>
                ))}
              </div>
            ) : error ? (
              <div className="file-error">
                <strong>{t("pane.error")}</strong>
                <div className="mt-1 opacity-90">{error}</div>
              </div>
            ) : filteredRows.length === 0 ? (
              <div className="file-empty">
                <Unplug size={36} />
                <h3>{filter ? t("pane.noFiles") : t("pane.empty")}</h3>
                {!filter && <p>{side === "local" ? local.path : tab?.remote.path}</p>}
              </div>
            ) : (
              <FileTable
                side={side}
                rows={filteredRows}
                selected={selected}
                sort={side === "local" ? local.sort : (remote?.sort ?? DEFAULT_SORT)}
                onSortChange={(key) => {
                  if (side === "local") toggleLocalSort(key);
                  else if (tab) toggleRemoteSort(tab.id, key);
                }}
                onSelect={select}
                onOpen={openRow}
                onNavigate={navigateRow}
                onContextMenu={(event: MouseEvent, row) =>
                  setMenu({ x: event.clientX, y: event.clientY, row })
                }
                onRowDrop={(row, payload) => {
                  if (row.isDir) onDropPayload(payload, row.path);
                }}
              />
            )}
          </div>
        </>
      )}
      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menuItems} onClose={() => setMenu(null)} />
      )}
    </section>
  );
}
