export const GiB = 1024 ** 3;

export function formatBytes(bytes: number): string {
  if (bytes >= GiB) return `${(bytes / GiB).toFixed(1)} GiB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MiB`;
  return `${Math.round(bytes / 1024)} KiB`;
}

export function percentage(used: number, total: number): number {
  return total > 0 ? Math.max(0, Math.min(100, (used / total) * 100)) : 0;
}

export function signedBytes(bytes: number): string {
  return `${bytes > 0 ? "+" : bytes < 0 ? "−" : ""}${formatBytes(Math.abs(bytes))}`;
}

export function durationLabel(minutes: number): string {
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);
  const remainder = Math.floor(minutes % 60);
  const part = (value: number, unit: string) =>
    `${value} ${unit}${value === 1 ? "" : "s"}`;
  if (days)
    return [part(days, "day"), hours ? part(hours, "hour") : ""]
      .filter(Boolean)
      .join(" ");
  if (hours)
    return [part(hours, "hour"), remainder ? part(remainder, "minute") : ""]
      .filter(Boolean)
      .join(" ");
  return part(remainder, "minute");
}

export function timeLabel(
  timestamp: number,
  seconds = false,
  date = false,
): string {
  return new Date(timestamp).toLocaleString([], {
    month: date ? "short" : undefined,
    day: date ? "numeric" : undefined,
    hour: "2-digit",
    minute: "2-digit",
    second: seconds ? "2-digit" : undefined,
  });
}
