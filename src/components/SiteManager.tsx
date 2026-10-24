import { useEffect, useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Copy,
  Eye,
  EyeOff,
  FileDown,
  FileUp,
  Folder,
  FolderOpen,
  FolderPlus,
  KeyRound,
  Lock,
  Plug,
  Plus,
  Save,
  Server,
  Trash2,
} from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, isTauri } from "../api/backend";
import { useT } from "../i18n";
import { protocolLabel, protocolPort } from "../lib/format";
import { cn } from "../lib/cn";
import { defaultConfig, useAppStore } from "../store/app";
import type { Protocol, Site, SiteConfig, SiteInput } from "../types";
import { Button, Checkbox } from "./ui/controls";
import { ContextMenu, type MenuItem } from "./ui/ContextMenu";
import { Modal } from "./ui/Modal";

type Selection = { type: "site"; id: string } | { type: "group"; id: string } | { type: "new" } | null;

type Draft = SiteInput & { password?: string; keyPassphrase?: string; clearPassword?: boolean };

const ENCODINGS = [
  { value: "UTF-8", label: "UTF-8" },
  { value: "GBK", label: "GBK (简体中文)" },
  { value: "GB18030", label: "GB18030" },
  { value: "Big5", label: "Big5 (繁體中文)" },
  { value: "Shift_JIS", label: "Shift_JIS (日本語)" },
  { value: "EUC-KR", label: "EUC-KR (한국어)" },
  { value: "Windows-1252", label: "Windows-1252" },
  { value: "ISO-8859-1", label: "ISO-8859-1" },
];

function toDraft(site: Site): Draft {
  return {
    id: site.id,
    name: site.name,
    groupId: site.groupId ?? null,
    config: structuredClone(site.config),
    password: "",
    keyPassphrase: "",
    clearPassword: false,
  };
}

