import { useMediaQuery } from "@astryxdesign/core/hooks";

/**
 * Whether this is a compact touch screen: a coarse primary pointer on a
 * screen narrower than 721px or shorter than 500px (a phone in either
 * orientation). The same rule Astryx uses for `presentation="adaptive"`.
 *
 * Server renders and the first client render read `false`; the value settles
 * after hydration, so use it to choose a surface when something opens, not to
 * lay out first paint (lay out with `pageLayout.phoneMedia` instead).
 */
export function useCompactTouch(): boolean {
  return useMediaQuery(QUERY);
}

const QUERY = "(pointer: coarse) and (max-width: 720px), (pointer: coarse) and (max-height: 499px)";
