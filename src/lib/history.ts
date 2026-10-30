import type { MemorySnapshot } from "./types";

// Keep this contract in sync with src-tauri/src/history.rs. These are in-memory
// readings for this app session; selecting a longer range does not invent data.
export const HISTORY_DURATION_MS = 48 * 60 * 60_000;
export const HISTORY_RECENT_DURATION_MS = 60 * 60_000;
export const HISTORY_RECENT_BUCKET_MS = 1000;
export const HISTORY_ARCHIVE_BUCKET_MS = 5 * 60_000;
const ARCHIVE_BUCKET_SAMPLES = 11;
// One reading per second in the recent hour. Each older, UTC-aligned five-minute
// bucket keeps first/last, min/max used bytes for RAM, pagefile and system, and
// the first unavailable reading for each series: at most eleven actual samples.
// Include partial buckets at both cutoffs. The bound is 9,816 full snapshots.
export const HISTORY_LIMIT =
  HISTORY_RECENT_DURATION_MS / HISTORY_RECENT_BUCKET_MS +
  1 +
  (Math.ceil(
    (HISTORY_DURATION_MS - HISTORY_RECENT_DURATION_MS) /
      HISTORY_ARCHIVE_BUCKET_MS,
  ) +
    1) *
    ARCHIVE_BUCKET_SAMPLES;

/** Use the last retained timestamp, including when inspecting a paused chart. */
export function historyGapThreshold(
  previousTimestampMs: number,
  latestTimestampMs: number,
): number {
  return previousTimestampMs < latestTimestampMs - HISTORY_RECENT_DURATION_MS
    ? HISTORY_ARCHIVE_BUCKET_MS + 7500
    : 7500;
}

function archiveBucket(
  history: MemorySnapshot[],
  first: number,
  end: number,
): MemorySnapshot[] {
  const minimum = [-1, -1, -1];
  const maximum = [-1, -1, -1];
  const low = [Infinity, Infinity, Infinity];
  const high = [-Infinity, -Infinity, -Infinity];
  const missing = [-1, -1, -1];
  for (let index = first; index < end; index++) {
    const metrics = history[index].metrics;
    const values = [
      metrics ? metrics.physicalTotal - metrics.physicalAvailable : null,
      metrics?.pagefileUsed ?? null,
      metrics?.systemWorkingSet ?? null,
    ];
    for (let series = 0; series < values.length; series++) {
      const value = values[series];
      if (value === null) {
        if (missing[series] < 0) missing[series] = index;
      } else {
        if (value < low[series]) {
          low[series] = value;
          minimum[series] = index;
        }
        if (value > high[series]) {
          high[series] = value;
          maximum[series] = index;
        }
      }
    }
  }
  return [...new Set([first, end - 1, ...minimum, ...maximum, ...missing])]
    .filter((index) => index >= 0)
    .sort((a, b) => a - b)
    .map((index) => history[index]);
}

/** Compact valid, chronological readings without averaging or changing metrics. */
export function compactHistory(history: MemorySnapshot[]): MemorySnapshot[] {
  if (!history.length) return [];
  const latest = history[history.length - 1].timestampMs;
  const cutoff = latest - HISTORY_DURATION_MS;
  const recent = latest - HISTORY_RECENT_DURATION_MS;
  let first = 0;
  while (history[first].timestampMs < cutoff) first++;
  const retained: MemorySnapshot[] = [];
  let index = first;
  while (index < history.length && history[index].timestampMs < recent) {
    const bucket = Math.floor(
      history[index].timestampMs / HISTORY_ARCHIVE_BUCKET_MS,
    );
    let end = index + 1;
    while (
      end < history.length &&
      history[end].timestampMs < recent &&
      Math.floor(history[end].timestampMs / HISTORY_ARCHIVE_BUCKET_MS) === bucket
    )
      end++;
    retained.push(...archiveBucket(history, index, end));
    index = end;
  }
  // Even a burst in the first second must retain the original first reading.
  if (index === first) retained.push(history[first]);
  while (index < history.length) {
    const bucket = Math.floor(
      history[index].timestampMs / HISTORY_RECENT_BUCKET_MS,
    );
    let end = index + 1;
    while (
      end < history.length &&
      Math.floor(history[end].timestampMs / HISTORY_RECENT_BUCKET_MS) === bucket
    )
      end++;
    const last = history[end - 1];
    if (retained.at(-1) !== last) retained.push(last);
    index = end;
  }
  return retained;
}
