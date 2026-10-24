import { create } from "zustand";
import { api, ApiError } from "../api/backend";
import { onBackendEvent, type Unlisten } from "../api/events";
import {
  isSupportedLanguage,
  loadLanguage,
  resolveSystemLanguage,
  translate,
} from "../i18n/translate";
import type { DragPayload } from "../lib/drag";
import { basename, dirname, joinLocal, joinRemote, naturalCompare, posixName, posixParent, protocolPort } from "../lib/format";
import type {
  AppInfo,
  AppSettings,
  ConnectRequest,
  HostKeyPrompt,
  KnownHost,
  ManagedPolicy,
  LocalEntry,
  LocalListing,
  LogEntry,
  PaneSort,
  Protocol,
  RemoteEntry,
  SessionInfo,
  Site,
  SiteConfig,
  SiteGroup,
  SiteInput,
  SitesSnapshot,
  SortKey,
  TransferAction,
  TransferDirection,
  TransferItem,
  TransferTask,
} from "../types";

export interface RemotePaneState {
  path: string;
  parent: string | null;
  entries: RemoteEntry[];
  loading: boolean;
  error?: string;
  selected: string[];
  sort: PaneSort;
  history: string[];
  historyIndex: number;
}

export interface TabState {
  id: string;
  siteId?: string | null;
  name: string;
  protocol: Protocol;
  host: string;
  port: number;
  username: string;
  home: string;
  config: SiteConfig | null;
  remote: RemotePaneState;
}

export interface LocalPaneState {
  path: string | null;
  parent: string | null;
  entries: LocalEntry[];
  places: LocalEntry[];
  loading: boolean;
  error?: string;
  selected: string[];
  sort: PaneSort;
  history: (string | null)[];
  historyIndex: number;
}

export type PromptState = {
  title: string;
  label: string;
  value: string;
  placeholder?: string;
  confirmLabel?: string;
  hint?: string;
  onSubmit: (value: string) => void | Promise<void>;
};

export type ConfirmState = {
  title: string;
  message: string;
  confirmLabel?: string;
  danger?: boolean;
  onConfirm: () => void | Promise<void>;
};

export type ToastKind = "success" | "error" | "info" | "warning";

export interface Toast {
  id: string;
  kind: ToastKind;
  title?: string;
  message: string;
}

interface UiState {
  siteManagerOpen: boolean;
  settingsOpen: boolean;
  queueOpen: boolean;
  logOpen: boolean;
  bottomTab: "queue" | "log";
  quickConnectOpen: boolean;
  compareEnabled: boolean;
  searchOpen: boolean;
  bookmarksOpen: boolean;
  commandOpen: boolean;
  prompt: PromptState | null;
  confirm: ConfirmState | null;
}

export interface AppStore {
  ready: boolean;
  connecting: boolean;
  info: AppInfo | null;
  hostkeyPrompt: HostKeyPrompt | null;
  policy: ManagedPolicy | null;
  settings: AppSettings;
  sites: SitesSnapshot;
  tabs: TabState[];
  activeTabId: string | null;
  local: LocalPaneState;
  transfers: TransferTask[];
  logs: LogEntry[];
  knownHosts: KnownHost[];
  ui: UiState;
  toasts: Toast[];

  bootstrap: () => Promise<void>;
  setHostkeyPrompt: (prompt: HostKeyPrompt | null) => void;
  toast: (kind: ToastKind, message: string, title?: string) => void;
  dismissToast: (id: string) => void;
  addLog: (level: LogEntry["level"], scope: string, message: string) => void;

  updateSettings: (patch: Partial<AppSettings>) => void;
  setLanguage: (code: string) => Promise<void>;
  toggleUi: (key: "queueOpen" | "logOpen" | "quickConnectOpen" | "siteManagerOpen" | "settingsOpen" | "searchOpen" | "bookmarksOpen" | "commandOpen") => void;
  setBottomTab: (tab: "queue" | "log") => void;
  toggleCompare: () => void;
  openSiteManager: () => void;
  closeSiteManager: () => void;
  openSettings: () => Promise<void>;
  closeSettings: () => void;

  prompt: (state: PromptState) => void;
  confirm: (state: ConfirmState) => void;
  closeDialogs: () => void;

  setActiveTab: (id: string) => void;
  connectSite: (site: Site) => Promise<void>;
  connectQuick: (params: {
    protocol: Protocol;
    host: string;
    port: number;
    username: string;
    password: string;
    name?: string;
  }) => Promise<void>;
  closeTab: (id: string) => Promise<void>;
  reconnectTab: (id: string) => Promise<void>;

  loadRemote: (tabId: string, path: string, mode: "push" | "replace" | "none") => Promise<void>;
  navigateRemote: (tabId: string, path: string) => Promise<void>;
  syncPeer: (from: "local" | "remote", path: string) => Promise<void>;
  remoteBack: (tabId: string) => Promise<void>;
  remoteForward: (tabId: string) => Promise<void>;
  remoteUp: (tabId: string) => Promise<void>;
  refreshRemote: (tabId?: string) => Promise<void>;
  setRemoteSelection: (tabId: string, selected: string[]) => void;
  toggleRemoteSort: (tabId: string, key: SortKey) => void;

  loadLocal: (path: string | null, mode: "push" | "replace" | "none") => Promise<void>;
  navigateLocal: (path: string | null) => Promise<void>;
  localBack: () => Promise<void>;
  localForward: () => Promise<void>;
  localUp: () => Promise<void>;
  refreshLocal: () => Promise<void>;
  setLocalSelection: (selected: string[]) => void;
  toggleLocalSort: (key: SortKey) => void;

