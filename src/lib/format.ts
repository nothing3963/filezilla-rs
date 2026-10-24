import type { Protocol, TransferStatus } from "../types";

export function formatBytes(bytes: number, decimals = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const index = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  const value = bytes / Math.pow(1024, index);
  if (index === 0) return `${Math.round(value)} B`;
  return `${value.toFixed(value >= 100 ? 0 : value >= 10 ? 1 : decimals)} ${units[index]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  if (!bytesPerSecond) return "—";
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatDate(iso?: string): string {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  const now = new Date();
  const sameDay = date.toDateString() === now.toDateString();
  return new Intl.DateTimeFormat(undefined, {
    year: sameDay ? undefined : "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function formatClock(timestamp: number): string {
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(timestamp));
}

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "—";
  if (seconds < 60) return `${Math.ceil(seconds)}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = Math.ceil(seconds % 60);
  if (minutes < 60) return `${minutes}m ${rest}s`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

export function etaFor(transferred: number, total: number, speed: number): number {
  if (!speed || !total) return 0;
  return Math.max(0, (total - transferred) / speed);
}

export function percent(transferred: number, total: number): number {
  if (!total) return 0;
  return Math.max(0, Math.min(100, (transferred / total) * 100));
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

export function naturalCompare(a: string, b: string): number {
  return collator.compare(a, b);
}

export function posixName(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  return trimmed.slice(trimmed.lastIndexOf("/") + 1) || "/";
}

export function posixParent(path: string): string {
  const normalized = path.replace(/\/+$/, "");
  if (!normalized || normalized === "/") return "/";
  const index = normalized.lastIndexOf("/");
  if (index <= 0) return "/";
  return normalized.slice(0, index);
}

export function joinRemote(base: string, name: string): string {
  if (!base || base === "/") return `/${name}`;
  return `${base.replace(/\/+$/, "")}/${name}`;
}

export function joinLocal(base: string, name: string): string {
  if (!base) return name;
  const separator = base.includes("\\") || /^[A-Za-z]:/.test(base) ? "\\" : "/";
  return `${base.replace(/[\\/]+$/, "")}${separator}${name}`;
}

export function basename(path: string): string {
  const normalized = path.replace(/[\\/]+$/, "");
  const index = Math.max(normalized.lastIndexOf("/"), normalized.lastIndexOf("\\"));
  return index >= 0 ? normalized.slice(index + 1) : normalized;
}

export function dirname(path: string): string {
  const normalized = path.replace(/[\\/]+$/, "");
  const index = Math.max(normalized.lastIndexOf("/"), normalized.lastIndexOf("\\"));
  if (index <= 0) return normalized.slice(0, index + 1);
  return normalized.slice(0, index);
}

export function protocolLabel(protocol: Protocol): string {
  switch (protocol) {
    case "ftp":
      return "FTP";
    case "ftps":
      return "FTPS";
    case "ftpsi":
      return "FTPS";
    case "sftp":
      return "SFTP";
  }
}

export function protocolPort(protocol: Protocol): number {
  switch (protocol) {
    case "ftp":
    case "ftps":
      return 21;
    case "ftpsi":
      return 990;
    case "sftp":
      return 22;
  }
}

export function protocolUsesTls(protocol: Protocol): boolean {
  return protocol === "ftps" || protocol === "ftpsi";
}

export function statusLabelKey(status: TransferStatus): string {
  switch (status) {
    case "queued":
      return "transfer.queued";
    case "connecting":
      return "transfer.connecting";
    case "active":
      return "transfer.activeStatus";
    case "paused":
      return "transfer.paused";
    case "completed":
      return "transfer.completedStatus";
    case "failed":
      return "transfer.failed";
    case "cancelled":
      return "transfer.cancelled";
  }
}

export function fileCategory(name: string, isDir: boolean): string {
  if (isDir) return "folder";
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  if (["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"].includes(extension)) return "image";
  if (["mp4", "mkv", "avi", "mov", "webm"].includes(extension)) return "video";
  if (["mp3", "flac", "wav", "ogg", "m4a"].includes(extension)) return "audio";
  if (["zip", "rar", "7z", "gz", "tar", "xz", "bz2"].includes(extension)) return "archive";
  if (["rs", "ts", "tsx", "js", "jsx", "py", "go", "c", "cpp", "h", "json", "toml", "yaml", "yml", "html", "css", "sh"].includes(extension))
    return "code";
  if (["txt", "md", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "csv"].includes(extension)) return "document";
  return "file";
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
