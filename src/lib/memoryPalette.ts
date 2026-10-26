export const memoryRangeStops = [
  { percent: 0, color: "#6489af" },
  { percent: 19, color: "#6489af" },
  { percent: 20, color: "#638f7b" },
  { percent: 50, color: "#638f7b" },
  { percent: 51, color: "#ad914c" },
  { percent: 70, color: "#ad914c" },
  { percent: 71, color: "#bf8559" },
  { percent: 89, color: "#bf8559" },
  { percent: 90, color: "#b86b73" },
  { percent: 100, color: "#b86b73" },
] as const;

export function memoryRangeColor(percent: number): string {
  if (!Number.isFinite(percent)) return "#838b96";
  const value = Math.max(0, Math.min(100, percent));
  for (let i = 1; i < memoryRangeStops.length; i++) {
    const from = memoryRangeStops[i - 1];
    const to = memoryRangeStops[i];
    if (value > to.percent) continue;
    if (from.color === to.color) return from.color;
    const fraction = (value - from.percent) / (to.percent - from.percent);
    return (
      "#" +
      [1, 3, 5]
        .map((offset) => {
          const start = parseInt(from.color.slice(offset, offset + 2), 16);
          const end = parseInt(to.color.slice(offset, offset + 2), 16);
          return Math.round(start + (end - start) * fraction)
            .toString(16)
            .padStart(2, "0");
        })
        .join("")
    );
  }
  return memoryRangeStops.at(-1)!.color;
}
