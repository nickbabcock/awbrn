import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import {
  useInfiniteQuery,
  useQuery,
  useSuspenseInfiniteQuery,
  useSuspenseQuery,
} from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { Link } from "@tanstack/react-router";
import { useId, useMemo } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { chooseFirstMatchMap } from "#/home/depicted.ts";
import { FirstMatch } from "#/home/FirstMatch.tsx";
import { Landing, rankedMapsQueryOptions } from "#/home/Landing.tsx";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import type { MapCatalogEntry } from "#/maps/schemas.ts";
import { matchesBrowseQueryOptions, myMatchesQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchBrowseSummary, MyMatchSummary } from "#/matches/schemas.ts";
import { MatchFaces, MatchRow, MatchTitle } from "#/play/MatchRow.tsx";
import { headingLooks } from "#/play/headings.ts";
import { MatchThumb, useMinimapState } from "#/play/MatchThumb.tsx";
import { Postage, mapPostage } from "#/play/Postage.tsx";
import { matchKeyLabel, orderMatchRows, type MatchRowData } from "#/play/match_rows.ts";
import {
  clockTerms,
  isClockLow,
  seatsFigure,
  timeLeftFigure,
  timeLeftLabel,
} from "#/play/terms.ts";
import { useListNow, useRankedMatches } from "#/play/useRankedMatches.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { CountBadge } from "#/ui/CountBadge.tsx";
import { EmptySheet } from "#/ui/EmptySheet.tsx";
import { IconMap, IconNext, IconRoute } from "#/ui/icons.tsx";
import { RouterListRow } from "#/ui/ListRow.tsx";
import { Page } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Sheet, SheetBody, SheetHead, SheetRow, SheetRows } from "#/ui/Sheet.tsx";
import { TimeLeft } from "#/ui/TimeLeft.tsx";

/** How many open lobbies the rail lists before its head hands off to the full list. */
const RAIL_LOBBY_COUNT = 4;

/** The lead card's map well, and the room the minimap gets inside it. */
const LEAD_WELL = { desktop: 176, phone: 112 } as const;
const LEAD_ROOM = { desktop: 160, phone: 80 } as const;

export function HomePage() {
  const session = useAppSession();
  return session ? <PlayerHome viewerId={session.user.id} /> : <Landing />;
}

/**
 * Home is triage: what waits on the player leads; what waits on somebody else
 * is one row into My games.
 */
function PlayerHome({ viewerId }: { viewerId: string }) {
  const { data } = useSuspenseQuery(myMatchesQueryOptions());
  const lobbies = useOpenLobbies();
  const { confirmDeadlines, rankedMaps } = useRankedMatches(true);

  const rows = useMemo(
    () => orderMatchRows(data.matches, confirmDeadlines, rankedMaps),
    [confirmDeadlines, data.matches, rankedMaps],
  );
  const owed = rows.filter((row) => row.isOwed);
  const waiting = rows.filter((row) => !row.isOwed);
  const now = useListNow(owed.map((row) => row.deadlineAt));
  // A lobby the player already sits in is in their queue; the rail is for
  // lobbies they could still join.
  const joinable = useMemo(() => {
    const mine = new Set(data.matches.map((match) => match.matchId));
    return lobbies.filter((lobby) => !mine.has(lobby.matchId));
  }, [data.matches, lobbies]);

  const [lead, ...rest] = owed;

  if (data.matches.length === 0) {
    return <FirstRunHome lobbies={joinable} />;
  }

  return (
    <Page>
      <VStack xstyle={styles.grid}>
        <VStack xstyle={styles.column}>
          {lead ? (
            <OwedSheet count={owed.length} lead={lead} now={now} rest={rest} viewerId={viewerId} />
          ) : (
            <>
              <VisuallyHidden>
                <Heading level={1}>Home</Heading>
              </VisuallyHidden>
              <EmptySheet title="Nothing waits on you" />
            </>
          )}
          {waiting.length > 0 ? <WaitingLink count={waiting.length} /> : null}
        </VStack>
        <aside aria-label="More to play" {...stylex.props(styles.column)}>
          <LobbiesSheet lobbies={joinable} />
          <NewlyRanked />
          <PlanLink />
        </aside>
      </VStack>
    </Page>
  );
}

