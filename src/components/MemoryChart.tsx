import { memo, useId, useMemo, useState } from "react";
import type { CleanupReport, MemorySeries, MemorySnapshot } from "../lib/types";
import { formatBytes, percentage, timeLabel } from "../lib/format";
import { cleanupMarkerY } from "../lib/cleanupMarkerY";
import { memoryRangeColor, memoryRangeStops } from "../lib/memoryPalette";

export function seriesValues(
  sample: MemorySnapshot,
  series: MemorySeries,
): { used: number; total: number } | null {
  const m = sample.metrics;
  if (!m) return null;
  if (series === "physical")
    return {
      used: m.physicalTotal - m.physicalAvailable,
      total: m.physicalTotal,
    };
  if (series === "pagefile")
    return m.pagefileTotal === null || m.pagefileUsed === null
      ? null
      : { used: m.pagefileUsed, total: m.pagefileTotal };
  return m.systemWorkingSet === null || m.systemWorkingSetPeak === null
    ? null
    : { used: m.systemWorkingSet, total: m.systemWorkingSetPeak };
}

export function chartMaximum(
  history: MemorySnapshot[],
  start: number,
  end: number,
  series: MemorySeries,
): number {
  let maximum = 0;
  for (const sample of history) {
    if (sample.timestampMs < start || sample.timestampMs > end) continue;
    const values = seriesValues(sample, series);
    if (values)
      maximum = Math.max(
        maximum,
        series === "system" ? values.used : values.total,
      );
  }
  if (series !== "system") return Math.max(1, maximum);
  const mib = Math.max(1, (maximum / 1024 ** 2) * 1.1);
  const magnitude = 10 ** Math.floor(Math.log10(mib));
  const step = [1, 2, 5, 10].find((value) => value * magnitude >= mib)!;
  return step * magnitude * 1024 ** 2;
}

type ChartPoint = { x: number; y: number; sample: MemorySnapshot };

const cleanupLabels = {
  completed: "Cleaned",
  partial: "Partly cleaned",
  cancelled: "Cancelled",
  unsupported: "Unavailable",
  permissionDenied: "Permission denied",
  failed: "Failed",
  unknown: "Result unknown",
};

function changeLabel(before: number, after: number) {
  const change = before - after;
  return change === 0
    ? "No change"
    : `${formatBytes(Math.abs(change))} ${change > 0 ? "less" : "more"}`;
}

export function cleanupMarkers(
  reports: CleanupReport[],
  start: number,
  end: number,
  series: MemorySeries,
  maximum: number,
) {
  const point = (sample: MemorySnapshot | null) => {
    const values = sample && seriesValues(sample, series);
    return sample && values
      ? {
          x: ((sample.timestampMs - start) / Math.max(1, end - start)) * 800,
          y: 160 - Math.min(1, values.used / maximum) * 160,
          used: values.used,
          timestampMs: sample.timestampMs,
        }
      : null;
  };
  return reports.flatMap((report) => {
    const timestamp = report.after?.timestampMs ?? report.startedAtMs;
    if (timestamp < start || timestamp > end) return [];
    const before = point(report.before);
    const after = point(report.after);
    const measured = before !== null && after !== null;
    const delta = measured ? before.used - after.used : null;
    return [
      {
        report,
        before,
        after,
        x: ((timestamp - start) / Math.max(1, end - start)) * 800,
        timestamp,
        change: measured ? changeLabel(before.used, after.used) : null,
        tone:
          report.status !== "completed"
            ? "attention"
            : delta === null || delta === 0
              ? "neutral"
              : delta > 0
                ? "decrease"
                : "increase",
        observations: report.observations.flatMap((observation) => {
          const value = point(observation.snapshot);
          return value && value.timestampMs >= start && value.timestampMs <= end
            ? [{ ...value, atSeconds: observation.atSeconds }]
            : [];
        }),
      },
    ];
  });
}

