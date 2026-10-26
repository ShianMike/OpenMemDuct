import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { Preferences } from "./Preferences";
import { createDemoBackend } from "../dev/fixtures";
import { defaultPreferences } from "../lib/types";
import { memoryLevel, preferencesError } from "../lib/preferences";
import defaults from "../../tests/fixtures/preferences.json";

async function setup(preferences = defaultPreferences) {
  const backend = createDemoBackend();
  await backend.savePreferences(preferences);
  const base = await backend.bootstrap();
  const onSaved = vi.fn();
  const onClose = vi.fn();
  const save = vi.spyOn(backend, "savePreferences");
  render(
    <Preferences
      preferences={base.preferences}
      capabilities={base.capabilities}
      backend={backend}
      onSaved={onSaved}
      onClose={onClose}
    />,
  );
  return { backend, onSaved, onClose, save };
}

it("shares the complete native default contract and preserves threshold boundaries", () => {
  expect(defaultPreferences).toEqual(defaults);
  expect(preferencesError(defaultPreferences)).toBeNull();
  expect(memoryLevel(69.99, defaultPreferences)).toBe("normal");
  expect(memoryLevel(70, defaultPreferences)).toBe("warning");
  expect(memoryLevel(90, defaultPreferences)).toBe("danger");
  expect(
    preferencesError({ ...defaultPreferences, autoIntervalMinutes: 0 }),
  ).not.toBeNull();
  expect(
    preferencesError({ ...defaultPreferences, hotkey: "Ctrl+F25" }),
  ).not.toBeNull();
  expect(
    preferencesError({ ...defaultPreferences, hotkey: "Ctrl+Ctrl+F1" }),
  ).not.toBeNull();
});

it.each([
  { startWithWindows: true, startInTray: false },
  { startWithWindows: false, startInTray: true },
])(
  "saves startWithWindows=$startWithWindows without changing startInTray=$startInTray",
  async ({ startWithWindows, startInTray }) => {
    const { backend, onSaved, onClose, save } = await setup({
      ...defaultPreferences,
      startWithWindows: !startWithWindows,
      startInTray,
    });
    const startup = screen.getByRole("switch", { name: "Start with Windows" });
    expect(startup).toHaveProperty("checked", !startWithWindows);
    fireEvent.click(startup);
    expect(startup).toHaveProperty("checked", startWithWindows);
    expect(screen.getByRole("switch", { name: "Start in Tray" })).toHaveProperty(
      "checked",
      startInTray,
    );
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    const expected = { ...defaultPreferences, startWithWindows, startInTray };
    await waitFor(() => expect(onSaved).toHaveBeenCalledWith(expected));
    expect(save).toHaveBeenCalledExactlyOnceWith(expected);
    expect((await backend.bootstrap()).preferences).toEqual(expected);
    expect(onClose).toHaveBeenCalledOnce();
  },
);

it("saves both preference sections together and captures a global shortcut", async () => {
  const { onSaved, onClose } = await setup();
  fireEvent.change(screen.getByLabelText("Warning Level"), {
    target: { value: "72" },
  });
  fireEvent.change(screen.getByLabelText("Danger Level"), {
    target: { value: "92" },
  });
  fireEvent.change(screen.getByLabelText("Tray Click"), {
    target: { value: "none" },
  });
  expect(
    screen.getByRole("combobox", { name: "Tray Scroll-Wheel Click" }),
  ).toBeVisible();
  fireEvent.click(screen.getByLabelText("Show Cleanup Results"));
  expect(screen.getByLabelText("Notification Sound")).toBeDisabled();
  fireEvent.click(screen.getByRole("tab", { name: "Memory" }));
  fireEvent.click(screen.getByLabelText("Clean Every"));
  fireEvent.change(screen.getByLabelText("Cleanup Interval"), {
    target: { value: "120" },
  });
  fireEvent.click(screen.getByLabelText("Enable Cleanup Shortcut"));
  fireEvent.keyDown(screen.getByLabelText("Cleanup Shortcut"), {
    key: "F2",
    code: "F2",
    ctrlKey: true,
    altKey: true,
  });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(onSaved).toHaveBeenCalledWith(
      expect.objectContaining({
        warningPercent: 72,
        dangerPercent: 92,
        singleClickAction: "none",
        showCleanupResults: false,
        autoIntervalEnabled: true,
        autoIntervalMinutes: 120,
        hotkeyEnabled: true,
        hotkey: "Ctrl+Alt+F2",
      }),
    ),
  );
  expect(onClose).toHaveBeenCalledOnce();
  expect(defaultPreferences.warningPercent).toBe(70);
});

it("keeps invalid thresholds and native save failures inside the form", async () => {
  const { onClose, save } = await setup();
  fireEvent.change(screen.getByLabelText("Warning Level"), {
    target: { value: "95" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Warning must be lower",
  );
  expect(save).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("Warning Level"), {
    target: { value: "71" },
  });
  save.mockRejectedValueOnce(new Error("Shortcut is already in use"));
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("already in use");
  expect(onClose).not.toHaveBeenCalled();
});

it("discards unsaved preferences on Cancel", async () => {
  const { backend, onSaved, onClose, save } = await setup();
  fireEvent.click(screen.getByRole("switch", { name: "Start with Windows" }));
  fireEvent.click(screen.getByLabelText("Start in Tray"));
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(save).not.toHaveBeenCalled();
  expect(onSaved).not.toHaveBeenCalled();
  expect((await backend.bootstrap()).preferences).toEqual(defaultPreferences);
  expect(onClose).toHaveBeenCalledOnce();
});

it.each(["Start with Windows", "Start in Tray"])(
  "enables Save only for edits and returns to the unchanged state after reverting %s",
  async (label) => {
    await setup();
    const save = screen.getByRole("button", { name: "Save" });
    expect(screen.getByRole("switch", { name: label })).not.toBeChecked();
    expect(save).toBeDisabled();
    fireEvent.click(screen.getByRole("switch", { name: label }));
    expect(save).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("Unsaved changes");
    fireEvent.click(screen.getByRole("switch", { name: label }));
    expect(save).toBeDisabled();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  },
);

it("supports keyboard tabs and focuses a validation error on the other tab", async () => {
  const { save } = await setup();
  fireEvent.change(screen.getByLabelText("Warning Level"), {
    target: { value: "" },
  });
  fireEvent.keyDown(screen.getByRole("tab", { name: "General" }), {
    key: "ArrowRight",
  });
  expect(screen.getByRole("tab", { name: "Memory" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(screen.getByRole("tab", { name: "Memory" })).toHaveFocus();
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(screen.getByRole("tab", { name: "General" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(screen.getByLabelText("Warning Level")).toHaveFocus();
  expect(screen.getByLabelText("Warning Level")).toHaveAttribute(
    "aria-invalid",
    "true",
  );
  expect(save).not.toHaveBeenCalled();
});

it("lets Escape cancel shortcut recording without changing the saved combination", async () => {
  const { onSaved } = await setup();
  fireEvent.click(screen.getByRole("tab", { name: "Memory" }));
  fireEvent.click(
    screen.getByRole("switch", { name: "Enable Cleanup Shortcut" }),
  );
  const shortcut = screen.getByLabelText("Cleanup Shortcut");
  fireEvent.focus(shortcut);
  expect(shortcut).toHaveValue("Press new shortcut…");
  fireEvent.keyDown(shortcut, { key: "Escape" });
  expect(shortcut).toHaveValue("Ctrl + F1");
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(onSaved).toHaveBeenCalledWith(
      expect.objectContaining({ hotkeyEnabled: true, hotkey: "Ctrl+F1" }),
    ),
  );
});
