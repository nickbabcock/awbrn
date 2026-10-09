import {
  useInfiniteQuery,
  useQuery,
  useSuspenseInfiniteQuery,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Badge } from "@astryxdesign/core/Badge";
import { Card } from "@astryxdesign/core/Card";
import { Heading } from "@astryxdesign/core/Heading";
import { useMediaQuery } from "@astryxdesign/core/hooks";
import { List, ListItem } from "@astryxdesign/core/List";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  borderVars,
  colorVars,
  radiusVars,
  spacingVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useMemo, useState } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { getCoPortraitByAwbwId } from "#/components/co_portraits.ts";
import { FactionCrest } from "#/components/FactionCrest.tsx";
import { getFactionById } from "#/factions.ts";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { MapThumb } from "#/maps/components/MapThumb.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapCatalogQueryOptions } from "#/maps/maps.queries.ts";
import { clockTickMs, formatClockSummary, formatTurnRemaining } from "#/matches/match_clock.ts";
import {
  matchDetailQueryOptions,
  matchesBrowseQueryOptions,
  myMatchesQueryOptions,
} from "#/matches/matches.queries.ts";
import { myMatchPhaseRank, needsViewerAction, viewerStatusLabel } from "#/matches/my_matches.ts";
import type { MatchBrowseSummary, MatchSettings, MyMatchSummary } from "#/matches/schemas.ts";
import { rankedOverviewQueryOptions } from "#/matchmaking/matchmaking.queries.ts";
import { PushToggle } from "#/players/components/PushToggle.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { Page } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/** How many open lobbies the rail lists before it hands off to the full list. */
const RAIL_LOBBY_COUNT = 5;

/** The CO face beside an opponent's name: large enough to know, small enough to scan. */
const OPPONENT_FACE_SIZE = 24;

export function HomePage() {
  const session = useAppSession();
  return session ? <PlayerHome viewerId={session.user.id} /> : <VisitorHome />;
}

/**
 * A signed-in player: one queue, read top to bottom.
 *
 * Every row carries what a player decides on, the opponent, the time left,
 * and the terms, and its own key, so there is nothing to select before
 * acting. What waits on the player comes first; what waits on somebody else
 * follows under its own heading.
 */
function PlayerHome({ viewerId }: { viewerId: string }) {
  const { data } = useSuspenseQuery(myMatchesQueryOptions());
  const lobbies = useOpenLobbies();
  const confirmDeadlines = useConfirmDeadlines();

  const ordered = useMemo(
    () => orderMyMatches(data.matches, confirmDeadlines),
    [confirmDeadlines, data.matches],
  );
  const now = useNow(ordered.map((row) => row.deadlineAt));
  const owed = ordered.filter((row) => row.isOwed);
  // A lobby the player already holds a seat in is in the queue; the rail is
  // for lobbies they could still join.
  const joinable = useMemo(() => {
    const mine = new Set(data.matches.map((match) => match.matchId));
    return lobbies.filter((lobby) => !mine.has(lobby.matchId));
  }, [data.matches, lobbies]);
  const waiting = ordered.filter((row) => !row.isOwed);

  return (
    <Page>
      <VStack gap={6} xstyle={styles.split}>
        <VStack gap={6} xstyle={styles.main}>
          {ordered.length === 0 ? (
            <Card padding={6}>
              <VStack gap={3}>
                <Heading level={1}>Your move</Heading>
                <Text color="secondary">
                  You are not in a match yet. Join an open lobby, or start your own.
                </Text>
                <HStack gap={2} wrap="wrap">
                  <RouterButton label="Browse lobbies" to="/matches" variant="primary" />
                  <RouterButton label="New match" to="/matches/new" variant="secondary" />
                </HStack>
              </VStack>
            </Card>
          ) : (
            <>
              <MatchGroup
                heading="Your move"
                headingLevel={1}
                now={now}
                rows={owed}
                summary={
                  owed.length === 0
                    ? "Nothing waits on you. Your opponents are moving."
                    : owed.length === 1
                      ? "1 match waits on you."
                      : `${owed.length} matches wait on you.`
                }
                viewerId={viewerId}
              />
              {waiting.length > 0 ? (
                <MatchGroup
                  heading="Waiting on others"
                  headingLevel={2}
                  now={now}
                  rows={waiting}
                  viewerId={viewerId}
                />
              ) : null}
            </>
          )}
        </VStack>

        <VStack gap={6} xstyle={styles.rail}>
          <StartPanel isSignedIn />
          <LobbyRail lobbies={joinable} />
        </VStack>
      </VStack>
    </Page>
  );
}

