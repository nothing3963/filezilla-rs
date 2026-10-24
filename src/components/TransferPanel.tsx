import { useMemo, useState, type MouseEvent } from "react";
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  CheckCircle2,
  ChevronDown,
  FolderOpen,
  ListTodo,
  Pause,
  Play,
  RotateCcw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { isTauri } from "../api/backend";
import { useT } from "../i18n";
import {
  etaFor,
  formatBytes,
  formatDuration,
  formatSpeed,
  percent,
  statusLabelKey,
} from "../lib/format";
import { cn } from "../lib/cn";
import { useAppStore } from "../store/app";
import type { TransferTask } from "../types";
import { IconButton } from "./ui/controls";
import { ContextMenu } from "./ui/ContextMenu";

type Filter = "all" | "running" | "finished";

export function TransferPanel() {
  const t = useT();
  const transfers = useAppStore((state) => state.transfers);
  const transferAction = useAppStore((state) => state.transferAction);
  const locateTransfer = useAppStore((state) => state.locateTransfer);
  const clearTransfers = useAppStore((state) => state.clearTransfers);
  const tabs = useAppStore((state) => state.tabs);
  const toggleUi = useAppStore((state) => state.toggleUi);
  const [filter, setFilter] = useState<Filter>("all");
  const [menu, setMenu] = useState<{ x: number; y: number; taskId: string } | null>(null);

  const sorted = useMemo(
    () => [...transfers].sort((a, b) => a.createdAt - b.createdAt),
    [transfers],
  );
  const visible = sorted.filter((task) => {
    if (filter === "running") return !isTerminal(task);
    if (filter === "finished") return isTerminal(task);
    return true;
  });
  const active = sorted.filter((task) => !isTerminal(task));
  const speed = active.reduce((sum, task) => sum + (task.status === "active" ? task.speedBps : 0), 0);

  return (
    <section className="transfer-panel">
      <div className="transfer-header">
        <div className="transfer-title">
          <ListTodo size={14} />
          {t("transfer.title")}
          <span className="transfer-count">{sorted.length}</span>
        </div>
        <div className="segmented">
          <button className={cn(filter === "all" && "active")} onClick={() => setFilter("all")}>
            {t("transfer.all")}
          </button>
          <button className={cn(filter === "running" && "active")} onClick={() => setFilter("running")}>
            {t("transfer.running")}
          </button>
          <button className={cn(filter === "finished" && "active")} onClick={() => setFilter("finished")}>
            {t("transfer.finished")}
          </button>
        </div>
        {speed > 0 && (
          <span className="text-[10.5px] tabular-nums text-[var(--text-muted)]">
            {t("transfer.overall", { active: active.length, speed: formatSpeed(speed) })}
          </span>
        )}
        <div className="ml-auto flex items-center gap-2">
          <button
            className="btn subtle sm"
            onClick={() => void clearTransfers()}
            disabled={!sorted.some(isTerminal)}
          >
            <Trash2 size={12} />
            {t("transfer.clear")}
          </button>
          <IconButton label={t("common.close")} onClick={() => toggleUi("queueOpen")}>
            <ChevronDown />
          </IconButton>
        </div>
      </div>

      <div className="transfer-list">
        {visible.length === 0 ? (
          <div className="file-empty">
            <ListTodo size={34} />
            <h3>{t("transfer.empty")}</h3>
            <p>{t("transfer.emptyHint")}</p>
          </div>
        ) : (
          visible.map((task) => (
            <TransferRow
              key={task.id}
              task={task}
              onAction={(action) => void transferAction(task.id, action)}
              onContextMenu={(event) =>
                setMenu({ x: event.clientX, y: event.clientY, taskId: task.id })
              }
            />
          ))
        )}
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={[
            {
              label: t("transfer.locateLocal"),
              icon: <FolderOpen size={14} />,
              onClick: () => void locateTransfer(menu.taskId, "local"),
            },
            {
              label: t("transfer.locateRemote"),
              icon: <Search size={14} />,
              disabled: tabs.length === 0,
              onClick: () => void locateTransfer(menu.taskId, "remote"),
            },
            {
              label: t("transfer.clear"),
              icon: <Trash2 size={14} />,
              disabled: !sorted.some(isTerminal),
              onClick: () => void clearTransfers(),
            },
          ]}
          onClose={() => setMenu(null)}
        />
      )}
    </section>
  );
}

