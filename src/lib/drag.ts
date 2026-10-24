export const DRAG_MIME = "application/x-filezilla-items";

export interface DragPayload {
  side: "local" | "remote";
  items: { name: string; path: string; isDir: boolean; size: number }[];
}

export function parseDragPayload(dataTransfer: DataTransfer): DragPayload | null {
  const raw = dataTransfer.getData(DRAG_MIME);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as DragPayload;
  } catch {
    return null;
  }
}