/** A visitor: what AWBRN is, said once, then the lobbies they could join. */
function VisitorHome() {
  const lobbies = useOpenLobbies();

  return (
    <Page>
      <Card padding={6} xstyle={styles.intro}>
        <VStack as="header" gap={3}>
          <Heading level={1}>Advance Wars, in the browser</Heading>
          <Text color="secondary" type="large" xstyle={styles.introText}>
            Play Advance Wars against people or the CPU on maps from the AWBW community, and step
            through any replay turn by turn. Nothing to install.
          </Text>
        </VStack>
      </Card>
      <VStack gap={6} xstyle={styles.split}>
        <VStack gap={6} xstyle={styles.main}>
          {lobbies.length > 0 ? (
            <Card padding={0} xstyle={styles.panel}>
              <VStack gap={1} xstyle={styles.panelHead}>
                <Heading level={2}>Open lobbies</Heading>
                <Text color="secondary">Public matches looking for players.</Text>
              </VStack>
              <List density="spacious" hasDividers>
                {lobbies.map((lobby) => (
                  <LobbyRow key={lobby.matchId} lobby={lobby} />
                ))}
              </List>
            </Card>
          ) : (
            <FeaturedMap />
          )}
        </VStack>
        <VStack gap={6} xstyle={styles.rail}>
          <StartPanel isSignedIn={false} />
        </VStack>
      </VStack>
    </Page>
  );
}

/** One of the viewer's matches, with what the row needs to say about it. */
interface MatchRowData {
  match: MyMatchSummary;
  isOwed: boolean;
  /** When the thing the viewer owes runs out: a turn, or a pairing's confirm window. */
  deadlineAt: number | null;
}

function MatchGroup({
  heading,
  headingLevel,
  now,
  rows,
  summary,
  viewerId,
}: {
  heading: string;
  headingLevel: 1 | 2;
  now: number;
  rows: MatchRowData[];
  summary?: string;
  viewerId: string;
}) {
  return (
    <Card padding={0} xstyle={styles.panel}>
      <VStack gap={1} xstyle={styles.panelHead}>
        <Heading level={headingLevel}>{heading}</Heading>
        {summary ? <Text color="secondary">{summary}</Text> : null}
      </VStack>
      {rows.length > 0 ? (
        <List density="spacious" hasDividers>
          {rows.map((row) => (
            <MatchRow key={row.match.matchId} now={now} row={row} viewerId={viewerId} />
          ))}
        </List>
      ) : null}
    </Card>
  );
}

/**
 * One match as a row: the map, the name, who the viewer is facing and with
 * which commander, the terms, the time left, and the key.
 */
function MatchRow({ now, row, viewerId }: { now: number; row: MatchRowData; viewerId: string }) {
  const { match, isOwed, deadlineAt } = row;
  const deadline = deadlineAt === null ? null : formatTurnRemaining(deadlineAt - now);
  // A phone has no room beside the name for the status and the key, so they
  // take their own line under it rather than squeezing the name out.
  const isWide = useMediaQuery("(min-width: 600px)");

  const status = (
    <VStack align={isWide ? "end" : "start"} gap={0.5}>
      {/* A row that waits on the player says so with its orange key, so a
          status chip beside it would only repeat the key. A lobby is the
          exception: "Get ready" says what "Open lobby" does not. */}
      {isOwed && (match.phase === "lobby" || match.phase === "draft") ? (
        <Badge label={viewerStatusLabel(match)} variant="warning" />
      ) : isOwed ? null : (
        <Text color="secondary" type="label">
          {viewerStatusLabel(match)}
        </Text>
      )}
      {deadline ? (
        <Text type="label" weight="bold">
          {deadline}
        </Text>
      ) : null}
    </VStack>
  );
  const key = (
    <RouterButton
      label={actionFor(match, isOwed)}
      params={{ matchId: match.matchId }}
      size="sm"
      to="/matches/$matchId"
      variant={isOwed ? "primary" : "secondary"}
    />
  );

  return (
    <ListItem
      description={
        <VStack gap={1}>
          <Opponents match={match} viewerId={viewerId} />
          <Text color="secondary" type="label">
            {terms(match.settings)}
          </Text>
          {isWide ? null : (
            <HStack align="center" gap={3} justify="between" wrap="wrap">
              {status}
              {key}
            </HStack>
          )}
        </VStack>
      }
      endContent={
        isWide ? (
          <HStack align="center" gap={3} justify="end">
            {status}
            {key}
          </HStack>
        ) : undefined
      }
      label={match.name}
      startContent={
        <MapThumb mapId={match.mapId} revision={match.mapRevision} size={isWide ? "md" : "sm"} />
      }
      xstyle={styles.row}
    />
  );
}

