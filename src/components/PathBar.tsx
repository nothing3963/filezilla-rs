import { useEffect, useState } from "react";
import { ArrowLeft, ArrowRight, ArrowUp, ChevronRight, Pencil, RefreshCw } from "lucide-react";
import { IconButton } from "./ui/controls";
import { useT } from "../i18n";
import { cn } from "../lib/cn";

export function PathBar({
  path,
  side,
  canBack,
  canForward,
  onBack,
  onForward,
  onUp,
  onNavigate,
  onRefresh,
}: {
  path: string | null;
  side: "local" | "remote";
  canBack: boolean;
  canForward: boolean;
  onBack: () => void;
  onForward: () => void;
  onUp: () => void;
  onNavigate: (path: string) => void;
  onRefresh: () => void;
}) {
  const t = useT();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(path ?? "");

  useEffect(() => {
    if (!editing) setDraft(path ?? "");
  }, [path, editing]);

  const segments = buildSegments(path, side);

  const submit = () => {
    const value = draft.trim();
    setEditing(false);
    if (value && value !== path) onNavigate(value);
  };

  return (
    <div className="pathbar">
      <div className="pathbar-nav">
        <IconButton label={t("common.back")} disabled={!canBack} onClick={onBack}>
          <ArrowLeft />
        </IconButton>
        <IconButton label={t("common.forward")} disabled={!canForward} onClick={onForward}>
          <ArrowRight />
        </IconButton>
        <IconButton label={t("common.up")} disabled={!path} onClick={onUp}>
          <ArrowUp />
        </IconButton>
      </div>

      {editing ? (
        <input
          className="path-input"
          autoFocus
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={submit}
          onKeyDown={(event) => {
            if (event.key === "Enter") submit();
            if (event.key === "Escape") {
              setEditing(false);
              setDraft(path ?? "");
            }
          }}
        />
      ) : (
        <div
          className="breadcrumbs"
          onDoubleClick={() => {
            setDraft(path ?? "");
            setEditing(true);
          }}
          title={t("common.editPath")}
        >
          {segments.map((segment, index) => (
            <span key={`${segment.path}-${index}`} className="flex items-center gap-1">
              {index > 0 && <ChevronRight className="crumb-separator" size={11} />}
              <button
                className={cn("crumb", index === segments.length - 1 && "current")}
                onClick={() => onNavigate(segment.path)}
              >
                {segment.label}
              </button>
            </span>
          ))}
        </div>
      )}

      <IconButton
        label={t("common.editPath")}
        onClick={() => {
          setDraft(path ?? "");
          setEditing(true);
        }}
      >
        <Pencil />
      </IconButton>
      <IconButton label={t("common.refresh")} onClick={onRefresh}>
        <RefreshCw />
      </IconButton>
    </div>
  );
}

function buildSegments(path: string | null, side: "local" | "remote") {
  if (side === "local") {
    if (!path) return [{ label: "This PC", path: "" }];
    const parts = path.split(/[\\/]+/).filter(Boolean);
    const segments: { label: string; path: string }[] = [];
    let current = "";
    for (const part of parts) {
      current = current ? `${current}\\${part}` : `${part}\\`;
      segments.push({ label: part, path: current.replace(/\\$/, "") });
    }
    return segments.length ? segments : [{ label: "This PC", path: "" }];
  }
  const normalized = path ?? "/";
  const parts = normalized.split("/").filter(Boolean);
  const segments: { label: string; path: string }[] = [{ label: "/", path: "/" }];
  let current = "";
  for (const part of parts) {
    current += `/${part}`;
    segments.push({ label: part, path: current });
  }
  return segments;
}
