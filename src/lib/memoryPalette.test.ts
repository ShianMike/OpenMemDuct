import { describe, expect, it } from "vitest";
import { memoryRangeColor, memoryRangeStops } from "./memoryPalette";

const ranges = [
  { from: 0, to: 19, color: "#6489af" },
  { from: 20, to: 50, color: "#638f7b" },
  { from: 51, to: 70, color: "#ad914c" },
  { from: 71, to: 89, color: "#bf8559" },
  { from: 90, to: 100, color: "#b86b73" },
] as const;

function rgb(color: string): number[] {
  return [1, 3, 5].map((offset) =>
    Number.parseInt(color.slice(offset, offset + 2), 16),
  );
}

function contrastAgainstWhite(channels: number[]): number {
  const [red, green, blue] = channels.map((channel) => {
    const srgb = channel / 255;
    return srgb <= 0.04045
      ? srgb / 12.92
      : ((srgb + 0.055) / 1.055) ** 2.4;
  });
  const luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
  return 1.05 / (luminance + 0.05);
}

describe("memoryRangeColor", () => {
  it.each(ranges)(
    "keeps $color from $from through $to, including endpoints and fractional interiors",
    ({ from, to, color }) => {
      for (const percent of [from, from + 0.001, (from + to) / 2, to - 0.001, to]) {
        expect(memoryRangeColor(percent), `${percent}%`).toBe(color);
      }
    },
  );

  it.each([
    [19.25, "#648ba2"],
    [19.5, "#648c95"],
    [19.75, "#638e88"],
    [50.25, "#76906f"],
    [50.5, "#889064"],
    [50.75, "#9b9158"],
    [70.25, "#b28e4f"],
    [70.5, "#b68b53"],
    [70.75, "#bb8856"],
    [89.25, "#bd7f60"],
    [89.5, "#bc7866"],
    [89.75, "#ba726d"],
  ])("blends sRGB channels at %s%% and rounds to %s", (percent, expected) => {
    expect(memoryRangeColor(percent)).toBe(expected);
  });

  it.each([
    [-Number.MAX_VALUE, "#6489af"],
    [-0.001, "#6489af"],
    [100.001, "#b86b73"],
    [Number.MAX_VALUE, "#b86b73"],
  ])("clamps the finite value %s to %s", (percent, expected) => {
    expect(memoryRangeColor(percent)).toBe(expected);
  });

  it.each([Number.NaN, Number.NEGATIVE_INFINITY, Number.POSITIVE_INFINITY])(
    "returns neutral for %s",
    (percent) => {
      expect(memoryRangeColor(percent)).toBe("#838b96");
    },
  );
});

describe("memoryRangeStops", () => {
  it("exposes exactly the sorted stops within 0..100, including dusty red at 90", () => {
    expect(memoryRangeStops).toEqual(
      ranges.flatMap(({ from, to, color }) => [
        { percent: from, color },
        { percent: to, color },
      ]),
    );
  });
});

describe("palette legibility against white", () => {
  it("calibrates the independent WCAG calculation with known contrast ratios", () => {
    expect(contrastAgainstWhite(rgb("#ffffff"))).toBeCloseTo(1, 10);
    expect(contrastAgainstWhite(rgb("#000000"))).toBeCloseTo(21, 10);
    expect(contrastAgainstWhite(rgb("#777777"))).toBeCloseTo(4.47808945, 7);
  });

  it.each(ranges)("keeps the $color anchor stroke at least 3:1", ({ from }) => {
    expect(contrastAgainstWhite(rgb(memoryRangeColor(from)))).toBeGreaterThanOrEqual(3);
  });

  it.each(ranges)(
    "keeps $color percentage text mixed with #25262a at least 4.5:1",
    ({ from }) => {
      const base = rgb(memoryRangeColor(from));
      const dark = rgb("#25262a");
      const text = base.map((channel, index) => channel * 0.7 + dark[index] * 0.3);
      expect(contrastAgainstWhite(text)).toBeGreaterThanOrEqual(4.5);
    },
  );
});