/** Newly ranked drops to its row here so the first match stays the brightest art. */
function FirstRunHome({ lobbies }: { lobbies: MatchBrowseSummary[] }) {
  const { data } = useSuspenseInfiniteQuery(rankedMapsQueryOptions());
  const map = useMemo(() => chooseFirstMatchMap(data.pages.flatMap((page) => page.maps)), [data]);
  return (
    <Page>
      <VStack xstyle={styles.grid}>
        <VStack xstyle={styles.column}>
          <FirstMatch lobbies={lobbies.slice(0, RAIL_LOBBY_COUNT)} map={map} />
        </VStack>
        <aside aria-label="More to play" {...stylex.props(styles.column)}>
          <NewlyRanked isCompact />
          <PlanLink />
        </aside>
      </VStack>
    </Page>
  );
}

function OwedSheet({
  count,
  lead,
  now,
  rest,
  viewerId,
}: {
  count: number;
  lead: MatchRowData;
  now: number;
  rest: MatchRowData[];
  viewerId: string;
}) {
  const headingId = useId();
  return (
    <Sheet aria-labelledby={headingId} isOwed>
      <SheetHead>
        <Heading id={headingId} level={1}>
          Your move{" "}
          <CountBadge
            count={count}
            label={count === 1 ? "1 match" : `${count} matches`}
            size="heading"
            tone="ink"
          />
        </Heading>
      </SheetHead>
      <SheetBody>
        <LeadCard now={now} row={lead} viewerId={viewerId} />
      </SheetBody>
      {rest.length > 0 ? (
        <SheetRows>
          {rest.map((row) => (
            <SheetRow key={row.match.matchId}>
              <MatchRow isAboveFold now={now} row={row} viewerId={viewerId} />
            </SheetRow>
          ))}
        </SheetRows>
      ) : null}
    </Sheet>
  );
}

function LeadCard({ now, row, viewerId }: { now: number; row: MatchRowData; viewerId: string }) {
  const { match } = row;
  const remaining = row.deadlineAt === null ? null : row.deadlineAt - now;
  const tone =
    remaining !== null && match.phase === "active" && isClockLow(remaining, match.settings.clock)
      ? "urgent"
      : "default";

  return (
    <article {...stylex.props(styles.lead)}>
      <LeadBoard match={match} />
      <VStack xstyle={styles.leadBody}>
        <Heading level={2} xstyle={styles.leadTitle}>
          <MatchTitle isRanked={row.isRanked} mapName={row.mapName} match={match} />
        </Heading>
        <MatchFaces match={match} viewerId={viewerId} />
      </VStack>
      <HStack xstyle={styles.leadAct}>
        {remaining !== null ? (
          <TimeLeft
            label={timeLeftLabel(remaining)}
            size="lead"
            tone={tone}
            value={timeLeftFigure(remaining)}
          />
        ) : null}
        <RouterButton
          label={matchKeyLabel(row)}
          params={{ matchId: match.matchId }}
          size="lg"
          to="/matches/$matchId"
          variant="primary"
          xstyle={styles.leadKey}
        />
      </HStack>
    </article>
  );
}

/**
 * Paper until the minimap is drawn, then the minimap on ink at the largest
 * whole scale that fits, cropped to its middle when even 1x is larger.
 */
