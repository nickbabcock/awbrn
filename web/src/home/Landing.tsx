/*
 * The visitor's home page. It shows only what a new account can reach today;
 * what needs an account says so.
 */

import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  colorVars,
  focusVars,
  spacingVars,
  textSizeVars,
  typeScaleVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import { useSuspenseInfiniteQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { getRouteApi, Link } from "@tanstack/react-router";
import { useEffect, useId, useMemo, useState, type ReactNode } from "react";
import { getFactionById } from "#/factions.ts";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapCatalogQueryOptions } from "#/maps/maps.queries.ts";
import { MAP_RANKS, type MapCatalogEntry } from "#/maps/schemas.ts";
import { matchesBrowseQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchBrowseSummary } from "#/matches/schemas.ts";
import { MatchThumb } from "#/play/MatchThumb.tsx";
import { SeatStamp } from "#/play/Opponents.tsx";
import { Postage, mapPostage } from "#/play/Postage.tsx";
import { clockTerms, seatsFigure } from "#/play/terms.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { IconNext, IconUpload } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Postmark } from "#/ui/Postmark.tsx";
import { DepictedMatch } from "./DepictedMatch.tsx";
import { DepictedReplay } from "./DepictedReplay.tsx";
import { chooseLandingMaps } from "./depicted.ts";

/** The landing, the first match and Newly ranked all read this one list. */
export function rankedMapsQueryOptions() {
  return mapCatalogQueryOptions(null, { ranks: [...MAP_RANKS] });
}

const homeRoute = getRouteApi("/");

/** Of the map rows beside the featured map, a phone shows the first two. */
const PHONE_MAP_ROWS = 2;

export function Landing() {
  const { data } = useSuspenseInfiniteQuery(rankedMapsQueryOptions());
  const lobbies = useSuspenseInfiniteQuery(matchesBrowseQueryOptions());
  const {
    featured,
    rows,
    replay: replayMap,
  } = useMemo(() => chooseLandingMaps(data.pages.flatMap((page) => page.maps)), [data]);
  const lobby = lobbies.data.pages[0]?.matches[0] ?? null;

  return (
    <VStack>
      <Hero map={featured} />
      <VStack xstyle={[styles.paper, styles.gutters]}>
        <Section
          body="Live on a clock, or async over days. Turn alerts tell you when it is your move."
          title="Someone to play at any hour."
        >
          <Opponents lobby={lobby} />
        </Section>
        {featured ? (
          <Section
            body="Curated from AWBW and ranked S to C. Signed in, add any other by its AWBW map ID."
            title="The maps you know."
          >
            <MapsSheet featured={featured} rows={rows} />
          </Section>
        ) : null}
      </VStack>
      <Replays map={replayMap} />
      <Close />
    </VStack>
  );
}

function Hero({ map }: { map: MapCatalogEntry | null }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} {...stylex.props(styles.hero)}>
      <VStack xstyle={[styles.heroIn, styles.gutters]}>
        <Text as="div" color="inherit" display="block" type="inherit" xstyle={styles.copy}>
          <Heading id={headingId} level={1} xstyle={styles.display}>
            Your move.
            <br />
            On your phone.
          </Heading>
          <p {...stylex.props(styles.lede)}>
            Play Advance Wars in your browser, on a phone or at a desk. Sign up and play the CPU
            right away.
          </p>
          <CreateAccountKey />
        </Text>
        {map ? (
          <VStack xstyle={styles.heroScreen}>
            <DepictedMatch map={map} />
          </VStack>
        ) : null}
      </VStack>
    </section>
  );
}

function CreateAccountKey() {
  return (
    <RouterButton
      label="Create account"
      search={{ mode: "register" }}
      size="lg"
      to="/auth"
      variant="primary"
      xstyle={[styles.phoneWide, styles.cta]}
    />
  );
}

function Section({ body, children, title }: { body: string; children: ReactNode; title: string }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} {...stylex.props(styles.section)}>
      <Text as="div" color="inherit" display="block" type="inherit" xstyle={styles.sectionCopy}>
        <Heading id={headingId} level={2} xstyle={styles.h2}>
          {title}
        </Heading>
        <p {...stylex.props(styles.body)}>{body}</p>
      </Text>
      {children}
    </section>
  );
}

