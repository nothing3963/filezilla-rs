import { useState } from "react";
import { Eye, EyeOff, Loader2, Zap } from "lucide-react";
import { useT } from "../i18n";
import { useAppStore } from "../store/app";
import { protocolPort } from "../lib/format";
import type { Protocol as ProtocolType } from "../types";

const PROTOCOLS: { value: ProtocolType; label: string }[] = [
  { value: "sftp", label: "SFTP" },
  { value: "ftp", label: "FTP" },
  { value: "ftps", label: "FTPS" },
  { value: "ftpsi", label: "FTPS (implicit)" },
];

export function QuickConnect() {
  const t = useT();
  const connectQuick = useAppStore((state) => state.connectQuick);
  const connecting = useAppStore((state) => state.connecting);
  const toggleUi = useAppStore((state) => state.toggleUi);

  const [protocol, setProtocol] = useState<ProtocolType>("sftp");
  const [host, setHost] = useState("");
  const [port, setPort] = useState(22);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);

  const submit = async () => {
    if (!host.trim()) return;
    await connectQuick({ protocol, host, port, username, password });
    toggleUi("quickConnectOpen");
  };

  return (
    <div className="quick-connect">
      <select
        className="select w-[132px] flex-none"
        value={protocol}
        aria-label={t("quick.protocol")}
        onChange={(event) => {
          const next = event.target.value as ProtocolType;
          setProtocol(next);
          setPort(protocolPort(next));
        }}
      >
        {PROTOCOLS.map((item) => (
          <option key={item.value} value={item.value}>
            {item.label}
          </option>
        ))}
      </select>
      <input
        className="input quick-host"
        placeholder={t("quick.host")}
        value={host}
        autoFocus
        onChange={(event) => setHost(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && void submit()}
      />
      <input
        className="input quick-port"
        type="number"
        min={1}
        max={65535}
        aria-label={t("quick.port")}
        value={port}
        onChange={(event) => setPort(Number(event.target.value))}
      />
      <input
        className="input quick-user"
        placeholder={t("quick.username")}
        value={username}
        onChange={(event) => setUsername(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && void submit()}
      />
      <div className="relative flex flex-[1.2] items-center">
        <input
          className="input w-full pr-8"
          placeholder={t("quick.password")}
          type={showPassword ? "text" : "password"}
          value={password}
          onChange={(event) => setPassword(event.target.value)}
          onKeyDown={(event) => event.key === "Enter" && void submit()}
        />
        <button
          className="absolute right-1 grid h-6 w-6 place-items-center rounded text-[var(--text-muted)] hover:bg-[var(--hover)]"
          type="button"
          onClick={() => setShowPassword((value) => !value)}
          aria-label="Toggle password visibility"
        >
          {showPassword ? <EyeOff size={13} /> : <Eye size={13} />}
        </button>
      </div>
      <button className="btn primary" onClick={() => void submit()} disabled={connecting || !host.trim()}>
        {connecting ? <Loader2 size={14} className="spin" /> : <Zap size={14} />}
        {t("quick.connect")}
      </button>
    </div>
  );
}
