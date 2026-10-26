type Point = Readonly<{ x: number; y: number }>;

export function cleanupMarkerY(
  segments: readonly (readonly Point[])[],
  x: number,
  fallbackY: number,
): number {
  const left = x - 16;
  const right = x + 16;
  let ceiling = fallbackY;

  for (const segment of segments) {
    for (let i = 0; i < segment.length; i++) {
      const point = segment[i];
      if (point.x >= left && point.x <= right)
        ceiling = Math.min(ceiling, point.y);

      const previous = segment[i - 1];
      if (!previous || previous.x === point.x) continue;

      const start = Math.max(left, Math.min(previous.x, point.x));
      const end = Math.min(right, Math.max(previous.x, point.x));
      if (start > end) continue;

      const dx = point.x - previous.x;
      const dy = point.y - previous.y;
      ceiling = Math.min(
        ceiling,
        previous.y + dy * ((start - previous.x) / dx),
        previous.y + dy * ((end - previous.x) / dx),
      );
    }
  }
  return ceiling;
}
