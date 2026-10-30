import { describe, expect, it, vi } from "vitest";
import fixture from "../../tests/fixtures/snapshot.json";
import {
  appendHistory,
  createTelemetryStore,
  validSnapshot,
} from "./telemetry";
import { formatBytes, percentage, signedBytes } from "./format";
import {
  chartMaximum,
  chartSegments,
  cleanupMarkers,
  seriesValues,
} from "../components/MemoryChart";
import { createDemoBackend } from "../dev/fixtures";
import type { CleanupReport, MemorySnapshot } from "./types";
import {
  compactHistory,
  HISTORY_ARCHIVE_BUCKET_MS,
  HISTORY_DURATION_MS,
  HISTORY_LIMIT,
  HISTORY_RECENT_DURATION_MS,
  historyGapThreshold,
} from "./history";

const snapshot: MemorySnapshot = fixture;

function rollbackReadings(): MemorySnapshot[] {
  return [60_000, 61_000, 62_000, 10_000, 11_000, 12_000].map(
    (timestampMs, sequence) => ({
      ...snapshot,
      sequence,
      timestampMs,
      elapsedMs: sequence * 1000,
    }),
  );
}

function archiveReadings(start = 0, firstSequence = 0): MemorySnapshot[] {
  return Array.from({ length: 12 }, (_, index) => {
    const metrics = {
      ...snapshot.metrics!,
      physicalTotal: 1000,
      physicalAvailable: index === 1 ? 100 : index === 2 ? 900 : 500,
      pagefileTotal: index === 7 ? null : 1000,
      pagefileUsed: index === 3 ? 100 : index === 4 ? 900 : index === 7 ? null : 500,
      systemWorkingSetPeak: index === 8 ? null : 1000,
      systemWorkingSet:
        index === 5 ? 100 : index === 6 ? 900 : index === 8 ? null : 500,
    };
    return {
      ...snapshot,
      sequence: firstSequence + index,
      timestampMs: start + index * 1000,
      elapsedMs: start + index * 1000,
      metrics: index === 9 ? null : metrics,
      error: index === 9 ? "Unavailable" : null,
    };
  });
}
describe("memory contract", () => {
  it("reads the shared Windows fixture and independent categories", () => {
    expect(validSnapshot(snapshot)).toBe(true);
    expect(seriesValues(snapshot, "physical")).toEqual({
      used: 9663676416,
      total: 17179869184,
    });
    expect(seriesValues(snapshot, "pagefile")?.used).toBe(1073741824);
    expect(seriesValues(snapshot, "system")?.total).toBe(484442112);
  });
  it("rejects imprecise bytes and impossible counters", () => {
    expect(
      validSnapshot({
        ...snapshot,
        metrics: {
          ...snapshot.metrics!,
          physicalTotal: Number.MAX_SAFE_INTEGER + 1,
        },
      }),
    ).toBe(false);
    expect(
      validSnapshot({
        ...snapshot,
        metrics: {
          ...snapshot.metrics!,
          physicalAvailable: snapshot.metrics!.physicalTotal + 1,
        },
      }),
    ).toBe(false);
    expect(
      validSnapshot({
        ...snapshot,
        metrics: {
          ...snapshot.metrics!,
          pagefileUsed: snapshot.metrics!.pagefileTotal! + 1,
        },
      }),
    ).toBe(false);
  });
  it("keeps optional failures unknown and a disabled pagefile at zero", () => {
    const absent = {
      ...snapshot,
      metrics: {
        ...snapshot.metrics!,
        pagefileTotal: null,
        pagefileUsed: null,
        pagefileError: "Denied",
      },
    };
    expect(validSnapshot(absent)).toBe(true);
    expect(seriesValues(absent, "pagefile")).toBeNull();
    expect(
      seriesValues(
        {
          ...snapshot,
          metrics: { ...snapshot.metrics!, pagefileTotal: 0, pagefileUsed: 0 },
        },
        "pagefile",
      ),
    ).toEqual({ used: 0, total: 0 });
  });
  it("preserves units, zero and negative observed changes", () => {
    expect(formatBytes(1024 ** 3)).toBe("1.0 GiB");
    expect(percentage(1, 0)).toBe(0);
    expect(signedBytes(-64 * 1024 ** 2)).toBe("−64.0 MiB");
    expect(signedBytes(0)).toBe("0 KiB");
  });
});