  promptNewFolder: (side: "local" | "remote") => void;
  promptRename: (side: "local" | "remote", item: RemoteEntry | LocalEntry) => void;
  promptChmod: (item: RemoteEntry) => void;
  deleteSelection: (side: "local" | "remote") => void;
  copySelectionPath: (side: "local" | "remote") => void;

  transferSelection: (direction: TransferDirection) => Promise<void>;
  queueTransferItems: (items: TransferItem[]) => Promise<void>;
  uploadExternalPaths: (paths: string[]) => Promise<void>;
  handleDrop: (payload: DragPayload, targetSide: "local" | "remote", targetDir: string) => Promise<void>;
  transferAction: (taskId: string, action: TransferAction) => Promise<void>;
  locateTransfer: (taskId: string, side: "local" | "remote") => Promise<void>;
  refreshTransfersFromBackend: () => Promise<void>;
  clearTransfers: () => Promise<void>;

  refreshSites: () => Promise<void>;
  saveSite: (input: SiteInput) => Promise<Site | null>;
  deleteSite: (id: string) => Promise<boolean>;
  duplicateSite: (id: string) => Promise<Site | null>;
  createGroup: (name: string, parentId?: string | null) => Promise<SiteGroup | null>;
  renameGroup: (id: string, name: string) => Promise<void>;
  deleteGroup: (id: string) => Promise<void>;

  loadKnownHosts: () => Promise<void>;
  forgetHost: (host: string) => Promise<void>;
  clearLogs: () => Promise<void>;
  copyText: (text: string) => Promise<void>;
}

export const DEFAULT_SETTINGS: AppSettings = {
  theme: "dark",
  accent: "blue",
  language: "zh-CN",
  density: "comfortable",
  showHiddenFiles: false,
  confirmDelete: true,
  doubleClickAction: "transfer",
  concurrentTransfers: 2,
  downloadLimitKbps: 0,
  uploadLimitKbps: 0,
  resumeTransfers: true,
  preserveTimestamps: true,
  syncBrowsing: false,
  bookmarks: [],
  notifyOnComplete: true,
  soundOnComplete: false,
  minimizeToTray: false,
  defaultLocalDir: null,
  logLevel: "info",
  showLogPanel: false,
  filezillaImportPrompted: false,
  animations: true,
  micaEffect: true,
  toolbarLabels: true,
};

export function defaultConfig(protocol: Protocol = "sftp"): SiteConfig {
  return {
    protocol,
    host: "",
    port: protocolPort(protocol),
    username: "",
    authMethod: "password",
    privateKeyPath: null,
    initialRemoteDir: null,
    localDir: null,
    passiveMode: true,
    verifyCertificate: true,
    connectTimeoutSecs: 20,
    keepaliveSecs: 30,
    transferMode: "auto",
    encoding: "UTF-8",
    maxConnections: 2,
    notes: null,
    proxyType: "",
    proxyHost: "",
    proxyPort: 0,
    proxyUsername: "",
  };
}

export const DEFAULT_SORT: PaneSort = { key: "name", dir: 1 };

const emptyLocal: LocalPaneState = {
  path: null,
  parent: null,
  entries: [],
  places: [],
  loading: false,
  selected: [],
  sort: DEFAULT_SORT,
  history: [null],
  historyIndex: 0,
};

let bootstrapStarted = false;
let unlisteners: Unlisten[] = [];
let persistTimer: number | undefined;

function t(language: string, key: string, vars?: Record<string, string | number>) {
  return translate(language, key, vars);
}

function applyThemeToDocument(settings: AppSettings) {
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const dark = settings.theme === "dark" || (settings.theme === "system" && prefersDark);
  const root = document.documentElement;
  root.dataset.theme = dark ? "dark" : "light";
  root.dataset.density = settings.density;
  root.dataset.animations = settings.animations ? "on" : "off";
  root.style.colorScheme = dark ? "dark" : "light";
  void api.applyWindowTheme(dark, settings.micaEffect).catch(() => undefined);
}

function sortRemote(entries: RemoteEntry[]): RemoteEntry[] {
  return [...entries].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return naturalCompare(a.name, b.name);
  });
}

function sortLocal(entries: LocalEntry[]): LocalEntry[] {
  return [...entries].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return naturalCompare(a.name, b.name);
  });
}

function mergeTransfers(current: TransferTask[], incoming: TransferTask[]): TransferTask[] {
  const map = new Map(current.map((task) => [task.id, task]));
  for (const task of incoming) map.set(task.id, task);
  return [...map.values()].sort((a, b) => a.createdAt - b.createdAt);
}

function pushLog(logs: LogEntry[], entry: LogEntry): LogEntry[] {
  const next = [...logs, entry];
  return next.length > 1500 ? next.slice(next.length - 1500) : next;
}

function hasPendingTransfer(tasks: TransferTask[], direction: TransferDirection): boolean {
  return tasks.some(
    (task) =>
      task.direction === direction &&
      (task.status === "queued" || task.status === "connecting" || task.status === "active"),
  );
}

function isFinishedStatus(status: TransferTask["status"]): boolean {
  return status === "completed" || status === "failed" || status === "cancelled";
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof ApiError) return error.message;
  if (error && typeof error === "object" && "message" in error) return String((error as { message: unknown }).message);
  if (typeof error === "string") return error;
  return fallback;
}

