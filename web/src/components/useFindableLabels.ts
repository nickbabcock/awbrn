import { useEffect, useRef, type RefObject } from "react";

/** Marks a label that only the browser's find in page shows. */
export const FIND_LABEL_ATTRIBUTE = "data-find-label";

const hideUntilFound = (label: Element) => label.setAttribute("hidden", "until-found");

/**
 * Makes a hidden label searchable by find in page. React writes `hidden` as
 * a boolean (https://github.com/facebook/react/issues/24740), so the server
 * sends plain `hidden` and this ref changes it to `until-found` when the
 * label mounts. A browser without support for `until-found` keeps the label
 * hidden. When React accepts the string, set `hidden="until-found"` in the
 * JSX and remove this ref.
 */
export const findableRef = (label: HTMLElement | null) => {
  if (label) hideUntilFound(label);
};

/**
 * Lets the browser's find in page search a board of faces by name.
 *
 * Each face holds its name in a label that carries `data-find-label`, set to
 * the value `onFind` receives. A label with `hidden="until-found"` takes no
 * space, but find in page can match it: the browser then shows the label of
 * the active match and fires `beforematch`. Only one label shows at a time. It
 * hides again when the next match shows, or when the user presses or tabs
 * away from its face, and `onFind` then receives null. A label under a face at
 * the edge of the board moves sideways to stay inside it.
 */
export function useFindableLabels(
  root: RefObject<HTMLElement | null>,
  onFind: (value: string | null) => void,
) {
  // The latest callback, so the listeners do not have to be set again on
  // every render.
  const onFindRef = useRef(onFind);
  useEffect(() => {
    onFindRef.current = onFind;
  });

  useEffect(() => {
    const board = root.current;
    if (!board) return;

    const fit = (label: HTMLElement) => {
      label.style.marginInlineStart = "";
      const box = label.getBoundingClientRect();
      const bounds = board.getBoundingClientRect();
      const shift = Math.max(bounds.left - box.left, 0) + Math.min(bounds.right - box.right, 0);
      label.style.marginInlineStart = `${shift}px`;
    };
    let shown: HTMLElement | null = null;
    // The label's face is its parent, so a press or a focus that stays on the
    // face keeps the label.
    const release = (target: EventTarget | null) => {
      if (!shown) return;
      if (target instanceof Node && shown.parentElement?.contains(target)) return;
      hideUntilFound(shown);
      shown = null;
      onFindRef.current(null);
    };

    const onMatch = (event: Event) => {
      const label = event.target;
      if (!(label instanceof HTMLElement) || !label.hasAttribute(FIND_LABEL_ATTRIBUTE)) return;
      if (shown && shown !== label) hideUntilFound(shown);
      shown = label;
      onFindRef.current(label.getAttribute(FIND_LABEL_ATTRIBUTE));
      // The browser shows the label after this event, so measure it on the
      // next frame.
      requestAnimationFrame(() => fit(label));
    };
    const onPointerDown = (event: PointerEvent) => release(event.target);
    const onFocusOut = (event: FocusEvent) => release(event.relatedTarget);

    board.addEventListener("beforematch", onMatch);
    board.addEventListener("focusout", onFocusOut);
    document.addEventListener("pointerdown", onPointerDown);
    return () => {
      board.removeEventListener("beforematch", onMatch);
      board.removeEventListener("focusout", onFocusOut);
      document.removeEventListener("pointerdown", onPointerDown);
    };
  }, [root]);
}
