import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { Preferences } from "./Preferences";
import { createDemoBackend } from "../dev/fixtures";

async function setup() {
  const backend = createDemoBackend();
  const base = await backend.bootstrap();
  const onSaved = vi.fn();
  const onClose = vi.fn();
  const save = vi.spyOn(backend, "savePreferences");
  const { container } = render(
    <Preferences
      preferences={base.preferences}
      capabilities={base.capabilities}
      backend={backend}
      onSaved={onSaved}
      onClose={onClose}
    />,
  );
  return { backend, onSaved, onClose, save, container };
}

it.each([
  { label: "Warning Level", value: 70 },
  { label: "Danger Level", value: 90 },
])(
  "steps $label in both directions without submitting",
  async ({ label, value }) => {
    const { save, onSaved, onClose } = await setup();
    const input = screen.getByRole("spinbutton", { name: label });
    const increase = screen.getByRole("button", {
      name: "Increase " + label.toLowerCase(),
    });
    const decrease = screen.getByRole("button", {
      name: "Decrease " + label.toLowerCase(),
    });
    const saveButton = screen.getByRole("button", { name: "Save" });

    expect(saveButton).toBeDisabled();
    fireEvent.pointerDown(increase);
    fireEvent.click(increase);
    expect(input).toHaveValue(value + 1);
    expect(input).toHaveFocus();
    expect(saveButton).toBeEnabled();
    fireEvent.pointerDown(decrease);
    fireEvent.click(decrease);
    expect(input).toHaveValue(value);
    expect(input).toHaveFocus();
    expect(saveButton).toBeDisabled();
    expect(save).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  },
);

it("persists the draft number changed by a stepper when Save is clicked", async () => {
  const { backend, save, onSaved, onClose } = await setup();
  fireEvent.click(
    screen.getByRole("button", { name: "Increase warning level" }),
  );
  expect(save).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Save" }));

  await waitFor(() =>
    expect(onSaved).toHaveBeenCalledWith(
      expect.objectContaining({ warningPercent: 71, dangerPercent: 90 }),
    ),
  );
  expect(save).toHaveBeenCalledOnce();
  expect(onClose).toHaveBeenCalledOnce();
  expect((await backend.bootstrap()).preferences.warningPercent).toBe(71);
});

it.each([
  { label: "Warning Level", min: 1, max: 99, enabledBy: null },
  { label: "Danger Level", min: 2, max: 100, enabledBy: null },
  {
    label: "Cleanup Threshold",
    min: 1,
    max: 100,
    enabledBy: "Clean When Above",
  },
  { label: "Cleanup Interval", min: 1, max: 1440, enabledBy: "Clean Every" },
])(
  "keeps $label within its native boundaries",
  async ({ label, min, max, enabledBy }) => {
    const { save } = await setup();
    if (enabledBy) {
      fireEvent.click(screen.getByRole("tab", { name: "Memory" }));
      fireEvent.click(screen.getByRole("switch", { name: enabledBy }));
    }
    const input = screen.getByRole("spinbutton", { name: label });
    const increase = screen.getByRole("button", {
      name: "Increase " + label.toLowerCase(),
    });
    const decrease = screen.getByRole("button", {
      name: "Decrease " + label.toLowerCase(),
    });

    expect(input).toHaveAttribute("type", "number");
    expect(input).toHaveAttribute("min", String(min));
    expect(input).toHaveAttribute("max", String(max));
    expect(increase).toHaveAttribute("type", "button");
    expect(decrease).toHaveAttribute("type", "button");
    fireEvent.change(input, { target: { value: String(min + 1) } });
    fireEvent.click(decrease);
    expect(input).toHaveValue(min);
    expect(decrease).toBeDisabled();
    expect(increase).toBeEnabled();
    fireEvent.click(decrease);
    expect(input).toHaveValue(min);

    fireEvent.change(input, { target: { value: String(max - 1) } });
    fireEvent.click(increase);
    expect(input).toHaveValue(max);
    expect(increase).toBeDisabled();
    expect(decrease).toBeEnabled();
    fireEvent.click(increase);
    expect(input).toHaveValue(max);
    expect(save).not.toHaveBeenCalled();
  },
);

it("recovers an empty number through its stepper and clears validation", async () => {
  const { save, onSaved } = await setup();
  const input = screen.getByRole("spinbutton", { name: "Warning Level" });
  fireEvent.change(input, { target: { value: "" } });
  expect(input).toHaveValue(null);
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(screen.getByRole("alert")).toBeInTheDocument();
  expect(input).toHaveAttribute("aria-invalid", "true");
  expect(save).not.toHaveBeenCalled();

  fireEvent.click(
    screen.getByRole("button", { name: "Increase warning level" }),
  );
  expect(input).toHaveValue(1);
  expect(input).toHaveFocus();
  expect(input).not.toHaveAttribute("aria-invalid");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(onSaved).toHaveBeenCalledWith(
      expect.objectContaining({ warningPercent: 1 }),
    ),
  );
});

