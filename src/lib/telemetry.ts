import type { Backend, Bootstrap, MemorySnapshot } from "./types";
import { compactHistory } from "./history";

export {
  HISTORY_LIMIT,
  HISTORY_DURATION_MS,
  HISTORY_RECENT_DURATION_MS,
  HISTORY_RECENT_BUCKET_MS,
  HISTORY_ARCHIVE_BUCKET_MS,
  historyGapThreshold,
} from "./history";

export interface DashboardState {
  data: Bootstrap | null;
  error: string | null;
}

export function validSnapshot(value: MemorySnapshot): boolean {
  if (
    ![value.sequence, value.timestampMs, value.elapsedMs].every(
      (n) => Number.isSafeInteger(n) && n >= 0,
    )
  )
    return false;
  if (!value.metrics)
    return value.metrics === null && typeof value.error === "string";
  const m = value.metrics;
  const numbers = [
    m.physicalTotal,
    m.physicalAvailable,
    m.commitUsed,
    m.commitLimit,
    m.systemCache,
    m.kernelPaged,
    m.kernelNonpaged,
  ];
  const optional = [
    m.pagefileTotal,
    m.pagefileUsed,
    m.systemWorkingSet,
    m.systemWorkingSetPeak,
  ];
  return (
    numbers.every((n) => Number.isSafeInteger(n) && n >= 0) &&
    optional.every((n) => n === null || (Number.isSafeInteger(n) && n >= 0)) &&
    ((m.pagefileUsed === null && m.pagefileTotal === null) ||
      (m.pagefileUsed !== null &&
        m.pagefileTotal !== null &&
        m.pagefileUsed <= m.pagefileTotal)) &&
    ((m.systemWorkingSet === null && m.systemWorkingSetPeak === null) ||
      (m.systemWorkingSet !== null &&
        m.systemWorkingSetPeak !== null &&
        m.systemWorkingSet <= m.systemWorkingSetPeak)) &&
    value.metrics.physicalTotal > 0 &&
    value.metrics.physicalAvailable <= value.metrics.physicalTotal &&
    value.metrics.commitLimit > 0 &&
    (m.lowMemory === null || typeof m.lowMemory === "boolean")
  );
}

export function appendHistory(
  history: MemorySnapshot[],
  reading: MemorySnapshot,
): MemorySnapshot[] {
  if (
    !validSnapshot(reading) ||
    (history.length > 0 &&
      (reading.sequence <= history[history.length - 1].sequence ||
        reading.timestampMs < history[history.length - 1].timestampMs))
  )
    return history;
  return compactHistory([...history, reading]);
}

export function createTelemetryStore(backend: Backend) {
  let state: DashboardState = { data: null, error: null };
  const listeners = new Set<() => void>();
  const publish = (next: DashboardState) => {
    state = next;
    listeners.forEach((fn) => fn());
  };
  const update = (data: Bootstrap) => publish({ data, error: null });

  return {
    getSnapshot: () => state,
    subscribe: (fn: () => void) => {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
    update,
    connect() {
      let disposed = false;
      const unlisten: (() => void)[] = [];
      let pending: MemorySnapshot[] = [];
      let pendingError: string | null = null;
      let pendingCleanup: Pick<
        Bootstrap,
        "reports" | "busy" | "cooldownUntilMs"
      > | null = null;
      void (async () => {
        try {
          const offSnapshot = await backend.onSnapshot((reading) => {
            if (disposed || !validSnapshot(reading)) return;
            if (!state.data) {
              pending = appendHistory(pending, reading);
              return;
            }
            const history = appendHistory(state.data.history, reading);
            if (history === state.data.history) return;
            update({ ...state.data, snapshot: reading, history });
          });
          if (disposed) {
            offSnapshot();
            return;
          }
          unlisten.push(offSnapshot);
          const offCleanup = await backend.onCleanup((cleanup) => {
            if (disposed) return;
            pendingCleanup = cleanup;
            if (state.data) update({ ...state.data, ...cleanup });
          });
          if (disposed) {
            offCleanup();
            return;
          }
          unlisten.push(offCleanup);
          if (backend.onSettingsError) {
            const offError = await backend.onSettingsError((error) => {
              if (disposed) return;
              pendingError = error;
              if (state.data) update({ ...state.data, settingsError: error });
            });
            if (disposed) {
              offError();
              return;
            }
            unlisten.push(offError);
          }
          const data = await backend.bootstrap();
          if (disposed) return;
          let history = compactHistory(data.history.filter(validSnapshot));
          for (const reading of pending)
            history = appendHistory(history, reading);
          // Keep the native backfill and any newer events received during bootstrap.
          for (const reading of state.data?.history ?? [])
            history = appendHistory(history, reading);
          update({
            ...data,
            ...(pendingCleanup ?? {}),
            settingsError: pendingError ?? data.settingsError,
            history,
            snapshot: history.at(-1) ?? data.snapshot,
          });
        } catch (error) {
          unlisten.splice(0).forEach((fn) => fn());
          if (!disposed) publish({ ...state, error: String(error) });
        }
      })();
      return () => {
        disposed = true;
        unlisten.forEach((fn) => fn());
      };
    },
    setPreferences(preferences: Bootstrap["preferences"]) {
      if (state.data)
        update({ ...state.data, preferences, settingsError: null });
    },
  };
}

export type TelemetryStore = ReturnType<typeof createTelemetryStore>;
