/**
 * Browser-preview backend.
 *
 * The real application talks to Rust through Tauri commands.  When the UI is
 * opened in a plain browser (`pnpm dev`) this module provides a small, stateful
 * fake so the entire interface can be designed and tested without a build.
 */
import { emitMockEvent } from "./events";
import type {
  AppInfo,
  AppSettings,
  LocalEntry,
  LocalListing,
  RemoteEntry,
  Site,
  SiteConfig,
  SiteGroup,
  TransferAction,
  TransferTask,
} from "../types";

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const uuid = () => crypto.randomUUID();
const iso = (daysAgo: number, hour = 9, minute = 12) => {
  const date = new Date();
  date.setDate(date.getDate() - daysAgo);
  date.setHours(hour, minute, 0, 0);
  return date.toISOString();
};

const DEFAULT_CONFIG: SiteConfig = {
  protocol: "sftp",
  host: "demo.filezilla.dev",
  port: 22,
  username: "demo",
  authMethod: "password",
  passiveMode: true,
  verifyCertificate: true,
  connectTimeoutSecs: 20,
  keepaliveSecs: 30,
  transferMode: "auto",
  encoding: "UTF-8",
  maxConnections: 2,
  initialRemoteDir: "/home/demo",
  localDir: null,
  notes: "",
  proxyType: "",
  proxyHost: "",
  proxyPort: 0,
  proxyUsername: "",
};

let settings: AppSettings = {
  theme: "dark",
  accent: "blue",
  language: "",
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
  filezillaImportPrompted: true,
  animations: true,
  micaEffect: true,
  toolbarLabels: true,
};

const groups: SiteGroup[] = [
  { id: "g-work", name: "Work", parentId: null },
  { id: "g-lab", name: "Homelab", parentId: null },
];

const sites: Site[] = [
  {
    id: "site-demo",
    name: "Demo SFTP",
    groupId: "g-work",
    config: { ...DEFAULT_CONFIG },
    hasPassword: true,
    createdAt: Date.now() - 86400000 * 12,
    updatedAt: Date.now() - 86400000 * 2,
  },
  {
    id: "site-ftp",
    name: "Public FTP",
    groupId: null,
    config: {
      ...DEFAULT_CONFIG,
      protocol: "ftps",
      host: "ftp.example.org",
      port: 21,
      username: "anonymous",
      initialRemoteDir: "/pub",
    },
    hasPassword: false,
    createdAt: Date.now() - 86400000 * 30,
    updatedAt: Date.now() - 86400000 * 30,
  },
  {
    id: "site-nas",
    name: "Home NAS",
    groupId: "g-lab",
    config: {
      ...DEFAULT_CONFIG,
      protocol: "ftp",
      host: "192.168.1.10",
      port: 2121,
      username: "admin",
      initialRemoteDir: "/volume1",
    },
    hasPassword: true,
    createdAt: Date.now() - 86400000 * 5,
    updatedAt: Date.now() - 86400000,
  },
];

function remote(name: string, path: string, isDir: boolean, size = 0, days = 3): RemoteEntry {
  return {
    name,
    path,
    isDir,
    isSymlink: false,
    size,
    modified: iso(days),
    permissions: isDir ? "drwxr-xr-x" : "-rw-r--r--",
    mode: isDir ? 0o755 : 0o644,
    owner: "demo",
    group: "demo",
  };
}

const tree = new Map<string, RemoteEntry[]>();

