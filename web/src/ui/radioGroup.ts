import type { KeyboardEvent } from "react";

/**
 * Arrow keys for a group of `role="radio"` buttons, as a native radio group
 * has them: the next or previous radio takes focus and is chosen, wrapping at
 * the ends and passing over any marked `aria-disabled`. Give the chosen radio
 * `tabIndex={0}` and the rest `-1`, so the group is one tab stop.
 */
export function onRadioGroupKeyDown(event: KeyboardEvent<HTMLElement>): void {
  const step =
    event.key === "ArrowRight" || event.key === "ArrowDown"
      ? 1
      : event.key === "ArrowLeft" || event.key === "ArrowUp"
        ? -1
        : 0;
  if (step === 0) return;
  const radios = [...event.currentTarget.querySelectorAll<HTMLElement>('[role="radio"]')];
  const at = radios.findIndex((radio) => radio === document.activeElement);
  if (at === -1) return;
  event.preventDefault();
  for (let offset = 1; offset < radios.length; offset++) {
    const next = radios[(at + step * offset + radios.length) % radios.length]!;
    if (next.getAttribute("aria-disabled") === "true") continue;
    next.focus();
    next.click();
    return;
  }
}