/**
 * Who the viewer is facing. The seats come from the match record, which the
 * list of matches does not carry, so the line fills in a moment after the row
 * and holds its height while it does.
 */
function Opponents({ match, viewerId }: { match: MyMatchSummary; viewerId: string }) {
  const { data: detail } = useQuery(matchDetailQueryOptions(match.matchId, null));

  if (!detail) {
    return (
      <Text color="secondary" type="supporting">
        Reading seats…
      </Text>
    );
  }

  const others = [...detail.participants]
    .filter((seat) => seat.userId !== viewerId)
    .sort((left, right) => left.slotIndex - right.slotIndex);
  const open = match.maxPlayers - detail.participants.length;

  if (others.length === 0) {
    return (
      <Text color="secondary" type="supporting">
        {open > 0
          ? `Waiting for ${open === 1 ? "an opponent" : `${open} opponents`}`
          : "Every seat is yours"}
      </Text>
    );
  }

  return (
    <HStack align="center" as="ul" gap={3} role="list" wrap="wrap" xstyle={styles.opponents}>
      <Text aria-hidden color="secondary" type="supporting">
        vs
      </Text>
      {others.map((seat) => {
        const faction = getFactionById(seat.factionId);
        const co = seat.coId === null ? null : getCoPortraitByAwbwId(seat.coId);
        const isOnTheMove = match.activeSlotIndex === seat.slotIndex;
        return (
          <HStack align="center" as="li" gap={1.5} key={seat.slotIndex}>
            {faction ? <FactionCrest factionCode={faction.code} size={16} /> : null}
            {co ? (
              <CoPortrait
                catalog={null}
                coKey={co.key}
                fallbackLabel={co.displayName}
                size={OPPONENT_FACE_SIZE}
              />
            ) : null}
            <Text type="supporting" weight="bold">
              {seat.userName}
            </Text>
            <Text color="secondary" type="supporting">
              {co ? co.displayName : "no CO yet"}
              {isOnTheMove ? " · to move" : ""}
            </Text>
          </HStack>
        );
      })}
      {open > 0 ? (
        <Text color="secondary" type="supporting">
          {open === 1 ? "1 seat open" : `${open} seats open`}
        </Text>
      ) : null}
    </HStack>
  );
}

/** A lobby as a row: the map, the name, the host, the seats, and the way in. */
function LobbyRow({
  isCompact = false,
  lobby,
}: {
  isCompact?: boolean;
  lobby: MatchBrowseSummary;
}) {
  const seats = `${lobby.participantCount}/${lobby.maxPlayers} seats`;
  return (
    <ListItem
      description={
        <Text color="secondary" type="label">
          {isCompact
            ? `${seats} · ${lobby.settings.fogEnabled ? "Fog" : "No fog"}`
            : `Host ${lobby.creatorName} · ${seats} · ${terms(lobby.settings)}`}
        </Text>
      }
      endContent={
        <RouterButton
          label="Open"
          params={{ matchId: lobby.matchId }}
          size="sm"
          to="/matches/$matchId"
          variant="secondary"
        />
      }
      label={lobby.name}
      startContent={
        <MapThumb mapId={lobby.mapId} revision={lobby.mapRevision} size={isCompact ? "sm" : "md"} />
      }
      xstyle={styles.row}
    />
  );
}

