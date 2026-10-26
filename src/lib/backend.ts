import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Backend,
  Bootstrap,
  CleanupState,
  MemorySnapshot,
  Preferences,
  ProcessList,
} from "./types";

export async function createBackend(): Promise<Backend> {
  if (!isTauri()) {
    if (import.meta.env.DEV) {
      const { createDemoBackend } = await import("../dev/fixtures");
      return createDemoBackend();
    }
    throw new Error("Open this dashboard in the OpenMemDuct desktop app.");
  }
  return {
    bootstrap: () => invoke<Bootstrap>("get_bootstrap"),
    onSnapshot: async (callback) =>
      listen<MemorySnapshot>("memory:snapshot", (event) =>
        callback(event.payload),
      ),
    onCleanup: async (callback) =>
      listen<CleanupState>("cleanup:state", (event) => callback(event.payload)),
    onSettingsError: async (callback) =>
      listen<string>("settings:error", (event) => callback(event.payload)),
    getTopProcesses: () => invoke<ProcessList>("get_top_processes"),
    savePreferences: (preferences) =>
      invoke<Preferences>("save_preferences", { preferences }),
    requestCleanup: (regions, id) => invoke("request_cleanup", { regions, id }),
    setPreferencesOpen: (open) => invoke("set_preferences_open", { open }),
  };
}
