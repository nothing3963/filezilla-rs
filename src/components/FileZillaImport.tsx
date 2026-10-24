import { useEffect, useRef, useState } from "react";
import { FolderInput } from "lucide-react";
import { api, isTauri } from "../api/backend";
import { useT } from "../i18n";
import { useAppStore } from "../store/app";
import { Button } from "./ui/controls";
import { Modal } from "./ui/Modal";

/** On first launch, offers to import sites from an existing FileZilla install. */
export function FileZillaImport() {
  const t = useT();
  const ready = useAppStore((state) => state.ready);
  const prompted = useAppStore((state) => state.settings.filezillaImportPrompted);
  const updateSettings = useAppStore((state) => state.updateSettings);
  const refreshSites = useAppStore((state) => state.refreshSites);
  const toast = useAppStore((state) => state.toast);
  const [source, setSource] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const checked = useRef(false);

  useEffect(() => {
    if (!ready || prompted || checked.current || !isTauri()) return;
    checked.current = true;
    api
      .sitesDetectAuto()
      .then((detected) => {
        if (detected) setSource(detected.source);
      })
      .catch(() => {
        // Detection is best-effort and must never block startup.
      });
  }, [ready, prompted]);

  const dismiss = () => {
    updateSettings({ filezillaImportPrompted: true });
    setSource(null);
  };

  const importSites = async () => {
    if (busy) return;
    setBusy(true);
    updateSettings({ filezillaImportPrompted: true });
    try {
      const outcome = await api.sitesImportAuto();
      await refreshSites();
      if (outcome) {
        toast("success", t("siteManager.importDone", { count: outcome.imported }));
        if (outcome.skipped > 0) {
          toast("warning", t("siteManager.importSkipped", { count: outcome.skipped }));
        }
      }
    } catch (error) {
      toast("error", String((error as { message?: string }).message ?? error));
    } finally {
      setBusy(false);
      setSource(null);
    }
  };

  return (
    <Modal
      open={source !== null}
      onClose={() => {
        if (!busy) dismiss();
      }}
      title={t("importPrompt.title")}
      subtitle={t("importPrompt.subtitle")}
      icon={<FolderInput size={16} />}
      size="small"
      footer={
        <>
          <Button variant="subtle" onClick={dismiss} disabled={busy}>
            {t("importPrompt.later")}
          </Button>
          <Button variant="primary" onClick={() => void importSites()} disabled={busy}>
            {t("importPrompt.import")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <p className="m-0 text-[12px] leading-relaxed text-[var(--text-secondary)]">
          {t("importPrompt.message")}
        </p>
        <code className="max-h-24 overflow-auto break-all rounded-md bg-[var(--surface-3)] px-2 py-1.5 text-[11px]">
          {source}
        </code>
        <span className="field-hint">{t("importPrompt.hint")}</span>
      </div>
    </Modal>
  );
}
