import { useEffect, useRef } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import { TitleBar } from "./components/TitleBar";
import { Toolbar } from "./components/Toolbar";
import { QuickConnect } from "./components/QuickConnect";
import { RemoteSearch } from "./components/RemoteSearch";
import { BookmarksDialog } from "./components/BookmarksDialog";
import { HostKeyDialog } from "./components/HostKeyDialog";
import { FileZillaImport } from "./components/FileZillaImport";
import { CommandPalette } from "./components/CommandPalette";
import { BottomDock } from "./components/BottomDock";
import { TabBar } from "./components/TabBar";
import { FilePane } from "./components/FilePane";
import { StatusBar } from "./components/StatusBar";
import { SiteManager } from "./components/SiteManager";
import { SettingsDialog } from "./components/SettingsDialog";
import { Dialogs, ToastStack } from "./components/Dialogs";
import { Logo } from "./components/Logo";
import { useT } from "./i18n";
import { basename } from "./lib/format";
import type { Bookmark } from "./types";
import { useAppStore } from "./store/app";
import { isTauri } from "./api/backend";

function playCompletionSound() {
  try {
    const context = new AudioContext();
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    oscillator.frequency.value = 880;
    gain.gain.value = 0.05;
    oscillator.connect(gain);
    gain.connect(context.destination);
    oscillator.start();
    window.setTimeout(() => {
      oscillator.stop();
      void context.close();
    }, 180);
  } catch {
    // audio is best-effort
  }
}
export default function App() {
  const ready = useAppStore((state) => state.ready);
  const logOpen = useAppStore((state) => state.ui.logOpen);
  const queueOpen = useAppStore((state) => state.ui.queueOpen);
  const quickOpen = useAppStore((state) => state.ui.quickConnectOpen);
  const bootstrap = useAppStore((state) => state.bootstrap);
  const t = useT();
  const transfers = useAppStore((state) => state.transfers);
  const settings = useAppStore((state) => state.settings);
  const toast = useAppStore((state) => state.toast);
  const wasBusy = useRef(false);
  const batchStart = useRef(0);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/webview").then(async ({ getCurrentWebview }) => {
      unlisten = await getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === "drop" && event.payload.paths.length > 0) {
          void useAppStore.getState().uploadExternalPaths(event.payload.paths);
        }
      });
    });
    return () => unlisten?.();
  }, []);
  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  useEffect(() => {
    document.documentElement.dataset.accent = settings.accent || "blue";
  }, [settings.accent]);
  useEffect(() => {
    const busy = transfers.some((task) =>
      ["queued", "connecting", "active"].includes(task.status),
    );
    if (!wasBusy.current && busy) {
      batchStart.current = Date.now();
    }
    if (wasBusy.current && !busy) {
      const finished = transfers.filter(
        (task) =>
          ["completed", "failed"].includes(task.status) &&
          (task.finishedAt ?? 0) >= batchStart.current,
      );
      if (finished.length > 0) {
        if (settings.notifyOnComplete) {
          toast("success", t("toast.queueComplete", { count: finished.length }));
        }
        if (settings.soundOnComplete) playCompletionSound();
      }
    }
    wasBusy.current = busy;
  }, [transfers, settings.notifyOnComplete, settings.soundOnComplete, t, toast]);
  useEffect(() => {
    const isEditable = (target: EventTarget | null) => {
      const element = target as HTMLElement | null;
      if (!element) return false;
      return (
        element.tagName === "INPUT" ||
        element.tagName === "TEXTAREA" ||
        element.tagName === "SELECT" ||
        element.isContentEditable
      );
    };

    const quit = async () => {
      if (isTauri()) {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().close();
      } else {
        window.close();
      }
    };

    const handler = (event: KeyboardEvent) => {
      const state = useAppStore.getState();
      if (event.key === "F5") {
        // Refresh both panes even when focus is inside a text field.
        event.preventDefault();
        void state.refreshAll();
        return;
      }
      if (isEditable(event.target)) return;

      const mod = event.ctrlKey || event.metaKey;
      const key = event.key;
      const lower = key.toLowerCase();
      const tab = state.tabs.find((item) => item.id === state.activeTabId) ?? null;
      const side = (): "local" | "remote" => {
        const focused = (document.activeElement as HTMLElement | null)?.closest?.(
          "[data-pane-side]",
        );
        const focusedSide = focused?.getAttribute("data-pane-side");
        if (focusedSide === "local" || focusedSide === "remote") return focusedSide;
        return tab ? "remote" : "local";
      };
      const addBookmark = (bookmarkSide: "local" | "remote") => {
        const path = bookmarkSide === "local" ? state.local.path : tab?.remote.path;
        if (!path) return;
        const bookmark: Bookmark = {
          id: crypto.randomUUID(),
          name: basename(path) || path,
          side: bookmarkSide,
          path,
          siteId: bookmarkSide === "remote" ? tab?.siteId ?? null : null,
          sort: bookmarkSide === "local" ? state.local.sort : tab?.remote.sort ?? null,
        };
        state.updateSettings({ bookmarks: [...state.settings.bookmarks, bookmark] });
      };

      if (mod && lower === "k") {
        event.preventDefault();
        state.toggleUi("commandOpen");
        return;
      }
      if (mod && event.shiftKey && lower === "b") {
        event.preventDefault();
        state.toggleUi("bookmarksOpen");
        return;
      }
      if (mod && !event.shiftKey && lower === "b") {
        event.preventDefault();
        addBookmark(side());
        return;
      }
      if (mod && event.shiftKey && lower === "n") {
        event.preventDefault();
        state.promptNewFolder(side());
        return;
      }
      if (mod && !event.shiftKey && lower === "s") {
        event.preventDefault();
        state.toggleUi("siteManagerOpen");
        return;
      }
      if (mod && lower === "t") {
        event.preventDefault();
        const site = tab?.siteId
          ? state.sites.sites.find((item) => item.id === tab.siteId)
          : undefined;
        if (site) void state.connectSite(site);
        return;
      }
      if (mod && lower === "w") {
        event.preventDefault();
        if (state.activeTabId) void state.closeTab(state.activeTabId);
        return;
      }
      if (mod && lower === "d") {
        event.preventDefault();
        if (state.activeTabId) void state.closeTab(state.activeTabId);
        return;
      }
      if (mod && lower === "r") {
        event.preventDefault();
        if (tab) void state.reconnectTab(tab.id);
        return;
      }
      if (mod && lower === "p") {
        event.preventDefault();
        state.toggleUi("queueOpen");
        return;
      }
      if (mod && lower === "o") {
        event.preventDefault();
        state.toggleCompare();
        return;
      }
      if (mod && lower === "u") {
        event.preventDefault();
        state.updateSettings({ preserveTimestamps: !state.settings.preserveTimestamps });
        return;
      }
      if (mod && lower === "y") {
        event.preventDefault();
        state.updateSettings({ syncBrowsing: !state.settings.syncBrowsing });
        return;
      }
      if (mod && lower === "f") {
        // FileZilla's quick search: focus the filter box of the active pane.
        event.preventDefault();
        const pane =
          (document.activeElement as HTMLElement | null)?.closest?.("[data-pane-side]") ??
          document.querySelector(`[data-pane-side="${side()}"]`);
        const input = pane?.querySelector<HTMLInputElement>(".pane-search");
        if (input) {
          input.focus();
          input.select();
        } else {
          state.toggleUi("searchOpen");
        }
        return;
      }
      if (mod && lower === "c") {
        event.preventDefault();
        const running = state.transfers.filter((task) =>
          ["queued", "connecting", "active"].includes(task.status),
        );
        void Promise.all(running.map((task) => state.transferAction(task.id, "cancel")));
        return;
      }
      if (mod && lower === "q") {
        event.preventDefault();
        void quit();
        return;
      }
      if (mod && (key === "PageUp" || key === "PageDown")) {
        event.preventDefault();
        if (!tab || state.tabs.length < 2) return;
        const index = state.tabs.findIndex((item) => item.id === tab.id);
        const delta = key === "PageDown" ? 1 : -1;
        const next = state.tabs[(index + delta + state.tabs.length) % state.tabs.length];
        state.setActiveTab(next.id);
        return;
      }
      if (key === "F3") {
        event.preventDefault();
        state.toggleUi("searchOpen");
        return;
      }
      if (event.altKey && key === "ArrowDown") {
        const from = side();
        const hasSelection =
          from === "remote"
            ? Boolean(tab && tab.remote.selected.length > 0)
            : state.local.selected.length > 0;
        if (!hasSelection) return;
        event.preventDefault();
        void state.transferSelection(from === "remote" ? "download" : "upload");
        return;
      }
      if (event.altKey && key === "ArrowUp") {
        event.preventDefault();
        if (side() === "remote") {
          if (tab) void state.remoteUp(tab.id);
        } else {
          void state.localUp();
        }
        return;
      }
      if (key === "F2") {
        if (side() === "remote") {
          if (tab && tab.remote.selected.length === 1) {
            const entry = tab.remote.entries.find(
              (item) => item.path === tab.remote.selected[0],
            );
            if (entry) state.promptRename("remote", entry);
          }
        } else if (state.local.selected.length === 1) {
          const entry = state.local.entries.find(
            (item) => item.path === state.local.selected[0],
          );
          if (entry) state.promptRename("local", entry);
        }
        return;
      }
      if (key === "Delete") {
        if (side() === "remote") {
          if (tab && tab.remote.selected.length > 0) state.deleteSelection("remote");
        } else if (state.local.selected.length > 0) {
          state.deleteSelection("local");
        }
        return;
      }
      if (key === "Escape" && !state.ui.prompt && !state.ui.confirm) {
        state.setLocalSelection([]);
        if (tab) state.setRemoteSelection(tab.id, []);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  if (!ready) {
    return (
      <div className="app-shell">
        <div className="empty-workspace">
          <Logo size={64} />
          <h2>FileZilla Modern</h2>
          <p>Rust · Tauri 2 · React</p>
          <div className="text-[11px] text-[var(--text-faint)]">Starting…</div>
        </div>
      </div>
    );
  }

  return (
    <div className="app-shell">
      <TitleBar />
      <Toolbar />
      {quickOpen && <QuickConnect />}
      <RemoteSearch />
      <BookmarksDialog />
      <HostKeyDialog />
      <FileZillaImport />
      <CommandPalette />
      <TabBar />
      <div className="workspace">
        <Group orientation="vertical" className="h-full w-full">
          <Panel id="browser" defaultSize="73%" minSize="30%">
            <Group orientation="horizontal" className="h-full w-full">
              <Panel id="local" defaultSize="50%" minSize="20%">
                <FilePane side="local" />
              </Panel>
              <Separator className="resize-handle" />
              <Panel id="remote" defaultSize="50%" minSize="20%">
                <FilePane side="remote" />
              </Panel>
            </Group>
          </Panel>
          {(queueOpen || logOpen) && (
            <>
              <Separator className="resize-handle" />
              <Panel id="bottom-dock" defaultSize="27%" minSize="13%">
                <BottomDock />
              </Panel>
            </>
          )}
        </Group>
      </div>
      <StatusBar />
      <SiteManager />
      <SettingsDialog />
      <Dialogs />
      <ToastStack />
    </div>
  );
}
