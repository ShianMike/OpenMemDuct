import { useEffect, useId, useRef, useState } from "react";
import { durationLabel } from "../lib/format";

export const chartTimeRanges = [30, 60, 120, 240, 480, 960, 1440, 2880];

export function TimeRangeDropdown({
  minutes,
  onChange,
}: {
  minutes: number;
  onChange(minutes: number): void;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const listId = useId();
  const selected = Math.max(0, chartTimeRanges.indexOf(minutes));

  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target))
        setOpen(false);
    };
    const close = () => setOpen(false);
    document.addEventListener("pointerdown", outside, true);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("blur", close);
    };
  }, [open]);

  function choose(index: number) {
    onChange(chartTimeRanges[index]);
    setOpen(false);
    trigger.current?.focus();
  }

  return (
    <div
      className="time-range"
      ref={root}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
      }}
    >
      <button
        ref={trigger}
        type="button"
        className="time-range-trigger"
        role="combobox"
        aria-label="Chart time range"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={listId}
        aria-activedescendant={open ? `${listId}-${active}` : undefined}
        onClick={() => {
          setActive(selected);
          setOpen(!open);
        }}
        onKeyDown={(event) => {
          if (event.key === "Tab") {
            setOpen(false);
            return;
          }
          if (event.key === "Escape") {
            if (open) {
              event.preventDefault();
              event.stopPropagation();
              setOpen(false);
            }
            return;
          }
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            if (open) choose(active);
            else {
              setActive(selected);
              setOpen(true);
            }
            return;
          }
          if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key))
            return;
          event.preventDefault();
          setOpen(true);
          if (event.key === "Home") setActive(0);
          else if (event.key === "End") setActive(chartTimeRanges.length - 1);
          else if (!open) setActive(selected);
          else
            setActive((index) =>
              Math.max(
                0,
                Math.min(
                  chartTimeRanges.length - 1,
                  index + (event.key === "ArrowDown" ? 1 : -1),
                ),
              ),
            );
        }}
      >
        <svg viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <circle cx="8" cy="8" r="5.75" />
          <path d="M8 4.5V8l2.25 1.5" />
        </svg>
        <span>{durationLabel(minutes)}</span>
        <svg
          className="time-range-chevron"
          viewBox="0 0 16 16"
          fill="none"
          aria-hidden="true"
        >
          <path d="m5 6.5 3 3 3-3" />
        </svg>
      </button>
      {open && (
        <ul
          id={listId}
          className="time-range-menu"
          role="listbox"
          aria-label="Chart time range"
        >
          {chartTimeRanges.map((value, index) => (
            <li
              id={`${listId}-${index}`}
              key={value}
              role="option"
              aria-selected={minutes === value}
              data-active={active === index}
              onPointerMove={() => setActive(index)}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose(index)}
            >
              <span>{durationLabel(value)}</span>
              {minutes === value && (
                <svg viewBox="0 0 16 16" fill="none" aria-hidden="true">
                  <path d="m3.5 8 3 3 6-6" />
                </svg>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
