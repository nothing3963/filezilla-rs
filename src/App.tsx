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
    const handler = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable)
      ) {
        return;
      }
      const state = useAppStore.getState();
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        state.toggleUi("commandOpen");
        return;
      }
      const tab = state.tabs.find((item) => item.id === state.activeTabId);
      if (event.key === "F5") {
        event.preventDefault();
        void state.refreshRemote();
      } else if (event.key === "Delete") {
        const hasRemote = tab && tab.remote.selected.length > 0;
        if (hasRemote) state.deleteSelection("remote");
        else if (state.local.selected.length > 0) state.deleteSelection("local");
      } else if (event.key === "F2") {
        if (tab && tab.remote.selected.length === 1) {
          const entry = tab.remote.entries.find((item) => item.path === tab.remote.selected[0]);
          if (entry) state.promptRename("remote", entry);
        } else if (state.local.selected.length === 1) {
          const entry = state.local.entries.find((item) => item.path === state.local.selected[0]);
          if (entry) state.promptRename("local", entry);
        }
      } else if (event.key === "Escape" && !state.ui.prompt && !state.ui.confirm) {
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