function Opponents({ lobby }: { lobby: MatchBrowseSummary | null }) {
  return (
    <ul {...stylex.props(styles.sheet, styles.list)}>
      <WayRow
        line="Ready the moment you sign up."
        link={
          <Link
            search={{ mode: "register" }}
            to="/auth"
            {...stylex.props(stylex.defaultMarker(), styles.wayLink)}
          >
            The CPU
          </Link>
        }
        real={
          <SegmentedControl
            label="CPU"
            layout="fill"
            onChange={() => {}}
            value="standard"
            xstyle={styles.tier}
          >
            <SegmentedControlItem label="Easy" value="easy" />
            <SegmentedControlItem label="Standard" value="standard" />
            <SegmentedControlItem label="Hard" value="hard" />
          </SegmentedControl>
        }
      />
      <WayRow
        line="Browse now; join with an account."
        link={
          <Link to="/matches" {...stylex.props(stylex.defaultMarker(), styles.wayLink)}>
            Open lobbies
          </Link>
        }
        real={lobby ? <LobbyPicture lobby={lobby} /> : null}
      />
    </ul>
  );
}

/**
 * The whole row is the link (its name stretches over the row); the control
 * beside it is a picture of where the link lands.
 */
function WayRow({ line, link, real }: { line: string; link: ReactNode; real: ReactNode }) {
  return (
    <li {...stylex.props(styles.way)}>
      <VStack as="span" xstyle={styles.wayText}>
        <b {...stylex.props(styles.wayName)}>{link}</b>
        <Text color="inherit" type="inherit" xstyle={styles.wayLine}>
          {line}
        </Text>
      </VStack>
      <HStack as="span" aria-hidden="true" inert xstyle={styles.wayReal}>
        {real}
      </HStack>
      <Icon icon={IconNext} size="md" xstyle={styles.wayGo} />
    </li>
  );
}

function LobbyPicture({ lobby }: { lobby: MatchBrowseSummary }) {
  const host = lobby.seats.find((seat) => seat.isHost) ?? null;
  return (
    <HStack as="span" xstyle={styles.lobby}>
      <MatchThumb isEager mapId={lobby.mapId} revision={lobby.mapRevision} size={56} />
      <VStack as="span" xstyle={styles.lobbyMain}>
        <b {...stylex.props(styles.lobbyTitle)}>{lobby.name}</b>
        <HStack as="span" xstyle={styles.lobbyMeta}>
          {host ? (
            <HStack as="span" xstyle={styles.lobbyHost}>
              {/* The lobby seals the host's commander until the match starts. */}
              <SeatStamp
                coId={null}
                factionCode={getFactionById(host.factionId)?.code ?? null}
                isDecorative
              />
              {host.userName}
            </HStack>
          ) : null}
          <Postage xstyle={styles.inlinePostage}>
            {clockTerms(lobby.settings.clock)} ·{" "}
            {seatsFigure(lobby.participantCount, lobby.maxPlayers)}
          </Postage>
        </HStack>
      </VStack>
    </HStack>
  );
}

function MapsSheet({ featured, rows }: { featured: MapCatalogEntry; rows: MapCatalogEntry[] }) {
  const picture = mapScreenshotSize("full", featured.width, featured.height);
  return (
    <VStack xstyle={[styles.sheet, styles.mapSheet]}>
      <Link params={{ mapId: featured.mapId }} to="/maps/$mapId" {...stylex.props(styles.feat)}>
        <VStack as="span" xstyle={styles.featWell}>
          <img
            alt={`The ${featured.name} battlefield`}
            decoding="async"
            height={picture.height}
            loading="lazy"
            src={featured.screenshot.full}
            width={picture.width}
            {...stylex.props(styles.featPicture)}
          />
        </VStack>
        <HStack as="span" xstyle={styles.mapName}>
          <MapName map={featured} />
        </HStack>
      </Link>
      <VStack xstyle={styles.mapList}>
        <ul {...stylex.props(styles.list)}>
          {rows.map((map, index) => (
            <li
              key={map.mapId}
              {...stylex.props(styles.mapRowItem, index >= PHONE_MAP_ROWS && styles.notPhone)}
            >
              <Link
                params={{ mapId: map.mapId }}
                to="/maps/$mapId"
                {...stylex.props(styles.mapRow)}
              >
                <MatchThumb isEager map={map} size={56} />
                <MapName map={map} />
              </Link>
            </li>
          ))}
        </ul>
        <Link to="/maps" {...stylex.props(styles.more)}>
          Browse maps
          <Icon icon={IconNext} size="md" xstyle={styles.moreIcon} />
        </Link>
      </VStack>
    </VStack>
  );
}

