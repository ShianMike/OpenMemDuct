import type { Preferences } from "./types";

export function preferencesIssue(
  p: Preferences,
): { field: keyof Preferences; message: string } | null {
  const modifiers = p.hotkey.split("+").slice(0, -1);
  if (new Set(modifiers).size !== modifiers.length)
    return { field: "hotkey", message: "Use each shortcut modifier once." };
  if (
    (p.autoThresholdEnabled || p.autoIntervalEnabled) &&
    !p.cleanupRegions.length
  )
    return {
      field: "cleanupRegions",
      message: "Select at least one cleanup region for automatic cleanup.",
    };
  if (
    !Number.isInteger(p.warningPercent) ||
    !Number.isInteger(p.dangerPercent) ||
    p.warningPercent < 1 ||
    p.dangerPercent > 100 ||
    p.warningPercent >= p.dangerPercent
  )
    return {
      field:
        !Number.isInteger(p.dangerPercent) || p.dangerPercent > 100
          ? "dangerPercent"
          : "warningPercent",
      message: "Warning must be lower than danger, between 1% and 100%.",
    };
  if (
    !Number.isInteger(p.autoThresholdPercent) ||
    p.autoThresholdPercent < 1 ||
    p.autoThresholdPercent > 100
  )
    return {
      field: "autoThresholdPercent",
      message: "Choose a cleanup threshold between 1% and 100%.",
    };
  if (
    !Number.isInteger(p.autoIntervalMinutes) ||
    p.autoIntervalMinutes < 1 ||
    p.autoIntervalMinutes > 1440
  )
    return {
      field: "autoIntervalMinutes",
      message: "Choose a cleanup interval between 1 and 1,440 minutes.",
    };
  if (
    !/^(?:(?:Ctrl|Alt|Shift|Super)\+)+(?:[A-Z0-9]|F(?:[1-9]|1\d|2[0-4]))$/.test(
      p.hotkey,
    ) ||
    !/(?:Ctrl|Alt|Super)\+/.test(p.hotkey)
  )
    return {
      field: "hotkey",
      message:
        "Use Ctrl, Alt, or Windows with a letter, number, or function key.",
    };
  return null;
}

export function preferencesError(p: Preferences): string | null {
  return preferencesIssue(p)?.message ?? null;
}

export function memoryLevel(
  percent: number,
  preferences: Preferences,
): "normal" | "warning" | "danger" {
  return percent >= preferences.dangerPercent
    ? "danger"
    : percent >= preferences.warningPercent
      ? "warning"
      : "normal";
}