/** The open lobbies, in the rail beside the player's own matches. */
function LobbyRail({ lobbies }: { lobbies: MatchBrowseSummary[] }) {
  return (
    <Card padding={0} xstyle={styles.panel}>
      <HStack align="center" gap={2} justify="between" xstyle={styles.panelHead}>
        <Heading level={2}>Open lobbies</Heading>
        <RouterButton label="All" size="sm" to="/matches" variant="ghost" />
      </HStack>
      {lobbies.length === 0 ? (
        <VStack gap={0} padding={4}>
          <Text color="secondary" xstyle={styles.emptyWell}>
            No lobby is open right now. Start one and it appears here for everybody.
          </Text>
        </VStack>
      ) : (
        <List density="balanced" hasDividers>
          {lobbies.slice(0, RAIL_LOBBY_COUNT).map((lobby) => (
            <LobbyRow isCompact key={lobby.matchId} lobby={lobby} />
          ))}
        </List>
      )}
    </Card>
  );
}

/**
 * With no lobby open, a visitor still sees a battlefield: the first map the
 * catalog holds, and the key to start a match on it.
 */
function FeaturedMap() {
  const { data } = useInfiniteQuery(mapCatalogQueryOptions());
  const map = data?.pages[0]?.maps[0] ?? null;
  const picture = map ? mapScreenshotSize("full", map.width, map.height) : null;

  return (
    <Card padding={0} xstyle={styles.panel}>
      <VStack gap={1} xstyle={styles.panelHead}>
        <Heading level={2}>{map ? map.name : "Pick a battlefield"}</Heading>
        <Text color="secondary">
          No lobby is open right now. Every match starts with a map: start one here, or browse the
          catalog.
        </Text>
      </VStack>
      {map && picture ? (
        <VStack align="center" gap={0} xstyle={styles.mapWell}>
          <MapPicture
            alt={`The ${map.name} battlefield`}
            sourceHeight={picture.height}
            sourceWidth={picture.width}
            src={map.screenshot.full}
          />
        </VStack>
      ) : null}
      <HStack align="center" gap={2} justify="between" wrap="wrap" xstyle={styles.panelFoot}>
        <Text color="secondary" type="label">
          {map ? `${map.playerCount}P · ${map.width}×${map.height} · by ${map.author}` : ""}
        </Text>
        <HStack gap={2} wrap="wrap">
          <RouterButton label="Browse maps" to="/maps" variant="secondary" />
          <RouterButton
            label="Start a match here"
            search={map ? { map: map.mapId } : {}}
            to="/matches/new"
            variant="primary"
          />
        </HStack>
      </HStack>
    </Card>
  );
}

/** The keys that start something, at the head of the rail. */
function StartPanel({ isSignedIn }: { isSignedIn: boolean }) {
  return (
    <Card padding={0} variant="muted" xstyle={styles.panel}>
      <VStack gap={3} padding={4}>
        <Text type="label">Start something</Text>
        <HStack gap={2} wrap="wrap">
          <RouterButton label="New match" to="/matches/new" variant="secondary" />
          {isSignedIn ? <RouterButton label="Ranked" to="/ranked" variant="secondary" /> : null}
          <RouterButton label="Review a replay" to="/replay" variant="secondary" />
          {isSignedIn ? null : <RouterButton label="Browse maps" to="/maps" variant="secondary" />}
        </HStack>
        {isSignedIn ? (
          <HStack gap={2} wrap="wrap">
            <PushToggle isSignedIn />
          </HStack>
        ) : null}
      </VStack>
    </Card>
  );
}

function useOpenLobbies(): MatchBrowseSummary[] {
  const { data } = useSuspenseInfiniteQuery(matchesBrowseQueryOptions());
  return useMemo(() => data.pages.flatMap((page) => page.matches).slice(0, 8), [data]);
}

/**
 * The window each pending ranked pairing has to be confirmed in, by match.
 * The ranked overview holds it; the list of matches does not.
 */
function useConfirmDeadlines(): ReadonlyMap<string, number> {
  const { data: ranked } = useQuery(rankedOverviewQueryOptions());
  return useMemo(
    () =>
      new Map(
        (ranked?.pools ?? []).flatMap((pool) =>
          pool.pending.map((pairing) => [pairing.matchId, Date.parse(pairing.deadlineAt)] as const),
        ),
      ),
    [ranked],
  );
}

/**
 * What the viewer owes first, soonest deadline first, then everything waiting
 * on somebody else in the order the match list already ranks phases.
 */