export function SiteManager() {
  const t = useT();
  const siteManagerOpen = useAppStore((state) => state.ui.siteManagerOpen);
  const sites = useAppStore((state) => state.sites);
  const closeSiteManager = useAppStore((state) => state.closeSiteManager);
  const saveSite = useAppStore((state) => state.saveSite);
  const deleteSite = useAppStore((state) => state.deleteSite);
  const duplicateSite = useAppStore((state) => state.duplicateSite);
  const createGroup = useAppStore((state) => state.createGroup);
  const deleteGroup = useAppStore((state) => state.deleteGroup);
  const connectSite = useAppStore((state) => state.connectSite);
  const refreshSites = useAppStore((state) => state.refreshSites);
  const prompt = useAppStore((state) => state.prompt);
  const confirm = useAppStore((state) => state.confirm);
  const toast = useAppStore((state) => state.toast);
  const policy = useAppStore((state) => state.policy);

  const [selection, setSelection] = useState<Selection>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [dirty, setDirty] = useState(false);
  const [tab, setTab] = useState<"general" | "advanced">("general");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [showPassword, setShowPassword] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; kind: "site" | "group" | "root"; id?: string } | null>(null);

  const selectedSite = useMemo(
    () => (draft?.id ? sites.sites.find((site) => site.id === draft.id) ?? null : null),
    [draft?.id, sites.sites],
  );

  useEffect(() => {
    if (!siteManagerOpen) return;
    setExpanded((current) => {
      const next = new Set(current);
      for (const group of sites.groups) next.add(group.id);
      return next;
    });
    if (!selection && !draft && sites.sites.length > 0) {
      const first = sites.sites[0];
      setSelection({ type: "site", id: first.id });
      setDraft(toDraft(first));
      setDirty(false);
    }
    // Only react to opening the dialog and the first load.
  }, [siteManagerOpen]);

  useEffect(() => {
    if (selection?.type === "site" && !sites.sites.some((site) => site.id === selection.id)) {
      setSelection(null);
      setDraft(null);
    }
  }, [selection, sites.sites]);

  if (!siteManagerOpen) return null;

  const selectSite = (site: Site) => {
    setSelection({ type: "site", id: site.id });
    setDraft(toDraft(site));
    setDirty(false);
    setTab("general");
  };

  const selectGroup = (id: string) => {
    setSelection({ type: "group", id });
    setDraft(null);
    setDirty(false);
  };

  const newSite = (groupId?: string | null) => {
    const folder =
      groupId ?? (selection?.type === "group" ? selection.id : null);
    setDraft({
      name: "",
      groupId: folder,
      config: defaultConfig("sftp"),
      password: "",
      keyPassphrase: "",
      clearPassword: false,
    });
    setSelection({ type: "new" });
    setDirty(false);
    setTab("general");
  };

  const patch = (updater: (current: Draft) => Draft) => {
    setDraft((current) => (current ? updater(current) : current));
    setDirty(true);
  };

  const patchConfig = (value: Partial<SiteConfig>) => {
    patch((current) => ({ ...current, config: { ...current.config, ...value } }));
  };

  const toggleGroup = (id: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const validate = (value: Draft): boolean => {
    if (!value.name.trim()) {
      toast("warning", t("toast.siteNameRequired"));
      return false;
    }
    if (!value.config.host.trim()) {
      toast("warning", t("toast.hostRequired"));
      return false;
    }
    if (value.config.port < 1 || value.config.port > 65535) {
      toast("warning", t("toast.invalidPort"));
      return false;
    }
    return true;
  };

  const ensureSaved = async (): Promise<Site | null> => {
    if (!draft) return selectedSite;
    if (!validate(draft)) return null;
    const saved = await saveSite({
      ...draft,
      password: draft.password?.trim() ? draft.password : undefined,
      keyPassphrase: draft.keyPassphrase?.trim() ? draft.keyPassphrase : undefined,
      proxyPassword: draft.proxyPassword?.trim() ? draft.proxyPassword : undefined,
    });
    if (saved) {
      setSelection({ type: "site", id: saved.id });
      setDraft(toDraft(saved));
      setDirty(false);
    }
    return saved;
  };

  const handleClose = () => {
    if (dirty) {
      confirm({
        title: t("siteManager.unsaved"),
        message: t("siteManager.unsaved"),
        confirmLabel: t("dialog.close"),
        onConfirm: () => closeSiteManager(),
      });
      return;
    }
    closeSiteManager();
  };

  const handleConnect = async () => {
    const site = await ensureSaved();
    if (!site) return;
    closeSiteManager();
    await connectSite(site);
  };

  const handleDeleteSite = (site: Site) => {
    confirm({
      title: t("dialog.deleteTitle"),
      message: t("siteManager.deleteConfirm", { name: site.name }),
      confirmLabel: t("dialog.delete"),
      danger: true,
      onConfirm: async () => {
        const removed = await deleteSite(site.id);
        if (removed) {
          setSelection(null);
          setDraft(null);
          setDirty(false);
        }
      },
    });
  };

  const handleDeleteGroup = (id: string, name: string) => {
    confirm({
      title: t("dialog.deleteTitle"),
      message: t("siteManager.deleteGroupConfirm", { name }),
      confirmLabel: t("dialog.delete"),
      danger: true,
      onConfirm: async () => {
        await deleteGroup(id);
        setSelection(null);
        setDraft(null);
      },
    });
  };

  const newGroup = (parentId?: string | null) => {
    prompt({
      title: t("siteManager.newGroup"),
      label: t("siteManager.name"),
      value: "",
      confirmLabel: t("dialog.create"),
      onSubmit: async (value) => {
        if (!value.trim()) return;
        const group = await createGroup(value.trim(), parentId ?? null);
        if (group) {
          setExpanded((current) => new Set(current).add(group.id));
          setSelection({ type: "group", id: group.id });
          setDraft(null);
        }
      },
    });
  };

  const renameGroup = (id: string, name: string) => {
    prompt({
      title: t("siteManager.newGroup"),
      label: t("siteManager.name"),
      value: name,
      confirmLabel: t("dialog.rename"),
      onSubmit: async (value) => {
        if (!value.trim()) return;
        await useAppStore.getState().renameGroup(id, value.trim());
      },
    });
  };

  const handleDuplicate = async () => {
    if (!draft?.id) return;
    const copy = await duplicateSite(draft.id);
    if (copy) {
      selectSite(copy);
      toast("success", t("toast.saved"));
    }
  };

  const importSites = async () => {
    if (!isTauri()) {
      toast("info", "XML import is available in the desktop app");
      return;
    }
    try {
      const detected = await api.sitesImportAuto();
      if (detected) {
        await refreshSites();
        toast("success", t("siteManager.importDone", { count: detected.imported }));
        if (detected.skipped > 0) {
          toast("warning", t("siteManager.importSkipped", { count: detected.skipped }));
        }
        return;
      }
    } catch (error) {
      toast("error", String((error as { message?: string }).message ?? error));
      return;
    }
    toast("info", t("siteManager.importNotFound"));
    const path = await openDialog({
      multiple: false,
      directory: false,
      title: t("siteManager.import"),
      filters: [{ name: "FileZilla Site Manager", extensions: ["xml"] }],
    });
    if (typeof path !== "string") return;
    try {
      const result = await api.sitesImport(path);
      await refreshSites();
      toast("success", t("siteManager.importDone", { count: result.imported }));
      if (result.skipped > 0) {
        toast("warning", t("siteManager.importSkipped", { count: result.skipped }));
      }
    } catch (error) {
      toast("error", String((error as { message?: string }).message ?? error));
    }
  };

  const exportSites = async () => {
    if (!isTauri()) {
      toast("info", "XML export is available in the desktop app");
      return;
    }
    const path = await saveDialog({
      defaultPath: "sitemanager.xml",
      filters: [{ name: "FileZilla Site Manager", extensions: ["xml"] }],
    });
    if (!path) return;
    try {
      const result = await api.sitesExport(path, false);
      toast("success", t("siteManager.exportDone", { count: result.sites }));
    } catch (error) {
      toast("error", String((error as { message?: string }).message ?? error));
    }
  };
  const browseKey = async () => {
    if (!isTauri()) {
      toast("info", "File picker is available in the desktop app");
      return;
    }
    const path = await openDialog({
      multiple: false,
      directory: false,
      title: t("siteManager.keyFile"),
      filters: [{ name: "Private keys", extensions: ["pem", "key", "rsa", "ed25519", "openssh"] }],
    });
    if (typeof path === "string") patchConfig({ privateKeyPath: path });
  };

  const groupsByParent = (parentId: string | null) =>
    sites.groups.filter((group) => (group.parentId ?? null) === parentId);
  const sitesInGroup = (groupId: string | null) =>
    sites.sites.filter((site) => (site.groupId ?? null) === groupId);

  const groupOptions = flattenGroups(sites.groups);

  const renderGroup = (groupId: string, depth: number) => {
    const group = sites.groups.find((item) => item.id === groupId);
    if (!group) return null;
    const children = groupsByParent(group.id);
    const childSites = sitesInGroup(group.id);
    const isOpen = expanded.has(group.id);
    return (
      <div key={group.id}>
        <div
          className={cn("tree-group", selection?.type === "group" && selection.id === group.id && "selected")}
          style={{ paddingLeft: 6 + depth * 14 }}
          onClick={() => selectGroup(group.id)}
          onContextMenu={(event) => {
            event.preventDefault();
            setMenu({ x: event.clientX, y: event.clientY, kind: "group", id: group.id });
          }}
        >
          <button
            className="grid h-4 w-4 flex-none place-items-center rounded hover:bg-[var(--surface-3)]"
            onClick={(event) => {
              event.stopPropagation();
              toggleGroup(group.id);
            }}
          >
            {isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          </button>
          {isOpen ? <FolderOpen size={13} /> : <Folder size={13} />}
          <span className="flex-1 truncate">{group.name}</span>
          <span className="text-[10px] text-[var(--text-faint)]">{childSites.length}</span>
        </div>
        {isOpen && (
          <>
            {children.map((child) => renderGroup(child.id, depth + 1))}
            {childSites.map((site) => renderSite(site, depth + 1))}
          </>
        )}
      </div>
    );
  };

  const renderSite = (site: Site, depth: number) => (
    <div
      key={site.id}
      className={cn("tree-site", selection?.type === "site" && selection.id === site.id && "selected")}
      style={{ paddingLeft: 12 + depth * 14 }}
      onClick={() => selectSite(site)}
      onDoubleClick={() => {
        void connectSite(site).then(() => closeSiteManager());
      }}
      onContextMenu={(event) => {
        event.preventDefault();
        setMenu({ x: event.clientX, y: event.clientY, kind: "site", id: site.id });
      }}
    >
      <Server size={14} />
      <span className="tree-site-name">{site.name}</span>
      <span className="tree-site-proto">{protocolLabel(site.config.protocol)}</span>
    </div>
  );

  const menuItems: MenuItem[] = menu
    ? menu.kind === "site"
      ? [
          {
            label: t("siteManager.connect"),
            icon: <Plug size={14} />,
            onClick: async () => {
              const site = sites.sites.find((item) => item.id === menu.id);
              if (site) {
                closeSiteManager();
                await connectSite(site);
              }
            },
          },
          {
            label: t("siteManager.duplicate"),
            icon: <Copy size={14} />,
            onClick: async () => {
              if (!menu.id) return;
              const copy = await duplicateSite(menu.id);
              if (copy) selectSite(copy);
            },
          },
          { separator: true, label: "" },
          {
            label: t("siteManager.delete"),
            icon: <Trash2 size={14} />,
            danger: true,
            onClick: () => {
              const site = sites.sites.find((item) => item.id === menu.id);
              if (site) handleDeleteSite(site);
            },
          },
        ]
      : menu.kind === "group"
        ? [
            {
              label: t("siteManager.newSite"),
              icon: <Plus size={14} />,
              onClick: () => newSite(menu.id ?? null),
            },
            {
              label: t("siteManager.newGroup"),
              icon: <FolderPlus size={14} />,
              onClick: () => newGroup(menu.id ?? null),
            },
            { separator: true, label: "" },
            {
              label: t("dialog.rename"),
              onClick: () => {
                const group = sites.groups.find((item) => item.id === menu.id);
                if (group) renameGroup(group.id, group.name);
              },
            },
            {
              label: t("siteManager.delete"),
              icon: <Trash2 size={14} />,
              danger: true,
              onClick: () => {
                const group = sites.groups.find((item) => item.id === menu.id);
                if (group) handleDeleteGroup(group.id, group.name);
              },
            },
          ]
        : [
            {
              label: t("siteManager.newSite"),
              icon: <Plus size={14} />,
              onClick: () => newSite(null),
            },
            {
              label: t("siteManager.newGroup"),
              icon: <FolderPlus size={14} />,
              onClick: () => newGroup(null),
            },
          ]
    : [];

  return (
    <Modal
      open={siteManagerOpen}
      onClose={handleClose}
      title={t("siteManager.title")}
      subtitle={t("siteManager.subtitle")}
      icon={<Server size={16} className="text-[var(--accent)]" />}
      size="large"
      className="site-manager-dialog"
      bodyClassName="flush"
      footer={
        <>
          <Button variant="subtle" onClick={handleClose}>
            {t("siteManager.cancel")}
          </Button>
          <Button variant="subtle" onClick={() => void importSites()}>
            <FileUp size={14} />
            {t("siteManager.import")}
          </Button>
          <Button variant="subtle" onClick={() => void exportSites()}>
            <FileDown size={14} />
            {t("siteManager.export")}
          </Button>
          <div className="spacer" />
          {draft && (
            <>
              <Button variant="danger" onClick={() => selectedSite && handleDeleteSite(selectedSite)} disabled={!selectedSite}>
                <Trash2 size={14} />
                {t("siteManager.delete")}
              </Button>
              <Button onClick={handleDuplicate} disabled={!draft.id}>
                <Copy size={14} />
                {t("siteManager.duplicate")}
              </Button>
              <Button onClick={() => void ensureSaved()} disabled={!dirty && Boolean(draft.id)}>
                <Save size={14} />
                {t("siteManager.save")}
              </Button>
              <Button variant="primary" onClick={() => void handleConnect()}>
                <Plug size={14} />
                {t("siteManager.connect")}
              </Button>
            </>
          )}
          {!draft && selection?.type === "group" && (
            <Button variant="primary" onClick={() => newSite(selection.id)}>
              <Plus size={14} />
              {t("siteManager.newSite")}
            </Button>
          )}
        </>
      }
    >
      <div className="site-manager">
        <aside className="site-sidebar">
          <div className="site-sidebar-head">
            <span>{t("siteManager.sites")}</span>
            <span>{sites.sites.length}</span>
          </div>
          <div
            className="site-tree"
            onContextMenu={(event) => {
              if (event.target !== event.currentTarget) return;
              event.preventDefault();
              setMenu({ x: event.clientX, y: event.clientY, kind: "root" });
            }}
          >
            {sites.sites.length === 0 && sites.groups.length === 0 ? (
              <div className="px-2 py-6 text-center text-[11px] leading-relaxed text-[var(--text-faint)]">
                {t("siteManager.noSites")}
                <br />
                {t("siteManager.noSitesHint")}
              </div>
            ) : (
              <>
                {sitesInGroup(null).map((site) => renderSite(site, 0))}
                {groupsByParent(null).map((group) => renderGroup(group.id, 0))}
              </>
            )}
          </div>
          <div className="site-sidebar-actions">
            <Button size="sm" onClick={() => newSite()}>
              <Plus size={13} />
              {t("siteManager.newSite")}
            </Button>
            <Button size="sm" onClick={() => newGroup()}>
              <FolderPlus size={13} />
              {t("siteManager.newGroup")}
            </Button>
          </div>
        </aside>

        <div className="site-editor">
          {draft ? (
            <>
              <div className="site-editor-head">
                <div className="flex items-center gap-2">
                  <div className="site-editor-title">{draft.name || t("siteManager.untitled")}</div>
                  {dirty && <span className="pane-badge">{t("siteManager.unsaved")}</span>}
                </div>
                <div className="site-editor-subtitle">
                  {protocolLabel(draft.config.protocol)} · {draft.config.host || "—"}
                  {draft.config.port ? `:${draft.config.port}` : ""}
                </div>
                <div className="mt-3 flex gap-4 border-b border-[var(--border)]">
                  <button
                    className={cn(
                      "border-b-2 px-1 pb-2 text-[12px]",
                      tab === "general"
                        ? "border-[var(--accent)] font-semibold text-[var(--text)]"
                        : "border-transparent text-[var(--text-muted)]",
                    )}
                    onClick={() => setTab("general")}
                  >
                    {t("siteManager.general")}
                  </button>
                  <button
                    className={cn(
                      "border-b-2 px-1 pb-2 text-[12px]",
                      tab === "advanced"
                        ? "border-[var(--accent)] font-semibold text-[var(--text)]"
                        : "border-transparent text-[var(--text-muted)]",
                    )}
                    onClick={() => setTab("advanced")}
                  >
                    {t("siteManager.advanced")}
                  </button>
                </div>
              </div>

              <div className="site-form">
                {tab === "general" ? (
                  <div className="flex flex-col gap-4">
                    <div className="form-grid">
                      <div className="field">
                        <label className="field-label">{t("siteManager.name")}</label>
                        <input
                          className="input"
                          value={draft.name}
                          placeholder={t("siteManager.untitled")}
                          onChange={(event) => patch((current) => ({ ...current, name: event.target.value }))}
                        />
                      </div>
                      <div className="field">
                        <label className="field-label">{t("siteManager.group")}</label>
                        <select
                          className="select"
                          value={draft.groupId ?? ""}
                          onChange={(event) =>
                            patch((current) => ({ ...current, groupId: event.target.value || null }))
                          }
                        >
                          <option value="">—</option>
                          {groupOptions.map((group) => (
                            <option key={group.id} value={group.id}>
                              {group.label}
                            </option>
                          ))}
                        </select>
                      </div>
                    </div>

                    <div className="field">
                      <label className="field-label">{t("siteManager.protocol")}</label>
                      <div className="protocol-row">
                        {(["sftp", "ftp", "ftps", "ftpsi"] as Protocol[]).map((protocol) => (
                          <button
                            key={protocol}
                            className={cn("protocol-chip", draft.config.protocol === protocol && "active")}
                            onClick={() => {
                              const oldDefault = protocolPort(draft.config.protocol);
                              patchConfig({
                                protocol,
                                port:
                                  draft.config.port === oldDefault || draft.config.port === 0
                                    ? protocolPort(protocol)
                                    : draft.config.port,
                              });
                            }}
                          >
                            {protocol === "ftpsi" ? "FTPS (I)" : protocolLabel(protocol)}
                          </button>
                        ))}
                      </div>
                    </div>

                    <div className="form-grid">
                      <div className="field">
                        <label className="field-label">{t("siteManager.host")}</label>
                        <input
                          className="input"
                          value={draft.config.host}
                          placeholder="example.com"
                          onChange={(event) => patchConfig({ host: event.target.value })}
                        />
                      </div>
                      <div className="field">
                        <label className="field-label">{t("siteManager.port")}</label>
                        <input
                          className="input"
                          type="number"
                          min={1}
                          max={65535}
                          value={draft.config.port}
                          onChange={(event) => patchConfig({ port: Number(event.target.value) })}
                        />
                      </div>
                      <div className="field">
                        <label className="field-label">{t("siteManager.username")}</label>
                        <input
                          className="input"
                          value={draft.config.username}
                          autoComplete="off"
                          onChange={(event) => patchConfig({ username: event.target.value })}
                        />
                      </div>
                      <div className="field">
                        <label className="field-label">
                          {t("siteManager.authPassword")} / {t("siteManager.authKey")}
                        </label>
                        <div className="auth-row">
                          <button
                            className={cn("protocol-chip", draft.config.authMethod === "password" && "active")}
                            onClick={() => patchConfig({ authMethod: "password" })}
                          >
                            <Lock size={12} />
                            {t("siteManager.authPassword")}
                          </button>
                          <button
                            className={cn("protocol-chip", draft.config.authMethod === "key" && "active")}
                            disabled={draft.config.protocol !== "sftp"}
                            onClick={() => patchConfig({ authMethod: "key" })}
                          >
                            <KeyRound size={12} />
                            {t("siteManager.authKey")}
                          </button>
                        </div>
                      </div>
                    </div>

                    {draft.config.authMethod === "password" ? (
                      <div className="field">
                        <label className="field-label">{t("siteManager.password")}</label>
                        <div className="relative">
                          <input
                            className="input w-full pr-9"
                            type={showPassword ? "text" : "password"}
                            value={draft.password ?? ""}
                            autoComplete="new-password"
                            placeholder={
                              selectedSite?.hasPassword
                                ? t("siteManager.passwordSaved")
                                : t("siteManager.passwordHint")
                            }
                            disabled={draft.clearPassword}
                            onChange={(event) => patch((current) => ({ ...current, password: event.target.value }))}
                          />
                          <button
                            type="button"
                            className="absolute right-1 top-1/2 grid h-7 w-7 -translate-y-1/2 place-items-center rounded text-[var(--text-muted)] hover:bg-[var(--hover)]"
                            onClick={() => setShowPassword((value) => !value)}
                          >
                            {showPassword ? <EyeOff size={14} /> : <Eye size={14} />}
                          </button>
                        </div>
                        {policy && policy.kioskMode >= 1 && (
                          <span className="field-hint">{t("siteManager.kioskPassword")}</span>
                        )}
                        {selectedSite?.hasPassword && (
                          <Checkbox
                            checked={Boolean(draft.clearPassword)}
                            onChange={(value) =>
                              patch((current) => ({ ...current, clearPassword: value, password: "" }))
                            }
                            label={t("siteManager.passwordCleared")}
                          />
                        )}
                      </div>
                    ) : (
                      <div className="form-grid">
                        <div className="field span-2">
                          <label className="field-label">{t("siteManager.keyFile")}</label>
                          <div className="flex gap-2">
                            <input
                              className="input flex-1"
                              value={draft.config.privateKeyPath ?? ""}
                              placeholder="C:\\Users\\you\\.ssh\\id_ed25519"
                              onChange={(event) => patchConfig({ privateKeyPath: event.target.value })}
                            />
                            <Button onClick={() => void browseKey()}>{t("siteManager.browse")}</Button>
                          </div>
                          <span className="field-hint">OpenSSH / PEM · PuTTY .ppk is not supported</span>
                        </div>
                        <div className="field span-2">
                          <label className="field-label">
                            {t("siteManager.passphrase")} <span className="text-[var(--text-faint)]">({t("common.optional")})</span>
                          </label>
                          <input
                            className="input"
                            type="password"
                            value={draft.keyPassphrase ?? ""}
                            placeholder={t("siteManager.passphrase")}
                            onChange={(event) =>
                              patch((current) => ({ ...current, keyPassphrase: event.target.value }))
                            }
                          />
                        </div>
                      </div>
                    )}
                  </div>
                ) : (
                  <div className="flex flex-col gap-4">
                    <div className="form-grid">
                      <div className="field">
                        <label className="field-label">{t("siteManager.initialRemote")}</label>
                        <input
                          className="input"
                          value={draft.config.initialRemoteDir ?? ""}
                          placeholder="/var/www"
                          onChange={(event) => patchConfig({ initialRemoteDir: event.target.value || null })}
                        />
                      </div>
                      <div className="field">
                        <label className="field-label">{t("siteManager.initialLocal")}</label>
                        <input
                          className="input"
                          value={draft.config.localDir ?? ""}
                          placeholder="C:\\Projects"
                          onChange={(event) => patchConfig({ localDir: event.target.value || null })}
                        />
                      </div>
                      <div className="field">
                        <label className="field-label">{t("siteManager.timeout")}</label>
                        <input
                          className="input"
                          type="number"
                          min={3}
                          max={300}
                          value={draft.config.connectTimeoutSecs}
                          onChange={(event) => patchConfig({ connectTimeoutSecs: Number(event.target.value) })}
                        />
                      </div>
                      <div className="field">
                        <label className="field-label">{t("siteManager.keepalive")}</label>
                        <input
                          className="input"
                          type="number"
                          min={0}
                          max={3600}
                          value={draft.config.keepaliveSecs}
                          onChange={(event) => patchConfig({ keepaliveSecs: Number(event.target.value) })}
                        />
                      </div>
                      <div className="field">
                        <label className="field-label">{t("siteManager.maxConnections")}</label>
                        <input
                          className="input"
                          type="number"
                          min={1}
                          max={10}
                          value={draft.config.maxConnections}
                          onChange={(event) => patchConfig({ maxConnections: Number(event.target.value) })}
                        />
                      </div>
                      <div className="field">
                        <label className="field-label">{t("siteManager.encoding")}</label>
                        <select
                          className="select"
                          value={draft.config.encoding}
                          disabled={draft.config.protocol === "sftp"}
                          onChange={(event) => patchConfig({ encoding: event.target.value })}
                        >
                          {ENCODINGS.map((encoding) => (
                            <option key={encoding.value} value={encoding.value}>
                              {encoding.label}
                            </option>
                          ))}
                        </select>
                        <span className="field-hint">{t("siteManager.encodingHint")}</span>
                      </div>
                    </div>

                    <div className="flex flex-col gap-1">
                      <Checkbox
                        checked={draft.config.passiveMode}
                        onChange={(value) => patchConfig({ passiveMode: value })}
                        label={t("siteManager.passive")}
                      />
                      <Checkbox
                        checked={draft.config.verifyCertificate}
                        onChange={(value) => patchConfig({ verifyCertificate: value })}
                        label={t("siteManager.verifyCert")}
                      />
                    </div>

                    <div className="field">
                      <label className="field-label">{t("siteManager.proxy")}</label>
                      <select
                        className="select"
                        value={draft.config.proxyType || ""}
                        onChange={(event) => {
                          const type = event.target.value;
                          patchConfig({
                            proxyType: type,
                            proxyPort: type === "http" ? 8080 : type === "socks5" ? 1080 : 0,
                          });
                        }}
                      >
                        <option value="">{t("siteManager.proxyNone")}</option>
                        <option value="http">{t("siteManager.proxyHttp")}</option>
                        <option value="socks5">{t("siteManager.proxySocks5")}</option>
                      </select>
                      {draft.config.proxyType && draft.config.proxyType !== "none" && (
                        <>
                          <div className="form-grid mt-2">
                            <div className="field">
                              <label className="field-label">{t("siteManager.proxyHost")}</label>
                              <input
                                className="input"
                                value={draft.config.proxyHost}
                                placeholder="127.0.0.1"
                                onChange={(event) => patchConfig({ proxyHost: event.target.value })}
                              />
                            </div>
                            <div className="field">
                              <label className="field-label">{t("siteManager.proxyPort")}</label>
                              <input
                                className="input"
                                type="number"
                                min={1}
                                max={65535}
                                value={draft.config.proxyPort}
                                onChange={(event) => patchConfig({ proxyPort: Number(event.target.value) })}
                              />
                            </div>
                            <div className="field">
                              <label className="field-label">{t("siteManager.proxyUser")}</label>
                              <input
                                className="input"
                                value={draft.config.proxyUsername}
                                autoComplete="off"
                                onChange={(event) => patchConfig({ proxyUsername: event.target.value })}
                              />
                            </div>
                            <div className="field">
                              <label className="field-label">{t("siteManager.proxyPassword")}</label>
                              <input
                                className="input"
                                type="password"
                                value={draft.proxyPassword ?? ""}
                                autoComplete="new-password"
                                onChange={(event) =>
                                  patch((current) => ({ ...current, proxyPassword: event.target.value }))
                                }
                              />
                            </div>
                          </div>
                          <span className="field-hint">{t("siteManager.proxyHint")}</span>
                        </>
                      )}
                    </div>

                    <div className="field">
                      <label className="field-label">{t("siteManager.comments")}</label>
                      <textarea
                        className="textarea"
                        value={draft.config.notes ?? ""}
                        onChange={(event) => patchConfig({ notes: event.target.value })}
                      />
                    </div>
                  </div>
                )}
              </div>
            </>
          ) : (
            <div className="site-empty">
              <Server size={42} />
              <h3 className="text-[14px] font-semibold text-[var(--text-secondary)]">
                {sites.sites.length ? t("siteManager.sites") : t("siteManager.noSites")}
              </h3>
              <p className="max-w-[320px] text-[11.5px] leading-relaxed">{t("siteManager.noSitesHint")}</p>
              <Button variant="primary" onClick={() => newSite()}>
                <Plus size={14} />
                {t("siteManager.newSite")}
              </Button>
            </div>
          )}
        </div>
      </div>

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menuItems} onClose={() => setMenu(null)} />}
    </Modal>
  );
}

function flattenGroups(groups: { id: string; name: string; parentId?: string | null }[]) {
  const result: { id: string; label: string }[] = [];
  const walk = (parentId: string | null, depth: number) => {
    for (const group of groups.filter((item) => (item.parentId ?? null) === parentId)) {
      result.push({ id: group.id, label: `${"  ".repeat(depth)}${depth > 0 ? "└ " : ""}${group.name}` });
      walk(group.id, depth + 1);
    }
  };
  walk(null, 0);
  return result;
}