function LeadBoard({
  match,
}: {
  match: Pick<MyMatchSummary, "mapId" | "mapRevision" | "matchId">;
}) {
  const { data: map, isError } = useQuery(
    mapCatalogEntryQueryOptions(match.mapId, match.mapRevision),
  );
  const { imageProps, state } = useMinimapState();
  const isFailed = isError || state === "failed";
  return (
    <Link
      aria-hidden="true"
      params={{ matchId: match.matchId }}
      tabIndex={-1}
      to="/matches/$matchId"
      {...stylex.props(styles.leadBoard, state === "ready" && map && styles.leadBoardInked)}
    >
      {isFailed ? (
        <Icon icon={IconMap} size="lg" xstyle={styles.leadBoardFailed} />
      ) : map ? (
        <LeadMinimap imageProps={imageProps} isReady={state === "ready"} map={map} />
      ) : null}
    </Link>
  );
}

function LeadMinimap({
  imageProps,
  isReady,
  map,
}: {
  imageProps: ReturnType<typeof useMinimapState>["imageProps"];
  isReady: boolean;
  map: MapCatalogEntry;
}) {
  const picture = mapScreenshotSize("small", map.width, map.height);
  const longest = Math.max(picture.width, picture.height);
  const desktop = Math.max(1, Math.floor(LEAD_ROOM.desktop / longest));
  const phone = Math.max(1, Math.floor(LEAD_ROOM.phone / longest));
  return (
    <img
      alt=""
      fetchPriority="high"
      height={picture.height}
      src={map.screenshot.small}
      width={picture.width}
      {...imageProps}
      {...stylex.props(
        styles.leadPicture,
        !isReady && styles.leadPicturePending,
        styles.leadPictureSize(
          picture.width * desktop,
          picture.height * desktop,
          picture.width * phone,
          picture.height * phone,
        ),
      )}
    />
  );
}

/** One row into My games, not a second copy of that list. */
function WaitingLink({ count }: { count: number }) {
  return (
    <Link
      aria-label={`Waiting: ${count === 1 ? "1 match" : `${count} matches`}, in My games`}
      to="/my/matches"
      {...stylex.props(styles.waiting)}
    >
      <Text color="inherit" type="inherit" xstyle={styles.waitingTitle}>
        Waiting
      </Text>
      <Text color="inherit" type="inherit" xstyle={styles.waitingCount}>
        {count}
      </Text>
      <Icon icon={IconNext} size="md" />
    </Link>
  );
}

function LobbiesSheet({ lobbies }: { lobbies: MatchBrowseSummary[] }) {
  const headingId = useId();
  return (
    <Sheet aria-labelledby={headingId}>
      <SheetHead>
        <Heading id={headingId} level={2} xstyle={headingLooks.title}>
          <Link to="/matches" {...stylex.props(styles.headLink)}>
            Open lobbies
            <Icon icon={IconNext} size="sm" xstyle={styles.headLinkIcon} />
          </Link>
        </Heading>
      </SheetHead>
      {lobbies.length === 0 ? null : (
        <SheetRows>
          {lobbies.slice(0, RAIL_LOBBY_COUNT).map((lobby) => (
            <SheetRow key={lobby.matchId}>
              <RouterListRow
                media={
                  <MatchThumb isEager mapId={lobby.mapId} revision={lobby.mapRevision} size={56} />
                }
                meta={
                  <Postage>
                    {clockTerms(lobby.settings.clock)} ·{" "}
                    {seatsFigure(lobby.participantCount, lobby.maxPlayers)}
                  </Postage>
                }
                params={{ matchId: lobby.matchId }}
                title={lobby.name}
                to="/matches/$matchId"
              />
            </SheetRow>
          ))}
        </SheetRows>
      )}
    </Sheet>
  );
}

/**
 * The newest ranked map: the whole map on desktop; a row on a phone, and at
 * every width when `isCompact`.
 */
