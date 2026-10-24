import { useEffect, useMemo, useRef, useState, type DragEvent, type KeyboardEvent, type MouseEvent, type PointerEvent as ReactPointerEvent } from "react";
import { createPortal } from "react-dom";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
import type { DragPayload } from "../lib/drag";
import { DRAG_MIME } from "../lib/drag";
import { formatBytes, formatDate } from "../lib/format";
import type { PaneSort, SortKey } from "../types";
import { useAppStore } from "../store/app";
import { useT } from "../i18n";
import { cn } from "../lib/cn";
import { FileIcon } from "./FileIcon";

export interface FileRowData {
  key: string;
  name: string;
  path: string;
  isDir: boolean;
  isSymlink?: boolean;
  size: number;
  modified?: string;
  permissions?: string;
  hidden?: boolean;
  extension?: string;
  compare?: "missing" | "different" | "same";
}

export function FileTable({
  side,
  rows,
  selected,
  sort,
  onSortChange,
  onSelect,
  onOpen,
  onNavigate,
  onContextMenu,
  onRowDrop,
}: {
  side: "local" | "remote";
  rows: FileRowData[];
  selected: string[];
  sort: PaneSort;
  onSortChange: (key: SortKey) => void;
  onSelect: (keys: string[]) => void;
  onOpen: (row: FileRowData) => void;
  onNavigate: (row: FileRowData) => void;
  onContextMenu: (event: MouseEvent, row: FileRowData) => void;
  onRowDrop?: (row: FileRowData, payload: DragPayload) => void;
}) {
  const t = useT();
  const density = useAppStore((state) => state.settings.density);
  const parentRef = useRef<HTMLDivElement | null>(null);
  const selectedSet = useMemo(() => new Set(selected), [selected]);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const anchorRef = useRef<string | null>(null);
  const focusRef = useRef<string | null>(null);
  const typeaheadRef = useRef({ buffer: "", at: 0 });
  const typeaheadTimer = useRef<number | null>(null);
  const [typeahead, setTypeahead] = useState("");
  const handleDrop = useAppStore((state) => state.handleDrop);
  const sortedRef = useRef<FileRowData[]>([]);
  const selectedSetRef = useRef<Set<string>>(new Set());
  const onSelectRef = useRef(onSelect);
  const candidateRef = useRef<{
    x: number;
    y: number;
    row: FileRowData;
    active: boolean;
  } | null>(null);
  const dragPayloadRef = useRef<DragPayload | null>(null);
  const dragLabelRef = useRef("");
  const dropTargetRef = useRef<HTMLElement | null>(null);
  const [ghost, setGhost] = useState<{ x: number; y: number; label: string } | null>(null);
  const rowSignature = useMemo(() => rows.map((row) => row.key).join("\u0000"), [rows]);

  onSelectRef.current = onSelect;

  const sorted = useMemo(() => {
    return [...rows].sort((a, b) => {
      if (sort.key === "name" && a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      let result = 0;
      switch (sort.key) {
        case "size":
          result = a.size - b.size;
          break;
        case "modified":
          result = new Date(a.modified ?? 0).getTime() - new Date(b.modified ?? 0).getTime();
          break;
        case "permissions":
          result = (a.permissions ?? "").localeCompare(b.permissions ?? "");
          break;
        default:
          result = a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
      }
      return result * sort.dir;
    });
  }, [rows, sort]);

  useEffect(() => {
    if (!sorted.some((row) => row.key === anchorRef.current)) anchorRef.current = null;
    if (!sorted.some((row) => row.key === focusRef.current)) focusRef.current = null;
  }, [sorted]);

  const virtualizer = useVirtualizer({
    count: sorted.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => (density === "compact" ? 26 : 30),
    overscan: 14,
  });

  // Keep the selected file in view after a refresh or an external "locate"
  // request (which restores the selection through the store).
  useEffect(() => {
    if (selected.length === 0) return;
    const index = sorted.findIndex((row) => selectedSet.has(row.key));
    if (index < 0) return;
    const rowHeight = density === "compact" ? 26 : 30;
    const viewport = parentRef.current;
    if (viewport) {
      const top = index * rowHeight;
      const bottom = top + rowHeight;
      if (top < viewport.scrollTop || bottom > viewport.scrollTop + viewport.clientHeight) {
        virtualizer.scrollToIndex(index, { align: "auto" });
      }
    }
    anchorRef.current = sorted[index].key;
    focusRef.current = sorted[index].key;
  }, [selected, selectedSet, sorted, density, virtualizer]);

  const columns = side === "remote"
    ? "minmax(200px, 1fr) 92px 138px 104px"
    : "minmax(200px, 1fr) 92px 148px";

  const toggleSort = (key: SortKey) => {
    onSortChange(key);
  };

  useEffect(() => {
    typeaheadRef.current = { buffer: "", at: 0 };
    setTypeahead("");
  }, [rowSignature]);

  sortedRef.current = sorted;
  selectedSetRef.current = selectedSet;

  useEffect(() => {
    const clearDrag = () => {
      candidateRef.current = null;
      dragPayloadRef.current = null;
      dragLabelRef.current = "";
      dropTargetRef.current?.classList.remove("internal-drop");
      dropTargetRef.current = null;
      document.body.classList.remove("internal-dragging");
      setGhost(null);
    };

    const targetAt = (x: number, y: number) => {
      const element = document.elementFromPoint(x, y) as HTMLElement | null;
      const pane = element?.closest("[data-pane-side]") as HTMLElement | null;
      if (!pane) return null;
      const targetSide = pane.dataset.paneSide as "local" | "remote" | undefined;
      const dirRow = element?.closest("[data-file-dir='true']") as HTMLElement | null;
      const targetDir = dirRow?.dataset.filePath ?? pane.dataset.panePath ?? "";
      return { pane, side: targetSide, dir: targetDir };
    };

    const move = (event: PointerEvent) => {
      const candidate = candidateRef.current;
      if (!candidate) return;
      if (!candidate.active) {
        if (Math.hypot(event.clientX - candidate.x, event.clientY - candidate.y) < 6) {
          return;
        }
        candidate.active = true;
        const selected = selectedSetRef.current;
        const active = selected.has(candidate.row.key)
          ? sortedRef.current.filter((row) => selected.has(row.key))
          : [candidate.row];
        if (!selected.has(candidate.row.key)) {
          onSelectRef.current([candidate.row.key]);
        }
        dragPayloadRef.current = {
          side,
          items: active.map((row) => ({
            name: row.name,
            path: row.path,
            isDir: row.isDir,
            size: row.size,
          })),
        };
        dragLabelRef.current =
          active.length === 1 ? active[0].name : t("pane.items", { count: active.length });
        document.body.classList.add("internal-dragging");
      }
      event.preventDefault();
      setGhost({
        x: event.clientX + 12,
        y: event.clientY + 12,
        label: dragLabelRef.current,
      });
      const target = targetAt(event.clientX, event.clientY);
      if (dropTargetRef.current !== target?.pane) {
        dropTargetRef.current?.classList.remove("internal-drop");
        dropTargetRef.current = null;
        if (target && target.side && target.side !== side && target.dir) {
          target.pane.classList.add("internal-drop");
          dropTargetRef.current = target.pane;
        }
      }
    };

    const up = (event: PointerEvent) => {
      const candidate = candidateRef.current;
      const payload = dragPayloadRef.current;
      if (candidate?.active && payload) {
        const target = targetAt(event.clientX, event.clientY);
        if (target && target.side && target.side !== side && target.dir) {
          void handleDrop(payload, target.side, target.dir);
        }
      }
      clearDrag();
    };

    const cancel = () => clearDrag();
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      clearDrag();
    };
  }, [handleDrop, side]);

  const startPointerDrag = (event: ReactPointerEvent<HTMLDivElement>, row: FileRowData) => {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    if (target.closest("button, input, textarea, a")) return;
    focusRef.current = row.key;
    candidateRef.current = {
      x: event.clientX,
      y: event.clientY,
      row,
      active: false,
    };
  };

  useEffect(() => {
    return () => {
      if (typeaheadTimer.current !== null) window.clearTimeout(typeaheadTimer.current);
    };
  }, []);

  const showTypeahead = (value: string) => {
    setTypeahead(value);
    if (typeaheadTimer.current !== null) window.clearTimeout(typeaheadTimer.current);
    typeaheadTimer.current = window.setTimeout(() => setTypeahead(""), 900);
  };

  const quickLocate = (buffer: string) => {
    if (!buffer) return;
    const needle = buffer.toLowerCase();
    const index = sorted.findIndex((row) => row.name.toLowerCase().startsWith(needle));
    if (index < 0) return;
    const row = sorted[index];
    anchorRef.current = row.key;
    focusRef.current = row.key;
    onSelect([row.key]);
    virtualizer.scrollToIndex(index, { align: "auto" });
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (sorted.length === 0) return;
      const currentIndex = focusRef.current
        ? sorted.findIndex((item) => item.key === focusRef.current)
        : selected.length > 0
          ? sorted.findIndex((item) => item.key === selected[0])
          : -1;
      const delta = event.key === "ArrowDown" ? 1 : -1;
      const nextIndex =
        currentIndex < 0
          ? delta > 0
            ? 0
            : sorted.length - 1
          : Math.min(sorted.length - 1, Math.max(0, currentIndex + delta));
      const target = sorted[nextIndex];
      if (event.shiftKey) {
        const anchorIndex = anchorRef.current
          ? sorted.findIndex((item) => item.key === anchorRef.current)
          : currentIndex;
        const base = anchorIndex < 0 ? nextIndex : anchorIndex;
        const from = Math.min(base, nextIndex);
        const to = Math.max(base, nextIndex);
        focusRef.current = target.key;
        onSelect(sorted.slice(from, to + 1).map((item) => item.key));
      } else {
        anchorRef.current = target.key;
        focusRef.current = target.key;
        onSelect([target.key]);
      }
      virtualizer.scrollToIndex(nextIndex, { align: "auto" });
      return;
    }    if (event.key === "Enter") {
      if (selected.length === 1) {
        const row = sorted.find((item) => item.key === selected[0]);
        if (row) {
          event.preventDefault();
          if (row.isDir) onNavigate(row);
          else onOpen(row);
        }
      }
      return;
    }    if (event.key === "Escape") {
      typeaheadRef.current = { buffer: "", at: 0 };
      setTypeahead("");
      return;
    }
    if (event.key === "Backspace") {
      const next = typeaheadRef.current.buffer.slice(0, -1);
      typeaheadRef.current = { buffer: next, at: Date.now() };
      showTypeahead(next);
      quickLocate(next);
      event.preventDefault();
      return;
    }
    if (event.key.length !== 1 || event.key === " ") return;
    event.preventDefault();
    const now = Date.now();
    const current = typeaheadRef.current;
    const buffer = now - current.at > 900 ? event.key : current.buffer + event.key;
    typeaheadRef.current = { buffer, at: now };
    showTypeahead(buffer);
    quickLocate(buffer);
  };

  const handleClick = (event: MouseEvent, row: FileRowData) => {
    parentRef.current?.focus({ preventScroll: true });
    focusRef.current = row.key;
    const index = sorted.findIndex((item) => item.key === row.key);
    if (event.shiftKey && anchorRef.current) {
      const anchor = sorted.findIndex((item) => item.key === anchorRef.current);
      if (anchor >= 0) {
        const [from, to] = anchor < index ? [anchor, index] : [index, anchor];
        onSelect(sorted.slice(from, to + 1).map((item) => item.key));
        return;
      }
    }
    if (event.ctrlKey || event.metaKey) {
      const next = new Set(selectedSet);
      if (next.has(row.key)) next.delete(row.key);
      else next.add(row.key);
      anchorRef.current = row.key;
      onSelect([...next]);
      return;
    }
    anchorRef.current = row.key;
    onSelect([row.key]);
  };

  const handleDragStart = (event: DragEvent, row: FileRowData) => {
    const activeRows = selectedSet.has(row.key)
      ? sorted.filter((item) => selectedSet.has(item.key))
      : [row];
    if (!selectedSet.has(row.key)) {
      anchorRef.current = row.key;
      onSelect([row.key]);
    }
    const payload: DragPayload = {
      side,
      items: activeRows.map((item) => ({
        name: item.name,
        path: item.path,
        isDir: item.isDir,
        size: item.size,
      })),
    };
    event.dataTransfer.setData(DRAG_MIME, JSON.stringify(payload));
    event.dataTransfer.effectAllowed = "copy";
  };

  const handleRowDrop = (event: DragEvent, row: FileRowData) => {
    if (!row.isDir || !onRowDrop) return;
    const raw = event.dataTransfer.getData(DRAG_MIME);
    if (!raw) return;
    event.preventDefault();
    event.stopPropagation();
    setDropTarget(null);
    try {
      onRowDrop(row, JSON.parse(raw) as DragPayload);
    } catch {
      // ignore malformed drag data
    }
  };

  return (
    <>
    <div className="file-table">
      <div className="file-head" style={{ gridTemplateColumns: columns }}>
        <div className={cn("file-head-cell sortable")} onClick={() => toggleSort("name")}>
          {t("columns.name")}
          {sort.key === "name" && <SortArrow dir={sort.dir} />}
        </div>
        <div className="file-head-cell numeric sortable" onClick={() => toggleSort("size")}>
          {t("columns.size")}
          {sort.key === "size" && <SortArrow dir={sort.dir} />}
        </div>
        <div className="file-head-cell sortable" onClick={() => toggleSort("modified")}>
          {t("columns.modified")}
          {sort.key === "modified" && <SortArrow dir={sort.dir} />}
        </div>
        {side === "remote" && (
          <div className="file-head-cell sortable" onClick={() => toggleSort("permissions")}>
            {t("columns.permissions")}
            {sort.key === "permissions" && <SortArrow dir={sort.dir} />}
          </div>
        )}
      </div>

      <div
        className="file-scroll"
        ref={parentRef}
        tabIndex={0}
        onKeyDown={handleKeyDown}
        aria-label={t("pane.quickLocate")}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const row = sorted[virtualRow.index];
            const isSelected = selectedSet.has(row.key);
            return (
              <div
                key={row.key}
                className={cn(
                  "file-row",
                  isSelected && "selected",
                  row.compare && `compare-${row.compare}`,
                  dropTarget === row.key && row.isDir && "drop-target",
                )}
                style={{
                  transform: `translateY(${virtualRow.start}px)`,
                  gridTemplateColumns: columns,
                  height: virtualRow.size,
                }}
                draggable={false}
                data-file-key={row.key}
                data-file-path={row.path}
                data-file-dir={row.isDir ? "true" : "false"}
                onPointerDown={(event) => startPointerDrag(event, row)}
                onClick={(event) => handleClick(event, row)}
                onDoubleClick={() => (row.isDir ? onNavigate(row) : onOpen(row))}
                onContextMenu={(event) => {
                  event.preventDefault();
                  if (!isSelected) {
                    anchorRef.current = row.key;
                    onSelect([row.key]);
                  }
                  onContextMenu(event, row);
                }}
                onDragStart={(event) => handleDragStart(event, row)}
                onDragOver={(event) => {
                  if (!row.isDir) return;
                  if (!event.dataTransfer.types.includes(DRAG_MIME)) return;
                  event.preventDefault();
                  event.stopPropagation();
                  setDropTarget(row.key);
                }}
                onDragLeave={() => setDropTarget((current) => (current === row.key ? null : current))}
                onDrop={(event) => handleRowDrop(event, row)}
                title={row.name}
              >
                <div className="file-cell file-name">
                  <FileIcon name={row.name} isDir={row.isDir} isSymlink={row.isSymlink} size={15} />
                  <span className={cn("file-name-text", row.isDir && "dir")}>{row.name}</span>
                  {row.isSymlink && <span className="file-badge">link</span>}
                </div>
                <div className="file-cell numeric">{row.isDir ? "—" : formatBytes(row.size)}</div>
                <div className="file-cell dim">{formatDate(row.modified)}</div>
                {side === "remote" && (
                  <div className="file-cell dim mono text-[10.5px]">{row.permissions ?? "—"}</div>
                )}
              </div>
            );
          })}
        </div>
        {typeahead && <div className="file-typeahead">{typeahead}</div>}
        {sorted.length === 0 && (
          <div className="file-empty">
            <div className="text-[12px]">{t("pane.empty")}</div>
          </div>
        )}
        {sorted.length > 0 && <div onDragOver={(e) => e.preventDefault()} style={{ height: 20 }} />}
      </div>
    </div>
    {ghost &&
      createPortal(
        <div className="internal-drag-ghost" style={{ left: ghost.x, top: ghost.y }}>
          {ghost.label}
        </div>,
        document.body,
      )}
    </>
  );
}

function SortArrow({ dir }: { dir: 1 | -1 }) {
  return dir === 1 ? <ArrowUp size={10} className="sort-indicator" /> : <ArrowDown size={10} className="sort-indicator" />;
}