export function chartSegments(
  history: MemorySnapshot[],
  start: number,
  end: number,
  series: MemorySeries = "physical",
  maximum = chartMaximum(history, start, end, series),
): ChartPoint[][] {
  const segments: ChartPoint[][] = [];
  let current: ChartPoint[] = [];
  let previous: number | null = null;
  for (const sample of history) {
    if (sample.timestampMs < start || sample.timestampMs > end) continue;
    const values = seriesValues(sample, series);
    if (
      !values ||
      (previous !== null && sample.timestampMs - previous > 7500)
    ) {
      if (current.length) segments.push(current);
      current = [];
    }
    if (values)
      current.push({
        x: ((sample.timestampMs - start) / Math.max(1, end - start)) * 800,
        y: 160 - Math.min(1, values.used / maximum) * 160,
        sample,
      });
    previous = sample.timestampMs;
  }
  if (current.length) segments.push(current);
  return segments.map((segment) => {
    if (segment.length <= 1600) return segment;
    // Keep both extrema in each display column so brief spikes remain visible.
    const reduced = [segment[0]];
    for (let first = 0; first < segment.length;) {
      let end = first + 1;
      let minimum = first;
      let maximum = first;
      while (
        end < segment.length &&
        Math.floor(segment[end].x) === Math.floor(segment[first].x)
      ) {
        if (segment[end].y < segment[minimum].y) minimum = end;
        if (segment[end].y > segment[maximum].y) maximum = end;
        end++;
      }
      for (const index of [...new Set([minimum, maximum])].sort(
        (a, b) => a - b,
      )) {
        if (reduced.at(-1) !== segment[index]) reduced.push(segment[index]);
      }
      first = end;
    }
    if (reduced.at(-1) !== segment.at(-1)) reduced.push(segment.at(-1)!);
    return reduced;
  });
}

