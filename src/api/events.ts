import { listen } from "@tauri-apps/api/event";
import { isTauri } from "./backend";

export interface MockEvent {
  name: string;
  payload: unknown;
}

export type Unlisten = () => void;

/**
 * Subscribe to backend events. In browser preview mode the mock backend
 * dispatches a single `filezilla:mock-event` window event.
 */
export async function onBackendEvent<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<Unlisten> {
  if (isTauri()) {
    return listen<T>(name, (event) => handler(event.payload));
  }
  const listener = (event: Event) => {
    const detail = (event as CustomEvent<MockEvent>).detail;
    if (detail?.name === name) handler(detail.payload as T);
  };
  window.addEventListener("filezilla:mock-event", listener);
  return () => window.removeEventListener("filezilla:mock-event", listener);
}

export function emitMockEvent(name: string, payload: unknown) {
  window.dispatchEvent(
    new CustomEvent<MockEvent>("filezilla:mock-event", { detail: { name, payload } }),
  );
}