function NewlyRanked({ isCompact = false }: { isCompact?: boolean }) {
  const headingId = useId();
  const { data } = useInfiniteQuery(rankedMapsQueryOptions());
  const map = data?.pages[0]?.maps[0] ?? null;
  if (!map) return null;
  const full = mapScreenshotSize("full", map.width, map.height);

  return (
    <Sheet aria-labelledby={headingId}>
      <SheetHead actions={isCompact ? null : <MapRankMedal rank={map.rank} size="sm" />}>
        <Heading id={headingId} level={2} xstyle={headingLooks.title}>
          Newly ranked
        </Heading>
      </SheetHead>
      {isCompact ? null : (
        <Link
          params={{ mapId: map.mapId }}
          to="/maps/$mapId"
          {...stylex.props(styles.featured, styles.notPhone)}
        >
          <VStack as="span" xstyle={styles.featuredWell}>
            <MapPicture
              alt={`The ${map.name} battlefield`}
              maxScale={1}
              sourceHeight={full.height}
              sourceWidth={full.width}
              src={map.screenshot.full}
            />
          </VStack>
          <VStack as="span" xstyle={styles.featuredName}>
            <b>{map.name}</b>
            <Postage>{mapPostage(map)}</Postage>
          </VStack>
        </Link>
      )}
      <VStack xstyle={!isCompact && styles.phoneOnly}>
        <SheetRows>
          <SheetRow>
            <RouterListRow
              end={isCompact ? <MapRankMedal rank={map.rank} size="sm" /> : undefined}
              hasChevron={!isCompact}
              media={<MatchThumb map={map} size={56} />}
              meta={<Postage>{mapPostage(map)}</Postage>}
              params={{ mapId: map.mapId }}
              title={map.name}
              to="/maps/$mapId"
            />
          </SheetRow>
        </SheetRows>
      </VStack>
    </Sheet>
  );
}

function PlanLink() {
  return (
    <Link to="/planner" {...stylex.props(styles.quiet)}>
      <Icon icon={IconRoute} size="md" xstyle={styles.quietIcon} />
      <b>Plan a turn</b>
      <Icon icon={IconNext} size="sm" xstyle={styles.quietChevron} />
    </Link>
  );
}

function useOpenLobbies(): MatchBrowseSummary[] {
  const { data } = useSuspenseInfiniteQuery(matchesBrowseQueryOptions());
  return useMemo(() => data.pages.flatMap((page) => page.matches).slice(0, 8), [data]);
}

