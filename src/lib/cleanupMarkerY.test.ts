import { describe, expect, it } from "vitest";
import { cleanupMarkerY } from "./cleanupMarkerY";

describe("cleanupMarkerY", () => {
  it("preserves a higher fallback ceiling, including without curve data", () => {
    const segments = [
      [
        { x: 384, y: 70 },
        { x: 416, y: 90 },
      ],
    ] as const;
    expect(cleanupMarkerY(segments, 400, 20)).toBe(20);
    expect(cleanupMarkerY([], 400, 20)).toBe(20);
    expect(cleanupMarkerY([[]], 400, 20)).toBe(20);
  });

  it.each([
    [399.999, 12, 150],
    [399.999, 150, 12],
    [400, 12, 150],
    [400, 150, 12],
  ])("clears a jump from (%s, %s) to (400, %s)", (startX, beforeY, afterY) => {
    expect(
      cleanupMarkerY(
        [
          [
            { x: startX, y: beforeY },
            { x: 400, y: afterY },
          ],
        ],
        400,
        100,
      ),
    ).toBe(12);
  });

  it("clears a neighboring peak without taking a distant endpoint's height", () => {
    expect(
      cleanupMarkerY(
        [
          [
            { x: 380, y: 1 },
            { x: 390, y: 80 },
            { x: 400, y: 100 },
            { x: 412, y: 20 },
            { x: 420, y: 80 },
          ],
        ],
        400,
        120,
      ),
    ).toBe(20);
  });

  it.each([
    [0, 100],
    [100, 0],
  ])(
    "clips sparse edges from y=%s to y=%s at both footprint boundaries",
    (fromY, toY) => {
      const segment = [
        { x: 360, y: fromY },
        { x: 440, y: toY },
      ];
      expect(cleanupMarkerY([segment], 400, 150)).toBeCloseTo(30);
      expect(cleanupMarkerY([[...segment].reverse()], 400, 150)).toBeCloseTo(
        30,
      );
    },
  );

  it("keeps disconnected segments and empty segments from bridging a gap", () => {
    expect(
      cleanupMarkerY(
        [
          [
            { x: 360, y: 0 },
            { x: 380, y: 20 },
          ],
          [],
          [
            { x: 420, y: 30 },
            { x: 440, y: 0 },
          ],
        ],
        400,
        90,
      ),
    ).toBe(90);
  });

  it.each([384, 400, 416])(
    "includes a lone point at x=%s and excludes points outside the footprint",
    (pointX) => {
      expect(
        cleanupMarkerY(
          [[{ x: 383, y: 0 }], [{ x: pointX, y: 40 }], [{ x: 417, y: 0 }]],
          400,
          100,
        ),
      ).toBe(40);
    },
  );
});
