export type Protocol = "ftp" | "ftps" | "ftpsi" | "sftp";
export type AuthMethod = "password" | "key";
export type TransferMode = "auto" | "binary" | "ascii";
export type ThemeMode = "system" | "light" | "dark";

export type SortKey = "name" | "size" | "modified" | "permissions";

export interface PaneSort {
  key: SortKey;
  dir: 1 | -1;
}

export interface SiteConfig {
  protocol: Protocol;
  host: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  privateKeyPath?: string | null;
  initialRemoteDir?: string | null;
  localDir?: string | null;
  passiveMode: boolean;
  verifyCertificate: boolean;
  connectTimeoutSecs: number;
  keepaliveSecs: number;
  transferMode: TransferMode;
  encoding: string;
  maxConnections: number;
  notes?: string | null;
  proxyType: "" | "none" | "http" | "socks5" | string;
  proxyHost: string;
  proxyPort: number;
  proxyUsername: string;
}

export interface Site {
  id: string;
  name: string;
  groupId?: string | null;
  config: SiteConfig;
  hasPassword: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface SiteGroup {
  id: string;
  name: string;
  parentId?: string | null;
}

export interface ImportOutcome {
  imported: number;
  skipped: number;
  groupsCreated: number;
}

export interface AutoImportOutcome extends ImportOutcome {
  sources: string[];
}

export interface DetectedSites {
  source: string;
}

export interface ExportOutcome {
  sites: number;
  groups: number;
  passwordsIncluded: boolean;
}

export interface SitesSnapshot {
  sites: Site[];
  groups: SiteGroup[];
}

export interface SiteInput {
  id?: string | null;
  name: string;
  groupId?: string | null;
  config: SiteConfig;
  password?: string | null;
  keyPassphrase?: string | null;
  proxyPassword?: string | null;
  clearPassword?: boolean;
}

export interface Credentials {
  password?: string | null;
  keyPassphrase?: string | null;
}

export interface ConnectRequest {
  siteId?: string | null;
  name?: string | null;
  config?: SiteConfig | null;
  credentials?: Credentials | null;
  savePassword?: boolean;
}

export interface SessionInfo {
  sessionId: string;
  protocol: Protocol;
  host: string;
  port: number;
  username: string;
  name: string;
  homeDir: string;
  banner?: string;
  features: string[];
  siteId?: string | null;
}

export interface RemoteEntry {
  name: string;
  path: string;
  isDir: boolean;
  isSymlink: boolean;
  size: number;
  modified?: string;
  permissions?: string;
  mode?: number;
  owner?: string;
  group?: string;
}

export interface RemoteListing {
  path: string;
  parent: string | null;
  entries: RemoteEntry[];
}

export type LocalKind = "file" | "directory" | "drive" | "place";

export interface LocalEntry {
  name: string;
  path: string;
  kind: LocalKind;
  isDir: boolean;
  isSymlink: boolean;
  size: number;
  modified?: string;
  hidden: boolean;
  extension?: string;
  permissions?: string;
  freeSpace?: number;
}

export interface LocalListing {
  path: string | null;
  parent: string | null;
  entries: LocalEntry[];
  places: LocalEntry[];
}

export type TransferDirection = "download" | "upload";
export type TransferStatus =
  | "queued"
  | "connecting"
  | "active"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export interface TransferTask {
  id: string;
  sessionId: string;
  siteId?: string | null;
  siteName: string;
  direction: TransferDirection;
  localPath: string;
  remotePath: string;
  name: string;
  size: number;
  transferred: number;
  resumedFrom: number;
  mtime?: number;
  status: TransferStatus;
  error?: string;
  errorCode?: string;
  speedBps: number;
  createdAt: number;
  startedAt?: number;
  finishedAt?: number;
}

export interface TransferItem {
  direction: TransferDirection;
  localPath: string;
  remotePath: string;
  size?: number;
  isDir?: boolean;
}

export interface EnqueueRequest {
  sessionId: string;
  items: TransferItem[];
}

export type TransferAction = "pause" | "resume" | "cancel" | "retry" | "remove";

export interface TransferSummary {
  active: number;
  queued: number;
  completed: number;
  failed: number;
  totalSpeedBps: number;
}

export interface HostKeyPrompt {
  id: string;
  host: string;
  port: number;
  keyType: string;
  fingerprint: string;
}

export interface EditStart {
  localPath: string;
  remotePath: string;
}

export interface Bookmark {
  id: string;
  name: string;
  side: "local" | "remote";
  path: string;
}

export interface AppSettings {
  theme: ThemeMode;
  accent: string;
  language: string;
  density: "comfortable" | "compact";
  showHiddenFiles: boolean;
  confirmDelete: boolean;
  doubleClickAction: "transfer" | "open";
  concurrentTransfers: number;
  downloadLimitKbps: number;
  uploadLimitKbps: number;
  resumeTransfers: boolean;
  preserveTimestamps: boolean;
  syncBrowsing: boolean;
  bookmarks: Bookmark[];
  notifyOnComplete: boolean;
  soundOnComplete: boolean;
  minimizeToTray: boolean;
  defaultLocalDir?: string | null;
  logLevel: string;
  showLogPanel: boolean;
  filezillaImportPrompted: boolean;
  animations: boolean;
  micaEffect: boolean;
  toolbarLabels: boolean;
}

export interface LogEntry {
  id: number;
  timestamp: number;
  level: "debug" | "info" | "warn" | "error" | string;
  scope: string;
  message: string;
}

export interface AppInfo {
  name: string;
  version: string;
  tauriVersion: string;
  os: string;
  arch: string;
  configDir: string;
}

export interface ManagedPolicy {
  managed: boolean;
  loadedFrom?: string;
  configDir?: string;
  kioskMode: number;
  readOnly: boolean;
  disableUpdateCheck: boolean;
  cacheDir?: string;
  warning?: string;
}

export interface KnownHost {
  host: string;
  keyType: string;
  fingerprint: string;
  addedAt: number;
}

export interface AppErrorShape {
  code?: string;
  message?: string;
  detail?: string;
}