function orderMyMatches(
  matches: MyMatchSummary[],
  confirmDeadlines: ReadonlyMap<string, number>,
): MatchRowData[] {
  return matches
    .map((match): MatchRowData => {
      const isOwed = needsViewerAction(match);
      const deadlineAt =
        isOwed && match.phase === "active" && match.turnDeadlineAt !== null
          ? Date.parse(match.turnDeadlineAt)
          : isOwed && match.phase === "pending"
            ? (confirmDeadlines.get(match.matchId) ?? null)
            : null;
      return { match, isOwed, deadlineAt };
    })
    .sort((left, right) => {
      if (left.isOwed !== right.isOwed) return left.isOwed ? -1 : 1;
      if (left.deadlineAt !== null && right.deadlineAt !== null) {
        return left.deadlineAt - right.deadlineAt;
      }
      if (left.deadlineAt !== null) return -1;
      if (right.deadlineAt !== null) return 1;
      return myMatchPhaseRank(left.match.phase) - myMatchPhaseRank(right.match.phase);
    });
}

function actionFor(match: MyMatchSummary, isOwed: boolean): string {
  switch (match.phase) {
    case "active":
      return isOwed ? "Play turn" : "Watch";
    case "pending":
      return "Confirm pairing";
    case "lobby":
    case "draft":
      return "Open lobby";
    default:
      return "Open";
  }
}

/** The terms of a match in one HUD line. */
function terms(settings: MatchSettings): string {
  return [
    settings.fogEnabled ? "Fog" : "No fog",
    `${settings.startingFunds.toLocaleString()} funds`,
    formatClockSummary(settings.clock),
  ].join(" · ");
}

/**
 * A clock for the queue, redrawn only as often as the nearest deadline needs,
 * so a page of correspondence matches redraws once a minute and not once a
 * second.
 */
function useNow(deadlines: readonly (number | null)[]): number {
  const [now, setNow] = useState(() => Date.now());
  const next = useMemo(
    () =>
      deadlines
        .filter((deadline): deadline is number => deadline !== null && deadline > now)
        .sort((left, right) => left - right)[0],
    [deadlines, now],
  );

  useEffect(() => {
    if (next === undefined) return;
    const timer = setTimeout(() => setNow(Date.now()), clockTickMs(next - now));
    return () => clearTimeout(timer);
  }, [next, now]);

  return now;
}

const styles = stylex.create({
  // The statement spans the frame, so its edges are the edges the queue and
  // the rail below it share.
  intro: {
    inlineSize: "100%",
  },
  introText: {
    maxInlineSize: "60ch",
  },
  // The queue takes eight columns of twelve and the rail the other four.
  // Below the desktop width they stack, queue first.
  split: {
    display: {
      default: "flex",
      [pageLayout.desktopMedia]: "grid",
    },
    gridTemplateColumns: {
      default: null,
      [pageLayout.desktopMedia]: "minmax(0, 8fr) minmax(0, 4fr)",
    },
    alignItems: {
      default: "stretch",
      [pageLayout.desktopMedia]: "start",
    },
  },
  main: {
    minInlineSize: 0,
  },
  rail: {
    minInlineSize: 0,
  },
  panel: {
    overflow: "hidden",
  },
  panelHead: {
    paddingInline: spacingVars["--spacing-4"],
    paddingBlock: spacingVars["--spacing-4"],
    borderBlockEndColor: colorVars["--color-border-emphasized"],
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  panelFoot: {
    paddingInline: spacingVars["--spacing-4"],
    paddingBlock: spacingVars["--spacing-4"],
    backgroundColor: colorVars["--color-background-surface"],
    borderBlockStartColor: colorVars["--color-border-emphasized"],
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
  row: {
    paddingInline: spacingVars["--spacing-4"],
  },
  opponents: {
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  emptyWell: {
    padding: spacingVars["--spacing-4"],
    backgroundColor: colorVars["--color-background-muted"],
    borderColor: "var(--color-border-soft)",
    borderStyle: "dashed",
    borderWidth: borderVars["--border-width"],
    borderRadius: radiusVars["--radius-element"],
  },
  mapWell: {
    padding: spacingVars["--spacing-3"],
    backgroundColor: colorVars["--color-background-muted"],
  },
});