function seedRemote() {
  tree.clear();
  tree.set("/", [
    remote("home", "/home", true, 0, 12),
    remote("tmp", "/tmp", true, 0, 1),
    remote("var", "/var", true, 0, 20),
  ]);
  tree.set("/home", [remote("demo", "/home/demo", true, 0, 2)]);
  tree.set("/home/demo", [
    remote("Documents", "/home/demo/Documents", true, 0, 6),
    remote("Downloads", "/home/demo/Downloads", true, 0, 2),
    remote("public_html", "/home/demo/public_html", true, 0, 1),
    remote("backups", "/home/demo/backups", true, 0, 9),
    remote("welcome.txt", "/home/demo/welcome.txt", false, 2048, 1),
    remote("deploy.sh", "/home/demo/deploy.sh", false, 4096, 3),
    remote("notes.md", "/home/demo/notes.md", false, 13824, 4),
  ]);
  tree.set("/home/demo/Documents", [
    remote("project-plan.docx", "/home/demo/Documents/project-plan.docx", false, 284672, 5),
    remote("budget-2026.xlsx", "/home/demo/Documents/budget-2026.xlsx", false, 91432, 8),
    remote("architecture.pdf", "/home/demo/Documents/architecture.pdf", false, 2483921, 2),
  ]);
  tree.set("/home/demo/Downloads", [
    remote("backup-2026-10.zip", "/home/demo/Downloads/backup-2026-10.zip", false, 48123904, 2),
  ]);
  tree.set("/home/demo/public_html", [
    remote("assets", "/home/demo/public_html/assets", true, 0, 1),
    remote("index.html", "/home/demo/public_html/index.html", false, 5821, 1),
    remote("styles.css", "/home/demo/public_html/styles.css", false, 18432, 1),
    remote("app.js", "/home/demo/public_html/app.js", false, 91822, 1),
  ]);
  tree.set("/home/demo/public_html/assets", [
    remote("logo.svg", "/home/demo/public_html/assets/logo.svg", false, 4211, 1),
    remote("hero.webp", "/home/demo/public_html/assets/hero.webp", false, 184320, 1),
  ]);
  tree.set("/home/demo/backups", [
    remote("db-2026-10-21.sql.gz", "/home/demo/backups/db-2026-10-21.sql.gz", false, 12048192, 2),
    remote("db-2026-10-20.sql.gz", "/home/demo/backups/db-2026-10-20.sql.gz", false, 11839211, 3),
  ]);
  tree.set("/pub", [
    remote("incoming", "/pub/incoming", true, 0, 1),
    remote("README.txt", "/pub/README.txt", false, 1200, 10),
  ]);
  tree.set("/pub/incoming", []);
  tree.set("/volume1", [
    remote("media", "/volume1/media", true, 0, 3),
    remote("docker", "/volume1/docker", true, 0, 3),
    remote("shared", "/volume1/shared", true, 0, 3),
  ]);
}
seedRemote();

function entriesFor(path: string): RemoteEntry[] {
  return tree.get(path) ?? [];
}

const HOME = "C:\\Users\\demo";

function local(
  name: string,
  path: string,
  isDir: boolean,
  size = 0,
  days = 3,
  hidden = false,
): LocalEntry {
  return {
    name,
    path,
    kind: isDir ? "directory" : "file",
    isDir,
    isSymlink: false,
    size,
    modified: iso(days),
    hidden,
    extension: isDir ? undefined : name.split(".").pop()?.toLowerCase(),
    permissions: isDir ? "drwxr-xr-x" : "-rw-r--r--",
  };
}

const locals = new Map<string, LocalEntry[]>();

function seedLocal() {
  locals.set(HOME, [
    local("Desktop", `${HOME}\\Desktop`, true, 0, 1),
    local("Documents", `${HOME}\\Documents`, true, 0, 2),
    local("Downloads", `${HOME}\\Downloads`, true, 0, 1),
    local("Pictures", `${HOME}\\Pictures`, true, 0, 4),
    local("Projects", `${HOME}\\Projects`, true, 0, 1),
    local("Videos", `${HOME}\\Videos`, true, 0, 12),
    local("notes.txt", `${HOME}\\notes.txt`, false, 4211, 1),
    local("resume.pdf", `${HOME}\\resume.pdf`, false, 284112, 6),
    local("screenshot-2026-10-23.png", `${HOME}\\screenshot-2026-10-23.png`, false, 1843201, 1),
    local(".env", `${HOME}\\.env`, false, 412, 30, true),
    local("desktop.ini", `${HOME}\\desktop.ini`, false, 282, 60, true),
  ]);
  locals.set(`${HOME}\\Desktop`, [
    local("FileZilla Modern.lnk", `${HOME}\\Desktop\\FileZilla Modern.lnk`, false, 1821, 2),
    local("todo.txt", `${HOME}\\Desktop\\todo.txt`, false, 812, 1),
  ]);
  locals.set(`${HOME}\\Documents`, [
    local("Contracts", `${HOME}\\Documents\\Contracts`, true, 0, 8),
    local("project-plan.docx", `${HOME}\\Documents\\project-plan.docx`, false, 284672, 5),
    local("budget-2026.xlsx", `${HOME}\\Documents\\budget-2026.xlsx`, false, 91432, 8),
  ]);
  locals.set(`${HOME}\\Downloads`, [
    local("filezilla-modern_0.1.0_x64-setup.exe", `${HOME}\\Downloads\\filezilla-modern_0.1.0_x64-setup.exe`, false, 64221184, 1),
    local("rust-book.pdf", `${HOME}\\Downloads\\rust-book.pdf`, false, 12048192, 10),
  ]);
  locals.set(`${HOME}\\Pictures`, [
    local("wallpaper.jpg", `${HOME}\\Pictures\\wallpaper.jpg`, false, 3841024, 14),
  ]);
  locals.set(`${HOME}\\Projects`, [
    local("filezilla-rs", `${HOME}\\Projects\\filezilla-rs`, true, 0, 1),
    local("README.md", `${HOME}\\Projects\\README.md`, false, 2482, 1),
  ]);
  locals.set(`${HOME}\\Videos`, []);
  locals.set("C:\\", [
    local("Program Files", "C:\\Program Files", true, 0, 40),
    local("Users", "C:\\Users", true, 0, 40),
    local("Windows", "C:\\Windows", true, 0, 40),
  ]);
  locals.set("D:\\", [
    local("Games", "D:\\Games", true, 0, 30),
    local("Workspace", "D:\\Projects", true, 0, 1),
  ]);
}
seedLocal();

