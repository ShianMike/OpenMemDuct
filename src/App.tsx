import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Backend, DemoScenario, MemorySeries } from "./lib/types";
import type { TelemetryStore } from "./lib/telemetry";
import { formatBytes, percentage } from "./lib/format";
import { WindowBar } from "./components/WindowBar";
import { Preferences as PreferencesEditor } from "./components/Preferences";
import { memoryRangeColor } from "./lib/memoryPalette";
import { MemoryChart, seriesValues } from "./components/MemoryChart";

export function App({
  backend,
  store,
}: {
  backend: Backend;
  store: TelemetryStore;
}) {
  const { data, error } = useSyncExternalStore(
    store.subscribe,
    store.getSnapshot,
  );
  const [sheet, setSheet] = useState<"settings" | null>(null);
  const [cleanupId, setCleanupId] = useState<string | null>(null);
  const [cleanupDetails, setCleanupDetails] = useState<{
    series: MemorySeries;
    id: string;
  } | null>(null);
  const changeCleanupDetails = useCallback(
    (series: MemorySeries, id: string | null) => {
      setCleanupDetails((current) =>
        id !== null && !(current?.series === series && current.id === id)
          ? { series, id }
          : null,
      );
    },
    [],
  );
  const [minutes, setMinutes] = useState(15);
  const [now, setNow] = useState(Date.now());
  const [scenario, setScenario] = useState<DemoScenario>("normal");
  const [actionError, setActionError] = useState<string | null>(null);
  const [requesting, setRequesting] = useState(false);
  const [savingPreferences, setSavingPreferences] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    if (!cleanupDetails) return;
    const close = () => setCleanupDetails(null);
    const outside = (event: PointerEvent) => {
      if (
        !(event.target instanceof Element) ||
        !event.target.closest('[data-cleanup-details-open="true"]')
      )
        close();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("keydown", escape);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("keydown", escape);
      window.removeEventListener("blur", close);
    };
  }, [cleanupDetails]);
  useEffect(() => {
    let disposed = false;
    void backend.setPreferencesOpen?.(sheet === "settings").catch((error) => {
      if (!disposed) setActionError(String(error));
    });
    return () => {
      disposed = true;
    };
  }, [backend, sheet]);
  useEffect(() => {
    let disconnect = store.connect();
    let disposed = false;
    let offFocus: (() => void) | undefined;
    const resume = () => {
      if (!disposed && !document.hidden) {
        disconnect();
        disconnect = store.connect();
      }
    };
    document.addEventListener("visibilitychange", resume);
    if (isTauri()) {
      void getCurrentWindow()
        .onFocusChanged(({ payload }) => {
          if (payload) resume();
        })
        .then((off) => {
          if (disposed) off();
          else offFocus = off;
        })
        .catch((error) => {
          if (!disposed)
            setActionError(
              "Could not refresh resumed history: " + String(error),
            );
        });
    }
    return () => {
      disposed = true;
      document.removeEventListener("visibilitychange", resume);
      offFocus?.();
      disconnect();
    };
  }, [store]);
  useEffect(() => {
    const timer = setInterval(() => {
      if (!document.hidden) setNow(Date.now());
    }, 1000);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    if (sheet) dialog.current?.showModal();
    else dialog.current?.close();
  }, [sheet]);

  if (!data)
    return (
      <main className="graph-app">
        <WindowBar />
        <div className="loading-view">
          <span role={error ? "alert" : "status"}>{error ?? "···"}</span>
          {error && <button onClick={() => location.reload()}>Retry</button>}
        </div>
      </main>
    );
  const current = data.snapshot?.metrics
    ? data.snapshot
    : [...data.history].reverse().find((s) => s.metrics);
  const stale =
    !data.snapshot?.metrics || now - data.snapshot.timestampMs > 6500;
  const latest = data.reports[0];
  const chartNow = Math.max(
    stale ? now : (data.snapshot?.timestampMs ?? now),
    latest?.after?.timestampMs ?? latest?.startedAtMs ?? 0,
    latest?.observations.at(-1)?.snapshot.timestampMs ?? 0,
  );
  const chartStart = Math.max(
    chartNow - minutes * 60_000,
    Math.min(data.history[0]?.timestampMs ?? chartNow, chartNow - 60_000),
  );
  const recordedMinutes = Math.min(
    60,
    Math.round(
      ((data.history.at(-1)?.timestampMs ?? chartNow) -
        (data.history[0]?.timestampMs ?? chartNow)) /
        60_000,
    ),
  );
  const cooldown = Math.max(
    0,
    Math.min(60, Math.ceil((data.cooldownUntilMs - now) / 1000)),
  );
  const busy = data.busy || requesting;
  const selectedCleanupId = data.reports.some((r) => r.id === cleanupId)
    ? cleanupId
    : latest?.id;
  const cleanupUnavailable = !data.capabilities.cleanupSupported
    ? (data.capabilities.reason ??
      "Cleanup is unavailable on this Windows build.")
    : !data.preferences.cleanupRegions.length
      ? "Choose memory regions in Preferences."
      : null;

  async function clean() {
    if (busy || cooldown > 0 || cleanupUnavailable) return;
    setCleanupDetails(null);
    setActionError(null);
    setRequesting(true);
    try {
      await backend.requestCleanup(
        data!.preferences.cleanupRegions,
        crypto.randomUUID(),
      );
    } catch (e) {
      setActionError(String(e));
    } finally {
      setRequesting(false);
    }
  }

  return (
    <main className="graph-app">
      <WindowBar>
        <div
          className="time-options"
          role="group"
          aria-label="Chart time range"
        >
          {[5, 15, 30, 60].map((value) => (
            <button
              key={value}
              aria-pressed={minutes === value}
              onClick={() => setMinutes(value)}
            >
              {value === 60 ? "1 hour" : `${value} min`}
            </button>
          ))}
        </div>
      </WindowBar>
      <div className="graphs">
        {(["physical", "pagefile", "system"] as MemorySeries[]).map(
          (series) => {
            const label = {
              physical: "Physical Memory",
              pagefile: "Pagefile",
              system: "System Working Set",
            }[series];
            const values = current ? seriesValues(current, series) : null;
            const counterError =
              series === "pagefile"
                ? current?.metrics?.pagefileError
                : series === "system"
                  ? current?.metrics?.systemWorkingSetError
                  : data.snapshot?.error;
            return (
              <section className="graph-row" key={series} aria-label={label}>
                <div className="graph-topline">
                  <span className="series-label">{label}</span>
                  <div className="series-readings">
                    <span className="series-used">
                      {values ? formatBytes(values.used) : "—"}
                    </span>
                    <span className="series-total">
                      of {values ? formatBytes(values.total) : "—"}
                      {series === "system" ? " peak" : ""}
                    </span>
                    <span
                      className="percentage"
                      style={
                        values && values.total > 0
                          ? {
                              color: `color-mix(in srgb, ${memoryRangeColor(percentage(values.used, values.total))} 70%, #25262a)`,
                            }
                          : undefined
                      }
                      title={
                        series === "system"
                          ? "Current working set as a share of its observed peak"
                          : undefined
                      }
                    >
                      {values && values.total > 0
                        ? `${percentage(values.used, values.total).toFixed(1)}%`
                        : values?.total === 0
                          ? "Off"
                          : "—"}
                    </span>
                  </div>
                </div>
                <MemoryChart
                  history={data.history}
                  now={chartNow}
                  start={chartStart}
                  minutes={minutes}
                  series={series}
                  label={label}
                  reports={data.reports}
                  selectedCleanupId={selectedCleanupId}
                  onSelectCleanup={setCleanupId}
                  expandedCleanupId={
                    cleanupDetails?.series === series ? cleanupDetails.id : null
                  }
                  onCleanupDetailsChange={changeCleanupDetails}
                />
                {counterError && (
                  <span className="counter-error" role="status">
                    {counterError}
                  </span>
                )}
              </section>
            );
          },
        )}
      </div>
      <div className="graph-controls">
        <div className="recording-state">
          {data.demo && <span className="demo-badge">Demo</span>}
          {stale && (
            <span className="stale-badge" role="status">
              Paused
            </span>
          )}
          <span hidden={busy || cooldown > 0 || !!cleanupUnavailable}>
            {recordedMinutes < 1
              ? "Less than a minute recorded"
              : `${recordedMinutes} min recorded`}
          </span>
          <span className="cleanup-state" role="status">
            {busy
              ? latest?.status === "unknown"
                ? "Waiting for the cleanup result…"
                : "Cleaning memory…"
              : cleanupUnavailable}
          </span>
          {!busy && cooldown > 0 && (
            <span className="observation-state" aria-live="off">
              Observing · {cooldown}s
            </span>
          )}
        </div>
        <div className="action-buttons">
          <button
            className="text-button"
            aria-label="Open Preferences"
            onClick={async () => {
              setCleanupDetails(null);
              setActionError(null);
              try {
                await backend.setPreferencesOpen?.(true);
                setSheet("settings");
              } catch (error) {
                setActionError(String(error));
              }
            }}
          >
            Preferences
          </button>
          <button
            className="primary-button"
            aria-label="Clean Memory"
            aria-busy={busy}
            disabled={busy || cooldown > 0 || !!cleanupUnavailable}
            title={
              cleanupUnavailable ??
              (cooldown > 0 ? "Measuring the previous cleanup" : undefined)
            }
            onClick={() => void clean()}
          >
            {busy ? "Cleaning…" : "Clean Memory"}
          </button>
        </div>
      </div>
      {(error || data.settingsError || (!sheet && actionError)) && (
        <p className="inline-error" role="alert">
          {error ?? data.settingsError ?? actionError}
        </p>
      )}
      <dialog
        className="action-sheet preferences-sheet"
        ref={dialog}
        aria-label="Preferences"
        onCancel={(event) => {
          event.preventDefault();
          if (sheet === "settings" && savingPreferences) return;
          setSheet(null);
        }}
      >
        {sheet === "settings" && (
          <PreferencesEditor
            preferences={data.preferences}
            capabilities={data.capabilities}
            backend={backend}
            onSaved={(value) => store.setPreferences(value)}
            onClose={() => setSheet(null)}
            onSavingChange={setSavingPreferences}
          >
            {data.demo && backend.setScenario && (
              <label className="setting-row">
                <span>Demo</span>
                <select
                  aria-label="Demo Scenario"
                  value={scenario}
                  onChange={async (event) => {
                    const value = event.target.value as DemoScenario;
                    setScenario(value);
                    setActionError(null);
                    try {
                      store.update(await backend.setScenario!(value));
                    } catch (e) {
                      setActionError(String(e));
                    }
                  }}
                >
                  <option value="normal">Normal</option>
                  <option value="pressure">Pressure</option>
                  <option value="stale">Paused</option>
                  <option value="cancelled">Cancelled</option>
                  <option value="denied">Denied</option>
                  <option value="unsupported">Unsupported</option>
                  <option value="failed">Failed</option>
                  <option value="partial">Partial Completion</option>
                  <option value="noChange">No Change</option>
                  <option value="negative">Negative Change</option>
                </select>
              </label>
            )}
          </PreferencesEditor>
        )}
        {actionError && (
          <p className="inline-error" role="alert">
            {actionError}
          </p>
        )}
      </dialog>
    </main>
  );
}
