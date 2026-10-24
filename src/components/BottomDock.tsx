import { ListTodo, ScrollText, X } from "lucide-react";
import { useT } from "../i18n";
import { cn } from "../lib/cn";
import { useAppStore } from "../store/app";
import { IconButton } from "./ui/controls";
import { LogPanel } from "./LogPanel";
import { TransferPanel } from "./TransferPanel";

export function BottomDock() {
  const t = useT();
  const queueOpen = useAppStore((state) => state.ui.queueOpen);
  const logOpen = useAppStore((state) => state.ui.logOpen);
  const bottomTab = useAppStore((state) => state.ui.bottomTab);
  const setBottomTab = useAppStore((state) => state.setBottomTab);
  const toggleUi = useAppStore((state) => state.toggleUi);
  const transfers = useAppStore((state) => state.transfers);
  const logs = useAppStore((state) => state.logs);

  if (!queueOpen && !logOpen) return null;
  const active = queueOpen && bottomTab === "queue" ? "queue" : logOpen ? "log" : "queue";

  return (
    <section className="bottom-dock">
      <div className="bottom-tabs">
        {queueOpen && (
          <button
            className={cn("bottom-tab", active === "queue" && "active")}
            onClick={() => setBottomTab("queue")}
          >
            <ListTodo size={14} />
            <span>{t("transfer.title")}</span>
            <span className="bottom-tab-count">{transfers.length}</span>
          </button>
        )}
        {logOpen && (
          <button
            className={cn("bottom-tab", active === "log" && "active")}
            onClick={() => setBottomTab("log")}
          >
            <ScrollText size={14} />
            <span>{t("log.title")}</span>
            <span className="bottom-tab-count">{logs.length}</span>
          </button>
        )}
        <div className="bottom-tabs-spacer" />
        <IconButton
          label={t("common.close")}
          onClick={() => toggleUi(active === "queue" ? "queueOpen" : "logOpen")}
        >
          <X size={14} />
        </IconButton>
      </div>
      <div className="bottom-content">
        <div className={cn("bottom-page", active !== "queue" && "hidden")}>
          <TransferPanel />
        </div>
        <div className={cn("bottom-page", active !== "log" && "hidden")}>
          <LogPanel />
        </div>
      </div>
    </section>
  );
}
