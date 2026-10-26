import { GiB } from "../lib/format";
import { defaultPreferences } from "../lib/types";
import { appendHistory, HISTORY_LIMIT } from "../lib/telemetry";
import { preferencesError } from "../lib/preferences";
import type {
  Backend,
  Bootstrap,
  CleanupReport,
  CleanupState,
  DemoScenario,
  MemorySnapshot,
  Preferences,
} from "../lib/types";

export function createDemoBackend(): Backend {
  let scenario: DemoScenario = "normal";
  let sequence = 0;
  let reclaimedBytes = 0;
  let trimmedSystemBytes = 0;
  let preferences: Preferences = { ...defaultPreferences };
  let cleanup: CleanupState = { reports: [], busy: false, cooldownUntilMs: 0 };
  let interval: ReturnType<typeof setInterval> | undefined;
  const snapshots = new Set<(snapshot: MemorySnapshot) => void>();
  const cleanups = new Set<(state: CleanupState) => void>();
  const publishCleanup = () => cleanups.forEach((fn) => fn({ ...cleanup }));
  function reading(timestampMs = Date.now()): MemorySnapshot {
    const t = sequence++;
    const wave = Math.sin(t / 210) * 0.6 + Math.sin(t / 57) * 0.18;
    const used =
      scenario === "pressure" ? 29.9 : 18.6 + wave + (t % 900 > 720 ? 1.3 : 0);
    return {
      sequence: t,
      timestampMs,
      elapsedMs: t * 1000,
      error: null,
      metrics: {
        physicalTotal: 32 * GiB,
        physicalAvailable: Math.max(
          0,
          Math.min(32 * GiB, Math.round((32 - used) * GiB) + reclaimedBytes),
        ),
        commitUsed: Math.round((scenario === "pressure" ? 40.1 : 23.4) * GiB),
        commitLimit: 48 * GiB,
        systemCache: Math.round(7.2 * GiB),
        kernelPaged: Math.round(0.48 * GiB),
        kernelNonpaged: Math.round(0.62 * GiB),
        pagefileTotal: 10 * GiB,
        pagefileUsed: Math.round(
          (1.4 + Math.sin(t / 380) * 0.22 + (t % 1500 > 1000 ? 0.35 : 0)) * GiB,
        ),
        pagefileError: null,
        systemWorkingSet: Math.max(
          0,
          Math.round(
            (112 +
              Math.sin(t / 125) * 15 +
              Math.sin(t / 31) * 3 +
              (t % 420 > 410 ? 30 : 0)) *
              1024 ** 2,
          ) - trimmedSystemBytes,
        ),
        systemWorkingSetPeak: 462 * 1024 ** 2,
        systemWorkingSetError: null,
        lowMemory: scenario === "pressure",
      },
    };
  }
  let latest: MemorySnapshot;
  let history: MemorySnapshot[];
  function seed() {
    const end = Date.now() - (scenario === "stale" ? 30_000 : 0);
    history = Array.from({ length: HISTORY_LIMIT }, (_, i) =>
      reading(end - (HISTORY_LIMIT - 1 - i) * 1000),
    );
    latest = history[history.length - 1];
  }
  seed();
  function publishSnapshot(snapshot: MemorySnapshot) {
    latest = snapshot;
    history = appendHistory(history, latest);
    snapshots.forEach((callback) => callback(latest));
  }
  const bootstrap = (): Bootstrap => ({
    ...cleanup,
    preferences,
    snapshot: latest,
    history,
    demo: true,
    settingsError: null,
    capabilities: {
      platform: "Browser preview",
      build: null,
      elevated: false,
      cleanupSupported: scenario !== "unsupported",
      reason:
        scenario === "unsupported"
          ? "Cleanup is unavailable on this Windows build. Monitoring remains available."
          : null,
    },
  });

  return {
    bootstrap: async () => bootstrap(),
    onSnapshot: async (fn) => {
      snapshots.add(fn);
      interval ??= setInterval(() => {
        if (scenario === "stale") return;
        publishSnapshot(reading());
        const report = cleanup.reports[0];
        if (report?.after && cleanup.cooldownUntilMs > 0) {
          const elapsed = Math.floor(
            (latest.timestampMs - report.after.timestampMs) / 1000,
          );
          const target = [60, 10, 1].find((seconds) => elapsed >= seconds);
          if (
            target &&
            !report.observations.some(
              (observation) => observation.atSeconds >= target,
            )
          ) {
            cleanup = {
              ...cleanup,
              reports: [
                {
                  ...report,
                  observations: [
                    ...report.observations,
                    { atSeconds: elapsed, snapshot: latest },
                  ],
                },
                ...cleanup.reports.slice(1),
              ],
            };
            publishCleanup();
          }
        }
      }, 1000);
      return () => {
        snapshots.delete(fn);
        if (!snapshots.size) {
          clearInterval(interval);
          interval = undefined;
        }
      };
    },
    onCleanup: async (fn) => {
      cleanups.add(fn);
      return () => {
        cleanups.delete(fn);
      };
    },
    getTopProcesses: async () => ({
      timestampMs: Date.now(),
      inaccessible: 3,
      rows: [
        ["msedge.exe", 1216],
        ["Code.exe", 742],
        ["explorer.exe", 218],
        ["dwm.exe", 146],
        ["OpenMemDuct.exe", 86],
        ["SearchHost.exe", 71],
        ["OneDrive.exe", 58],
        ["RuntimeBroker.exe", 34],
      ].map(([name, amount], i) => ({
        pid: 1000 + i,
        createdAt: 100 + i,
        name: String(name),
        bytes: Number(amount) * 1024 ** 2,
        metric: "privateWorkingSet" as const,
      })),
    }),
    savePreferences: async (value) => {
      const error = preferencesError(value);
      if (error) throw new Error(error);
      preferences = value;
      return preferences;
    },
    async requestCleanup(regions, id) {
      if (
        !regions.length ||
        regions.length > 8 ||
        new Set(regions).size !== regions.length
      )
        throw new Error("Choose at least one distinct region.");
      if (cleanup.busy) throw new Error("A cleanup is already in progress.");
      if (cleanup.cooldownUntilMs > Date.now())
        throw new Error("Wait for the current observation period to finish.");
      const startedAtMs = Date.now();
      const before = reading(startedAtMs);
      publishSnapshot(before);
      cleanup = { ...cleanup, busy: true };
      publishCleanup();
      await new Promise((resolve) => setTimeout(resolve, 850));
      const statuses = {
        cancelled: "cancelled",
        denied: "permissionDenied",
        unsupported: "unsupported",
        failed: "failed",
        partial: "partial",
      } as const;
      const status =
        scenario in statuses
          ? statuses[scenario as keyof typeof statuses]
          : "completed";
      const measured = status === "completed" || status === "partial";
      const after = reading();
      if (measured && after.metrics && before.metrics) {
        const delta =
          scenario === "negative"
            ? -64 * 1024 ** 2
            : scenario === "noChange"
              ? 0
              : Math.round(1.8 * GiB);
        const systemDelta =
          scenario === "noChange" ||
          scenario === "negative" ||
          !regions.includes("systemFileCache")
            ? 0
            : Math.min(64 * 1024 ** 2, before.metrics.systemWorkingSet ?? 0);
        reclaimedBytes += delta;
        trimmedSystemBytes += systemDelta;
        after.metrics.physicalAvailable =
          before.metrics.physicalAvailable + delta;
        after.metrics.pagefileUsed = before.metrics.pagefileUsed;
        after.metrics.systemWorkingSet =
          before.metrics.systemWorkingSet === null
            ? null
            : before.metrics.systemWorkingSet - systemDelta;
      }
      publishSnapshot(after);
      const messages = {
        completed:
          "The cache operation completed. Changes below are observations, not a speedup guarantee.",
        cancelled:
          "Administrator approval was cancelled. No cleanup was performed.",
        permissionDenied:
          "Windows did not grant the required privilege. No cleanup was performed.",
        unsupported: "This operation is unavailable on this Windows build.",
        failed:
          "Windows could not complete the cache operation. Monitoring is still running.",
        partial:
          "Some selected operations did not complete. Check individual results.",
      };
      const report: CleanupReport = {
        id,
        regions,
        results: regions.map((region, i) => ({
          region,
          status:
            status === "partial"
              ? i === 0
                ? "permissionDenied"
                : "completed"
              : status,
          durationMs: 8,
          errorCode: status === "failed" ? "0xC0000001" : null,
        })),
        status,
        startedAtMs,
        durationMs: measured ? after.timestampMs - startedAtMs : null,
        before: measured ? before : null,
        after: measured ? after : null,
        observations: [],
        message: messages[status],
        errorCode: status === "failed" ? "0xC0000001" : null,
      };
      cleanup = {
        reports: [report, ...cleanup.reports].slice(0, 20),
        busy: false,
        cooldownUntilMs:
          status === "completed" || status === "partial"
            ? Date.now() + 60_000
            : 0,
      };
      publishCleanup();
    },
    async setScenario(value) {
      scenario = value;
      reclaimedBytes = 0;
      trimmedSystemBytes = 0;
      cleanup = { reports: [], busy: false, cooldownUntilMs: 0 };
      seed();
      return bootstrap();
    },
  };
}
