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

const snapshot: MemorySnapshot = fixture;
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
  it("deduplicates, bounds history, and prunes old samples", () => {
    const history = [snapshot];
    expect(appendHistory(history, snapshot)).toBe(history);
    let samples: MemorySnapshot[] = [];
    for (let i = 0; i < 3700; i++)
      samples = appendHistory(samples, {
        ...snapshot,
        sequence: i,
        timestampMs: i * 1000,
      });
    expect(samples).toHaveLength(3600);
    expect(samples.at(-1)!.timestampMs - samples[0].timestampMs).toBe(
      3_599_000,
    );
    expect(
      appendHistory(samples, {
        ...snapshot,
        sequence: 3701,
        timestampMs: 8_000_000,
      }),
    ).toHaveLength(1);
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