const styles = stylex.create({
  grid: {
    display: "grid",
    gridTemplateColumns: {
      default: `minmax(0, 1fr) ${pageLayout.railWidth}`,
      [pageLayout.belowRailMedia]: "minmax(0, 1fr)",
    },
    alignItems: "start",
    gap: {
      default: "24px",
      [pageLayout.phoneMedia]: "16px",
    },
  },
  column: {
    display: "flex",
    flexDirection: "column",
    gap: {
      default: "24px",
      [pageLayout.phoneMedia]: "16px",
    },
    minInlineSize: 0,
  },
  lead: {
    display: "grid",
    gridTemplateColumns: {
      default: "auto minmax(0, 1fr)",
      [pageLayout.phoneMedia]: `${LEAD_WELL.phone}px minmax(0, 1fr)`,
    },
    gridTemplateAreas: {
      default: '"board body" "board act"',
      [pageLayout.phoneMedia]: '"board body" "act act"',
    },
    columnGap: {
      default: "24px",
      [pageLayout.phoneMedia]: "14px",
    },
    rowGap: {
      default: 0,
      [pageLayout.phoneMedia]: "12px",
    },
  },
  leadBoard: {
    gridArea: "board",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    alignSelf: "start",
    overflow: "hidden",
    inlineSize: {
      default: `${LEAD_WELL.desktop}px`,
      [pageLayout.phoneMedia]: `${LEAD_WELL.phone}px`,
    },
    blockSize: {
      default: `${LEAD_WELL.desktop}px`,
      [pageLayout.phoneMedia]: `${LEAD_WELL.phone}px`,
    },
    borderRadius: "6px",
    backgroundColor: awbrnVars.paperWell,
  },
  leadBoardInked: {
    backgroundColor: awbrnVars.ink900,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink700}`,
  },
  leadBoardFailed: {
    color: awbrnVars.textTertiary,
  },
  leadPicturePending: {
    visibility: "hidden",
  },
  leadPicture: {
    display: "block",
    flexShrink: 0,
    maxInlineSize: "none",
    imageRendering: "pixelated",
  },
  leadPictureSize: (dw: number, dh: number, pw: number, ph: number) => ({
    inlineSize: {
      default: `${dw}px`,
      [pageLayout.phoneMedia]: `${pw}px`,
    },
    blockSize: {
      default: `${dh}px`,
      [pageLayout.phoneMedia]: `${ph}px`,
    },
  }),
  leadBody: {
    gridArea: "body",
    alignSelf: {
      default: "end",
      [pageLayout.phoneMedia]: "center",
    },
    display: "grid",
    gap: "8px",
    minInlineSize: 0,
  },
  leadTitle: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  leadAct: {
    gridArea: "act",
    alignSelf: "start",
    alignItems: {
      default: "flex-end",
      [pageLayout.phoneMedia]: "center",
    },
    justifyContent: "space-between",
    flexWrap: "wrap",
    gap: "16px",
    marginBlockStart: {
      default: "16px",
      [pageLayout.phoneMedia]: 0,
    },
  },
  leadKey: {
    flexGrow: {
      default: 0,
      [pageLayout.phoneMedia]: 1,
    },
    minBlockSize: {
      default: null,
      [pageLayout.phoneMedia]: "52px",
    },
  },
  waiting: {
    display: "flex",
    alignItems: "center",
    gap: "12px",
    minBlockSize: "64px",
    paddingInline: "20px",
    borderRadius: "10px",
    backgroundColor: {
      default: awbrnVars.paperWell,
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.rule },
      ":active": awbrnVars.rule,
    },
    color: colorVars["--color-text-secondary"],
    textDecoration: "none",
    transitionProperty: "background-color",
    transitionDuration: {
      default: "90ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionTimingFunction: awbrnVars.easeStep,
  },
  waitingTitle: {
    color: colorVars["--color-text-primary"],
    fontSize: awbrnVars.text17,
    fontWeight: 700,
  },
  waitingCount: {
    marginInlineStart: "auto",
    fontFamily: awbrnVars.fontFigure,
    fontStretch: "87.5%",
    fontVariantNumeric: "tabular-nums",
    fontSize: textSizeVars["--font-size-base"],
    fontWeight: 600,
  },
  headLink: {
    display: "inline-flex",
    alignItems: "center",
    gap: "6px",
    minBlockSize: "44px",
    marginBlock: "-10px",
    color: "inherit",
    textDecoration: {
      default: "none",
      ":hover": { default: null, [pageLayout.hoverMedia]: "underline" },
      ":active": "underline",
    },
    textUnderlineOffset: "3px",
  },
  headLinkIcon: {
    color: colorVars["--color-text-secondary"],
  },
  featured: {
    display: "block",
    paddingInline: "16px",
    paddingBlockEnd: "16px",
    color: "inherit",
    textDecoration: "none",
  },
  notPhone: {
    display: {
      default: "block",
      [pageLayout.phoneMedia]: "none",
    },
  },
  phoneOnly: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "block",
    },
  },
  featuredWell: {
    display: "grid",
    placeItems: "center",
    padding: "0 0 8px",
    overflow: "hidden",
    borderRadius: "6px",
    backgroundColor: awbrnVars.ink900,
  },
  featuredName: {
    display: "grid",
    gap: "4px",
    marginBlockStart: "12px",
  },
  quiet: {
    display: "grid",
    gridTemplateColumns: "22px minmax(0, 1fr) 16px",
    alignItems: "center",
    gap: "12px",
    minBlockSize: "56px",
    paddingInline: "18px",
    borderRadius: "10px",
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.sheet },
      ":active": awbrnVars.sheet,
    },
    color: colorVars["--color-text-primary"],
    textDecoration: "none",
  },
  quietIcon: {
    color: colorVars["--color-text-secondary"],
  },
  quietChevron: {
    color: awbrnVars.textTertiary,
  },
});
