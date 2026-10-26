import { useState, type ReactNode } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function WindowBar({ children }: { children?: ReactNode }) {
  const native = isTauri();
  const [error, setError] = useState<string | null>(null);

  async function control(action: "close" | "minimize") {
    try {
      setError(null);
      await getCurrentWindow()[action]();
    } catch (error) {
      setError(String(error));
    }
  }

  return (
    <header className="window-bar">
      <div
        className="window-controls"
        role="group"
        aria-label="Window Controls"
      >
        <button
          className="window-close"
          aria-label="Close Window"
          title={native ? "Close Window" : "Available in the desktop app"}
          disabled={!native}
          onClick={() => void control("close")}
        >
          <span aria-hidden="true" />
        </button>
        <button
          className="window-minimize"
          aria-label="Minimize Window"
          title={native ? "Minimize Window" : "Available in the desktop app"}
          disabled={!native}
          onClick={() => void control("minimize")}
        >
          <span aria-hidden="true" />
        </button>
      </div>
      <div className="window-drag" data-tauri-drag-region />
      {children}
      {error && (
        <span className="window-error" role="alert">
          {error}
        </span>
      )}
    </header>
  );
}
