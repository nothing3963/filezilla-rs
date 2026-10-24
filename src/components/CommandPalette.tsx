import { useMemo, useState } from "react";
import { Command } from "lucide-react";
import { useT } from "../i18n";
import { useAppStore } from "../store/app";
import { Modal } from "./ui/Modal";

type PaletteCommand = {
  id: string;
  label: string;
  hint?: string;
  run: () => void;
};

export function CommandPalette() {
  const t = useT();
  const open = useAppStore((state) => state.ui.commandOpen);
  const toggleUi = useAppStore((state) => state.toggleUi);
  const state = useAppStore.getState();
  const [query, setQuery] = useState("");

  const commands: PaletteCommand[] = [
    { id: "sites", label: t("command.sites"), run: () => state.openSiteManager() },
    { id: "quick", label: t("command.quickConnect"), run: () => toggleUi("quickConnectOpen") },
    { id: "settings", label: t("command.settings"), run: () => void state.openSettings() },
    { id: "refresh-remote", label: t("command.refreshRemote"), run: () => void state.refreshRemote() },
    { id: "refresh-local", label: t("command.refreshLocal"), run: () => void state.refreshLocal() },
    { id: "search", label: t("command.search"), run: () => toggleUi("searchOpen") },
    { id: "bookmarks", label: t("command.bookmarks"), run: () => toggleUi("bookmarksOpen") },
    { id: "compare", label: t("command.compare"), run: () => state.toggleCompare() },
    {
      id: "sync",
      label: t("command.sync"),
      run: () => state.updateSettings({ syncBrowsing: !state.settings.syncBrowsing }),
    },
    { id: "queue", label: t("command.queue"), run: () => toggleUi("queueOpen") },
    { id: "logs", label: t("command.logs"), run: () => toggleUi("logOpen") },
    {
      id: "theme",
      label: t("command.toggleTheme"),
      run: () => {
        const next = state.settings.theme === "dark" ? "light" : "dark";
        state.updateSettings({ theme: next });
      },
    },
  ];

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return commands;
    return commands.filter((command) => command.label.toLowerCase().includes(needle));
  }, [commands, query]);

  const run = (command: PaletteCommand) => {
    command.run();
    toggleUi("commandOpen");
    setQuery("");
  };

  return (
    <Modal
      open={open}
      onClose={() => toggleUi("commandOpen")}
      title={t("command.title")}
      subtitle={t("command.subtitle")}
      icon={<Command size={16} />}
      size="small"
    >
      <div className="command-palette">
        <input
          className="input"
          autoFocus
          value={query}
          placeholder={t("command.placeholder")}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && filtered[0]) run(filtered[0]);
          }}
        />
        <div className="command-list">
          {filtered.map((command) => (
            <button key={command.id} className="command-row" onClick={() => run(command)}>
              <span>{command.label}</span>
            </button>
          ))}
        </div>
        <div className="command-hint">{t("command.hint")}</div>
      </div>
    </Modal>
  );
}