it("enables automatic cleanup steppers independently and caps the interval at 1440", async () => {
  const { save } = await setup();
  fireEvent.click(screen.getByRole("tab", { name: "Memory" }));
  const threshold = screen.getByRole("spinbutton", {
    name: "Cleanup Threshold",
  });
  const interval = screen.getByRole("spinbutton", { name: "Cleanup Interval" });
  const thresholdUp = screen.getByRole("button", {
    name: "Increase cleanup threshold",
  });
  const thresholdDown = screen.getByRole("button", {
    name: "Decrease cleanup threshold",
  });
  const intervalUp = screen.getByRole("button", {
    name: "Increase cleanup interval",
  });
  const intervalDown = screen.getByRole("button", {
    name: "Decrease cleanup interval",
  });

  expect(threshold).toBeDisabled();
  expect(interval).toBeDisabled();
  for (const button of [thresholdUp, thresholdDown, intervalUp, intervalDown]) {
    expect(button).toBeDisabled();
    fireEvent.click(button);
  }
  expect(threshold).toHaveValue(80);
  expect(interval).toHaveValue(90);
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

  fireEvent.click(screen.getByRole("switch", { name: "Clean When Above" }));
  expect(threshold).toBeEnabled();
  expect(thresholdUp).toBeEnabled();
  expect(thresholdDown).toBeEnabled();
  expect(interval).toBeDisabled();
  expect(intervalUp).toBeDisabled();
  expect(intervalDown).toBeDisabled();
  fireEvent.click(thresholdUp);
  expect(threshold).toHaveValue(81);
  expect(threshold).toHaveFocus();
  fireEvent.click(thresholdDown);
  expect(threshold).toHaveValue(80);

  fireEvent.click(screen.getByRole("switch", { name: "Clean Every" }));
  expect(interval).toBeEnabled();
  expect(intervalUp).toBeEnabled();
  expect(intervalDown).toBeEnabled();
  fireEvent.change(interval, { target: { value: "1439" } });
  fireEvent.click(intervalUp);
  expect(interval).toHaveValue(1440);
  expect(interval).toHaveFocus();
  expect(intervalUp).toBeDisabled();
  fireEvent.click(intervalUp);
  expect(interval).toHaveValue(1440);
  fireEvent.click(intervalDown);
  expect(interval).toHaveValue(1439);
  expect(intervalUp).toBeEnabled();

  fireEvent.click(screen.getByRole("switch", { name: "Clean When Above" }));
  fireEvent.click(screen.getByRole("switch", { name: "Clean Every" }));
  for (const control of [
    threshold,
    interval,
    thresholdUp,
    thresholdDown,
    intervalUp,
    intervalDown,
  ]) {
    expect(control).toBeDisabled();
  }
  expect(save).not.toHaveBeenCalled();
});

it.each(["General", "Memory"])(
  "disables every %s control while saving",
  async (page) => {
    const { container, save, onSaved, onClose } = await setup();
    let finish: () => void = () => {};
    const pending = new Promise<void>((resolve) => {
      finish = resolve;
    });
    save.mockImplementationOnce(async (value) => {
      await pending;
      return value;
    });
    if (page === "Memory") {
      fireEvent.click(screen.getByRole("tab", { name: "Memory" }));
      fireEvent.click(screen.getByRole("switch", { name: "Clean When Above" }));
      fireEvent.click(screen.getByRole("switch", { name: "Clean Every" }));
    }
    const input = screen.getByRole("spinbutton", {
      name: page === "General" ? "Warning Level" : "Cleanup Interval",
    });
    const increase = screen.getByRole("button", {
      name:
        page === "General"
          ? "Increase warning level"
          : "Increase cleanup interval",
    });
    fireEvent.click(increase);
    const editedValue = (input as HTMLInputElement).valueAsNumber;
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(screen.getByRole("button", { name: "Saving…" })).toBeDisabled();
    expect(input.closest("form")).toHaveAttribute("aria-busy", "true");
    for (const control of container.querySelectorAll("button, input, select")) {
      expect(control).toBeDisabled();
    }
    fireEvent.click(increase);
    expect(input).toHaveValue(editedValue);
    expect(save).toHaveBeenCalledOnce();
    expect(onSaved).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();

    finish();
    await waitFor(() => expect(onSaved).toHaveBeenCalledOnce());
    expect(onClose).toHaveBeenCalledOnce();
    expect(input).toBeEnabled();
  },
);
