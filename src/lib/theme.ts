import { useLayoutEffect } from "react";
import type { ThemePreference } from "./types";

const themeCacheKey = "openmemduct-theme";
const darkQuery = "(prefers-color-scheme: dark)";

export function applyTheme(preference: ThemePreference) {
  const dark =
    preference === "dark" ||
    (preference === "system" && window.matchMedia?.(darkQuery).matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

export function initializeTheme() {
  let preference: ThemePreference = "system";
  try {
    const cached = localStorage.getItem(themeCacheKey);
    if (cached === "light" || cached === "dark") preference = cached;
  } catch {
    // Native preferences remain authoritative when browser storage is unavailable.
  }
  applyTheme(preference);
}

export function useTheme(
  preference: ThemePreference | undefined,
  preview: ThemePreference | null,
) {
  useLayoutEffect(() => {
    if (!preference) return;
    try {
      localStorage.setItem(themeCacheKey, preference);
    } catch {
      // Saving the native preferences does not depend on this first-paint cache.
    }
  }, [preference]);

  useLayoutEffect(() => {
    const selected = preview ?? preference;
    if (!selected) return;
    applyTheme(selected);
    if (selected !== "system") return;
    const media = window.matchMedia?.(darkQuery);
    const update = () => applyTheme("system");
    media?.addEventListener("change", update);
    return () => media?.removeEventListener("change", update);
  }, [preference, preview]);
}
