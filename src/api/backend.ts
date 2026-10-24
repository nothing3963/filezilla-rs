import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  AutoImportOutcome,
  DetectedSites,
  EditStart,
  ExportOutcome,
  ImportOutcome,
  AppSettings,
  ConnectRequest,
  EnqueueRequest,
  KnownHost,
  LocalListing,
  LogEntry,
  ManagedPolicy,
  RemoteEntry,
  RemoteListing,
  SessionInfo,
  Site,
  SiteGroup,
  SiteInput,
  SitesSnapshot,
  TransferAction,
  TransferSummary,
  TransferTask,
} from "../types";
import { mockInvoke } from "./mock";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export class ApiError extends Error {
  code: string;
  detail?: string;
  constructor(message: string, code = "ERROR", detail?: string) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.detail = detail;
  }
}

function normalizeError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  if (typeof error === "string") return new ApiError(error, "ERROR");
  if (error && typeof error === "object") {
    const value = error as { code?: string; message?: string; detail?: string };
    return new ApiError(value.message ?? "Unknown error", value.code ?? "ERROR", value.detail);
  }
  return new ApiError(String(error), "ERROR");
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    if (!isTauri()) {
      return await mockInvoke<T>(command, args);
    }
    return await tauriInvoke<T>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  managedPolicy: () => call<ManagedPolicy>("managed_policy"),
  settingsGet: () => call<AppSettings>("settings_get"),
  settingsSet: (settings: AppSettings) => call<AppSettings>("settings_set", { settings }),
  applyWindowTheme: (dark: boolean, mica: boolean) =>
    call<void>("apply_window_theme", { dark, mica }),

  sitesGet: () => call<SitesSnapshot>("sites_get"),
  siteSave: (input: SiteInput) => call<Site>("site_save", { input }),
  siteDelete: (id: string) => call<void>("site_delete", { id }),
  siteDuplicate: (id: string) => call<Site>("site_duplicate", { id }),
  sitesImport: (path: string) => call<ImportOutcome>("sites_import", { path }),
  sitesImportAuto: () => call<AutoImportOutcome | null>("sites_import_auto"),
  sitesDetectAuto: () => call<DetectedSites | null>("sites_detect_auto"),
  sitesExport: (path: string, includePasswords = false) =>
    call<ExportOutcome>("sites_export", { path, includePasswords }),

  groupCreate: (name: string, parentId?: string | null) =>
    call<SiteGroup>("group_create", { name, parentId: parentId ?? null }),
  groupRename: (id: string, name: string) => call<SiteGroup>("group_rename", { id, name }),
  groupDelete: (id: string) => call<void>("group_delete", { id }),

  connect: (request: ConnectRequest) => call<SessionInfo>("connect", { request }),
  disconnect: (sessionId: string) => call<void>("disconnect", { sessionId }),

  remoteList: (sessionId: string, path: string) =>
    call<RemoteListing>("remote_list", { sessionId, path }),
  remoteStat: (sessionId: string, path: string) =>
    call<RemoteEntry>("remote_stat", { sessionId, path }),
  remoteMkdir: (sessionId: string, path: string) =>
    call<void>("remote_mkdir", { sessionId, path }),
  remoteRename: (sessionId: string, from: string, to: string) =>
    call<void>("remote_rename", { sessionId, from, to }),
  remoteChmod: (sessionId: string, path: string, mode: number) =>
    call<void>("remote_chmod", { sessionId, path, mode }),
  remoteDelete: (sessionId: string, path: string, recursive: boolean) =>
    call<void>("remote_delete", { sessionId, path, recursive }),

  localList: (path: string | null, showHidden?: boolean) =>
    call<LocalListing>("local_list", { path, showHidden }),
  localMkdir: (path: string) => call<void>("local_mkdir", { path }),
  localRename: (from: string, to: string) => call<void>("local_rename", { from, to }),
  localDelete: (path: string) => call<void>("local_delete", { path }),

  sshHostkeyResolve: (id: string, accepted: boolean) =>
    call<boolean>("ssh_hostkey_resolve", { id, accepted }),
  remoteEditStart: (sessionId: string, remotePath: string) =>
    call<EditStart>("remote_edit_start", { sessionId, remotePath }),
  remoteEditStop: (remotePath: string) =>
    call<void>("remote_edit_stop", { remotePath }),
  searchRemote: (sessionId: string, root: string, query: string, maxResults = 200) =>
    call<RemoteEntry[]>("search_remote", { sessionId, root, query, maxResults }),
  enqueue: (request: EnqueueRequest) => call<TransferTask[]>("enqueue", { request }),
  transfersList: () => call<TransferTask[]>("transfers_list"),
  transferSummary: () => call<TransferSummary>("transfer_summary"),
  transferAction: (taskId: string, action: TransferAction) =>
    call<void>("transfer_action", { taskId, action }),
  transferClear: () => call<void>("transfer_clear"),

  logsHistory: () => call<LogEntry[]>("logs_history"),
  logsClear: () => call<void>("logs_clear"),
  knownHostsList: () => call<KnownHost[]>("known_hosts_list"),
  knownHostForget: (host: string) => call<void>("known_host_forget", { host }),
  logClient: (level: string, scope: string, message: string) =>
    call<void>("log_client", { level, scope, message }),
};