const places: LocalEntry[] = [
  { ...local("Home", HOME, true), kind: "place" },
  { ...local("Desktop", `${HOME}\\Desktop`, true), kind: "place" },
  { ...local("Downloads", `${HOME}\\Downloads`, true), kind: "place" },
  { ...local("Documents", `${HOME}\\Documents`, true), kind: "place" },
  { ...local("Pictures", `${HOME}\\Pictures`, true), kind: "place" },
];

function parentOf(path: string): string | null {
  if (path === HOME) return "C:\\Users";
  if (/^[A-Z]:\\$/.test(path)) return null;
  const trimmed = path.replace(/\\+$/, "");
  const idx = trimmed.lastIndexOf("\\");
  if (idx <= 2) return trimmed.slice(0, 2) + "\\";
  return trimmed.slice(0, idx);
}

function localListing(path: string | null): LocalListing {
  if (!path) {
    return {
      path: null,
      parent: null,
      entries: [
        { ...local("C:", "C:\\", true), kind: "drive", freeSpace: 184_000_000_000 },
        { ...local("D:", "D:\\", true), kind: "drive", freeSpace: 512_000_000_000 },
      ],
      places,
    };
  }
  return {
    path,
    parent: parentOf(path),
    entries: locals.get(path) ?? [],
    places,
  };
}

const transfers = new Map<string, TransferTask>();
const timers = new Map<string, ReturnType<typeof setInterval>>();

function stopTimer(id: string) {
  const timer = timers.get(id);
  if (timer) clearInterval(timer);
  timers.delete(id);
}

function emitTransfer(task: TransferTask) {
  transfers.set(task.id, task);
  emitMockEvent("transfer:update", task);
}

function startTransfer(task: TransferTask) {
  stopTimer(task.id);
  task.status = "connecting";
  task.startedAt = Date.now();
  emitTransfer({ ...task });
  setTimeout(() => {
    if (task.status === "cancelled") return;
    task.status = "active";
    emitTransfer({ ...task });
    const timer = setInterval(() => {
      if (task.status === "paused") return;
      if (task.status === "cancelled") {
        stopTimer(task.id);
        return;
      }
      const step = Math.max(16384, Math.round(task.size * 0.035));
      task.transferred = Math.min(task.size, task.transferred + step);
      task.speedBps = Math.round(2_400_000 + Math.random() * 4_000_000);
      if (task.transferred >= task.size) {
        task.status = "completed";
        task.speedBps = 0;
        task.finishedAt = Date.now();
        stopTimer(task.id);
        if (task.direction === "download") {
          const name = task.localPath.split("\\").pop() ?? task.name;
          const parent = task.localPath.slice(0, task.localPath.length - name.length - 1);
          const list = locals.get(parent);
          if (list && !list.some((entry) => entry.name === name)) {
            list.unshift(local(name, task.localPath, false, task.size, 0));
          }
        }
      }
      emitTransfer({ ...task });
    }, 320);
    timers.set(task.id, timer);
  }, 500);
}

async function enqueue(args: Record<string, unknown>) {
  const request = args.request as { sessionId: string; items: { direction: "download" | "upload"; localPath: string; remotePath: string; size?: number }[] };
  const created: TransferTask[] = [];
  for (const item of request.items) {
    const name = item.remotePath.split("/").pop() || "item";
    const size = item.size ?? Math.round(512_000 + Math.random() * 8_000_000);
    const task: TransferTask = {
      id: uuid(),
      sessionId: request.sessionId,
      siteName: "Demo SFTP",
      direction: item.direction,
      localPath: item.localPath,
      remotePath: item.remotePath,
      name,
      size,
      transferred: 0,
      resumedFrom: 0,
      status: "queued",
      speedBps: 0,
      createdAt: Date.now(),
    };
    transfers.set(task.id, task);
    created.push(task);
    setTimeout(() => startTransfer(task), 250);
  }
  return created;
}