export const MemoryChart = memo(function MemoryChart({
  history,
  now,
  start,
  minutes,
  series,
  label,
  reports,
  selectedCleanupId,
  onSelectCleanup,
  expandedCleanupId,
  onCleanupDetailsChange,
}: {
  history: MemorySnapshot[];
  now: number;
  start: number;
  minutes: number;
  series: MemorySeries;
  label: string;
  reports: CleanupReport[];
  selectedCleanupId: string | null | undefined;
  onSelectCleanup: (id: string) => void;
  expandedCleanupId: string | null;
  onCleanupDetailsChange: (series: MemorySeries, id: string | null) => void;
}) {
  const tooltipId = useId();
  const { maximum, segments, paths, points, markers } = useMemo(() => {
    const readings = [
      ...history,
      ...reports.flatMap((report) => [
        ...(report.before ? [report.before] : []),
        ...(report.after ? [report.after] : []),
        ...report.observations.map((observation) => observation.snapshot),
      ]),
    ];
    const maximum = chartMaximum(readings, start, now, series);
    const segments = chartSegments(history, start, now, series, maximum);
    return {
      maximum,
      segments,
      markers: cleanupMarkers(reports, start, now, series, maximum).map(
        (marker) => ({
          ...marker,
          ceiling: marker.after
            ? cleanupMarkerY(
                segments,
                marker.x,
                Math.min(marker.before?.y ?? marker.after.y, marker.after.y),
              )
            : null,
        }),
      ),
      points: segments.flat(),
      paths: segments.map((segment) =>
        segment
          .map(
            (point, i) =>
              (i ? "L" : "M") + point.x.toFixed(2) + "," + point.y.toFixed(2),
          )
          .join(" "),
      ),
    };
  }, [history, start, now, series, reports]);
  const active =
    markers.find((marker) => marker.report.id === selectedCleanupId) ??
    markers[0];
  const observation = active?.observations.at(-1);
  const annotationX = active ? active.x / 8 : 0;
  const annotationLeft = Math.max(
    16,
    Math.min(
      84,
      annotationX +
        (active?.ceiling !== null &&
        active?.ceiling !== undefined &&
        active.ceiling < 24
          ? annotationX < 34
            ? 18
            : -18
          : 0),
    ),
  );
  const incomplete =
    active?.report.results.filter((result) => result.status !== "completed")
      .length ?? 0;
  const [cursorX, setCursorX] = useState<number | null>(null);
  const selectedIndex =
    cursorX === null
      ? -1
      : points.reduce(
          (best, point, index) =>
            best < 0 ||
            Math.abs(point.x - cursorX) < Math.abs(points[best].x - cursorX)
              ? index
              : best,
          -1,
        );
  const selected = points[selectedIndex];
  const selectedValues = selected
    ? seriesValues(selected.sample, series)
    : null;
  const endpoint = points.at(-1);
  const latest = endpoint ? seriesValues(endpoint.sample, series) : null;
  // Color follows capacity or observed peak, independently of the plot's byte-axis zoom.
  const colorReference = latest && latest.total > 0 ? latest.total : null;
  const colorGradient = `url(#${tooltipId}-range)`;
  return (
    <div className="chart-wrap">
      <div className="chart-scale" aria-hidden="true">
        <span>{maximum > 1 ? formatBytes(maximum) : "—"}</span>
        <span>{maximum > 1 ? formatBytes(maximum / 2) : "—"}</span>
        <span>0</span>
      </div>
      <div
        className="chart-plot"
        tabIndex={0}
        role="group"
        aria-label={"Inspect " + label.toLowerCase() + " history"}
        aria-description="Use left and right arrows to inspect readings. Home and End select the first and last reading."
        aria-describedby={selected ? tooltipId : undefined}
        onFocus={(event) => {
          if (event.target === event.currentTarget) setCursorX(800);
        }}
        onBlur={() => setCursorX(null)}
        onKeyDown={(event) => {
          if (event.target !== event.currentTarget) return;
          if (!points.length) return;
          let index = selectedIndex < 0 ? points.length - 1 : selectedIndex;
          if (event.key === "ArrowLeft") index = Math.max(0, index - 1);
          else if (event.key === "ArrowRight")
            index = Math.min(points.length - 1, index + 1);
          else if (event.key === "Home") index = 0;
          else if (event.key === "End") index = points.length - 1;
          else if (event.key === "Escape") {
            setCursorX(null);
            return;
          } else return;
          event.preventDefault();
          setCursorX(points[index].x);
        }}
        onPointerMove={(event) => {
          if ((event.target as Element).closest("button, .cleanup-inspector")) {
            setCursorX(null);
            return;
          }
          const bounds = event.currentTarget.getBoundingClientRect();
          setCursorX(
            Math.max(
              0,
              Math.min(
                800,
                Math.round(
                  ((event.clientX - bounds.left) / bounds.width) * 800,
                ),
              ),
            ),
          );
        }}
        onPointerLeave={() => setCursorX(null)}
      >
        <svg
          viewBox="0 0 800 160"
          preserveAspectRatio="none"
          role="img"
          aria-label={
            label +
            " over " +
            minutes +
            " minutes. " +
            (latest
              ? formatBytes(latest.used) +
                " of " +
                formatBytes(latest.total) +
                (series === "system" ? " observed peak" : "") +
                "."
              : "Waiting for readings.")
          }
        >
          <defs>
            <linearGradient
              id={tooltipId + "-range"}
              gradientUnits="userSpaceOnUse"
              x1="0"
              x2="0"
              y1="160"
              y2={160 - ((colorReference ?? maximum) / maximum) * 160}
              colorInterpolation="sRGB"
            >
              {memoryRangeStops.map((stop) => (
                <stop
                  key={stop.percent}
                  offset={`${stop.percent}%`}
                  stopColor={stop.color}
                />
              ))}
            </linearGradient>
          </defs>
          {[0, 80, 160].map((y) => (
            <line
              key={y}
              x1="0"
              x2="800"
              y1={y}
              y2={y}
              stroke="var(--grid)"
              vectorEffect="non-scaling-stroke"
            />
          ))}
          {segments.map((segment, i) => (
            <g key={i}>
              <path
                d={
                  paths[i] +
                  " L" +
                  segment.at(-1)!.x.toFixed(2) +
                  ",160 L" +
                  segment[0].x.toFixed(2) +
                  ",160 Z"
                }
                fill={colorReference ? colorGradient : "#838b96"}
                fillOpacity="0.045"
              />
              <path
                d={paths[i]}
                fill="none"
                stroke={colorReference ? colorGradient : "#838b96"}
                strokeWidth="1.65"
                strokeLinejoin="round"
                vectorEffect="non-scaling-stroke"
              />
            </g>
          ))}
          {selected && (
            <>
              <line
                x1={selected.x}
                x2={selected.x}
                y1="0"
                y2="160"
                stroke="var(--crosshair)"
                vectorEffect="non-scaling-stroke"
              />
              <circle
                cx={selected.x}
                cy={selected.y}
                r="3.5"
                fill={
                  colorReference
                    ? memoryRangeColor(
                        percentage(selectedValues!.used, colorReference),
                      )
                    : "#838b96"
                }
                stroke="white"
                strokeWidth="1.5"
                vectorEffect="non-scaling-stroke"
              />
            </>
          )}
        </svg>
        {markers.map((marker) => (
          <button
            key={marker.report.id}
            className="cleanup-marker-hit"
            data-tone={marker.tone}
            style={{
              left: `calc(${marker.x / 8}% - 12px)`,
              top:
                marker.ceiling !== null
                  ? `calc(${marker.ceiling / 1.6}% - 28px)`
                  : "0px",
            }}
            aria-label={`Inspect cleanup at ${timeLabel(marker.timestamp, true)}: ${cleanupLabels[marker.report.status]}${marker.change ? ", " + marker.change : ""}`}
            aria-pressed={marker === active}
            onClick={() => {
              onSelectCleanup(marker.report.id);
              setCursorX(null);
            }}
          >
            <span className="cleanup-chevron" aria-hidden="true" />
          </button>
        ))}
        {active && (
          <div
            className="cleanup-annotation"
            data-cleanup-details-open={expandedCleanupId === active.report.id}
            data-tone={active.tone}
            style={{ left: `${annotationLeft}%` }}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.stopPropagation();
                onCleanupDetailsChange(series, null);
              }
            }}
          >
            <button
              className="cleanup-label"
              aria-label={`${cleanupLabels[active.report.status]}${active.change ? " " + active.change : ""}`}
              aria-expanded={expandedCleanupId === active.report.id}
              aria-controls={tooltipId + "-cleanup"}
              onClick={() => {
                setCursorX(null);
                onCleanupDetailsChange(series, active.report.id);
              }}
            >
              <span>{cleanupLabels[active.report.status]}</span>
              {active.change && <strong>{active.change}</strong>}
            </button>
            {expandedCleanupId === active.report.id && (
              <div
                className="cleanup-inspector"
                id={tooltipId + "-cleanup"}
                role="region"
                aria-label={`${label} cleanup details`}
              >
                <div className="cleanup-inspector-meta">
                  <span>{timeLabel(active.timestamp, true)}</span>
                  {active.report.durationMs !== null && (
                    <span>{active.report.durationMs.toLocaleString()} ms</span>
                  )}
                </div>
                {(active.before || active.after) && (
                  <div className="cleanup-comparison">
                    <span>
                      Before
                      <strong>
                        {active.before ? formatBytes(active.before.used) : "—"}
                      </strong>
                    </span>
                    <span>
                      After
                      <strong>
                        {active.after ? formatBytes(active.after.used) : "—"}
                      </strong>
                    </span>
                  </div>
                )}
                {observation && (
                  <div className="cleanup-followup">
                    <span>After {observation.atSeconds}s</span>
                    <strong>
                      {active.before
                        ? changeLabel(active.before.used, observation.used)
                        : `${formatBytes(observation.used)} used`}
                    </strong>
                  </div>
                )}
                {active.report.status !== "completed" && (
                  <p
                    className="cleanup-message"
                    title={active.report.errorCode ?? undefined}
                  >
                    {active.report.status === "partial" && incomplete > 0
                      ? `${incomplete} of ${active.report.results.length} operations did not complete.`
                      : active.report.message}
                  </p>
                )}
              </div>
            )}
          </div>
        )}
        {series === "physical" && (
          <span className="sr-only" role="status">
            {active
              ? `${cleanupLabels[active.report.status]}${active.change ? ": " + active.change : ""}`
              : ""}
          </span>
        )}
        {!points.length && (
          <span className="chart-empty">Waiting for readings</span>
        )}
        {selected && selectedValues && (
          <div
            id={tooltipId}
            role="tooltip"
            className="chart-tooltip"
            style={{ left: Math.max(15, Math.min(85, selected.x / 8)) + "%" }}
          >
            <strong>{formatBytes(selectedValues.used)}</strong>
            <span>{timeLabel(selected.sample.timestampMs, true)}</span>
          </div>
        )}
      </div>
      <div className="chart-times" aria-hidden="true">
        {[0, 0.25, 0.5, 0.75, 1].map((fraction) => (
          <span key={fraction}>
            {fraction === 1
              ? "Now"
              : timeLabel(
                  start + (now - start) * fraction,
                  now - start <= 120_000,
                )}
          </span>
        ))}
      </div>
    </div>
  );
});
