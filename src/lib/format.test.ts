import { describe, expect, it } from "vitest";
import { durationLabel, timeLabel } from "./format";

describe("chart duration and time labels", () => {
  it("formats elapsed recording time in minutes, hours and days", () => {
    expect(durationLabel(1)).toBe("1 minute");
    expect(durationLabel(30)).toBe("30 minutes");
    expect(durationLabel(61)).toBe("1 hour 1 minute");
    expect(durationLabel(960)).toBe("16 hours");
    expect(durationLabel(1500)).toBe("1 day 1 hour");
    expect(durationLabel(2880)).toBe("2 days");
  });

  it("distinguishes the same clock time on separate days for long chart ranges", () => {
    const firstDay = new Date(2026, 9, 28, 14, 30).getTime();
    const secondDay = new Date(2026, 9, 29, 14, 30).getTime();
    expect(timeLabel(firstDay)).toBe(timeLabel(secondDay));
    expect(timeLabel(firstDay, false, true))
      .not.toBe(timeLabel(secondDay, false, true));
  });
});
