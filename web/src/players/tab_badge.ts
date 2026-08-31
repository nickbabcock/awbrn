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

/**
 * The tab icon, with a count on it when anything is waiting.
 *
 * It is drawn rather than fetched so that the number is part of the icon: a
 * badge laid over a loaded image would need a canvas, and would be blank for
 * as long as the image took to arrive.
 */
export function faviconDataUrl(count: number): string {
  const badge =
    count > 0
      ? `<circle cx="24" cy="8" r="8" fill="#d92d20"/>` +
        `<text x="24" y="8" fill="#ffffff" font-family="system-ui, sans-serif" font-size="${
          count > MAX_DRAWN_COUNT ? 9 : 11
        }" font-weight="700" text-anchor="middle" dominant-baseline="central">${countLabel(
          count,
        )}</text>`
      : "";

  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">` +
    `<rect width="32" height="32" rx="7" fill="#1f2937"/>` +
    `<text x="16" y="19" fill="#f9fafb" font-family="system-ui, sans-serif" font-size="14"` +
    ` font-weight="700" text-anchor="middle" dominant-baseline="middle">AW</text>` +
    badge +
    `</svg>`;

  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}
