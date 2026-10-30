import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { initializeTheme, useTheme } from "./theme";
import type { ThemePreference } from "./types";

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
  delete document.documentElement.dataset.theme;
});

function systemTheme(initialDark = false) {
  let dark = initialDark;
  const listeners = new Set<() => void>();
  vi.stubGlobal("matchMedia", vi.fn(() => ({
    get matches() { return dark; },
    addEventListener: (_event: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_event: string, listener: () => void) => listeners.delete(listener),
  })));
  return {
    listeners,
    change(value: boolean) {
      act(() => {
        dark = value;
        listeners.forEach((listener) => listener());
      });
    },
  };
}

describe("saved and previewed appearance", () => {
  it("follows Windows appearance changes and removes its listener on unmount", () => {
    const system = systemTheme();
    const { unmount } = renderHook(() => useTheme("system", null));
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    system.change(true);
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(localStorage.getItem("openmemduct-theme")).toBe("system");
    unmount();
    expect(system.listeners.size).toBe(0);
  });

  it("previews without overwriting the saved theme and restores it on cancel", () => {
    const system = systemTheme();
    const { rerender } = renderHook(
      ({ preview }: { preview: ThemePreference | null }) => useTheme("dark", preview),
      { initialProps: { preview: null as ThemePreference | null } },
    );
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    rerender({ preview: "light" });
    system.change(true);
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(localStorage.getItem("openmemduct-theme")).toBe("dark");
    rerender({ preview: null });
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(system.listeners.size).toBe(0);
  });

  it("uses the cached saved theme on startup until native preferences arrive", () => {
    systemTheme();
    localStorage.setItem("openmemduct-theme", "dark");
    initializeTheme();
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    const { rerender } = renderHook(
      ({ saved }: { saved: ThemePreference | undefined }) => useTheme(saved, null),
      { initialProps: { saved: undefined as ThemePreference | undefined } },
    );
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    rerender({ saved: "light" });
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(localStorage.getItem("openmemduct-theme")).toBe("light");
  });

  it("ignores invalid startup cache values and uses the system appearance", () => {
    systemTheme(true);
    localStorage.setItem("openmemduct-theme", "unknown-theme");
    initializeTheme();
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
  });
});
