/**
 * The sections of the site, and the pages each one holds.
 *
 * The bar names a section; the page header names the pages inside it. Both
 * read this one table, so the tab that is lit in the bar and the tab that is
 * lit under a title can never disagree about where the player is.
 */

export interface SitePage {
  to: string;
  label: string;
  /** A page only a signed-in player can open, which a visitor is not shown. */
  requiresSession?: boolean;
  matches: (pathname: string) => boolean;
}

export interface SiteSection {
  id: "play" | "mine" | "maps" | "replay";
  label: string;
  /** Where the section's entry in the bar goes. */
  to: string;
  requiresSession?: boolean;
  pages: SitePage[];
}

const exactly =
  (...paths: string[]) =>
  (pathname: string) =>
    paths.includes(pathname.replace(/\/$/, "") || "/");

/** A match's own page belongs to Play: it is the room the player joined. */
const isMatchRoom = (pathname: string) =>
  /^\/matches\/[^/]+\/?$/.test(pathname) && !exactly("/matches/new")(pathname);

export const SITE_SECTIONS: readonly SiteSection[] = [
  {
    id: "play",
    label: "Play",
    to: "/matches",
    pages: [
      {
        to: "/matches",
        label: "Open lobbies",
        matches: (pathname) => exactly("/matches")(pathname) || isMatchRoom(pathname),
      },
      { to: "/ranked", label: "Ranked", requiresSession: true, matches: exactly("/ranked") },
      { to: "/matches/new", label: "New match", matches: exactly("/matches/new") },
    ],
  },
  {
    id: "mine",
    label: "My games",
    to: "/my/matches",
    requiresSession: true,
    pages: [
      { to: "/my/matches", label: "Ongoing", matches: exactly("/my/matches") },
      { to: "/my/history", label: "History", matches: exactly("/my/history") },
    ],
  },
  {
    id: "maps",
    label: "Maps",
    to: "/maps",
    pages: [{ to: "/maps", label: "Catalog", matches: (pathname) => pathname.startsWith("/maps") }],
  },
  {
    id: "replay",
    label: "Replays",
    to: "/replay",
    pages: [{ to: "/replay", label: "Review", matches: exactly("/replay") }],
  },
];

export function sectionForPath(pathname: string): SiteSection | null {
  return (
    SITE_SECTIONS.find((section) => section.pages.some((page) => page.matches(pathname))) ?? null
  );
}
