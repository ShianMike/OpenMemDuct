import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { TimeRangeDropdown } from "./TimeRangeDropdown";

const labels = [
  "30 minutes",
  "1 hour",
  "2 hours",
  "4 hours",
  "8 hours",
  "16 hours",
  "1 day",
  "2 days",
];

function setup() {
  function Example() {
    const [minutes, setMinutes] = useState(30);
    return (
      <>
        <TimeRangeDropdown minutes={minutes} onChange={setMinutes} />
        <button>Outside</button>
      </>
    );
  }
  render(<Example />);
  return screen.getByRole("combobox", { name: "Chart time range" });
}

describe("chart time range dropdown", () => {
  it("offers the eight requested durations and keeps the selected item checked", () => {
    const trigger = setup();
    expect(trigger).toHaveTextContent("30 minutes");
    for (const label of labels) {
      fireEvent.click(trigger);
      expect(screen.getAllByRole("option").map((option) => option.textContent))
        .toEqual(labels);
      fireEvent.click(screen.getByRole("option", { name: label }));
      expect(trigger).toHaveTextContent(label);
      expect(trigger).toHaveFocus();
      expect(trigger).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
      fireEvent.click(trigger);
      expect(screen.getByRole("option", { name: label }))
        .toHaveAttribute("aria-selected", "true");
      expect(screen.getAllByRole("option", { selected: true })).toHaveLength(1);
      fireEvent.keyDown(trigger, { key: "Escape" });
    }
  });

  it("supports arrow keys, Home, End, Enter and Space without committing on Escape", () => {
    const trigger = setup();
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    expect(document.getElementById(trigger.getAttribute("aria-activedescendant")!))
      .toHaveTextContent("1 hour");
    expect(trigger).toHaveTextContent("30 minutes");
    fireEvent.keyDown(trigger, { key: "Escape" });
    expect(trigger).toHaveTextContent("30 minutes");
    expect(trigger).toHaveFocus();

    fireEvent.keyDown(trigger, { key: "End" });
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(trigger).toHaveTextContent("2 days");
    fireEvent.keyDown(trigger, { key: " " });
    fireEvent.keyDown(trigger, { key: "ArrowUp" });
    fireEvent.keyDown(trigger, { key: " " });
    expect(trigger).toHaveTextContent("1 day");
    fireEvent.keyDown(trigger, { key: "Home" });
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(trigger).toHaveTextContent("30 minutes");
  });

  it("dismisses on an outside click, Tab and window blur", () => {
    const trigger = setup();
    fireEvent.click(trigger);
    fireEvent.pointerDown(screen.getByRole("button", { name: "Outside" }));
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Tab" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(trigger);
    fireEvent.blur(window);
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveTextContent("30 minutes");
  });
});
