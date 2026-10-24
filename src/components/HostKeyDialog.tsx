import { KeyRound, ShieldAlert } from "lucide-react";
import { api } from "../api/backend";
import { useT } from "../i18n";
import { useAppStore } from "../store/app";
import { Button } from "./ui/controls";
import { Modal } from "./ui/Modal";

export function HostKeyDialog() {
  const t = useT();
  const prompt = useAppStore((state) => state.hostkeyPrompt);
  const setPrompt = useAppStore((state) => state.setHostkeyPrompt);
  const toast = useAppStore((state) => state.toast);

  const decide = async (accepted: boolean) => {
    if (!prompt) return;
    setPrompt(null);
    try {
      await api.sshHostkeyResolve(prompt.id, accepted);
      if (accepted) {
        toast("success", t("hostkey.trusted", { host: prompt.host }));
      }
    } catch (error) {
      toast("error", String((error as { message?: string }).message ?? error));
    }
  };

  return (
    <Modal
      open={Boolean(prompt)}
      onClose={() => void decide(false)}
      title={t("hostkey.title")}
      subtitle={t("hostkey.subtitle")}
      icon={<ShieldAlert size={16} />}
      size="small"
      footer={
        <>
          <Button variant="subtle" onClick={() => void decide(false)}>
            {t("hostkey.reject")}
          </Button>
          <Button variant="primary" onClick={() => void decide(true)}>
            {t("hostkey.accept")}
          </Button>
        </>
      }
    >
      {prompt && (
        <div className="hostkey-dialog">
          <div className="hostkey-host">
            <KeyRound size={15} />
            <strong>
              {prompt.host}:{prompt.port}
            </strong>
          </div>
          <div className="hostkey-row">
            <span>{t("hostkey.type")}</span>
            <code>{prompt.keyType}</code>
          </div>
          <div className="hostkey-row">
            <span>{t("hostkey.fingerprint")}</span>
            <code className="hostkey-fingerprint">{prompt.fingerprint}</code>
          </div>
          <p className="hostkey-warning">{t("hostkey.warning")}</p>
        </div>
      )}
    </Modal>
  );
}
