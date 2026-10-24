import { useEffect, useState, type ReactNode } from "react";
import {
  Cpu,
  Info,
  Languages,
  Monitor,
  Moon,
  Palette,
  Settings,
  Shield,
  SlidersHorizontal,
  Sun,
  Trash2,
  FolderOpen,
} from "lucide-react";
import { openPath } from "@tauri-apps/plugin-opener";
import { isTauri } from "../api/backend";
import { useT } from "../i18n";
import { languageDisplayName, supportedLanguages } from "../i18n/translate";
import { cn } from "../lib/cn";
import { useAppStore } from "../store/app";
import type { ThemeMode } from "../types";
import { Button, IconButton, Switch } from "./ui/controls";
import { Modal } from "./ui/Modal";
import { Logo } from "./Logo";

type Section = "appearance" | "transfers" | "behavior" | "security" | "about";

const ACCENTS = [
  { value: "blue", key: "settings.accentBlue" },
  { value: "teal", key: "settings.accentTeal" },
  { value: "purple", key: "settings.accentPurple" },
  { value: "orange", key: "settings.accentOrange" },
  { value: "pink", key: "settings.accentPink" },
  { value: "green", key: "settings.accentGreen" },
] as const;

export function SettingsDialog() {
  const t = useT();
  const open = useAppStore((state) => state.ui.settingsOpen);
  const closeSettings = useAppStore((state) => state.closeSettings);
  const settings = useAppStore((state) => state.settings);
  const updateSettings = useAppStore((state) => state.updateSettings);
  const setLanguage = useAppStore((state) => state.setLanguage);
  const info = useAppStore((state) => state.info);
  const knownHosts = useAppStore((state) => state.knownHosts);
  const policy = useAppStore((state) => state.policy);
  const forgetHost = useAppStore((state) => state.forgetHost);
  const loadKnownHosts = useAppStore((state) => state.loadKnownHosts);
  const [section, setSection] = useState<Section>("appearance");

  useEffect(() => {
    if (open) void loadKnownHosts();
  }, [open, loadKnownHosts]);

  return (
    <Modal
      open={open}
      onClose={closeSettings}
      title={t("settings.title")}
      icon={<Settings size={16} className="text-[var(--accent)]" />}
      size="large"
      bodyClassName="flush"
      footer={
        <>
          <span className="text-[10.5px] text-[var(--text-faint)]">FileZilla Modern {info?.version ?? "0.1.0"}</span>
          <div className="spacer" />
          <Button variant="primary" onClick={closeSettings}>
            {t("dialog.close")}
          </Button>
        </>
      }
    >
      <div className="settings-layout">
        <nav className="settings-nav">
          <NavItem active={section === "appearance"} icon={<Palette size={15} />} onClick={() => setSection("appearance")}>
            {t("settings.appearance")}
          </NavItem>
          <NavItem active={section === "transfers"} icon={<SlidersHorizontal size={15} />} onClick={() => setSection("transfers")}>
            {t("settings.transfers")}
          </NavItem>
          <NavItem active={section === "behavior"} icon={<Monitor size={15} />} onClick={() => setSection("behavior")}>
            {t("settings.behavior")}
          </NavItem>
          <NavItem active={section === "security"} icon={<Shield size={15} />} onClick={() => setSection("security")}>
            {t("settings.security")}
          </NavItem>
          <NavItem active={section === "about"} icon={<Info size={15} />} onClick={() => setSection("about")}>
            {t("settings.about")}
          </NavItem>
        </nav>

        <div className="settings-content">
          {section === "appearance" && (
            <>
              <div className="settings-section">
                <h3>{t("settings.theme")}</h3>
                <p>{t("settings.themeHint")}</p>
                <div className="theme-cards">
                  <ThemeCard mode="system" current={settings.theme} onSelect={(theme) => updateSettings({ theme })} />
                  <ThemeCard mode="light" current={settings.theme} onSelect={(theme) => updateSettings({ theme })} />
                  <ThemeCard mode="dark" current={settings.theme} onSelect={(theme) => updateSettings({ theme })} />
                </div>
              </div>

              <div className="settings-section">
                <h3>{t("settings.accent")}</h3>
                <p>{t("settings.accentHint")}</p>
                <div className="accent-swatches">
                  {ACCENTS.map(({ value, key }) => (
                    <button
                      key={value}
                      className={cn("accent-swatch", value, settings.accent === value && "active")}
                      title={t(key)}
                      aria-label={t(key)}
                      onClick={() => updateSettings({ accent: value })}
                    />
                  ))}
                </div>
              </div>

              <div className="settings-section">
                <h3>{t("settings.language")}</h3>
                <SettingRow
                  icon={<Languages size={15} />}
                  title={t("settings.language")}
                  desc={t("settings.languageHint")}
                >
                  <select
                    className="select w-[220px]"
                    value={settings.language}
                    onChange={(event) => void setLanguage(event.target.value)}
                  >
                    <option value="zh-CN">简体中文</option>
                    <option value="en-US">English</option>
                    {supportedLanguages.map((language) => (
                      <option key={language.code} value={language.code}>
                        {languageDisplayName(language.code, language.code)}
                      </option>
                    ))}
                  </select>
                </SettingRow>
                <SettingRow title={t("settings.density")} desc={t("settings.densityHint")}>
                  <div className="segmented">
                    <button
                      className={cn(settings.density === "comfortable" && "active")}
                      onClick={() => updateSettings({ density: "comfortable" })}
                    >
                      {t("settings.comfortable")}
                    </button>
                    <button
                      className={cn(settings.density === "compact" && "active")}
                      onClick={() => updateSettings({ density: "compact" })}
                    >
                      {t("settings.compact")}
                    </button>
                  </div>
                </SettingRow>
                <SettingRow title={t("settings.animations")} desc={t("settings.animationsHint")}>
                  <Switch checked={settings.animations} onChange={(value) => updateSettings({ animations: value })} label={t("settings.animations")} />
                </SettingRow>
                <SettingRow title={t("settings.mica")} desc={t("settings.micaHint")}>
                  <Switch checked={settings.micaEffect} onChange={(value) => updateSettings({ micaEffect: value })} label={t("settings.mica")} />
                </SettingRow>
                <SettingRow title={t("settings.toolbarLabels")} desc={t("settings.toolbarLabelsDesc")}>
                  <Switch
                    checked={settings.toolbarLabels}
                    onChange={(value) => updateSettings({ toolbarLabels: value })}
                    label={t("settings.toolbarLabels")}
                  />
                </SettingRow>
              </div>
            </>
          )}

          {section === "transfers" && (
            <div className="settings-section">
              <h3>{t("settings.transfers")}</h3>
              <p>{t("settings.transfersHint")}</p>
              <SettingRow title={t("settings.concurrency")} desc={t("settings.concurrencyHint")}>
                <input
                  className="input number-input"
                  type="number"
                  min={1}
                  max={10}
                  value={settings.concurrentTransfers}
                  onChange={(event) => updateSettings({ concurrentTransfers: Number(event.target.value) })}
                />
              </SettingRow>
              <SettingRow title={t("settings.downloadLimit")} desc={t("settings.limitHint")}>
                <input
                  className="input number-input"
                  type="number"
                  min={0}
                  step={64}
                  value={settings.downloadLimitKbps}
                  onChange={(event) => updateSettings({ downloadLimitKbps: Math.max(0, Number(event.target.value)) })}
                />
              </SettingRow>
              <SettingRow title={t("settings.uploadLimit")} desc={t("settings.limitHint")}>
                <input
                  className="input number-input"
                  type="number"
                  min={0}
                  step={64}
                  value={settings.uploadLimitKbps}
                  onChange={(event) => updateSettings({ uploadLimitKbps: Math.max(0, Number(event.target.value)) })}
                />
              </SettingRow>
              <SettingRow title={t("settings.resume")} desc={t("settings.resumeHint")}>
                <Switch checked={settings.resumeTransfers} onChange={(value) => updateSettings({ resumeTransfers: value })} label={t("settings.resume")} />
              </SettingRow>
              <SettingRow title={t("settings.notifyComplete")} desc={t("settings.notifyCompleteHint")}>
                <Switch checked={settings.notifyOnComplete} onChange={(value) => updateSettings({ notifyOnComplete: value })} label={t("settings.notifyComplete")} />
              </SettingRow>
              <SettingRow title={t("settings.soundComplete")} desc={t("settings.soundCompleteHint")}>
                <Switch checked={settings.soundOnComplete} onChange={(value) => updateSettings({ soundOnComplete: value })} label={t("settings.soundComplete")} />
              </SettingRow>              <SettingRow title={t("settings.timestamps")} desc={t("settings.timestampsHint")}>
                <Switch checked={settings.preserveTimestamps} onChange={(value) => updateSettings({ preserveTimestamps: value })} label={t("settings.timestamps")} />
              </SettingRow>
            </div>
          )}

          {section === "behavior" && (
            <div className="settings-section">
              <h3>{t("settings.behavior")}</h3>
              <p>{t("settings.behaviorHint")}</p>
              <SettingRow title={t("settings.minimizeToTray")} desc={t("settings.minimizeToTrayHint")}>
                <Switch checked={settings.minimizeToTray} onChange={(value) => updateSettings({ minimizeToTray: value })} label={t("settings.minimizeToTray")} />
              </SettingRow>              <SettingRow title={t("settings.showHidden")} desc={t("settings.showHiddenHint")}>
                <Switch checked={settings.showHiddenFiles} onChange={(value) => updateSettings({ showHiddenFiles: value })} label={t("settings.showHidden")} />
              </SettingRow>
              <SettingRow title={t("settings.confirmDelete")} desc={t("settings.confirmDeleteHint")}>
                <Switch checked={settings.confirmDelete} onChange={(value) => updateSettings({ confirmDelete: value })} label={t("settings.confirmDelete")} />
              </SettingRow>
              <SettingRow title={t("settings.doubleClick")} desc={t("settings.doubleClickHint")}>
                <select
                  className="select w-[180px]"
                  value={settings.doubleClickAction}
                  onChange={(event) =>
                    updateSettings({ doubleClickAction: event.target.value as "transfer" | "open" })
                  }
                >
                  <option value="transfer">{t("settings.doubleClickTransfer")}</option>
                  <option value="open">{t("settings.doubleClickOpen")}</option>
                </select>
              </SettingRow>
              <SettingRow title={t("settings.logLevel")} desc={t("settings.logLevelHint")}>
                <select
                  className="select w-[140px]"
                  value={settings.logLevel}
                  onChange={(event) => updateSettings({ logLevel: event.target.value })}
                >
                  <option value="debug">{t("settings.logDebug")}</option>
                  <option value="info">{t("settings.logInfo")}</option>
                  <option value="warn">{t("settings.logWarn")}</option>
                  <option value="error">{t("settings.logError")}</option>
                </select>
              </SettingRow>
            </div>
          )}

          {section === "security" && (
            <>
              {policy?.managed && (
                <div className="settings-section">
                  <h3>{t("settings.managed")}</h3>
                  <p>{t("settings.managedHint")}</p>
                  <table className="kv-table">
                    <tbody>
                      <tr>
                        <td>{t("settings.managedSource")}</td>
                        <td className="mono">{policy.loadedFrom ?? "—"}</td>
                      </tr>
                      <tr>
                        <td>{t("settings.managedKiosk")}</td>
                        <td>
                          {policy.kioskMode === 0
                            ? "0"
                            : `${policy.kioskMode} · ${
                                policy.kioskMode === 1 ? t("settings.kioskPasswords") : t("settings.kioskReadOnly")
                              }`}
                        </td>
                      </tr>
                      <tr>
                        <td>{t("settings.managedReadOnly")}</td>
                        <td>{policy.readOnly ? "✓" : "—"}</td>
                      </tr>
                      <tr>
                        <td>{t("settings.managedNoUpdate")}</td>
                        <td>{policy.disableUpdateCheck ? "✓" : "—"}</td>
                      </tr>
                    </tbody>
                  </table>
                  {policy.warning && <div className="file-error mt-2">{policy.warning}</div>}
                </div>
              )}
              <div className="settings-section">
                <h3>{t("settings.knownHosts")}</h3>
                <p>{t("settings.knownHostsHint")}</p>
                {knownHosts.length === 0 ? (
                  <div className="rounded-md border border-[var(--border)] px-3 py-4 text-center text-[11px] text-[var(--text-faint)]">
                    {t("settings.noKnownHosts")}
                  </div>
                ) : (
                  <div className="rounded-md border border-[var(--border)] px-3">
                    {knownHosts.map((host) => (
                      <div className="host-row" key={`${host.host}-${host.keyType}`}>
                        <Shield size={15} className="text-[var(--success)]" />
                        <div className="host-meta">
                          <div className="host-name">
                            {host.host} · {host.keyType}
                          </div>
                          <div className="host-fingerprint">{host.fingerprint}</div>
                        </div>
                        <IconButton label={t("settings.forget")} onClick={() => void forgetHost(host.host)}>
                          <Trash2 />
                        </IconButton>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className="settings-section">
                <h3>{t("settings.configDir")}</h3>
                <p>{t("settings.configDirHint")}</p>
                <div className="flex items-center gap-2">
                  <code className="min-w-0 flex-1 truncate rounded border border-[var(--border)] bg-[var(--code-bg)] px-2 py-1.5 font-mono text-[10.5px] text-[var(--text-secondary)]">
                    {info?.configDir ?? "—"}
                  </code>
                  <Button
                    onClick={() => {
                      if (isTauri() && info?.configDir) void openPath(info.configDir);
                    }}
                  >
                    <FolderOpen size={14} />
                    {t("settings.openConfigDir")}
                  </Button>
                </div>
              </div>
            </>
          )}

          {section === "about" && (
            <div className="settings-section">
              <div className="about-card">
                <Logo size={54} className="about-logo" />
                <div>
                  <div className="about-name">FileZilla Modern</div>
                  <div className="about-desc">{t("settings.aboutText")}</div>
                </div>
              </div>
              <table className="kv-table mt-4">
                <tbody>
                  <tr>
                    <td>{t("settings.version")}</td>
                    <td>{info?.version ?? "0.1.0"}</td>
                  </tr>
                  <tr>
                    <td>{t("settings.tauriVersion")}</td>
                    <td>{info?.tauriVersion ?? "2.x"}</td>
                  </tr>
                  <tr>
                    <td>{t("settings.os")}</td>
                    <td>
                      {info ? `${info.os} · ${info.arch}` : "—"}
                    </td>
                  </tr>
                  <tr>
                    <td>{t("settings.configDir")}</td>
                    <td className="mono">{info?.configDir ?? "—"}</td>
                  </tr>
                  <tr>
                    <td>{t("settings.license")}</td>
                    <td>GPL-3.0-or-later</td>
                  </tr>
                </tbody>
              </table>
              <div className="mt-4 flex items-center gap-2 text-[10.5px] text-[var(--text-faint)]">
                <Cpu size={12} />
                Rust · Tauri 2 · React 19 · TypeScript
              </div>
            </div>
          )}
        </div>
      </div>
    </Modal>
  );
}

function NavItem({
  active,
  icon,
  onClick,
  children,
}: {
  active: boolean;
  icon: ReactNode;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button className={cn("settings-nav-item", active && "active")} onClick={onClick}>
      {icon}
      {children}
    </button>
  );
}

function SettingRow({
  icon,
  title,
  desc,
  children,
}: {
  icon?: ReactNode;
  title: string;
  desc?: string;
  children: ReactNode;
}) {
  return (
    <div className="setting-row">
      {icon && <span className="text-[var(--text-muted)]">{icon}</span>}
      <div className="setting-info">
        <div className="setting-title">{title}</div>
        {desc && <div className="setting-desc">{desc}</div>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}

function ThemeCard({
  mode,
  current,
  onSelect,
}: {
  mode: ThemeMode;
  current: ThemeMode;
  onSelect: (mode: ThemeMode) => void;
}) {
  const t = useT();
  const label =
    mode === "system" ? t("settings.themeSystem") : mode === "light" ? t("settings.themeLight") : t("settings.themeDark");
  const Icon = mode === "system" ? Monitor : mode === "light" ? Sun : Moon;
  const dark = mode === "dark";
  const light = mode === "light";
  const split = "linear-gradient(90deg,#f5f6f8 50%,#1b1d21 50%)";
  const paneBackground = (side: "left" | "right") =>
    dark ? "#23262c" : light ? "#fff" : side === "left" ? "#fff" : "#23262c";
  return (
    <button className={cn("theme-card", current === mode && "active")} onClick={() => onSelect(mode)}>
      <div
        className="theme-preview"
        style={{
          background: dark ? "#1b1d21" : light ? "#f5f6f8" : split,
        }}
      >
        <div
          className="preview-bar"
          style={{ background: dark ? "#2b2f36" : light ? "#e7e9ed" : "linear-gradient(90deg,#e7e9ed 50%,#2b2f36 50%)" }}
        />
        <div className="preview-body">
          <div className="preview-pane" style={{ background: paneBackground("left") }} />
          <div className="preview-pane" style={{ background: paneBackground("right") }} />
        </div>
      </div>
      <div className="theme-label flex items-center justify-center gap-1.5">
        <Icon size={12} />
        {label}
      </div>
    </button>
  );
}
