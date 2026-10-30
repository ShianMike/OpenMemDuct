import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import type {
  Backend,
  Capabilities,
  Preferences as Settings,
  ThemePreference,
  TrayAction,
} from "../lib/types";
import { cleanupOptions } from "../lib/types";
import { preferencesIssue } from "../lib/preferences";

type NumberSetting =
  | "warningPercent"
  | "dangerPercent"
  | "autoThresholdPercent"
  | "autoIntervalMinutes";
type ToggleSetting =
  | "startWithWindows"
  | "startInTray"
  | "autoUpdateEnabled"
  | "showCleanupResults"
  | "notificationSound";

export function Preferences({
  preferences,
  capabilities,
  backend,
  onSaved,
  onClose,
  onSavingChange,
  onThemePreview,
  children,
}: {
  preferences: Settings;
  capabilities: Capabilities;
  backend: Backend;
  onSaved(value: Settings): void;
  onClose(): void;
  onSavingChange?(saving: boolean): void;
  onThemePreview?(theme: ThemePreference | null): void;
  children?: ReactNode;
}) {
  const [draft, setDraft] = useState(() => ({ ...preferences }));
  const [page, setPage] = useState<"general" | "memory">("general");
  const [saving, setSaving] = useState(false);
  const [issue, setIssue] = useState<ReturnType<typeof preferencesIssue>>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [recording, setRecording] = useState(false);
  const form = useRef<HTMLFormElement>(null);
  const id = useId();
  const changed =
    JSON.stringify({
      ...draft,
      cleanupRegions: [...draft.cleanupRegions].sort(),
    }) !==
    JSON.stringify({
      ...preferences,
      cleanupRegions: [...preferences.cleanupRegions].sort(),
    });
  const message = issue?.message ?? saveError;
  const errorId = id + "-error";
  const shortcut = draft.hotkey.replace("Super", "Win").replaceAll("+", " + ");

  useEffect(() => {
    onThemePreview?.(draft.theme);
    return () => onThemePreview?.(null);
  }, [draft.theme, onThemePreview]);

  useEffect(() => {
    if (!issue) return;
    const control = form.current?.querySelector<HTMLInputElement>(
      '[data-setting="' + issue.field + '"]',
    );
    if (control?.disabled) {
      form.current
        ?.querySelector<HTMLInputElement>(
          '[data-enables="' + issue.field + '"]',
        )
        ?.focus();
    } else control?.focus();
  }, [issue, page]);

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setDraft((p) => ({ ...p, [key]: value }));
    setIssue(null);
    setSaveError(null);
  };
  const toggle = (key: ToggleSetting, label: string, disabled = false) => (
    <input
      type="checkbox"
      role="switch"
      className="preference-toggle"
      aria-label={label}
      checked={draft[key]}
      disabled={disabled}
      onChange={(e) => set(key, e.target.checked)}
    />
  );
  const number = (
    key: NumberSetting,
    label: string,
    unit: string,
    min: number,
    max: number,
    disabled = false,
  ) => (
    <span className="number-field">
      <input
        id={id + "-" + key}
        name={key}
        data-setting={key}
        aria-label={label}
        type="number"
        min={min}
        max={max}
        step="1"
        value={Number.isNaN(draft[key]) ? "" : draft[key]}
        disabled={disabled}
        aria-invalid={issue?.field === key || undefined}
        aria-describedby={issue?.field === key ? errorId : undefined}
        onChange={(e) => set(key, e.target.valueAsNumber)}
      />
      <span className="number-unit" aria-hidden="true">
        {unit}
      </span>
      <span className="number-stepper">
        {([1, -1] as const).map((direction) => (
          <button
            key={direction}
            type="button"
            tabIndex={-1}
            data-direction={direction}
            aria-label={
              (direction === 1 ? "Increase " : "Decrease ") +
              label.toLowerCase()
            }
            disabled={
              disabled ||
              (direction === 1 ? draft[key] >= max : draft[key] <= min)
            }
            onPointerDown={(event) => event.preventDefault()}
            onClick={(event) => {
              const input = event.currentTarget
                .closest(".number-field")
                ?.querySelector("input");
              if (!input || input.matches(":disabled")) return;
              if (direction === 1) input.stepUp();
              else input.stepDown();
              set(key, input.valueAsNumber);
              input.focus();
            }}
          />
        ))}
      </span>
    </span>
  );

  return (
    <form
      ref={form}
      className="preferences-form"
      noValidate
      aria-busy={saving}
      onSubmit={async (event) => {
        event.preventDefault();
        const invalid = preferencesIssue(draft);
        setIssue(invalid);
        if (invalid) {
          setPage(
            invalid.field === "warningPercent" ||
              invalid.field === "dangerPercent"
              ? "general"
              : "memory",
          );
          return;
        }
        if (!changed || saving) return;
        setSaveError(null);
        setSaving(true);
        onSavingChange?.(true);
        try {
          onSaved(await backend.savePreferences(draft));
          onClose();
        } catch (error) {
          setSaveError(String(error).replace(/^Error:\s*/, ""));
        } finally {
          setSaving(false);
          onSavingChange?.(false);
        }
      }}
    >
      <header className="preferences-heading">
        <span>Preferences</span>
        <div
          className="preferences-tabs"
          role="tablist"
          aria-label="Preferences sections"
        >
          {(["general", "memory"] as const).map((value) => (
            <button
              key={value}
              id={id + "-" + value}
              type="button"
              role="tab"
              data-page={value}
              aria-selected={page === value}
              aria-controls={id + "-panel"}
              tabIndex={page === value ? 0 : -1}
              disabled={saving}
              onClick={() => {
                setPage(value);
                setRecording(false);
              }}
              onKeyDown={(event) => {
                let next: "general" | "memory";
                if (event.key === "ArrowLeft" || event.key === "ArrowRight")
                  next = page === "general" ? "memory" : "general";
                else if (event.key === "Home") next = "general";
                else if (event.key === "End") next = "memory";
                else return;
                event.preventDefault();
                setPage(next);
                event.currentTarget.parentElement
                  ?.querySelector<HTMLButtonElement>(
                    '[data-page="' + next + '"]',
                  )
                  ?.focus();
              }}
            >
              {value === "general" ? "General" : "Memory"}
            </button>
          ))}
        </div>
      </header>

      <fieldset
        key={page}
        id={id + "-panel"}
        role="tabpanel"
        aria-labelledby={id + "-" + page}
        className="preferences-content"
        disabled={saving}
      >
        {page === "general" ? (
          <>
            <section className="preferences-group" aria-label="Appearance">
              <div className="preferences-group-label">Appearance</div>
              <div
                className="theme-picker"
                role="radiogroup"
                aria-label="Color theme"
              >
                {(["light", "dark", "system"] as const).map((theme) => (
                  <label className="theme-choice" key={theme}>
                    <input
                      className="sr-only"
                      type="radio"
                      name={id + "-theme"}
                      value={theme}
                      checked={draft.theme === theme}
                      onChange={() => set("theme", theme)}
                    />
                    <span
                      className="theme-swatch"
                      data-preview={theme}
                      aria-hidden="true"
                    >
                      <span />
                      <span />
                      <span />
                    </span>
                    <span>
                      {theme === "system"
                        ? "System"
                        : theme === "dark"
                          ? "Dark"
                          : "Light"}
                    </span>
                    <span className="theme-choice-indicator" aria-hidden="true" />
                  </label>
                ))}
              </div>
              <p className="preference-note">
                {draft.theme === "system"
                  ? "Follows your Windows appearance."
                  : draft.theme === "dark"
                    ? "Warm graphite, soft contrast, and a little copper."
                    : "A clear, quiet workspace."}
              </p>
            </section>
            <section className="preferences-group" aria-label="App and Tray">
              <div className="preferences-group-label">App and Tray</div>
              <div className="preferences-rows">
                <label className="preference-row">
                  <span>Start with Windows</span>
                  {toggle("startWithWindows", "Start with Windows")}
                </label>
                <label className="preference-row">
                  <span>Start in Tray</span>
                  {toggle("startInTray", "Start in Tray")}
                </label>
                {(["singleClickAction", "middleClickAction"] as const).map(
                  (key, index) => (
                    <label className="preference-row" key={key}>
                      <span>
                        {index ? "Tray Scroll-Wheel Click" : "Tray Click"}
                      </span>
                      <select
                        aria-label={
                          index ? "Tray Scroll-Wheel Click" : "Tray Click"
                        }
                        value={draft[key]}
                        onChange={(e) => set(key, e.target.value as TrayAction)}
                      >
                        <option value="toggleWindow">Show / Hide</option>
                        <option value="cleanMemory">Clean Memory</option>
                        <option value="none">Do Nothing</option>
                      </select>
                    </label>
                  ),
                )}
              </div>
            </section>
            <section
              className="preferences-group"
              aria-label="Tray Warning Levels"
            >
              <div className="preferences-group-label">Tray Warning Levels</div>
              <div className="preferences-rows">
                <div className="preference-row">
                  <label htmlFor={id + "-warningPercent"}>Warning Level</label>
                  {number("warningPercent", "Warning Level", "%", 1, 99)}
                </div>
                <div className="preference-row">
                  <label htmlFor={id + "-dangerPercent"}>Danger Level</label>
                  {number("dangerPercent", "Danger Level", "%", 2, 100)}
                </div>
              </div>
            </section>
            <section className="preferences-group" aria-label="Notifications">
              <div className="preferences-group-label">Notifications</div>
              <div className="preferences-rows">
                <label className="preference-row">
                  <span>Show Cleanup Results</span>
                  {toggle("showCleanupResults", "Show Cleanup Results")}
                </label>
                <label
                  className="preference-row"
                  data-disabled={!draft.showCleanupResults}
                >
                  <span>Notification Sound</span>
                  {toggle(
                    "notificationSound",
                    "Notification Sound",
                    !draft.showCleanupResults,
                  )}
                </label>
              </div>
            </section>
            <section className="preferences-group" aria-label="Updates">
              <div className="preferences-group-label">Updates</div>
              <div className="preferences-rows">
                <div className="preference-row">
                  <span>App version</span>
                  <span>{backend.appVersion}</span>
                </div>
                <label className="preference-row">
                  <span>Install updates automatically</span>
                  {toggle("autoUpdateEnabled", "Install updates automatically")}
                </label>
              </div>
              <p className="preference-note">
                Check GitHub releases when OpenMemDuct starts and every 6 hours.
                New versions download and install automatically, then restart the
                app.
              </p>
            </section>
            {children && <div className="preferences-preview">{children}</div>}
          </>
        ) : (
          <>
            <section
              className="preferences-group"
              aria-label="Automatic Cleanup"
            >
              <div className="preferences-group-label">Automatic Cleanup</div>
              <div className="preferences-rows">
                <div className="preference-row">
                  <label htmlFor={id + "-threshold"}>Clean When Above</label>
                  <div className="preference-controls">
                    {number(
                      "autoThresholdPercent",
                      "Cleanup Threshold",
                      "%",
                      1,
                      100,
                      !draft.autoThresholdEnabled,
                    )}
                    <input
                      id={id + "-threshold"}
                      data-enables="autoThresholdPercent"
                      className="preference-toggle"
                      role="switch"
                      type="checkbox"
                      checked={draft.autoThresholdEnabled}
                      onChange={(e) =>
                        set("autoThresholdEnabled", e.target.checked)
                      }
                    />
                  </div>
                </div>
                <div className="preference-row">
                  <label htmlFor={id + "-interval"}>Clean Every</label>
                  <div className="preference-controls">
                    {number(
                      "autoIntervalMinutes",
                      "Cleanup Interval",
                      "min",
                      1,
                      1440,
                      !draft.autoIntervalEnabled,
                    )}
                    <input
                      id={id + "-interval"}
                      data-enables="autoIntervalMinutes"
                      className="preference-toggle"
                      role="switch"
                      type="checkbox"
                      checked={draft.autoIntervalEnabled}
                      onChange={(e) =>
                        set("autoIntervalEnabled", e.target.checked)
                      }
                    />
                  </div>
                </div>
              </div>
              {(draft.autoThresholdEnabled || draft.autoIntervalEnabled) &&
                (!capabilities.elevated || !capabilities.cleanupSupported) && (
                  <p className="preference-note">
                    {!capabilities.cleanupSupported
                      ? capabilities.reason
                      : "Paused. OpenMemDuct needs an administrator session for automatic cleanup."}
                  </p>
                )}
            </section>
            <section className="preferences-group" aria-label="Memory Regions">
              <div className="preferences-group-label">
                <span>Memory Regions</span>
                <span className="preference-count">
                  {draft.cleanupRegions.length} selected
                </span>
              </div>
              <div
                className="preferences-regions"
                aria-invalid={issue?.field === "cleanupRegions" || undefined}
              >
                {cleanupOptions.map((option) => (
                  <label key={option.id} title={option.hint}>
                    <input
                      data-setting="cleanupRegions"
                      type="checkbox"
                      checked={draft.cleanupRegions.includes(option.id)}
                      onChange={(e) =>
                        set(
                          "cleanupRegions",
                          e.target.checked
                            ? [...draft.cleanupRegions, option.id]
                            : draft.cleanupRegions.filter(
                                (region) => region !== option.id,
                              ),
                        )
                      }
                    />
                    <span>{option.label}</span>
                  </label>
                ))}
              </div>
              <p className="preference-note">
                Approve cleanup once per app session. Closing the dashboard keeps
                the session active; quitting OpenMemDuct ends it.
              </p>
            </section>
            <section
              className="preferences-group"
              aria-label="Keyboard Shortcut"
            >
              <div className="preferences-group-label">Keyboard Shortcut</div>
              <div className="preferences-rows">
                <div className="preference-row">
                  <label htmlFor={id + "-hotkey"}>Clean Memory</label>
                  <div className="preference-controls">
                    <input
                      className="shortcut-field"
                      data-setting="hotkey"
                      aria-label="Cleanup Shortcut"
                      readOnly
                      disabled={!draft.hotkeyEnabled}
                      value={recording ? "Press new shortcut…" : shortcut}
                      aria-invalid={issue?.field === "hotkey" || undefined}
                      aria-describedby={
                        issue?.field === "hotkey" ? errorId : id + "-recording"
                      }
                      title="Click to record a shortcut"
                      onFocus={() => setRecording(true)}
                      onClick={() => setRecording(true)}
                      onBlur={() => setRecording(false)}
                      onKeyDown={(e) => {
                        if (e.key === "Escape") {
                          if (recording) {
                            e.preventDefault();
                            e.stopPropagation();
                            setRecording(false);
                            if (issue?.field === "hotkey") setIssue(null);
                          }
                          return;
                        }
                        if (e.key === "Tab") return;
                        e.preventDefault();
                        if (["Control", "Alt", "Shift", "Meta"].includes(e.key))
                          return;
                        if (
                          !recording &&
                          (e.key === "Enter" || e.key === " ")
                        ) {
                          setRecording(true);
                          return;
                        }
                        const key = e.code.replace(/^Key|^Digit/, "");
                        if (
                          !(e.ctrlKey || e.altKey || e.metaKey) ||
                          !/^(?:[A-Z0-9]|F(?:[1-9]|1\d|2[0-4]))$/.test(key)
                        ) {
                          setIssue({
                            field: "hotkey",
                            message:
                              "Use Ctrl, Alt, or Windows with a letter, number, or function key.",
                          });
                          return;
                        }
                        set(
                          "hotkey",
                          [
                            e.ctrlKey && "Ctrl",
                            e.altKey && "Alt",
                            e.shiftKey && "Shift",
                            e.metaKey && "Super",
                            key,
                          ]
                            .filter(Boolean)
                            .join("+"),
                        );
                        setRecording(false);
                      }}
                    />
                    <input
                      id={id + "-hotkey"}
                      data-enables="hotkey"
                      aria-label="Enable Cleanup Shortcut"
                      className="preference-toggle"
                      role="switch"
                      type="checkbox"
                      checked={draft.hotkeyEnabled}
                      onChange={(e) => {
                        set("hotkeyEnabled", e.target.checked);
                        setRecording(false);
                      }}
                    />
                  </div>
                </div>
              </div>
              <p id={id + "-recording"} className="preference-note">
                {recording
                  ? "Press a new combination, or Esc to keep " + shortcut + "."
                  : "Click the combination to record. Shortcut actions pause in Preferences."}
              </p>
            </section>
          </>
        )}
      </fieldset>
      <footer className="preferences-footer">
        {message && (
          <div id={errorId} className="preference-feedback" role="alert">
            {message}
          </div>
        )}
        <span className="preference-change-state" role="status">
          {changed ? "Unsaved changes" : ""}
        </span>
        <button
          type="button"
          className="text-button"
          disabled={saving}
          onClick={onClose}
        >
          Cancel
        </button>
        <button
          type="submit"
          className="primary-button"
          disabled={!changed || saving}
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </footer>
    </form>
  );
}
