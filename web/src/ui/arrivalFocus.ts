import { useRouter } from "@tanstack/react-router";
import { useEffect } from "react";

/**
 * Marks a screen's arrival point when it is not the first laid-out `<h1>`.
 * Only a target that is laid out counts, so a pair of width twins (the desktop
 * strip and the phone turn line) can both carry it.
 */
export const arrivalFocusProps = { "data-arrival-focus": "", tabIndex: -1 } as const;

const WAIT_MS = 3000;

/** Moves focus to the new page's title or arrival point after each route change. */
export function useArrivalFocus(): void {
  const router = useRouter();

  useEffect(() => {
    let stop: (() => void) | null = null;
    const unsubscribe = router.subscribe("onRendered", (event) => {
      stop?.();
      stop = null;
      // The first load, and a change of search on the same page (a filter, a
      // replay step), keep focus where it is.
      if (event.fromLocation === undefined || !event.pathChanged) return;
      stop = focusWhenDrawn(document.activeElement);
    });
    return () => {
      unsubscribe();
      stop?.();
    };
  }, [router]);
}

/** Gives up when the player moves focus first, or after `WAIT_MS`. */
function focusWhenDrawn(focusedBefore: Element | null): () => void {
  let frame = 0;
  const deadline = performance.now() + WAIT_MS;

  const attempt = () => {
    const active = document.activeElement;
    const playerMoved = active !== focusedBefore && active !== document.body && active !== null;
    if (playerMoved || performance.now() > deadline) return;
    const target = arrivalTarget();
    if (target === null) {
      frame = requestAnimationFrame(attempt);
      return;
    }
    if (!target.hasAttribute("tabindex")) target.setAttribute("tabindex", "-1");
    target.focus({ preventScroll: true });
  };

  frame = requestAnimationFrame(attempt);
  return () => cancelAnimationFrame(frame);
}

function arrivalTarget(): HTMLElement | null {
  const marked = document.querySelectorAll<HTMLElement>("[data-arrival-focus]");
  for (const element of marked) {
    if (isLaidOut(element)) return element;
  }
  const titles = document.querySelectorAll<HTMLElement>("h1");
  for (const title of titles) {
    if (isLaidOut(title)) return title;
  }
  return null;
}

function isLaidOut(element: HTMLElement): boolean {
  return element.getClientRects().length > 0;
}
