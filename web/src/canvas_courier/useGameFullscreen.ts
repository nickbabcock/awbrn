import { useCallback, useEffect, useRef, useState, type RefObject } from "react";

/**
 * How the board is filling the screen.
 *
 * `native` is the browser's own full screen, with its chrome hidden and its own
 * exit toast. `immersive` is the same mode built out of CSS: the board is
 * pinned over the page and the document stops scrolling.
 *
 * The fallback is not a degradation. iOS Safari refuses element full screen
 * outright — only a `<video>` may take the screen — and the phone is a device
 * this product treats as a requirement rather than a courtesy, so the two paths
 * present the same mode and differ only in the mechanism underneath.
 */
export type GameFullscreenMode = "off" | "native" | "immersive";

/**
 * A board that can take the whole screen, by whichever route this browser
 * allows.
 *
 * The element that goes full screen is the surface, not the page, because the
 * menus the board opens position themselves inside it. Handing the surface to
 * the browser keeps those menus on the board they belong to instead of
 * stranding them behind a screen that no longer paints the page.
 */
export function useGameFullscreen({
  focusSurface,
  surfaceRef,
}: {
  /** Hands the keyboard to the board, so play continues without a click. */
  focusSurface: () => void;
  surfaceRef: RefObject<HTMLElement | null>;
}) {
  const [mode, setMode] = useState<GameFullscreenMode>("off");
  // What had the keyboard before the board took the screen. Full screen is
  // entered from a control on the page, and that control is gone for as long as
  // the board is up, so leaving has to give the focus back deliberately.
  const returnFocusRef = useRef<HTMLElement | null>(null);

  const exit = useCallback(() => {
    if (mode === "native" && document.fullscreenElement !== null) {
      void document.exitFullscreen().catch((error: unknown) => {
        console.error("Error leaving full screen:", error);
      });
      return;
    }
    setMode("off");
  }, [mode]);

  const enter = useCallback(() => {
    const surface = surfaceRef.current;
    if (!surface || mode !== "off") return;

    returnFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;

    if (!document.fullscreenEnabled || typeof surface.requestFullscreen !== "function") {
      setMode("immersive");
      return;
    }

    // A browser may still refuse the request — a permissions policy, or a
    // gesture it did not count. The mode is the same either way, so a refusal
    // takes the other route rather than reporting a failure the player cannot
    // act on.
    surface.requestFullscreen({ navigationUI: "hide" }).then(
      () => setMode("native"),
      () => setMode("immersive"),
    );
  }, [mode, surfaceRef]);

  // The browser owns native full screen and can end it without asking: the Esc
  // key, the tab losing the screen, a device rotation on some browsers.
  useEffect(() => {
    const onFullscreenChange = () => {
      if (document.fullscreenElement === surfaceRef.current) return;
      setMode((previous) => (previous === "native" ? "off" : previous));
    };

    document.addEventListener("fullscreenchange", onFullscreenChange);
    return () => document.removeEventListener("fullscreenchange", onFullscreenChange);
  }, [surfaceRef]);

  // Immersive full screen has to supply what the browser supplies for its own:
  // a page that cannot scroll behind the board, and an Esc that leaves. Esc is
  // bound here rather than in the engine so that leaving works the same way in
  // both modes, which is the whole point of having two.
  useEffect(() => {
    if (mode !== "immersive") return;

    const { documentElement } = document;
    const previousOverflow = documentElement.style.overflow;
    documentElement.style.overflow = "hidden";

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setMode("off");
    };

    document.addEventListener("keydown", onKeyDown);
    return () => {
      documentElement.style.overflow = previousOverflow;
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [mode]);

  // Entering hands the keyboard to the board, because the screen now holds
  // nothing else.
  //
  // Leaving offers the keyboard back to whatever held it before. Usually that
  // is the enter command, which is unmounted for as long as the board holds the
  // screen, so usually the board simply keeps the keyboard — which is the right
  // resting place, since the player is still looking at it. The restore is here
  // for the case where the trigger did survive; what it exists to prevent is
  // focus falling to the top of the document.
  useEffect(() => {
    if (mode !== "off") {
      focusSurface();
      return;
    }

    const returnTo = returnFocusRef.current;
    returnFocusRef.current = null;
    if (returnTo?.isConnected) returnTo.focus({ preventScroll: true });
  }, [focusSurface, mode]);

  return {
    isFullscreen: mode !== "off",
    mode,
    enterFullscreen: enter,
    exitFullscreen: exit,
  };
}