function TransferRow({
  task,
  onAction,
  onContextMenu,
}: {
  task: TransferTask;
  onAction: (action: "pause" | "resume" | "cancel" | "retry" | "remove") => void;
  onContextMenu: (event: MouseEvent) => void;
}) {
  const t = useT();
  const progress = percent(task.transferred, task.size);
  const terminal = isTerminal(task);
  const statusKey = statusLabelKey(task.status);
  const isDownload = task.direction === "download";
  const meta =
    task.status === "active" && task.speedBps > 0
      ? `${formatBytes(task.transferred)} / ${formatBytes(task.size)} · ${formatSpeed(task.speedBps)}`
      : `${formatBytes(task.transferred)} / ${formatBytes(task.size)}`;

  const reveal = () => {
    if (!isTauri() || task.status !== "completed") return;
    void revealItemInDir(task.localPath).catch(() => undefined);
  };

  return (
    <div
      className="transfer-row"
      onDoubleClick={() => task.status === "completed" && isDownload && reveal()}
      onContextMenu={(event) => {
        event.preventDefault();
        onContextMenu(event);
      }}
      title={task.error ?? task.remotePath}
    >
      <div className={cn("transfer-direction", task.direction)}>
        {isDownload ? <ArrowDown size={14} /> : <ArrowUp size={14} />}
      </div>
      <div className="transfer-main">
        <div className="transfer-name-row">
          <span className="transfer-name">{task.name}</span>
          <StatusChip status={task.status} label={t(statusKey)} />
        </div>
        <div className="transfer-path">
          {isDownload ? `${task.remotePath} → ${task.localPath}` : `${task.localPath} → ${task.remotePath}`}
        </div>
        <div className={cn("transfer-progress", task.status === "completed" && "completed", task.status === "failed" && "failed")}>
          <span style={{ width: `${task.status === "completed" ? 100 : progress}%` }} />
        </div>
      </div>
      <div className="transfer-meta">
        <span>{meta}</span>
        <span className="transfer-speed">
          {task.status === "active" && task.speedBps > 0
            ? t("transfer.eta", { time: formatDuration(etaFor(task.transferred, task.size, task.speedBps)) })
            : task.error
              ? task.error
              : ""}
        </span>
      </div>
      <div className="transfer-actions">
        {!terminal && (task.status === "active" || task.status === "connecting") && (
          <IconButton label={t("transfer.pause")} onClick={() => onAction("pause")}>
            <Pause />
          </IconButton>
        )}
        {task.status === "paused" && (
          <IconButton label={t("transfer.resume")} onClick={() => onAction("resume")}>
            <Play />
          </IconButton>
        )}
        {!terminal && (
          <IconButton label={t("transfer.cancel")} onClick={() => onAction("cancel")}>
            <X />
          </IconButton>
        )}
        {terminal && (
          <>
            <IconButton label={t("transfer.retry")} onClick={() => onAction("retry")}>
              <RotateCcw />
            </IconButton>
            <IconButton label={t("transfer.remove")} onClick={() => onAction("remove")}>
              <Trash2 />
            </IconButton>
          </>
        )}
      </div>
    </div>
  );
}

function StatusChip({ status, label }: { status: TransferTask["status"]; label: string }) {
  const icon =
    status === "completed" ? (
      <CheckCircle2 size={10} />
    ) : status === "failed" || status === "cancelled" ? (
      <AlertCircle size={10} />
    ) : null;
  return (
    <span className={cn("transfer-status", status)}>
      {icon}
      {label}
    </span>
  );
}

function isTerminal(task: TransferTask): boolean {
  return task.status === "completed" || task.status === "failed" || task.status === "cancelled";
}
