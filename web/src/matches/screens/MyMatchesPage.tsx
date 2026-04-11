import { useSuspenseQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { awbwSmallMapAssetPath } from "#/awbw/paths.ts";
import { getCoPortraitByAwbwId } from "#/components/co_portraits.ts";
import { getFactionById } from "#/factions.ts";
import {
  Badge,
  ButtonLink,
  EmptyState,
  Frame,
  Heading,
  Inline,
  Kicker,
  Page,
  Section,
  Stack,
  Text,
} from "#/ui/primitives.tsx";
import { sxClassName } from "#/ui/stylex.ts";
import { media, tokens } from "#/ui/theme.stylex.ts";
import type { MatchPhase, MyMatchSummary } from "#/matches/schemas.ts";
import { formatMyMatchPhaseLabel, myMatchActionLabel } from "#/matches/my_matches.ts";
import { myMatchesQueryOptions } from "#/matches/matches.queries.ts";

export function MyMatchesPage() {
  const { data } = useSuspenseQuery(myMatchesQueryOptions());
  const { loadedAt, matches } = data;

  return (
    <Page width="wide">
      <Section>
        <Stack gap="lg">
          <div {...stylex.props(styles.header)}>
            <Stack gap="sm">
              <Kicker>My Matches</Kicker>
              <Heading size="display">Ongoing games</Heading>
              <Text size="lg" tone="strong" xstyle={styles.headerText}>
                Jump back into lobbies and active matches you have joined.
              </Text>
            </Stack>
            <Inline gap="sm" xstyle={styles.headerActions}>
              <ButtonLink size="sm" tone="brand" to="/matches/new">
                Create Match
              </ButtonLink>
              <ButtonLink size="sm" tone="neutral" variant="outline" to="/matches">
                Browse Lobbies
              </ButtonLink>
            </Inline>
          </div>

          {matches.length === 0 ? (
            <Frame xstyle={styles.stateFrame}>
              <EmptyState
                kicker="No Ongoing Games"
                title="You are not in any active matches or lobbies"
                description="Create a match or join an open lobby to see it here."
                actions={
                  <>
                    <ButtonLink size="sm" tone="brand" to="/matches/new">
                      Create Match
                    </ButtonLink>
                    <ButtonLink size="sm" tone="neutral" variant="outline" to="/matches">
                      Browse Lobbies
                    </ButtonLink>
                  </>
                }
              />
            </Frame>
          ) : (
            <Frame padding="none">
              <div {...stylex.props(styles.listHeader)}>
                <Text size="sm" tone="muted" xstyle={styles.listMeta}>
                  {matches.length === 1 ? "1 ongoing game" : `${matches.length} ongoing games`}
                </Text>
              </div>
              <div {...stylex.props(styles.list)}>
                {matches.map((match) => (
                  <MyMatchRow key={match.matchId} loadedAt={loadedAt} match={match} />
                ))}
              </div>
            </Frame>
          )}
        </Stack>
      </Section>
    </Page>
  );
}

function MyMatchRow({ loadedAt, match }: { loadedAt: string; match: MyMatchSummary }) {
  const faction = getFactionById(match.viewerParticipant.factionId);
  const coName = getCoPortraitByAwbwId(match.viewerParticipant.coId)?.displayName ?? "No CO";

  return (
    <Link className={rowClassName} params={{ matchId: match.matchId }} to="/matches/$matchId">
      <div {...stylex.props(styles.rowMain)}>
        <div {...stylex.props(styles.thumbWrap)}>
          <img
            alt={`Map preview for ${match.name}`}
            src={awbwSmallMapAssetPath(match.mapId)}
            {...stylex.props(styles.thumb)}
          />
        </div>

        <Stack gap="xs" xstyle={styles.rowTitleBlock}>
          <Inline gap="sm" xstyle={styles.titleLine}>
            <Heading size="lg" xstyle={styles.rowTitle}>
              {match.name}
            </Heading>
            <Badge tone={phaseBadgeTone(match.phase)} xstyle={styles.phaseBadge}>
              {formatMyMatchPhaseLabel(match.phase)}
            </Badge>
          </Inline>
          <Inline gap="sm" xstyle={styles.rowMetaWrap}>
            <Text size="sm" tone="muted" xstyle={styles.rowMeta}>
              Host {match.creatorName}
            </Text>
            <Text size="sm" tone="muted" xstyle={styles.rowMeta}>
              Map {match.mapId}
            </Text>
            <Text size="sm" tone="muted" xstyle={styles.rowMeta}>
              {match.isPrivate ? "Private" : "Public"}
            </Text>
            <Text size="sm" tone="muted" xstyle={styles.rowMeta}>
              {match.settings.fogEnabled ? "Fog on" : "Fog off"}
            </Text>
            <Text size="sm" tone="muted" xstyle={styles.rowMeta}>
              {match.settings.startingFunds.toLocaleString()} funds
            </Text>
          </Inline>
          <Text size="sm" tone="muted">
            Slot {match.viewerParticipant.slotIndex + 1}: {faction?.displayName ?? "Unknown army"} |{" "}
            {coName} | {match.viewerParticipant.ready ? "Ready" : "Not ready"}
          </Text>
        </Stack>

        <div {...stylex.props(styles.rowStats)}>
          <div {...stylex.props(styles.statBlock)}>
            <Text size="sm" tone="muted" xstyle={styles.statLabel}>
              Players
            </Text>
            <Text tone="strong">
              {match.participantCount} / {match.maxPlayers}
            </Text>
          </div>
          <div {...stylex.props(styles.statBlock)}>
            <Text size="sm" tone="muted" xstyle={styles.statLabel}>
              Updated
            </Text>
            <Text tone="strong">{formatRelativeTime(match.updatedAt, loadedAt)}</Text>
          </div>
          <div {...stylex.props(styles.statBlock, styles.actionBlock)}>
            <Text size="sm" tone="muted" xstyle={styles.statLabel}>
              Next
            </Text>
            <Text tone="strong">{myMatchActionLabel(match.phase)}</Text>
          </div>
        </div>
      </div>
    </Link>
  );
}

function phaseBadgeTone(phase: MatchPhase): "neutral" | "brand" | "success" | "danger" {
  switch (phase) {
    case "active":
      return "success";
    case "starting":
      return "brand";
    case "cancelled":
      return "danger";
    case "draft":
    case "lobby":
    case "completed":
      return "neutral";
  }
}

function formatRelativeTime(iso: string, nowIso: string): string {
  const deltaMs = Date.parse(nowIso) - Date.parse(iso);
  const deltaMinutes = Math.round(deltaMs / 60_000);
  const formatter = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

  if (Math.abs(deltaMinutes) < 60) {
    return formatter.format(-deltaMinutes, "minute");
  }

  const deltaHours = Math.round(deltaMinutes / 60);
  if (Math.abs(deltaHours) < 24) {
    return formatter.format(-deltaHours, "hour");
  }

  const deltaDays = Math.round(deltaHours / 24);
  return formatter.format(-deltaDays, "day");
}

const styles = stylex.create({
  header: {
    display: "grid",
    gap: tokens.space4,
    gridTemplateColumns: {
      default: "minmax(0, 1fr) auto",
      [media.compact]: "1fr",
    },
    alignItems: "end",
  },
  headerText: {
    maxWidth: 640,
  },
  headerActions: {
    justifyContent: {
      default: "flex-end",
      [media.compact]: "flex-start",
    },
  },
  stateFrame: {
    minHeight: 280,
  },
  listHeader: {
    minHeight: 54,
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    paddingInline: tokens.space4,
    borderBottomWidth: 3,
    borderBottomStyle: "solid",
    borderBottomColor: tokens.strokeHeavy,
    backgroundColor: tokens.panelRaised,
  },
  listMeta: {
    fontFamily: tokens.fontPixel,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  list: {
    display: "grid",
  },
  row: {
    color: tokens.inkStrong,
    textDecoration: "none",
    borderBottomWidth: 2,
    borderBottomStyle: "solid",
    borderBottomColor: tokens.strokeBase,
    backgroundColor: {
      default: tokens.panelBg,
      ":hover": tokens.panelRaised,
    },
    transitionDuration: tokens.transitionFast,
    transitionProperty: "background-color, transform, box-shadow",
  },
  rowInteractive: {
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
    },
    boxShadow: {
      default: "none",
      ":hover": tokens.shadowHardSm,
      ":active": "none",
    },
  },
  rowMain: {
    display: "grid",
    gap: tokens.space4,
    gridTemplateColumns: {
      default: "128px minmax(0, 1fr) minmax(220px, auto)",
      [media.narrow]: "96px minmax(0, 1fr)",
      [media.compact]: "1fr",
    },
    alignItems: "center",
    padding: tokens.space4,
  },
  thumbWrap: {
    inlineSize: "100%",
    aspectRatio: "4 / 3",
    overflow: "hidden",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius2,
    backgroundColor: tokens.panelInset,
  },
  thumb: {
    display: "block",
    width: "100%",
    height: "100%",
    objectFit: "cover",
    imageRendering: "pixelated",
  },
  rowTitleBlock: {
    minWidth: 0,
  },
  titleLine: {
    alignItems: "center",
  },
  phaseBadge: {
    flexShrink: 0,
  },
  rowTitle: {
    minWidth: 0,
    overflowWrap: "anywhere",
  },
  rowMetaWrap: {
    rowGap: tokens.space1,
  },
  rowMeta: {
    fontFamily: tokens.fontPixel,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  rowStats: {
    display: "grid",
    gap: tokens.space3,
    gridColumn: {
      default: "auto",
      [media.narrow]: "1 / -1",
    },
    gridTemplateColumns: {
      default: "repeat(3, minmax(0, 1fr))",
      [media.compact]: "1fr",
    },
    minWidth: {
      default: 220,
      [media.compact]: 0,
    },
  },
  statBlock: {
    minWidth: 0,
    display: "grid",
    gap: tokens.space1,
    padding: tokens.space3,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    backgroundColor: tokens.panelRaised,
    boxShadow: tokens.highlightInset,
  },
  actionBlock: {
    borderColor: tokens.strokeHeavy,
  },
  statLabel: {
    fontFamily: tokens.fontPixel,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
});

const rowClassName = sxClassName(styles.row, styles.rowInteractive);