describe("history and graph", () => {
  it("places cleanup measurements and follow-ups at their actual timestamps without inventing missing counters", () => {
    const before = { ...snapshot, timestampMs: 1000 };
    const after = {
      ...snapshot,
      timestampMs: 2000,
      metrics: {
        ...snapshot.metrics!,
        physicalAvailable: snapshot.metrics!.physicalAvailable + 1024 ** 3,
        pagefileUsed: null,
        pagefileTotal: null,
      },
    };
    const report: CleanupReport = {
      id: "cleanup",
      status: "completed",
      regions: [],
      results: [],
      startedAtMs: 1000,
      durationMs: 1000,
      before,
      after,
      message: "",
      errorCode: null,
      observations: [
        { atSeconds: 16, snapshot: { ...before, timestampMs: 18000 } },
      ],
    };
    const marker = cleanupMarkers(
      [report],
      0,
      20000,
      "physical",
      snapshot.metrics!.physicalTotal,
    )[0];
    expect(marker.x).toBe(80);
    expect(marker.before?.x).toBe(40);
    expect(marker.after!.y).toBeGreaterThan(marker.before!.y);
    expect(marker.change).toBe("1.0 GiB less");
    expect(marker.observations[0]).toMatchObject({ atSeconds: 16, x: 720 });
    expect(
      cleanupMarkers([report], 0, 20000, "pagefile", 1)[0].change,
    ).toBeNull();
    expect(cleanupMarkers([report], 3000, 20000, "physical", 1)).toHaveLength(
      0,
    );
    expect(
      cleanupMarkers(
        [{ ...report, status: "cancelled", before: null, after: null }],
        0,
        20000,
        "physical",
        1,
      )[0],
    ).toMatchObject({
      x: 40,
      change: null,
      before: null,
      after: null,
      tone: "attention",
    });
  });
  it("rejects invalid and stale sequences before considering wall-clock changes", () => {
    const history = [snapshot];
    expect(appendHistory(history, snapshot)).toBe(history);
    for (const sequence of [snapshot.sequence - 1, snapshot.sequence])
      for (const timestampMs of [
        snapshot.timestampMs - 1000,
        snapshot.timestampMs + 1000,
      ])
        expect(
          appendHistory(history, { ...snapshot, sequence, timestampMs }),
        ).toBe(history);
    expect(
      appendHistory(history, {
        ...snapshot,
        sequence: snapshot.sequence + 1,
        timestampMs: snapshot.timestampMs - 1000,
        elapsedMs: Number.NaN,
      }),
    ).toBe(history);
  });
  it("starts fresh history on a newer sequence with an earlier wall clock", () => {
    const rows = rollbackReadings();
    const history = appendHistory(rows.slice(0, 3), rows[3]);
    expect(history).toEqual([rows[3]]);
    expect(appendHistory(history, rows[4])).toEqual(rows.slice(3, 5));
  });
  it("bounds subsecond bursts while preserving the first and latest readings", () => {
    let samples: MemorySnapshot[] = [];
    for (let i = 0; i < 10_000; i++)
      samples = appendHistory(samples, {
        ...snapshot,
        sequence: i,
        timestampMs: i,
      });
    expect(samples).toHaveLength(11);
    expect(samples[0].sequence).toBe(0);
    expect(samples.at(-1)!.sequence).toBe(9999);
    expect(samples.slice(1).map((sample) => sample.timestampMs)).toEqual(
      Array.from({ length: 10 }, (_, index) => index * 1000 + 999),
    );
  });
  it("compacts two days of readings and keeps every second of the newest hour", () => {
    const rows = Array.from(
      { length: HISTORY_DURATION_MS / 1000 + 1 },
      (_, sequence) => ({
        ...snapshot,
        sequence,
        timestampMs: sequence * 1000,
        elapsedMs: sequence * 1000,
      }),
    );
    const retained = compactHistory(rows);
    expect(retained.length).toBeLessThan(5000);
    expect(retained.length).toBeLessThanOrEqual(HISTORY_LIMIT);
    expect(retained[0]).toBe(rows[0]);
    expect(retained.at(-1)).toBe(rows.at(-1));
    expect(
      retained.filter(
        (sample) =>
          sample.timestampMs >= HISTORY_DURATION_MS - HISTORY_RECENT_DURATION_MS,
      ),
    ).toHaveLength(3601);
    expect(compactHistory(retained)).toEqual(retained);
  });
  it("keeps independent extrema, unknown metrics and endpoints in each old bucket", () => {
    const rows = archiveReadings();
    expect(rows.every(validSnapshot)).toBe(true);
    const retained = appendHistory(rows, {
      ...snapshot,
      sequence: 12,
      timestampMs: 2 * HISTORY_RECENT_DURATION_MS,
    });
    // This is also the native fixture's expected selection. Index 10 is neither
    // an endpoint, an independent extremum nor an unavailable-series marker.
    expect(retained.map((sample) => sample.sequence)).toEqual([
      0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12,
    ]);
    for (const sample of retained.slice(0, -1))
      expect(sample).toBe(rows[sample.sequence]);
  });
  it("stays below the fixed bound even when all eleven archive slots are needed", () => {
    const archiveBuckets =
      (HISTORY_DURATION_MS - HISTORY_RECENT_DURATION_MS) /
      HISTORY_ARCHIVE_BUCKET_MS;
    const rows = Array.from({ length: archiveBuckets }, (_, bucket) =>
      archiveReadings(bucket * HISTORY_ARCHIVE_BUCKET_MS, bucket * 12),
    ).flat();
    for (let second = 0; second <= 3600; second++)
      rows.push({
        ...snapshot,
        sequence: archiveBuckets * 12 + second,
        timestampMs:
          HISTORY_DURATION_MS - HISTORY_RECENT_DURATION_MS + second * 1000,
      });
    const retained = compactHistory(rows);
    expect(retained).toHaveLength(archiveBuckets * 11 + 3601);
    expect(retained.length).toBeLessThanOrEqual(HISTORY_LIMIT);
    expect(HISTORY_LIMIT).toBe(9816);
    expect(retained[0]).toBe(rows[0]);
    expect(retained.at(-1)).toBe(rows.at(-1));
  });
  it("expires only readings older than 48 hours and does not fill long outages", () => {
    let samples = [
      { ...snapshot, sequence: 0, timestampMs: 0 },
      { ...snapshot, sequence: 1, timestampMs: 1000 },
    ];
    samples = appendHistory(samples, {
      ...snapshot,
      sequence: 2,
      timestampMs: HISTORY_DURATION_MS,
    });
    expect(samples[0].timestampMs).toBe(0);
    samples = appendHistory(samples, {
      ...snapshot,
      sequence: 3,
      timestampMs: HISTORY_DURATION_MS + 1000,
    });
    expect(samples[0].timestampMs).toBe(1000);
    expect(
      appendHistory(samples, {
        ...snapshot,
        sequence: 4,
        timestampMs: 3 * HISTORY_DURATION_MS,
      }),
    ).toHaveLength(1);
  });
  it("matches incremental compaction as a recent hour moves into the archive", () => {
    const rows = Array.from({ length: 3901 }, (_, sequence) => ({
      ...snapshot,
      sequence,
      timestampMs: sequence * 1000,
      elapsedMs: sequence * 1000,
    }));
    let retained = rows.slice(0, 3601);
    for (const reading of rows.slice(3601))
      retained = appendHistory(retained, reading);
    expect(retained).toEqual(compactHistory(rows));
    expect(retained[0]).toBe(rows[0]);
    expect(retained.at(-1)).toBe(rows.at(-1));
  });
  it("draws compacted archive spacing while preserving real long gaps and unknown metrics", () => {
    const latest = HISTORY_DURATION_MS;
    const rows = [0, 300_000, 607_501, latest - 1000, latest].map(
      (timestampMs, sequence) => ({ ...snapshot, sequence, timestampMs }),
    );
    expect(historyGapThreshold(0, latest)).toBe(307_500);
    expect(historyGapThreshold(latest - 3_600_001, latest)).toBe(307_500);
    expect(historyGapThreshold(latest - 3_600_000, latest)).toBe(7500);
    expect(chartSegments(rows, 0, latest)).toHaveLength(3);
    expect(chartSegments(rows, 0, latest)[0]).toHaveLength(2);
    const missingPagefile = rows.map((row, index) =>
      index === 1
        ? {
            ...row,
            metrics: {
              ...row.metrics!,
              pagefileUsed: null,
              pagefileTotal: null,
            },
          }
        : row,
    );
    expect(chartSegments(missingPagefile, 0, latest, "pagefile")[0]).toHaveLength(1);
  });
  it("uses elapsed spacing and breaks the line after a missing sample or sleep", () => {
    const rows = [0, 1000, 20000, 21000].map((time, i) => ({
      ...snapshot,
      sequence: i,
      timestampMs: time,
    }));
    const segments = chartSegments(rows, 0, 40000, "physical");
    expect(segments).toHaveLength(2);
    expect(segments[1][0].x).toBe(400);
    expect(
      chartSegments([rows[0], rows[2]], 0, 40000, "physical"),
    ).toHaveLength(2);
    rows[1] = { ...rows[1], metrics: null, error: "Unavailable" };
    expect(chartSegments(rows, 0, 40000, "pagefile")).toHaveLength(2);
  });
  it("uses physical capacity and an explicit byte scale for the system working set", () => {
    expect(chartMaximum([snapshot], 0, snapshot.timestampMs, "physical")).toBe(
      snapshot.metrics!.physicalTotal,
    );
    const small = {
      ...snapshot,
      metrics: { ...snapshot.metrics!, systemWorkingSet: 96 * 1024 ** 2 },
    };
    expect(chartMaximum([small], 0, snapshot.timestampMs, "system")).toBe(
      200 * 1024 ** 2,
    );
    expect(
      chartSegments([small], 0, snapshot.timestampMs, "system")[0][0].y,
    ).toBeCloseTo(83.2);
  });
  it("preserves peaks, troughs and time order when drawing an hour of samples", () => {
    const rows = Array.from({ length: 3600 }, (_, i) => ({
      ...snapshot,
      sequence: i,
      timestampMs: i * 1000,
      metrics: {
        ...snapshot.metrics!,
        physicalAvailable:
          snapshot.metrics!.physicalTotal *
          (i === 1234 ? 0.01 : i === 1235 ? 0.99 : 0.5),
      },
    }));
    const segment = chartSegments(rows, 0, 3_599_000)[0];
    expect(segment.length).toBeLessThanOrEqual(1604);
    expect(segment.some((point) => point.sample.sequence === 1234)).toBe(true);
    expect(segment.some((point) => point.sample.sequence === 1235)).toBe(true);
    expect(segment[0].sample.sequence).toBe(0);
    expect(segment.at(-1)!.sample.sequence).toBe(3599);
    expect(
      segment.every(
        (point, i) =>
          i === 0 || point.sample.sequence > segment[i - 1].sample.sequence,
      ),
    ).toBe(true);
  });
  it("cleans subscriptions during asynchronous StrictMode remounts", async () => {
    const backend = createDemoBackend();
    const off = vi.fn();
    backend.onSnapshot = async () => off;
    const store = createTelemetryStore(backend);
    const disconnect = store.connect();
    disconnect();
    await Promise.resolve();
    expect(off).toHaveBeenCalledOnce();
    const disconnectAgain = store.connect();
    await vi.waitFor(() => expect(store.getSnapshot().data).not.toBeNull());
    expect(store.getSnapshot()).toBe(store.getSnapshot());
    disconnectAgain();
    expect(off).toHaveBeenCalledTimes(2);
  });
  it("releases native subscriptions when bootstrap fails", async () => {
    const backend = createDemoBackend();
    const offSnapshot = vi.fn();
    const offCleanup = vi.fn();
    backend.onSnapshot = async () => offSnapshot;
    backend.onCleanup = async () => offCleanup;
    backend.bootstrap = async () => {
      throw new Error("Startup failure");
    };
    const store = createTelemetryStore(backend);
    const disconnect = store.connect();
    await vi.waitFor(() =>
      expect(store.getSnapshot().error).toContain("Startup failure"),
    );
    expect(offSnapshot).toHaveBeenCalledOnce();
    expect(offCleanup).toHaveBeenCalledOnce();
    disconnect();
    expect(offSnapshot).toHaveBeenCalledOnce();
    expect(offCleanup).toHaveBeenCalledOnce();
  });
  it("updates the live snapshot after rollback and keeps sampling before the old clock catches up", async () => {
    const backend = createDemoBackend();
    const base = await backend.bootstrap();
    const rows = rollbackReadings();
    let emit: (reading: MemorySnapshot) => void = () => {};
    backend.onSnapshot = async (callback) => {
      emit = callback;
      return () => {};
    };
    backend.bootstrap = async () => ({
      ...base,
      history: rows.slice(0, 3),
      snapshot: rows[2],
    });
    const store = createTelemetryStore(backend);
    const disconnect = store.connect();
    await vi.waitFor(() =>
      expect(store.getSnapshot().data?.snapshot).toBe(rows[2]),
    );
    emit(rows[3]);
    expect(store.getSnapshot().data?.snapshot).toBe(rows[3]);
    expect(store.getSnapshot().data?.history).toEqual([rows[3]]);
    const afterRollback = store.getSnapshot();
    emit(rows[2]);
    emit({ ...rows[3], timestampMs: 63_000 });
    emit({ ...rows[3], timestampMs: 9000 });
    emit({ ...rows[4], timestampMs: 9000, elapsedMs: Number.NaN });
    emit({
      ...rows[4],
      timestampMs: 9000,
      metrics: { ...snapshot.metrics!, physicalTotal: 0 },
    });
    expect(store.getSnapshot()).toBe(afterRollback);
    emit(rows[4]);
    expect(store.getSnapshot().data?.snapshot).toBe(rows[4]);
    expect(store.getSnapshot().data?.history).toEqual(rows.slice(3, 5));
    disconnect();
  });
  it.each([false, true])(
    "merges pending rollback events with native backfill (native newer: %s)",
    async (nativeNewer) => {
      const backend = createDemoBackend();
      const base = await backend.bootstrap();
      const rows = rollbackReadings();
      let emit: (reading: MemorySnapshot) => void = () => {};
      backend.onSnapshot = async (callback) => {
        emit = callback;
        return () => {};
      };
      backend.bootstrap = async () => {
        for (const reading of rows.slice(1, 5)) emit(reading);
        emit(rows[2]);
        emit({ ...rows[4], timestampMs: 63_000 });
        emit({ ...rows[5], elapsedMs: Number.NaN });
        const history = nativeNewer ? rows.slice(3) : rows.slice(0, 3);
        return { ...base, history, snapshot: history.at(-1)! };
      };
      const store = createTelemetryStore(backend);
      const disconnect = store.connect();
      await vi.waitFor(() => expect(store.getSnapshot().data).not.toBeNull());
      expect(store.getSnapshot().data?.history).toEqual(
        nativeNewer ? rows.slice(3) : rows.slice(3, 5),
      );
      expect(store.getSnapshot().data?.snapshot).toBe(
        nativeNewer ? rows[5] : rows[4],
      );
      disconnect();
    },
  );
  it.each([false, true])(
    "keeps the newest sequence when rollback occurs during reconnect (native newer: %s)",
    async (nativeNewer) => {
      const backend = createDemoBackend();
      const base = await backend.bootstrap();
      const rows = rollbackReadings();
      let emit: (reading: MemorySnapshot) => void = () => {};
      backend.onSnapshot = async (callback) => {
        emit = callback;
        return () => {};
      };
      backend.bootstrap = vi
        .fn()
        .mockResolvedValueOnce({ ...base, history: [rows[0]], snapshot: rows[0] })
        .mockImplementationOnce(async () => {
          for (const reading of rows.slice(1, 5)) emit(reading);
          emit(rows[2]);
          const history = nativeNewer ? rows.slice(3) : rows.slice(0, 3);
          return { ...base, history, snapshot: history.at(-1)! };
        });
      const store = createTelemetryStore(backend);
      const disconnect = store.connect();
      await vi.waitFor(() =>
        expect(store.getSnapshot().data?.snapshot).toBe(rows[0]),
      );
      disconnect();
      const disconnectAgain = store.connect();
      await vi.waitFor(() =>
        expect(store.getSnapshot().data?.history).toEqual(
          nativeNewer ? rows.slice(3) : rows.slice(3, 5),
        ),
      );
      expect(store.getSnapshot().data?.snapshot).toBe(
        nativeNewer ? rows[5] : rows[4],
      );
      emit(rows[2]);
      expect(store.getSnapshot().data?.snapshot).toBe(
        nativeNewer ? rows[5] : rows[4],
      );
      disconnectAgain();
    },
  );
  it("seeds a bounded two-day preview and preserves native backfill during bootstrap", async () => {
    const backend = createDemoBackend();
    const base = await backend.bootstrap();
    expect(base.history.length).toBeLessThanOrEqual(HISTORY_LIMIT);
    expect(base.history.length).toBeLessThan(6500);
    expect(base.history.every(validSnapshot)).toBe(true);
    expect(base.history.at(-1)!.timestampMs - base.history[0].timestampMs).toBe(
      HISTORY_DURATION_MS,
    );
    expect(
      base.history.filter(
        (reading) =>
          reading.timestampMs >=
          base.snapshot!.timestampMs - HISTORY_RECENT_DURATION_MS,
      ),
    ).toHaveLength(3601);
    const history = base.history.slice(1);
    const reading = {
      ...base.snapshot!,
      sequence: base.snapshot!.sequence + 1,
      timestampMs: base.snapshot!.timestampMs + 1000,
      elapsedMs: base.snapshot!.elapsedMs + 1000,
    };
    let emit: (reading: MemorySnapshot) => void = () => {};
    backend.onSnapshot = async (callback) => {
      emit = callback;
      return () => {};
    };
    backend.bootstrap = async () => {
      emit(reading);
      return { ...base, history };
    };
    const store = createTelemetryStore(backend);
    const disconnect = store.connect();
    await vi.waitFor(() => expect(store.getSnapshot().data).not.toBeNull());
    const restored = store.getSnapshot().data!;
    expect(restored.history[0]).toBe(history[0]);
    expect(restored.history.at(-1)).toBe(reading);
    expect(restored.snapshot).toBe(reading);
    expect(restored.history.length).toBeLessThanOrEqual(HISTORY_LIMIT);
    expect(restored.history.at(-1)!.timestampMs - restored.history[0].timestampMs)
      .toBeGreaterThan(47 * 60 * 60_000);
    disconnect();
  });
  it("merges tray history with events that arrive while a resumed window bootstraps", async () => {
    const backend = createDemoBackend();
    const base = await backend.bootstrap();
    const rows = Array.from({ length: 5 }, (_, sequence) => ({
      ...snapshot,
      sequence,
      timestampMs: sequence * 1000,
    }));
    let emit: (reading: MemorySnapshot) => void = () => {};
    backend.onSnapshot = async (callback) => {
      emit = callback;
      return () => {};
    };
    backend.bootstrap = vi
      .fn()
      .mockResolvedValueOnce({ ...base, history: [rows[0]], snapshot: rows[0] })
      .mockImplementationOnce(async () => {
        emit(rows[4]);
        return { ...base, history: rows.slice(0, 4), snapshot: rows[3] };
      });
    const store = createTelemetryStore(backend);
    const disconnect = store.connect();
    await vi.waitFor(() =>
      expect(store.getSnapshot().data?.history).toHaveLength(1),
    );
    disconnect();
    const disconnectAgain = store.connect();
    await vi.waitFor(() =>
      expect(
        store.getSnapshot().data?.history.map((row) => row.sequence),
      ).toEqual([0, 1, 2, 3, 4]),
    );
    expect(store.getSnapshot().data?.snapshot?.sequence).toBe(4);
    disconnectAgain();
  });
});
