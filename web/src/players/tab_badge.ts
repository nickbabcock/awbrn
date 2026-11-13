/**
 * What the browser tab says while the player is looking somewhere else.
 *
 * Everything waiting on a player collapses to one number here, whatever page
 * or badge it came from, because a tab strip has room for one and a player
 * reading it wants to know whether to come back rather than what about.
 */

/** The largest number drawn as itself. Anything more is shown as `9+`. */
const MAX_DRAWN_COUNT = 9;

/** The title a tab carries, with what is waiting counted in front of it. */
export function tabTitle(baseTitle: string, count: number): string {
  return count > 0 ? `(${countLabel(count)}) ${baseTitle}` : baseTitle;
}

/** A count as the tab draws it, kept to two characters. */
export function countLabel(count: number): string {
  return count > MAX_DRAWN_COUNT ? `${MAX_DRAWN_COUNT}+` : String(count);
}

/** The palette the icon is drawn in: ink 900, Airmail red and blue, white. */
const INK_900 = "#101b3a";
const AIRMAIL_RED = "#d42a3c";
const AIRMAIL_BLUE = "#2747b0";

/**
 * The brand mark on ink 900: the corner of an airmail envelope, as the top
 * bar draws it. The same picture is `public/favicon.svg`, the tab's icon
 * before the page runs, and the push alert's icon.
 */
export function brandMarkSvg(badge = ""): string {
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">` +
    `<defs><pattern id="band" width="16" height="16" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">` +
    `<rect width="16" height="16" fill="#ffffff"/>` +
    `<rect width="5.33" height="16" fill="${AIRMAIL_RED}"/>` +
    `<rect x="8" width="5.33" height="16" fill="${AIRMAIL_BLUE}"/>` +
    `</pattern></defs>` +
    `<rect width="32" height="32" rx="7" fill="${INK_900}"/>` +
    `<rect x="4" y="8" width="24" height="16" rx="2.5" fill="url(#band)"/>` +
    `<rect x="5" y="9" width="22" height="14" rx="1.5" fill="none" stroke="#ffffff" stroke-width="2"/>` +
    badge +
    `</svg>`
  );
}

/**
 * The tab icon, with the count on it in Airmail red when anything is waiting.
 *
 * It is drawn rather than fetched so that the number is part of the icon: a
 * badge laid over a loaded image would need a canvas, and would be blank for
 * as long as the image took to arrive.
 */
export function faviconDataUrl(count: number): string {
  const badge =
    count > 0
      ? `<circle cx="23" cy="9" r="9" fill="${AIRMAIL_RED}" stroke="${INK_900}" stroke-width="1.5"/>` +
        `<text x="23" y="9" fill="#ffffff" font-family="system-ui, sans-serif" font-size="${
          count > MAX_DRAWN_COUNT ? 10 : 13
        }" font-weight="800" text-anchor="middle" dominant-baseline="central">${countLabel(
          count,
        )}</text>`
      : "";
  return `data:image/svg+xml,${encodeURIComponent(brandMarkSvg(badge))}`;
}
