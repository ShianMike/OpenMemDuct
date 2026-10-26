import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { createDemoBackend } from "./dev/fixtures";
import { createTelemetryStore } from "./lib/telemetry";
import { MemoryChart } from "./components/MemoryChart";
import type { CleanupReport } from "./lib/types";

beforeAll(() => {
  // jsdom has no native dialog focus implementation; browser checks cover the modal.
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});

function setup() {
  const backend = createDemoBackend();
  const store = createTelemetryStore(backend);
  const view = render(<App backend={backend} store={store} />);
  return { backend, store, ...view };
}

describe("minimal dashboard", () => {
  it("colors system working-set readings against the observed peak independently of chart zoom", async () => {
    const base = (await createDemoBackend().bootstrap()).snapshot!;
    const sample = {
      ...base,
      metrics: { ...base.metrics!, systemWorkingSet: 150 * 1024 ** 2 },
    };
    const { container } = render(
      <MemoryChart
        history={[
          { ...sample, timestampMs: sample.timestampMs - 1000 },
          sample,
        ]}
        start={sample.timestampMs - 60_000}
        now={sample.timestampMs}
        minutes={5}
        series="system"
        label="System Working Set"
        reports={[]}
        selectedCleanupId={null}
        onSelectCleanup={() => {}}
        expandedCleanupId={null}
        onCleanupDetailsChange={() => {}}
      />,
    );
    const gradient = container.querySelector("linearGradient")!;
    expect(gradient).toHaveAttribute("gradientUnits", "userSpaceOnUse");
    expect(Number(gradient.getAttribute("y2"))).toBeCloseTo(
      160 - (462 / 200) * 160,
    );
    expect(container.querySelector('path[fill="none"]')).toHaveAttribute(
      "stroke",
      `url(#${gradient.id})`,
    );
    const plot = screen.getByRole("group", {
      name: "Inspect system working set history",
    });
    fireEvent.focus(plot);
    expect(container.querySelector("circle")).toHaveAttribute(
      "fill",
      "#638f7b",
    );
  });
  it("keeps an off pagefile neutral instead of giving a zero-capacity gradient a severity color", async () => {
    const base = (await createDemoBackend().bootstrap()).snapshot!;
    const sample = {
      ...base,
      metrics: { ...base.metrics!, pagefileUsed: 0, pagefileTotal: 0 },
    };
    const { container } = render(
      <MemoryChart
        history={[sample]}
        start={sample.timestampMs - 60_000}
        now={sample.timestampMs}
        minutes={5}
        series="pagefile"
        label="Pagefile"
        reports={[]}
        selectedCleanupId={null}
        onSelectCleanup={() => {}}
        expandedCleanupId={null}
        onCleanupDetailsChange={() => {}}
      />,
    );
    expect(container.querySelector('path[fill="none"]')).toHaveAttribute(
      "stroke",
      "#838b96",
    );
  });
  it("keeps known follow-ups in compact details when a cleanup counter was temporarily unavailable", async () => {
    const before = (await createDemoBackend().bootstrap()).snapshot!;
    const after = {
      ...before,
      timestampMs: before.timestampMs + 1000,
      metrics: { ...before.metrics!, pagefileUsed: null, pagefileTotal: null },
    };
    const followup = { ...before, timestampMs: after.timestampMs + 16_000 };
    const report: CleanupReport = {
      id: "missing-counter",
      regions: [],
      results: [],
      status: "completed",
      before,
      after,
      startedAtMs: before.timestampMs,
      durationMs: 1000,
      message: "",
      errorCode: null,
      observations: [{ atSeconds: 16, snapshot: followup }],
    };
    const { container } = render(
      <MemoryChart
        history={[before]}
        start={before.timestampMs - 60_000}
        now={followup.timestampMs}
        minutes={5}
        series="pagefile"
        label="Pagefile"
        reports={[report]}
        selectedCleanupId={report.id}
        onSelectCleanup={() => {}}
        expandedCleanupId={report.id}
        onCleanupDetailsChange={() => {}}
      />,
    );
    expect(
      screen.getByRole("button", { name: /Inspect cleanup/ }),
    ).toBeVisible();
    expect(container.querySelectorAll("svg circle")).toHaveLength(0);
    expect(container.querySelectorAll("svg line")).toHaveLength(3);
    expect(screen.getByRole("button", { name: "Cleaned" })).toBeVisible();
    expect(screen.getByText("After 16s")).toBeVisible();
    expect(screen.getByText("No change")).toBeVisible();
  });
  it("keeps the preferences dialog open while native saving is in progress", async () => {
    const { backend } = setup();
    let finish: () => void = () => {};
    const pending = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const original = backend.savePreferences;
    backend.savePreferences = async (value) => {
      await pending;
      return original(value);
    };
    await screen.findByRole("region", { name: "Physical Memory" });
    fireEvent.click(screen.getByRole("button", { name: "Open Preferences" }));
    fireEvent.click(
      await screen.findByRole("switch", { name: "Start in Tray" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    const dialog = screen.getByRole("dialog", { name: "Preferences" });
    fireEvent(dialog, new Event("cancel", { bubbles: true, cancelable: true }));
    expect(dialog).toHaveAttribute("open");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    finish();
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });
  it("pauses native shortcuts before opening preferences and restores them on close", async () => {
    const backend = createDemoBackend();
    let arm: () => void = () => {};
    let ready = false;
    const paused = new Promise<void>((resolve) => {
      arm = () => {
        ready = true;
        resolve();
      };
    });
    backend.setPreferencesOpen = vi.fn(async (open) => {
      if (open && !ready) await paused;
    });
    render(<App backend={backend} store={createTelemetryStore(backend)} />);
    await screen.findByRole("region", { name: "Physical Memory" });
    fireEvent.click(screen.getByRole("button", { name: "Open Preferences" }));
    expect(
      screen.queryByRole("button", { name: "Cancel" }),
    ).not.toBeInTheDocument();
    arm();
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(backend.setPreferencesOpen).toHaveBeenLastCalledWith(false),
    );
  });
  it("renders exactly three independent graph categories without page headings", async () => {
    setup();
    await screen.findByRole("region", { name: "Physical Memory" });
    expect(screen.getAllByRole("img")).toHaveLength(3);
    expect(
      new Set(
        screen
          .getAllByRole("img")
          .map((svg) => svg.querySelector("linearGradient")?.id),
      ).size,
    ).toBe(3);
    expect(screen.getByRole("region", { name: "Pagefile" })).toBeVisible();
    expect(
      screen.getByRole("region", { name: "System Working Set" }),
    ).toBeVisible();
    expect(screen.queryByRole("heading")).not.toBeInTheDocument();
    expect(screen.getByText("Demo")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "1 hour" }));
    expect(
      screen.getByRole("img", { name: /Pagefile over 60 minutes\./ }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Minimize Window" }),
    ).toBeDisabled();
    const chart = screen.getByRole("group", {
      name: "Inspect physical memory history",
    });
    fireEvent.focus(chart);
    fireEvent.keyDown(chart, { key: "Home" });
    const first = screen.getByRole("tooltip").textContent;
    fireEvent.keyDown(chart, { key: "End" });
    expect(screen.getByRole("tooltip").textContent).not.toBe(first);
    fireEvent.keyDown(chart, { key: "Escape" });
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });
  it("reports staleness and preserves the three graph layout", async () => {
    const { backend, store } = setup();
    await screen.findByRole("region", { name: "Physical Memory" });
    store.update(await backend.setScenario!("stale"));
    await screen.findByText("Paused");
    expect(screen.getAllByRole("img")).toHaveLength(3);
  });
  it("starts cleanup from one click without a dialog and marks the measured graph changes", async () => {
    const { backend } = setup();
    const request = vi.spyOn(backend, "requestCleanup");
    await screen.findByRole("region", { name: "Physical Memory" });
    const action = screen.getByRole("button", { name: "Clean Memory" });
    fireEvent.click(action);
    fireEvent.click(action);
    expect(action).toBeDisabled();
    expect(request).toHaveBeenCalledOnce();
    expect(request.mock.calls[0][0]).toEqual(
      (await backend.bootstrap()).preferences.cleanupRegions,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    const physical = within(
      screen.getByRole("region", { name: "Physical Memory" }),
    );
    const result = await physical.findByRole(
      "button",
      { name: "Cleaned 1.8 GiB less" },
      { timeout: 2500 },
    );
    expect(result).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByRole("region", { name: /cleanup details/ }),
    ).not.toBeInTheDocument();
    expect(
      within(screen.getByRole("region", { name: "Pagefile" })).getByRole(
        "button",
        { name: "Cleaned No change" },
      ),
    ).toBeVisible();
    expect(
      within(
        screen.getByRole("region", { name: "System Working Set" }),
      ).getByRole("button", { name: "Cleaned 64.0 MiB less" }),
    ).toBeVisible();
    fireEvent.click(result);
    expect(
      physical.getByRole("region", { name: "Physical Memory cleanup details" }),
    ).toBeVisible();
    expect(physical.getByText("Before")).toBeVisible();
    expect(physical.getByText("After")).toBeVisible();
    expect(physical.queryByText("Done")).not.toBeInTheDocument();
    expect(physical.queryByText("Working Set")).not.toBeInTheDocument();
    fireEvent.keyDown(result, { key: "Escape" });
    expect(result).toHaveAttribute("aria-expanded", "false");
    expect(action).toBeDisabled();
  });
  it("opens only one cleanup panel and dismisses it outside, with Escape, or on window blur", async () => {
    setup();
    await screen.findByRole("region", { name: "Physical Memory" });
    fireEvent.click(screen.getByRole("button", { name: "Clean Memory" }));
    const physical = await screen.findByRole(
      "button",
      { name: "Cleaned 1.8 GiB less" },
      { timeout: 2500 },
    );
    fireEvent.click(physical);
    const pagefile = screen.getByRole("button", { name: "Cleaned No change" });
    fireEvent.pointerDown(pagefile);
    fireEvent.click(pagefile);
    expect(
      screen.queryByRole("region", { name: "Physical Memory cleanup details" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getAllByRole("region", { name: /cleanup details/ }),
    ).toHaveLength(1);
    const panel = screen.getByRole("region", {
      name: "Pagefile cleanup details",
    });
    fireEvent.pointerDown(panel);
    expect(panel).toBeVisible();
    fireEvent.pointerDown(screen.getByRole("img", { name: /Pagefile over/ }));
    expect(
      screen.queryByRole("region", { name: /cleanup details/ }),
    ).not.toBeInTheDocument();
    fireEvent.click(physical);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(
      screen.queryByRole("region", { name: /cleanup details/ }),
    ).not.toBeInTheDocument();
    fireEvent.click(physical);
    fireEvent(window, new Event("blur"));
    expect(
      screen.queryByRole("region", { name: /cleanup details/ }),
    ).not.toBeInTheDocument();
  });
  it("keeps negative cleanup observations and disables duplicate operations", async () => {
    const { backend, store } = setup();
    await screen.findByRole("region", { name: "Physical Memory" });
    store.update(await backend.setScenario!("negative"));
    const action = screen.getByRole("button", { name: "Clean Memory" });
    fireEvent.click(action);
    expect(action).toBeDisabled();
    await screen.findByRole(
      "button",
      { name: "Cleaned 64.0 MiB more" },
      { timeout: 2500 },
    );
    expect(action).toBeDisabled();
  });
  it("retains cancelled attempts in session history", async () => {
    const { backend, store } = setup();
    await screen.findByRole("region", { name: "Physical Memory" });
    store.update(await backend.setScenario!("cancelled"));
    const button = screen.getByRole("button", { name: "Clean Memory" });
    fireEvent.click(button);
    const physical = within(
      screen.getByRole("region", { name: "Physical Memory" }),
    );
    await physical.findByRole(
      "button",
      { name: "Cancelled" },
      { timeout: 2500 },
    );
    expect(button).toBeEnabled();
    fireEvent.click(button);
    await waitFor(
      () =>
        expect(
          physical.getAllByRole("button", { name: /Inspect cleanup/ }),
        ).toHaveLength(2),
      { timeout: 2500 },
    );
    const old = physical.getAllByRole("button", { name: /Inspect cleanup/ })[1];
    fireEvent.click(old);
    expect(old).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it.each(["noChange", "partial", "failed", "denied"] as const)(
    "shows %s honestly on the graph",
    async (scenario) => {
      const { backend, store } = setup();
      await screen.findByRole("region", { name: "Physical Memory" });
      store.update(await backend.setScenario!(scenario));
      fireEvent.click(screen.getByRole("button", { name: "Clean Memory" }));
      const physical = within(
        screen.getByRole("region", { name: "Physical Memory" }),
      );
      const name = {
        noChange: "Cleaned No change",
        partial: "Partly cleaned 1.8 GiB less",
        failed: "Failed",
        denied: "Permission denied",
      }[scenario];
      const result = await physical.findByRole(
        "button",
        { name },
        { timeout: 2500 },
      );
      fireEvent.click(result);
      if (scenario === "partial")
        expect(
          physical.getByText("1 of 6 operations did not complete."),
        ).toBeVisible();
      if (scenario === "failed" || scenario === "denied") {
        expect(
          physical.queryByText(/less|more|No change/, { selector: "strong" }),
        ).not.toBeInTheDocument();
        expect(
          screen.getByRole("button", { name: "Clean Memory" }),
        ).toBeEnabled();
      }
    },
  );
  it("disables direct cleanup when unsupported or no regions are selected", async () => {
    const { backend, store } = setup();
    await screen.findByRole("region", { name: "Physical Memory" });
    store.update(await backend.setScenario!("unsupported"));
    const button = screen.getByRole("button", { name: "Clean Memory" });
    await waitFor(() => expect(button).toBeDisabled());
    store.update(await backend.setScenario!("normal"));
    store.setPreferences({
      ...store.getSnapshot().data!.preferences,
      cleanupRegions: [],
    });
    await waitFor(() => expect(button).toBeDisabled());
    expect(
      screen.getByText("Choose memory regions in Preferences."),
    ).toBeVisible();
  });
});