async function setupEvents(
  set: (partial: Partial<AppStore> | ((state: AppStore) => Partial<AppStore>)) => void,
  get: () => AppStore,
) {
  if (unlisteners.length) return;
  unlisteners.push(
    await onBackendEvent<TransferTask>("transfer:update", (task) => {
      const previous = get().transfers.find((item) => item.id === task.id);
      const next = mergeTransfers(get().transfers, [task]);
      set({ transfers: next });
      if (!isFinishedStatus(task.status) || (previous && isFinishedStatus(previous.status))) {
        return;
      }
      if (hasPendingTransfer(next, task.direction)) return;
      if (task.direction === "upload") void get().refreshRemote();
      else void get().refreshLocal();
    }),
  );
  unlisteners.push(
    await onBackendEvent<string>("transfer:removed", (id) => {
      set((state) => ({ transfers: state.transfers.filter((task) => task.id !== id) }));
    }),
  );
  unlisteners.push(
    await onBackendEvent<string[]>("transfers:cleared", (ids) => {
      const remove = new Set(ids);
      set((state) => ({ transfers: state.transfers.filter((task) => !remove.has(task.id)) }));
    }),
  );
  unlisteners.push(
    await onBackendEvent<{ localPath: string; remotePath: string }>(
      "edit:uploaded",
      (payload) => {
        const name = basename(payload.remotePath);
        const language = get().settings.language;
        get().toast("success", translate(language, "toast.editUploaded", { name }));
        get().addLog("info", "edit", `Uploaded ${payload.remotePath}`);
        const tab = activeTab(get());
        const parent = payload.remotePath.replace(/\/[^/]*$/, "") || "/";
        if (tab && tab.remote.path === parent) void get().refreshRemote(tab.id);
      },
    ),
  );  unlisteners.push(
    await onBackendEvent<HostKeyPrompt>("ssh:hostkey", (prompt) => {
      set({ hostkeyPrompt: prompt });
    }),
  );  unlisteners.push(
    await onBackendEvent<LogEntry>("log:entry", (entry) => {
      set((state) => ({ logs: pushLog(state.logs, entry) }));
    }),
  );

  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (get().settings.theme === "system") applyThemeToDocument(get().settings);
  });
}