function MapName({ map }: { map: MapCatalogEntry }) {
  return (
    <>
      <VStack as="span" xstyle={styles.mapNameText}>
        <b {...stylex.props(styles.mapTitle)}>{map.name}</b>
        <Postage>{mapPostage(map)}</Postage>
      </VStack>
      <MapRankMedal rank={map.rank} size="sm" xstyle={styles.mapMedal} />
    </>
  );
}

function Replays({ map }: { map: MapCatalogEntry | null }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} {...stylex.props(styles.band)}>
      <VStack xstyle={[styles.bandIn, styles.gutters]}>
        <Text as="div" color="inherit" display="block" type="inherit" xstyle={styles.sectionCopy}>
          <Heading id={headingId} level={2} xstyle={[styles.h2, styles.h2OnInk]}>
            Study your AWBW replays.
          </Heading>
          <p {...stylex.props(styles.body, styles.bodyOnInk)}>
            Open the .zip and step through it action by action. The file stays on your device.
          </p>
          <RouterButton
            icon={<Icon icon={IconUpload} />}
            label="Open a replay"
            size="lg"
            to="/replay"
            variant="on-ink"
            xstyle={[styles.phoneWide, styles.bandKey]}
          />
        </Text>
        {map ? <DepictedReplay map={map} /> : null}
      </VStack>
    </section>
  );
}

function Close() {
  const headingId = useId();
  const ringDate = useRingDate();
  return (
    <section aria-labelledby={headingId} {...stylex.props(styles.close, styles.gutters)}>
      <Postmark figure={1} label="Day 1" ringText={ringDate} word="DAY" xstyle={styles.closeMark} />
      <Heading id={headingId} level={2} xstyle={styles.h2}>
        Day 1 starts when you do.
      </Heading>
      <CreateAccountKey />
    </section>
  );
}

/** A day as the postmark's ring reads it (`NOV 12 · 2026`). */
export function ringDateText(day: Date): string {
  const month = day.toLocaleString("en-US", { month: "short" }).toUpperCase();
  return `${month} ${day.getDate()} · ${day.getFullYear()}`;
}

/**
 * The page is drawn with the date the route loaded on (the server's, on a
 * first visit), so the ring is there at first paint; once on the viewer's
 * device it reads their own day.
 */
function useRingDate(): string {
  const { ringDate } = homeRoute.useLoaderData();
  const [text, setText] = useState(ringDate);
  useEffect(() => setText(ringDateText(new Date())), []);
  return text;
}

/** Between a phone and 960px the hero stacks: copy over the screen. */
const TABLET_MEDIA = "@media (min-width: 721px) and (max-width: 960px)";
/**
 * Below the rail breakpoint but wider than a phone. Kept apart from the phone
 * query rather than overlapping it, so a phone value never loses to it.
 */
const BELOW_RAIL_MEDIA = "@media (min-width: 721px) and (max-width: 1180px)";

