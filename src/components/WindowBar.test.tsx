import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { WindowBar } from "./WindowBar";

const controls = vi.hoisted(() => ({ close: vi.fn(), minimize: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => controls }));
beforeEach(() => vi.clearAllMocks());

it("connects the two caption controls to their native window operations", () => {
  render(<WindowBar />);
  fireEvent.click(screen.getByRole("button", { name: "Minimize Window" }));
  expect(controls.minimize).toHaveBeenCalledOnce();
  expect(controls.close).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Close Window" }));
  expect(controls.close).toHaveBeenCalledOnce();
});

it("keeps a native window-control failure visible", async () => {
  controls.minimize.mockRejectedValueOnce(
    new Error("Window permission denied"),
  );
  render(<WindowBar />);
  fireEvent.click(screen.getByRole("button", { name: "Minimize Window" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Window permission denied",
  );
});