const apiInfo: AppInfo = {
  name: "FileZilla Modern",
  version: "0.1.0",
  tauriVersion: "2.12.4",
  os: "windows",
  arch: "x86_64",
  configDir: "C:\\Users\\demo\\AppData\\Roaming\\org.filezilla.modern",
};

export async function mockInvoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  await wait(80);
  switch (command) {
    case "app_info":
      return apiInfo as T;
    case "managed_policy":
      return { managed: false, kioskMode: 0, readOnly: false, disableUpdateCheck: false } as T;
    case "settings_get":
      return { ...settings } as T;
    case "settings_set":
      settings = { ...(args.settings as AppSettings) };
      return { ...settings } as T;
    case "sites_get":
      return { sites: sites.map((s) => ({ ...s })), groups: groups.map((g) => ({ ...g })) } as T;
    case "site_save": {
      const input = args.input as {
        id?: string;
        name: string;
        groupId?: string | null;
        config: SiteConfig;
        password?: string | null;
        clearPassword?: boolean;
      };
      const existing = sites.findIndex((s) => s.id === input.id);
      const site: Site = {
        id: input.id ?? uuid(),
        name: input.name,
        groupId: input.groupId ?? null,
        config: input.config,
        hasPassword: input.clearPassword ? false : Boolean(input.password) || (existing >= 0 && sites[existing].hasPassword),
        createdAt: existing >= 0 ? sites[existing].createdAt : Date.now(),
        updatedAt: Date.now(),
      };
      if (existing >= 0) sites[existing] = site;
      else sites.push(site);
      return site as T;
    }
    case "sites_import":
      return { imported: 0, skipped: 0, groupsCreated: 0 } as T;
    case "sites_import_auto":
      return null as T;
    case "sites_detect_auto":
      return null as T;
    case "sites_export":
      return { sites: sites.length, groups: groups.length, passwordsIncluded: false } as T;
    case "site_delete": {
      const idx = sites.findIndex((s) => s.id === args.id);
      if (idx >= 0) sites.splice(idx, 1);
      return undefined as T;
    }
    case "site_duplicate": {
      const source = sites.find((s) => s.id === args.id);
      if (!source) throw { code: "NOT_FOUND", message: "Site not found" };
      const copy: Site = { ...source, id: uuid(), name: `${source.name} (copy)`, hasPassword: false };
      sites.push(copy);
      return copy as T;
    }
    case "group_create": {
      const group: SiteGroup = { id: uuid(), name: String(args.name), parentId: (args.parentId as string | null) ?? null };
      groups.push(group);
      return group as T;
    }
    case "group_rename": {
      const group = groups.find((g) => g.id === args.id);
      if (!group) throw { code: "NOT_FOUND", message: "Group not found" };
      group.name = String(args.name);
      return { ...group } as T;
    }
    case "group_delete": {
      const idx = groups.findIndex((g) => g.id === args.id);
      if (idx >= 0) groups.splice(idx, 1);
      for (const site of sites) if (site.groupId === args.id) site.groupId = null;
      return undefined as T;
    }
    case "connect": {
      const request = args.request as { siteId?: string; name?: string; config?: SiteConfig };
      const site = sites.find((s) => s.id === request.siteId);
      const config = site?.config ?? request.config ?? DEFAULT_CONFIG;
      await wait(720);
      if (config.host.includes("fail")) {
        throw { code: "AUTH", message: "Authentication failed. Check the username and password." };
      }
      if (config.protocol === "sftp" && config.host === "demo.filezilla.dev") {
        seedRemote();
      }
      return {
        sessionId: uuid(),
        protocol: config.protocol,
        host: config.host,
        port: config.port,
        username: config.username,
        name: site?.name ?? request.name ?? config.host,
        homeDir: config.initialRemoteDir || "/home/demo",
        banner: "Welcome to the FileZilla Modern demo server",
        features: ["UTF8", "MLSD", "SIZE", "MDTM", "AUTH TLS"],
        siteId: site?.id ?? null,
      } as T;
    }
    case "disconnect":
      return undefined as T;
    case "remote_list": {
      const path = args.path as string;
      return { path, parent: path === "/" ? null : path.replace(/\/[^/]+$/, "") || "/", entries: entriesFor(path) } as T;
    }
    case "remote_stat": {
      const path = args.path as string;
      const parent = path.replace(/\/[^/]+$/, "") || "/";
      const entry = entriesFor(parent).find((e) => e.path === path) ?? remote(path.split("/").pop() ?? path, path, false, 0, 1);
      return entry as T;
    }
    case "remote_mkdir": {
      const path = args.path as string;
      const parent = path.replace(/\/[^/]+$/, "") || "/";
      entriesFor(parent).push(remote(path.split("/").pop() ?? path, path, true, 0, 0));
      tree.set(path, []);
      return undefined as T;
    }
    case "remote_rename": {
      const from = args.from as string;
      const to = args.to as string;
      const fromParent = from.replace(/\/[^/]+$/, "") || "/";
      const toParent = to.replace(/\/[^/]+$/, "") || "/";
      const list = entriesFor(fromParent);
      const idx = list.findIndex((e) => e.path === from);
      if (idx >= 0) {
        list[idx] = { ...list[idx], name: to.split("/").pop() ?? to, path: to };
        entriesFor(toParent).push(list.splice(idx, 1)[0]);
        const moved = entriesFor(toParent).pop()!;
        entriesFor(toParent).push(moved);
      }
      return undefined as T;
    }
    case "remote_delete": {
      const path = args.path as string;
      const parent = path.replace(/\/[^/]+$/, "") || "/";
      const list = entriesFor(parent);
      const idx = list.findIndex((e) => e.path === path);
      if (idx >= 0) list.splice(idx, 1);
      return undefined as T;
    }
    case "remote_chmod":
      return undefined as T;
    case "local_list": {
      const path = (args.path as string | null) ?? null;
      return localListing(path) as T;
    }
    case "local_mkdir": {
      const path = args.path as string;
      const parent = parentOf(path);
      if (parent) {
        const list = locals.get(parent) ?? [];
        list.push(local(path.split("\\").pop() ?? path, path, true, 0, 0));
        locals.set(parent, list);
      }
      return undefined as T;
    }
    case "local_rename":
      return undefined as T;
    case "local_delete":
      return undefined as T;
    case "ssh_hostkey_resolve":
      return true as T;
    case "remote_edit_start":
      return { localPath: "/tmp/remote-file", remotePath: "/remote-file" } as T;
    case "remote_edit_stop":
      return undefined as T;
    case "search_remote":
      return [] as T;
    case "enqueue":
      return (await enqueue(args)) as T;
    case "transfers_list":
      return [...transfers.values()] as T;
    case "transfer_summary": {
      const values = [...transfers.values()];
      return {
        active: values.filter((t) => t.status === "active" || t.status === "connecting").length,
        queued: values.filter((t) => t.status === "queued" || t.status === "paused").length,
        completed: values.filter((t) => t.status === "completed").length,
        failed: values.filter((t) => t.status === "failed" || t.status === "cancelled").length,
        totalSpeedBps: values.reduce((sum, t) => sum + (t.status === "active" ? t.speedBps : 0), 0),
      } as T;
    }
    case "transfer_action": {
      const id = args.taskId as string;
      const action = args.action as TransferAction;
      const task = transfers.get(id);
      if (!task) throw { code: "NOT_FOUND", message: "Transfer not found" };
      if (action === "pause") task.status = "paused";
      if (action === "resume") task.status = "active";
      if (action === "cancel") {
        task.status = "cancelled";
        task.finishedAt = Date.now();
        stopTimer(id);
      }
      if (action === "retry") {
        task.transferred = 0;
        task.status = "queued";
        task.finishedAt = undefined;
        startTransfer(task);
      }
      if (action === "remove") {
        transfers.delete(id);
        stopTimer(id);
        emitMockEvent("transfer:removed", id);
        return undefined as T;
      }
      emitTransfer({ ...task });
      return undefined as T;
    }
    case "transfer_clear": {
      const removed = [...transfers.values()].filter((t) => ["completed", "failed", "cancelled"].includes(t.status)).map((t) => t.id);
      for (const id of removed) transfers.delete(id);
      emitMockEvent("transfers:cleared", removed);
      return undefined as T;
    }
    case "logs_history":
      return [] as T;
    case "logs_clear":
      return undefined as T;
    case "known_hosts_list":
      return [
        {
          host: "demo.filezilla.dev:22",
          keyType: "ssh-ed25519",
          fingerprint: "SHA256:8fJ2kQ0mYc9V5u1s3T7pR4wX6nB8dL0aC2eG4hK6mN8",
          addedAt: Date.now() - 86400000 * 4,
        },
      ] as T;
    case "known_host_forget":
      return undefined as T;
    case "log_client":
      return undefined as T;
    case "apply_window_theme":
      return undefined as T;
    default:
      throw { code: "UNKNOWN_COMMAND", message: `Unknown command: ${command}` };
  }
}
