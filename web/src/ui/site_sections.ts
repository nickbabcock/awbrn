import { IconMail, IconMap, IconPlay, IconReplay, type DrawnIcon } from "#/ui/icons.tsx";

/**
 * The sections of the site, and the pages each one holds.
 *
 * The top bar (desktop) and the tab bar (phone) name a section; the page head
 * names the pages inside it as underline tabs. All three read this one table,
 * so the lit section and the lit tab can never disagree about where the
 * player is.
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
  /** The section's drawn icon, in the top bar and the tab bar. */
  icon: DrawnIcon;
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
    icon: IconPlay,
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
    icon: IconMail,
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
    icon: IconMap,
    to: "/maps",
    pages: [{ to: "/maps", label: "Catalog", matches: (pathname) => pathname.startsWith("/maps") }],
  },
  {
    id: "replay",
    label: "Replays",
    icon: IconReplay,
    to: "/replay",
    pages: [{ to: "/replay", label: "Review", matches: exactly("/replay") }],
  },
];

export function sectionForPath(pathname: string): SiteSection | null {
  return (
    SITE_SECTIONS.find((section) => section.pages.some((page) => page.matches(pathname))) ?? null
  );
}

/**
 * Where the top bar's New match key steps aside: the Play pages carry New
 * match as a tab, and map detail carries it as the page's own key.
 */
export function pageOffersNewMatch(pathname: string): boolean {
  return sectionForPath(pathname)?.id === "play" || /^\/maps\/[^/]+\/?$/.test(pathname);
}