export const useAppStore = create<AppStore>((set, get) => ({
  ready: false,
  connecting: false,
  info: null,
  hostkeyPrompt: null,
  policy: null,
  settings: DEFAULT_SETTINGS,
  sites: { sites: [], groups: [] },
  tabs: [],
  activeTabId: null,
  local: emptyLocal,
  transfers: [],
  logs: [],
  knownHosts: [],
  ui: {
    siteManagerOpen: false,
    settingsOpen: false,
    queueOpen: true,
    logOpen: false,
    bottomTab: "queue",
    quickConnectOpen: false,
    compareEnabled: false,
    searchOpen: false,
    bookmarksOpen: false,
    commandOpen: false,
    prompt: null,
    confirm: null,
  },
  toasts: [],

  setHostkeyPrompt(prompt) {
    set({ hostkeyPrompt: prompt });
  },
  async bootstrap() {
    if (bootstrapStarted) return;
    bootstrapStarted = true;
    try {
      const [storedSettings, sites, transfers, logs, info, policy] = await Promise.all([
        api.settingsGet(),
        api.sitesGet(),
        api.transfersList(),
        api.logsHistory(),
        api.appInfo(),
        api.managedPolicy(),
      ]);
      const storedLanguage = storedSettings.language;
      const language =
        storedLanguage && isSupportedLanguage(storedLanguage)
          ? storedLanguage
          : resolveSystemLanguage(
              navigator.languages?.length ? navigator.languages : [navigator.language],
            );
      const settings: AppSettings = { ...storedSettings, language };
      await loadLanguage(language).catch(() => undefined);
      set({
        settings,
        sites,
        transfers: mergeTransfers([], transfers),
        logs,
        info,
        policy,
        ready: true,
        ui: { ...get().ui, logOpen: settings.showLogPanel },
      });
      applyThemeToDocument(settings);
      await setupEvents(set, get);
      const root = await api.localList(null, settings.showHiddenFiles);
      set((state) => ({
        local: {
          ...state.local,
          path: null,
          parent: null,
          entries: sortLocal(root.entries),
          places: root.places,
          history: [null],
          historyIndex: 0,
          loading: false,
        },
      }));
      const home =
        settings.defaultLocalDir ||
        root.places.find((place) => place.name === "Downloads")?.path ||
        root.places.find((place) => place.name === "Home")?.path ||
        root.places[0]?.path ||
        root.entries[0]?.path ||
        null;
      if (home) {
        await get().loadLocal(home, "push");
        // ~/Downloads may be missing (e.g. a minimal Linux install); fall back
        // to the home directory when the listing fails.
        if (get().local.error) {
          const fallback = root.places.find((place) => place.name === "Home")?.path;
          if (fallback && fallback !== home) await get().loadLocal(fallback, "push");
        }
      }
    } catch (error) {
      bootstrapStarted = false;
      set({ ready: true });
      get().toast("error", errorMessage(error, "Failed to initialise"), "Startup");
    }
  },

  toast(kind, message, title) {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
    set((state) => ({ toasts: [...state.toasts, { id, kind, message, title }] }));
    window.setTimeout(() => {
      set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
    }, 5200);
  },

  dismissToast(id) {
    set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
  },

  addLog(level, scope, message) {
    const entry: LogEntry = {
      id: Date.now() * 1000 + Math.floor(Math.random() * 1000),
      timestamp: Date.now(),
      level,
      scope,
      message,
    };
    set((state) => ({ logs: pushLog(state.logs, entry) }));
  },

  updateSettings(patch) {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    applyThemeToDocument(next);
    if (patch.showHiddenFiles !== undefined) void get().refreshLocal();
    if (patch.showLogPanel !== undefined) {
      set((state) => ({ ui: { ...state.ui, logOpen: patch.showLogPanel === true } }));
    }
    window.clearTimeout(persistTimer);
    persistTimer = window.setTimeout(() => {
      api
        .settingsSet(next)
        .then((saved) => set({ settings: saved }))
        .catch((error) => {
          get().toast("error", errorMessage(error, "Could not save settings"), "Settings");
        });
    }, 320);
  },

  async setLanguage(code) {
    await loadLanguage(code).catch(() => undefined);
    get().updateSettings({ language: code });
  },

  toggleUi(key) {
    set((state) => {
      if (key === "queueOpen") {
        const queueOpen = !state.ui.queueOpen;
        return {
          ui: {
            ...state.ui,
            queueOpen,
            bottomTab: queueOpen ? "queue" : state.ui.logOpen ? "log" : "queue",
          },
        };
      }
      if (key === "logOpen") {
        const logOpen = !state.ui.logOpen;
        return {
          ui: {
            ...state.ui,
            logOpen,
            bottomTab: logOpen ? "log" : state.ui.queueOpen ? "queue" : "log",
          },
        };
      }
      return { ui: { ...state.ui, [key]: !state.ui[key] } };
    });
  },

  setBottomTab(tab) {
    set((state) => ({ ui: { ...state.ui, bottomTab: tab } }));
  },

  toggleCompare() {
    set((state) => ({ ui: { ...state.ui, compareEnabled: !state.ui.compareEnabled } }));
  },

  openSiteManager() {
    set((state) => ({ ui: { ...state.ui, siteManagerOpen: true } }));
  },

  closeSiteManager() {
    set((state) => ({ ui: { ...state.ui, siteManagerOpen: false } }));
  },

  async openSettings() {
    set((state) => ({ ui: { ...state.ui, settingsOpen: true } }));
    await get().loadKnownHosts();
  },

  closeSettings() {
    set((state) => ({ ui: { ...state.ui, settingsOpen: false } }));
  },

  prompt(state) {
    set((current) => ({ ui: { ...current.ui, prompt: state } }));
  },

  confirm(state) {
    set((current) => ({ ui: { ...current.ui, confirm: state } }));
  },

  closeDialogs() {
    set((current) => ({ ui: { ...current.ui, prompt: null, confirm: null } }));
  },

  setActiveTab(id) {
    set({ activeTabId: id });
  },

  async connectSite(site) {
    const language = get().settings.language;
    if (!site) return;
    const request: ConnectRequest = {
      siteId: site.id,
      name: site.name,
      config: site.config,
      credentials: null,
      savePassword: false,
    };
    await openConnection(request, site.config, site.name, language, set, get);
  },

  async connectQuick(params) {
    const language = get().settings.language;
    const host = params.host.trim();
    if (!host) {
      get().toast("warning", t(language, "toast.hostRequired"));
      return;
    }
    const config: SiteConfig = {
      ...defaultConfig(params.protocol),
      host,
      port: params.port || protocolPort(params.protocol),
      username: params.username.trim(),
      initialRemoteDir: null,
    };
    const request: ConnectRequest = {
      config,
      name: params.name ?? `${config.username || "anonymous"}@${host}`,
      credentials: { password: params.password },
      savePassword: false,
    };
    await openConnection(request, config, request.name ?? host, language, set, get);
  },

  async closeTab(id) {
    const tab = get().tabs.find((item) => item.id === id);
    if (!tab) return;
    try {
      await api.disconnect(id);
    } catch {
      // The connection may already be gone; closing the tab is still valid.
    }
    set((state) => {
      const index = state.tabs.findIndex((item) => item.id === id);
      const tabs = state.tabs.filter((item) => item.id !== id);
      let activeTabId = state.activeTabId;
      if (activeTabId === id) {
        activeTabId = tabs[Math.min(index, tabs.length - 1)]?.id ?? null;
      }
      return { tabs, activeTabId };
    });
  },

  async reconnectTab(id) {
    const tab = get().tabs.find((item) => item.id === id);
    if (!tab) return;
    const config = tab.config;
    if (!config) {
      get().toast("error", "No connection settings stored for this tab");
      return;
    }
    const request: ConnectRequest = {
      siteId: tab.siteId,
      name: tab.name,
      config,
      credentials: null,
      savePassword: false,
    };
    await get().closeTab(id);
    await openConnection(request, config, tab.name, get().settings.language, set, get);
  },

  async loadRemote(tabId, path, mode) {
    const tab = get().tabs.find((item) => item.id === tabId);
    if (!tab) return;
    // Refreshing the directory that is already shown keeps the table mounted
    // (no skeleton swap) so the list does not flash.
    const samePath = tab.remote.path === path && tab.remote.entries.length > 0;
    set((state) => ({
      tabs: state.tabs.map((item) =>
        item.id === tabId
          ? { ...item, remote: { ...item.remote, loading: !samePath, error: undefined } }
          : item,
      ),
    }));

    const history = [...tab.remote.history];
    let index = tab.remote.historyIndex;
    if (mode === "push") {
      if (history[index] !== path) {
        history.splice(index + 1);
        history.push(path);
        index = history.length - 1;
      }
    } else if (mode === "replace") {
      history[index] = path;
    }

    try {
      const listing = await api.remoteList(tabId, path);
      const entries = sortRemote(listing.entries);
      // A refresh of the same directory keeps the selection so the table can
      // scroll back to the file the user was looking at.
      const selected =
        listing.path === tab.remote.path
          ? tab.remote.selected.filter((key) => entries.some((row) => row.path === key))
          : [];
      set((state) => ({
        tabs: state.tabs.map((item) =>
          item.id === tabId
            ? {
                ...item,
                remote: {
                  ...item.remote,
                  path: listing.path,
                  parent: listing.parent,
                  entries,
                  loading: false,
                  error: undefined,
                  selected,
                  history,
                  historyIndex: index,
                },
              }
            : item,
        ),
      }));
    } catch (error) {
      const message = errorMessage(error, t(get().settings.language, "pane.error"));
      set((state) => ({
        tabs: state.tabs.map((item) =>
          item.id === tabId
            ? {
                ...item,
                remote: {
                  ...item.remote,
                  loading: false,
                  error: message,
                  history,
                  historyIndex: index,
                },
              }
            : item,
        ),
      }));
      get().toast("error", message);
      get().addLog("error", tab.host, `Unable to list ${path}: ${message}`);
    }
  },

  async navigateRemote(tabId, path) {
    await get().loadRemote(tabId, path, "push");
    await get().syncPeer("remote", path);
  },

  async syncPeer(from, path) {
    const { settings, tabs, activeTabId, local } = get();
    if (!settings.syncBrowsing || !path) return;
    const tab = tabs.find((item) => item.id === activeTabId);
    if (!tab) return;
    const name = path
      .replace(/[\\/]+$/, "")
      .split(/[\\/]/)
      .pop()
      ?.toLowerCase();
    if (!name) return;
    if (from === "local") {
      const peer = tab.remote.entries.find(
        (entry) => entry.isDir && entry.name.toLowerCase() === name,
      );
      if (peer && peer.path !== tab.remote.path) {
        await get().loadRemote(tab.id, peer.path, "push");
      }
    } else {
      const peer = local.entries.find(
        (entry) => entry.isDir && entry.name.toLowerCase() === name,
      );
      if (peer && peer.path !== local.path) {
        await get().loadLocal(peer.path, "push");
      }
    }
  },
  async remoteBack(tabId) {
    const tab = get().tabs.find((item) => item.id === tabId);
    if (!tab || tab.remote.historyIndex <= 0) return;
    const target = tab.remote.history[tab.remote.historyIndex - 1];
    set((state) => ({
      tabs: state.tabs.map((item) =>
        item.id === tabId
          ? { ...item, remote: { ...item.remote, historyIndex: item.remote.historyIndex - 1 } }
          : item,
      ),
    }));
    await get().loadRemote(tabId, target, "none");
  },

  async remoteForward(tabId) {
    const tab = get().tabs.find((item) => item.id === tabId);
    if (!tab || tab.remote.historyIndex >= tab.remote.history.length - 1) return;
    const target = tab.remote.history[tab.remote.historyIndex + 1];
    set((state) => ({
      tabs: state.tabs.map((item) =>
        item.id === tabId
          ? { ...item, remote: { ...item.remote, historyIndex: item.remote.historyIndex + 1 } }
          : item,
      ),
    }));
    await get().loadRemote(tabId, target, "none");
  },

  async remoteUp(tabId) {
    const tab = get().tabs.find((item) => item.id === tabId);
    if (!tab) return;
    await get().navigateRemote(tabId, tab.remote.parent ?? "/");
  },

  async refreshRemote(tabId) {
    const id = tabId ?? get().activeTabId;
    if (!id) return;
    const tab = get().tabs.find((item) => item.id === id);
    if (!tab) return;
    await get().loadRemote(id, tab.remote.path, "none");
  },

  setRemoteSelection(tabId, selected) {
    set((state) => ({
      tabs: state.tabs.map((item) =>
        item.id === tabId ? { ...item, remote: { ...item.remote, selected } } : item,
      ),
    }));
  },

  toggleRemoteSort(tabId, key) {
    const tab = get().tabs.find((item) => item.id === tabId);
    if (!tab) return;
    const current = tab.remote.sort;
    const sort: PaneSort =
      current.key === key ? { key, dir: current.dir === 1 ? -1 : 1 } : { key, dir: 1 };
    set((state) => ({
      tabs: state.tabs.map((item) =>
        item.id === tabId ? { ...item, remote: { ...item.remote, sort } } : item,
      ),
    }));
  },

  async loadLocal(path, mode) {
    // Same as the remote pane: a refresh keeps the current rows visible instead
    // of swapping them for the loading skeleton.
    const samePath = get().local.path === path && get().local.entries.length > 0;
    set((state) => ({
      local: { ...state.local, loading: !samePath, error: undefined },
    }));
    const history = [...get().local.history];
    let index = get().local.historyIndex;
    if (mode === "push") {
      if (history[index] !== path) {
        history.splice(index + 1);
        history.push(path);
        index = history.length - 1;
      }
    } else if (mode === "replace") {
      history[index] = path;
    }
    try {
      const listing: LocalListing = await api.localList(path, get().settings.showHiddenFiles);
      const entries = sortLocal(listing.entries);
      // Keep the selection when refreshing the directory we already show.
      const selected =
        listing.path === get().local.path
          ? get().local.selected.filter((key) => entries.some((row) => row.path === key))
          : [];
      set((state) => ({
        local: {
          ...state.local,
          path: listing.path,
          parent: listing.parent,
          entries,
          places: listing.places,
          loading: false,
          error: undefined,
          selected,
          history,
          historyIndex: index,
        },
      }));
    } catch (error) {
      const message = errorMessage(error, t(get().settings.language, "pane.error"));
      set((state) => ({ local: { ...state.local, loading: false, error: message } }));
      get().toast("error", message);
    }
  },

  async navigateLocal(path) {
    await get().loadLocal(path, "push");
    if (path) await get().syncPeer("local", path);
  },

  async localBack() {
    const { local } = get();
    if (local.historyIndex <= 0) return;
    const index = local.historyIndex - 1;
    set((state) => ({ local: { ...state.local, historyIndex: index } }));
    await get().loadLocal(local.history[index], "none");
  },

  async localForward() {
    const { local } = get();
    if (local.historyIndex >= local.history.length - 1) return;
    const index = local.historyIndex + 1;
    set((state) => ({ local: { ...state.local, historyIndex: index } }));
    await get().loadLocal(local.history[index], "none");
  },

  async localUp() {
    const { local } = get();
    if (local.path === null) return;
    await get().navigateLocal(local.parent);
  },

  async refreshLocal() {
    await get().loadLocal(get().local.path, "none");
  },

  setLocalSelection(selected) {
    set((state) => ({ local: { ...state.local, selected } }));
  },

  toggleLocalSort(key) {
    const current = get().local.sort;
    set((state) => ({
      local: {
        ...state.local,
        sort: current.key === key ? { key, dir: current.dir === 1 ? -1 : 1 } : { key, dir: 1 },
      },
    }));
  },

  promptNewFolder(side) {
    const language = get().settings.language;
    get().prompt({
      title: t(language, "dialog.newFolder"),
      label: t(language, "dialog.folderName"),
      value: "",
      confirmLabel: t(language, "dialog.create"),
      onSubmit: async (value) => {
        const name = value.trim();
        if (!name) return;
        try {
          if (side === "local") {
            const base = get().local.path;
            if (!base) return;
            await api.localMkdir(joinLocal(base, name));
            await get().refreshLocal();
          } else {
            const tab = activeTab(get());
            if (!tab) return;
            await api.remoteMkdir(tab.id, joinRemote(tab.remote.path, name));
            await get().refreshRemote(tab.id);
          }
          get().toast("success", t(language, "toast.created"));
        } catch (error) {
          get().toast("error", errorMessage(error, t(language, "toast.error")));
        }
      },
    });
  },

  promptRename(side, item) {
    const language = get().settings.language;
    get().prompt({
      title: t(language, "dialog.rename"),
      label: t(language, "dialog.newName"),
      value: item.name,
      confirmLabel: t(language, "dialog.rename"),
      onSubmit: async (value) => {
        const name = value.trim();
        if (!name || name === item.name) return;
        try {
          if (side === "local") {
            await api.localRename(item.path, joinLocal(dirOf(item.path), name));
            await get().refreshLocal();
          } else {
            const tab = activeTab(get());
            if (!tab) return;
            await api.remoteRename(tab.id, item.path, joinRemote(dirOfRemote(item.path), name));
            await get().refreshRemote(tab.id);
          }
          get().toast("success", t(language, "toast.renamed"));
        } catch (error) {
          get().toast("error", errorMessage(error, t(language, "toast.error")));
        }
      },
    });
  },

  promptChmod(item) {
    const language = get().settings.language;
    const tab = activeTab(get());
    if (!tab) return;
    get().prompt({
      title: t(language, "dialog.chmod"),
      label: t(language, "dialog.chmodLabel"),
      value: (item.mode ?? 0o644).toString(8).padStart(3, "0"),
      hint: t(language, "dialog.chmodHint"),
      confirmLabel: t(language, "dialog.save"),
      onSubmit: async (value) => {
        const mode = Number.parseInt(value.trim(), 8);
        if (Number.isNaN(mode)) {
          get().toast("error", "Invalid octal mode");
          return;
        }
        try {
          await api.remoteChmod(tab.id, item.path, mode);
          await get().refreshRemote(tab.id);
          get().toast("success", t(language, "toast.saved"));
        } catch (error) {
          get().toast("error", errorMessage(error, t(language, "toast.error")));
        }
      },
    });
  },

  deleteSelection(side) {
    const language = get().settings.language;
    const tab = activeTab(get());
    const entries =
      side === "local"
        ? get().local.entries.filter((entry) => get().local.selected.includes(entry.path))
        : tab?.remote.entries.filter((entry) => tab?.remote.selected.includes(entry.path)) ?? [];
    if (!entries.length) {
      get().toast("warning", t(language, "toast.noSelection"));
      return;
    }
    const run = async () => {
      try {
        if (side === "local") {
          for (const entry of entries) await api.localDelete(entry.path);
          await get().refreshLocal();
        } else if (tab) {
          for (const entry of entries) await api.remoteDelete(tab.id, entry.path, entry.isDir);
          await get().refreshRemote(tab.id);
        }
        get().toast("success", t(language, "toast.deleted"));
      } catch (error) {
        get().toast("error", errorMessage(error, t(language, "toast.error")));
      }
    };
    if (get().settings.confirmDelete) {
      get().confirm({
        title: t(language, "dialog.deleteTitle"),
        message: t(language, "dialog.deleteMessage", { count: entries.length }),
        confirmLabel: t(language, "dialog.delete"),
        danger: true,
        onConfirm: run,
      });
    } else {
      void run();
    }
  },

  copySelectionPath(side) {
    const tab = activeTab(get());
    const paths =
      side === "local"
        ? get().local.selected
        : tab?.remote.selected ?? [];
    if (!paths.length) {
      get().toast("warning", t(get().settings.language, "toast.noSelection"));
      return;
    }
    void get().copyText(paths.join("\n"));
  },

  async transferSelection(direction) {
    const language = get().settings.language;
    const tab = activeTab(get());
    if (!tab) {
      get().toast("warning", t(language, "toast.noRemoteTarget"));
      return;
    }
    if (direction === "upload") {
      const selected = get().local.entries.filter((entry) => get().local.selected.includes(entry.path));
      if (!selected.length) {
        get().toast("warning", t(language, "toast.noSelection"));
        return;
      }
      if (selected.some((entry) => entry.isDir)) {
        get().addLog("info", "transfer", "Expanding selected folders…");
      }
      const items = selected.map<TransferItem>((entry) => ({
        direction: "upload",
        localPath: entry.path,
        remotePath: joinRemote(tab.remote.path, entry.name),
        size: entry.size,
        isDir: entry.isDir,
      }));
      await get().queueTransferItems(items);
    } else {
      if (!get().local.path) {
        get().toast("warning", t(language, "toast.noLocalTarget"));
        return;
      }
      const selected = tab.remote.entries.filter((entry) => tab.remote.selected.includes(entry.path));
      if (!selected.length) {
        get().toast("warning", t(language, "toast.noSelection"));
        return;
      }
      if (selected.some((entry) => entry.isDir)) {
        get().addLog("info", "transfer", "Expanding selected folders…");
      }
      const items = selected.map<TransferItem>((entry) => ({
        direction: "download",
        localPath: joinLocal(get().local.path as string, entry.name),
        remotePath: entry.path,
        size: entry.size,
        isDir: entry.isDir,
      }));
      await get().queueTransferItems(items);
    }
  },

  async queueTransferItems(items) {
    const language = get().settings.language;
    const tab = activeTab(get());
    if (!tab) {
      get().toast("warning", t(language, "toast.noRemoteTarget"));
      return;
    }
    if (!items.length) return;
    try {
      const created = await api.enqueue({ sessionId: tab.id, items });
      set((state) => ({
        transfers: mergeTransfers(state.transfers, created),
        ui: { ...state.ui, queueOpen: true },
      }));
      if (created.length > 0) {
        get().toast("success", t(language, "toast.queued", { count: created.length }));
        get().addLog("info", "transfer", `Queued ${created.length} file(s) for ${tab.name}`);
      } else {
        get().toast("success", "Folders prepared; no files to transfer");
      }
    } catch (error) {
      get().toast("error", errorMessage(error, t(language, "toast.error")));
    }
  },

  async uploadExternalPaths(paths) {
    const tab = activeTab(get());
    const language = get().settings.language;
    if (!tab) {
      get().toast("warning", t(language, "toast.noRemoteTarget"));
      return;
    }
    if (!paths.length) return;
    const items = paths.map<TransferItem>((path) => ({
      direction: "upload",
      localPath: path,
      remotePath: joinRemote(tab.remote.path ?? "/", basename(path)),
      isDir: false,
    }));
    await get().queueTransferItems(items);
  },
  async handleDrop(payload, targetSide, targetDir) {
    const language = get().settings.language;
    if (payload.side === targetSide) return;
    const tab = activeTab(get());
    if (targetSide === "remote") {
      if (!tab) {
        get().toast("warning", t(language, "toast.noRemoteTarget"));
        return;
      }
      const items = payload.items.map<TransferItem>((item) => ({
        direction: "upload",
        localPath: item.path,
        remotePath: joinRemote(targetDir, item.name),
        size: item.size,
        isDir: item.isDir,
      }));
      await get().queueTransferItems(items);
    } else {
      if (!targetDir) {
        get().toast("warning", t(language, "toast.noLocalTarget"));
        return;
      }
      const items = payload.items.map<TransferItem>((item) => ({
        direction: "download",
        localPath: joinLocal(targetDir, item.name),
        remotePath: item.path,
        size: item.size,
        isDir: item.isDir,
      }));
      await get().queueTransferItems(items);
    }
  },

  async transferAction(taskId, action) {
    set((state) => ({
      transfers: state.transfers.map((task) => {
        if (task.id !== taskId) return task;
        if (action === "pause") return { ...task, status: "paused", speedBps: 0 };
        if (action === "resume") return { ...task, status: "active" };
        if (action === "cancel") return { ...task, status: "cancelled", speedBps: 0 };
        return task;
      }),
    }));
    try {
      await api.transferAction(taskId, action);
      if (action === "remove") {
        set((state) => ({ transfers: state.transfers.filter((task) => task.id !== taskId) }));
      }
    } catch (error) {
      get().toast("error", errorMessage(error, t(get().settings.language, "toast.error")));
      void get().refreshTransfersFromBackend();
    }
  },

  async locateTransfer(taskId, side) {
    const task = get().transfers.find((item) => item.id === taskId);
    if (!task) return;
    if (side === "local") {
      await get().loadLocal(dirname(task.localPath), "push");
      const entry = get().local.entries.find(
        (item) => item.path === task.localPath || item.name === basename(task.localPath),
      );
      get().setLocalSelection(entry ? [entry.path] : []);
      return;
    }
    const tabs = get().tabs;
    const tab =
      tabs.find((item) => task.siteId && item.siteId === task.siteId) ??
      tabs.find((item) => item.id === get().activeTabId);
    if (!tab) return;
    get().setActiveTab(tab.id);
    await get().loadRemote(tab.id, posixParent(task.remotePath), "push");
    const entry = get()
      .tabs.find((item) => item.id === tab.id)
      ?.remote.entries.find(
        (item) =>
          item.path === task.remotePath ||
          item.name === posixName(task.remotePath),
      );
    get().setRemoteSelection(tab.id, entry ? [entry.path] : []);
  },

  async refreshTransfersFromBackend() {
    try {
      const transfers = await api.transfersList();
      set({ transfers: mergeTransfers([], transfers) });
    } catch {
      // ignore
    }
  },

  async clearTransfers() {
    try {
      await api.transferClear();
      set((state) => ({ transfers: state.transfers.filter((task) => !isTerminal(task)) }));
    } catch (error) {
      get().toast("error", errorMessage(error, t(get().settings.language, "toast.error")));
    }
  },

  async refreshSites() {
    try {
      const sites = await api.sitesGet();
      set({ sites });
    } catch (error) {
      get().toast("error", errorMessage(error, "Could not load sites"));
    }
  },

  async saveSite(input) {
    const language = get().settings.language;
    try {
      const site = await api.siteSave(input);
      await get().refreshSites();
      get().toast("success", t(language, "toast.saved"));
      return site;
    } catch (error) {
      get().toast("error", errorMessage(error, t(language, "toast.error")));
      return null;
    }
  },

  async deleteSite(id) {
    try {
      await api.siteDelete(id);
      await get().refreshSites();
      get().toast("success", t(get().settings.language, "toast.deleted"));
      return true;
    } catch (error) {
      get().toast("error", errorMessage(error, "Could not delete site"));
      return false;
    }
  },

  async duplicateSite(id) {
    try {
      const site = await api.siteDuplicate(id);
      await get().refreshSites();
      return site;
    } catch (error) {
      get().toast("error", errorMessage(error, "Could not duplicate site"));
      return null;
    }
  },

  async createGroup(name, parentId) {
    try {
      const group = await api.groupCreate(name, parentId ?? null);
      await get().refreshSites();
      return group;
    } catch (error) {
      get().toast("error", errorMessage(error, "Could not create group"));
      return null;
    }
  },

  async renameGroup(id, name) {
    try {
      await api.groupRename(id, name);
      await get().refreshSites();
    } catch (error) {
      get().toast("error", errorMessage(error, "Could not rename group"));
    }
  },

  async deleteGroup(id) {
    try {
      await api.groupDelete(id);
      await get().refreshSites();
    } catch (error) {
      get().toast("error", errorMessage(error, "Could not delete group"));
    }
  },

  async loadKnownHosts() {
    try {
      const knownHosts = await api.knownHostsList();
      set({ knownHosts });
    } catch {
      // Security panel is optional; ignore failures.
    }
  },

  async forgetHost(host) {
    try {
      await api.knownHostForget(host);
      await get().loadKnownHosts();
      get().toast("success", t(get().settings.language, "toast.deleted"));
    } catch (error) {
      get().toast("error", errorMessage(error, "Could not forget host"));
    }
  },

  async clearLogs() {
    try {
      await api.logsClear();
    } catch {
      // Clearing local logs is still useful.
    }
    set({ logs: [] });
  },

  async copyText(text) {
    try {
      await navigator.clipboard.writeText(text);
      get().toast("success", t(get().settings.language, "toast.copied"));
    } catch {
      get().toast("error", "Clipboard is not available");
    }
  },
}));

