import { useEffect, useState } from "react";
import { AlertCircle, CheckCircle2, Info, TriangleAlert, X } from "lucide-react";
import { useT } from "../i18n";
import { useAppStore } from "../store/app";
import { Button, IconButton } from "./ui/controls";
import { Modal } from "./ui/Modal";
import { cn } from "../lib/cn";

export function Dialogs() {
  const t = useT();
  const prompt = useAppStore((state) => state.ui.prompt);
  const confirm = useAppStore((state) => state.ui.confirm);
  const closeDialogs = useAppStore((state) => state.closeDialogs);
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (prompt) setValue(prompt.value);
  }, [prompt]);

  const submit = async () => {
    if (!prompt || busy) return;
    setBusy(true);
    try {
      await prompt.onSubmit(value);
      closeDialogs();
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Modal
        open={Boolean(prompt)}
        onClose={closeDialogs}
        title={prompt?.title ?? ""}
        size="small"
        footer={
          <>
            <div className="spacer" />
            <Button variant="subtle" onClick={closeDialogs}>
              {t("dialog.cancel")}
            </Button>
            <Button variant="primary" onClick={() => void submit()} disabled={busy || !value.trim()}>
              {prompt?.confirmLabel ?? t("dialog.confirm")}
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-2">
          <label className="field-label">{prompt?.label}</label>
          <input
            className="input"
            autoFocus
            value={value}
            placeholder={prompt?.placeholder}
            onChange={(event) => setValue(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void submit();
            }}
          />
          {prompt?.hint && <span className="field-hint">{prompt.hint}</span>}
        </div>
      </Modal>

      <Modal
        open={Boolean(confirm)}
        onClose={closeDialogs}
        title={confirm?.title ?? ""}
        size="small"
        footer={
          <>
            <div className="spacer" />
            <Button variant="subtle" onClick={closeDialogs}>
              {t("dialog.cancel")}
            </Button>
            <Button
              variant={confirm?.danger ? "danger" : "primary"}
              onClick={async () => {
                if (!confirm) return;
                setBusy(true);
                try {
                  await confirm.onConfirm();
                  closeDialogs();
                } finally {
                  setBusy(false);
                }
              }}
              disabled={busy}
            >
              {confirm?.confirmLabel ?? t("dialog.confirm")}
            </Button>
          </>
        }
      >
        <div className="flex gap-3">
          <AlertCircle
            size={20}
            className={confirm?.danger ? "text-[var(--danger)]" : "text-[var(--accent)]"}
          />
          <p className="m-0 text-[12px] leading-relaxed text-[var(--text-secondary)]">
            {confirm?.message}
          </p>
        </div>
      </Modal>
    </>
  );
}

export function ToastStack() {
  const toasts = useAppStore((state) => state.toasts);
  const dismiss = useAppStore((state) => state.dismissToast);
  return (
    <div className="toast-stack">
      {toasts.map((toast) => {
        const Icon =
          toast.kind === "success"
            ? CheckCircle2
            : toast.kind === "error"
              ? AlertCircle
              : toast.kind === "warning"
                ? TriangleAlert
                : Info;
        return (
          <div key={toast.id} className={cn("toast", toast.kind)}>
            <Icon className="toast-icon" size={18} />
            <div className="toast-body">
              {toast.title && <div className="toast-title">{toast.title}</div>}
              <div className="toast-message">{toast.message}</div>
            </div>
            <IconButton label="Dismiss" className="toast-close" onClick={() => dismiss(toast.id)}>
              <X size={12} />
            </IconButton>
          </div>
        );
      })}
    </div>
  );
}