const styles = stylex.create({
  // The gutter `Page` in `#/ui/Page.tsx` keeps.
  gutters: {
    paddingInlineStart: {
      default: `max(${pageLayout.gutterDesktop}, ${pageLayout.safeInsetLeft})`,
      [pageLayout.phoneMedia]: `max(${pageLayout.gutterPhone}, ${pageLayout.safeInsetLeft})`,
    },
    paddingInlineEnd: {
      default: `max(${pageLayout.gutterDesktop}, ${pageLayout.safeInsetRight})`,
      [pageLayout.phoneMedia]: `max(${pageLayout.gutterPhone}, ${pageLayout.safeInsetRight})`,
    },
  },
  hero: {
    overflow: "hidden",
    backgroundColor: awbrnVars.ink950,
  },
  heroIn: {
    display: {
      default: "grid",
      [pageLayout.phoneMedia]: "block",
    },
    gridTemplateColumns: {
      default: "minmax(0, 520px) 390px",
      [TABLET_MEDIA]: "minmax(0, 1fr)",
    },
    justifyContent: "center",
    alignItems: "center",
    columnGap: "clamp(48px, 8vw, 120px)",
    rowGap: "48px",
    maxInlineSize: pageLayout.standardWidth,
    marginInline: "auto",
    paddingBlock: {
      default: "48px",
      [TABLET_MEDIA]: "56px",
      [pageLayout.phoneMedia]: "32px 40px",
    },
  },
  heroScreen: {
    marginBlockStart: {
      default: 0,
      [pageLayout.phoneMedia]: "32px",
    },
  },
  display: {
    color: awbrnVars.textOnInk,
    fontSize: {
      default: "96px",
      [BELOW_RAIL_MEDIA]: "80px",
      [pageLayout.phoneMedia]: "46px",
    },
    fontWeight: 820,
    lineHeight: 0.92,
    fontStretch: "62%",
    letterSpacing: "-0.01em",
    wordSpacing: "0.12em",
  },
  lede: {
    maxInlineSize: "34ch",
    marginBlock: 0,
    marginBlockStart: {
      default: "28px",
      [pageLayout.phoneMedia]: "16px",
    },
    color: awbrnVars.textOnInkSecondary,
    fontSize: {
      default: "19px",
      [pageLayout.phoneMedia]: awbrnVars.text16,
    },
    lineHeight: 1.5,
    textWrap: "pretty",
  },
  phoneWide: {
    display: {
      default: "inline-flex",
      [pageLayout.phoneMedia]: "flex",
    },
    inlineSize: {
      default: "auto",
      [pageLayout.phoneMedia]: "100%",
    },
  },
  cta: {
    minInlineSize: "220px",
    // A size up from lg.
    blockSize: "56px",
    paddingInline: spacingVars["--spacing-6"],
    fontSize: typeScaleVars["--text-large-size"],
    fontWeight: 700,
    marginBlockStart: {
      default: "36px",
      [pageLayout.phoneMedia]: "24px",
    },
  },
  paper: {
    display: "grid",
    gap: {
      default: "112px",
      [pageLayout.phoneMedia]: "64px",
    },
    boxSizing: "border-box",
    inlineSize: "100%",
    maxInlineSize: pageLayout.standardWidth,
    marginInline: "auto",
    paddingBlock: {
      default: "104px",
      [pageLayout.phoneMedia]: "56px",
    },
  },
  section: {
    display: "grid",
    gridTemplateColumns: {
      default: "360px minmax(0, 1fr)",
      [pageLayout.belowRailMedia]: "minmax(0, 1fr)",
    },
    alignItems: "start",
    gap: {
      default: "64px",
      [BELOW_RAIL_MEDIA]: "32px",
      [pageLayout.phoneMedia]: "24px",
    },
  },
  // A column of copy. Text would pass break-word down to the balanced
  // headings inside, which changes where they break.
  copy: {
    overflowWrap: "normal",
  },
  sectionCopy: {
    minInlineSize: 0,
    overflowWrap: "normal",
  },
  h2: {
    color: colorVars["--color-text-primary"],
    fontSize: {
      default: "44px",
      [pageLayout.phoneMedia]: "34px",
    },
    fontWeight: 800,
    lineHeight: 0.98,
    fontStretch: "62%",
    letterSpacing: "-0.005em",
    wordSpacing: "0.12em",
    textWrap: "balance",
  },
  h2OnInk: {
    color: awbrnVars.textOnInk,
  },
  body: {
    maxInlineSize: "38ch",
    marginBlock: 0,
    marginBlockStart: {
      default: "16px",
      [pageLayout.phoneMedia]: "12px",
    },
    color: colorVars["--color-text-secondary"],
    fontSize: {
      default: awbrnVars.text17,
      [pageLayout.phoneMedia]: awbrnVars.text16,
    },
    lineHeight: 1.5,
    textWrap: "pretty",
  },
  bodyOnInk: {
    color: awbrnVars.textOnInkSecondary,
  },
  sheet: {
    overflow: "hidden",
    borderRadius: "10px",
    backgroundColor: awbrnVars.sheet,
    boxShadow: `0 0 0 1px ${awbrnVars.rule}, ${awbrnVars.shadowSheet}`,
  },
  list: {
    marginBlock: 0,
    paddingInline: 0,
    listStyle: "none",
  },
  way: {
    position: "relative",
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(0, 240px) minmax(0, 1fr) 20px",
      [pageLayout.phoneMedia]: "minmax(0, 1fr) 16px",
    },
    gridTemplateAreas: {
      default: '"text real go"',
      [pageLayout.phoneMedia]: '"text go" "real go"',
    },
    alignItems: "center",
    columnGap: {
      default: "32px",
      [pageLayout.phoneMedia]: "16px",
    },
    rowGap: "12px",
    minBlockSize: {
      default: "104px",
      [pageLayout.phoneMedia]: 0,
    },
    paddingBlock: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
    paddingInline: {
      default: "24px",
      [pageLayout.phoneMedia]: "16px",
    },
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
      ":active": awbrnVars.paper,
    },
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: {
      default: "1px",
      ":first-child": 0,
    },
    transitionProperty: "background-color",
    transitionDuration: {
      default: "90ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionTimingFunction: awbrnVars.easeStep,
    outlineColor: focusVars["--focus-outline-color"],
    outlineOffset: "-2px",
    outlineStyle: {
      default: "none",
      [stylex.when.descendant(":focus-visible")]: "solid",
    },
    outlineWidth: "2px",
  },
  wayText: {
    gridArea: "text",
    display: "grid",
    gap: "4px",
  },
  wayName: {
    fontSize: {
      default: awbrnVars.text17,
      [pageLayout.phoneMedia]: awbrnVars.text16,
    },
    lineHeight: 1.25,
  },
  // The name is the link; it stretches over the whole row, and the row
  // carries its focus ring.
  wayLink: {
    color: colorVars["--color-text-primary"],
    textDecoration: "none",
    outline: "none",
    "::after": {
      content: "''",
      position: "absolute",
      inset: 0,
    },
  },
  wayLine: {
    color: colorVars["--color-text-secondary"],
    fontSize: textSizeVars["--font-size-base"],
    lineHeight: 1.5,
  },
  wayReal: {
    gridArea: "real",
    alignItems: "center",
    minInlineSize: 0,
    pointerEvents: "none",
    userSelect: "none",
  },
  tier: {
    flexShrink: 0,
    inlineSize: {
      default: "300px",
      [pageLayout.phoneMedia]: "100%",
    },
  },
  wayGo: {
    gridArea: "go",
    alignSelf: "center",
    color: awbrnVars.textTertiary,
  },
  lobby: {
    alignItems: "center",
    gap: "14px",
    minInlineSize: 0,
  },
  lobbyMain: {
    display: "grid",
    gap: "6px",
    minInlineSize: 0,
  },
  lobbyTitle: {
    minInlineSize: 0,
    fontSize: textSizeVars["--font-size-base"],
    fontWeight: 700,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  lobbyMeta: {
    flexWrap: "wrap",
    alignItems: "center",
    columnGap: "12px",
    rowGap: "6px",
  },
  lobbyHost: {
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    fontSize: awbrnVars.text13_5,
    fontWeight: 600,
  },
  inlinePostage: {
    display: "inline",
  },
  mapSheet: {
    display: "grid",
    gridTemplateColumns: {
      default: "352px minmax(0, 1fr)",
      [pageLayout.phoneMedia]: "minmax(0, 1fr)",
    },
  },
  feat: {
    display: "grid",
    alignContent: "start",
    gap: "12px",
    paddingBlock: {
      default: "16px",
      [pageLayout.phoneMedia]: "16px 12px",
    },
    paddingInline: "16px",
    color: colorVars["--color-text-primary"],
    textDecoration: "none",
    borderInlineEndColor: awbrnVars.rule,
    borderInlineEndStyle: "solid",
    borderInlineEndWidth: {
      default: "1px",
      [pageLayout.phoneMedia]: 0,
    },
    borderBlockEndColor: awbrnVars.rule,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: {
      default: 0,
      [pageLayout.phoneMedia]: "1px",
    },
  },
  featWell: {
    display: "grid",
    placeItems: "center",
    paddingBlockEnd: "8px",
    overflow: "hidden",
    borderRadius: "6px",
    backgroundColor: awbrnVars.ink900,
  },
  featPicture: {
    display: "block",
    maxInlineSize: "none",
    imageRendering: "pixelated",
  },
  mapName: {
    alignItems: "center",
    gap: "12px",
  },
  mapNameText: {
    gap: "2px",
    flexGrow: 1,
    minInlineSize: 0,
  },
  // The compact pennant beside a postage line; a phone keeps the sm pennant.
  mapMedal: {
    inlineSize: { default: "24px", [pageLayout.phoneMedia]: "26px" },
    blockSize: { default: "30px", [pageLayout.phoneMedia]: "34px" },
    paddingBlockEnd: { default: "5px", [pageLayout.phoneMedia]: "6px" },
    fontSize: {
      default: typeScaleVars["--text-label-size"],
      [pageLayout.phoneMedia]: typeScaleVars["--text-body-size"],
    },
  },
  mapTitle: {
    fontSize: textSizeVars["--font-size-base"],
    fontWeight: 700,
  },
  mapList: {
    minInlineSize: 0,
  },
  mapRowItem: {
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: {
      default: "1px",
      ":first-child": 0,
    },
  },
  notPhone: {
    display: {
      default: "block",
      [pageLayout.phoneMedia]: "none",
    },
  },
  mapRow: {
    display: "flex",
    alignItems: "center",
    gap: "12px",
    paddingBlock: {
      default: "18px",
      [pageLayout.phoneMedia]: "12px",
    },
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
    color: colorVars["--color-text-primary"],
    textDecoration: "none",
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
      ":active": awbrnVars.paper,
    },
  },
  more: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "6px",
    minBlockSize: "56px",
    marginBlockStart: "auto",
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
    color: {
      default: colorVars["--color-text-primary"],
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.blue },
      ":active": awbrnVars.blue,
    },
    fontSize: textSizeVars["--font-size-base"],
    fontWeight: 650,
    textDecoration: "none",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
  },
  moreIcon: {
    color: awbrnVars.textTertiary,
  },
  band: {
    backgroundColor: awbrnVars.ink950,
  },
  bandIn: {
    display: "grid",
    gridTemplateColumns: {
      default: "360px minmax(0, 1fr)",
      [pageLayout.belowRailMedia]: "minmax(0, 1fr)",
    },
    alignItems: "center",
    gap: {
      default: "64px",
      [BELOW_RAIL_MEDIA]: "32px",
      [pageLayout.phoneMedia]: "28px",
    },
    maxInlineSize: pageLayout.standardWidth,
    marginInline: "auto",
    paddingBlock: {
      default: "96px",
      [pageLayout.phoneMedia]: "56px",
    },
  },
  bandKey: {
    marginBlockStart: {
      default: "32px",
      [pageLayout.phoneMedia]: "24px",
    },
  },
  close: {
    display: "grid",
    justifyItems: "center",
    gap: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
    paddingBlock: {
      default: "112px 120px",
      [pageLayout.phoneMedia]: "64px 72px",
    },
    textAlign: "center",
  },
  closeMark: {
    inlineSize: {
      default: "88px",
      [pageLayout.phoneMedia]: "72px",
    },
    blockSize: {
      default: "88px",
      [pageLayout.phoneMedia]: "72px",
    },
  },
});