// The store interface is intentionally explicit; this helper keeps the
// transfer refresh action reachable from `transferAction`.
declare module "./app" {
  export interface AppStore {
    refreshTransfersFromBackend: () => Promise<void>;
  }
}

function activeTab(state: AppStore): TabState | undefined {
  return state.tabs.find((tab) => tab.id === state.activeTabId);
}

function isTerminal(task: TransferTask): boolean {
  return task.status === "completed" || task.status === "failed" || task.status === "cancelled";
}

function dirOf(path: string): string {
  const normalized = path.replace(/[\\/]+$/, "");
  const index = Math.max(normalized.lastIndexOf("/"), normalized.lastIndexOf("\\"));
  return index <= 0 ? normalized.slice(0, index + 1) : normalized.slice(0, index);
}

function dirOfRemote(path: string): string {
  const normalized = path.replace(/\/+$/, "");
  const index = normalized.lastIndexOf("/");
  return index <= 0 ? "/" : normalized.slice(0, index);
}

async function openConnection(
  request: ConnectRequest,
  config: SiteConfig | null,
  fallbackName: string,
  language: string,
  set: (partial: Partial<AppStore> | ((state: AppStore) => Partial<AppStore>)) => void,
  get: () => AppStore,
) {
  set({ connecting: true });
  try {
    const info: SessionInfo = await api.connect(request);
    const initial = config?.initialRemoteDir?.trim() || info.homeDir || "/";
    const tab: TabState = {
      id: info.sessionId,
      siteId: info.siteId ?? null,
      name: info.name || fallbackName,
      protocol: info.protocol,
      host: info.host,
      port: info.port,
      username: info.username,
      home: info.homeDir,
      config,
      remote: {
        path: initial,
        parent: null,
        entries: [],
        loading: true,
        selected: [],
        sort: DEFAULT_SORT,
        history: [initial],
        historyIndex: 0,
      },
    };
    set((state) => ({
      tabs: [...state.tabs, tab],
      activeTabId: tab.id,
      connecting: false,
    }));
    get().toast("success", t(language, "toast.connected", { name: info.name }));
    await get().loadRemote(tab.id, initial, "replace");
    const localDir = config?.localDir || get().settings.defaultLocalDir;
    if (localDir) await get().loadLocal(localDir, "push");
  } catch (error) {
    set({ connecting: false });
    const message = errorMessage(error, t(language, "toast.error"));
    get().toast("error", message, t(language, "toast.error"));
    get().addLog("error", "session", message);
  }
}
