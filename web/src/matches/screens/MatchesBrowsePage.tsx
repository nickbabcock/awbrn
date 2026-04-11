import { useSuspenseInfiniteQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import {
  Button,
  ButtonLink,
  EmptyState,
  Frame,
  Heading,
  Inline,
  Notice,
  Page,
  Section,
  Stack,
  Text,
} from "#/ui/primitives.tsx";
import { awbwSmallMapAssetPath } from "#/awbw/paths.ts";
import { sxClassName } from "#/ui/stylex.ts";
import { tokens } from "#/ui/theme.stylex.ts";
import { matchesBrowseQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchBrowseSummary } from "#/matches/schemas.ts";

export function MatchesBrowsePage() {
  const browseQuery = useSuspenseInfiniteQuery(matchesBrowseQueryOptions());
  const [paginationError, setPaginationError] = useState<string | null>(null);
  const matches = browseQuery.data.pages.flatMap((page) => page.matches);
  const relativeTimeBaseMs = parseLoadedAt(
    browseQuery.data.pages[browseQuery.data.pages.length - 1]?.loadedAt,
  );

  async function handleLoadMore(): Promise<void> {
    if (browseQuery.isFetchingNextPage || !browseQuery.hasNextPage) {
      return;
    }

    setPaginationError(null);

    try {
      await browseQuery.fetchNextPage();
    } catch (nextError) {
      setPaginationError(
        nextError instanceof Error ? nextError.message : "More lobbies failed to load.",
      );
    }
  }

  return (
    <Page width="wide">
      <Section>
        <Stack gap="lg">
          {paginationError ? (
            <Notice tone="danger">
              <Stack gap="sm">
                <Text tone="strong">More lobbies failed to load.</Text>
                <Text>{paginationError}</Text>
                <Inline gap="sm">
                  <Button
                    size="sm"
                    tone="neutral"
                    variant="outline"
                    onClick={() => {
                      void handleLoadMore();
                    }}
                  >
                    Retry
                  </Button>
                </Inline>
              </Stack>
            </Notice>
          ) : null}

          {matches.length === 0 ? (
            <Frame xstyle={styles.stateFrame}>
              <EmptyState
                kicker="No Open Rooms"
                title="No public lobbies are open right now"
                description="Create a new match to start the next lobby."
                actions={
                  <ButtonLink size="sm" to="/matches/new" tone="brand">
                    Create Match
                  </ButtonLink>
                }
              />
            </Frame>
          ) : null}

          {matches.length > 0 ? (
            <Frame padding="none">
              <div {...stylex.props(styles.listHeader)}>
                <Text size="sm" tone="muted" xstyle={styles.listMeta}>
                  Public open lobbies
                </Text>
              </div>

              <div {...stylex.props(styles.list)}>
                {matches.map((lobby) => (
                  <LobbyRow
                    key={lobby.matchId}
                    lobby={lobby}
                    relativeTimeBaseMs={relativeTimeBaseMs}
                  />
                ))}
              </div>
            </Frame>
          ) : null}

          {matches.length > 0 && browseQuery.hasNextPage ? (
            <div {...stylex.props(styles.pagination)}>
              <Button
                size="sm"
                tone="neutral"
                variant="outline"
                loading={browseQuery.isFetchingNextPage}
                onClick={() => {
                  void handleLoadMore();
                }}
              >
                {browseQuery.isFetchingNextPage ? "Loading..." : "Load More"}
              </Button>
            </div>
          ) : null}
        </Stack>
      </Section>
    </Page>
  );
}

function LobbyRow({
  lobby,
  relativeTimeBaseMs,
}: {
  lobby: MatchBrowseSummary;
  relativeTimeBaseMs: number;
}) {
  return (
    <Link className={rowClassName} params={{ matchId: lobby.matchId }} to="/matches/$matchId">
      <div {...stylex.props(styles.rowMain)}>
        <div {...stylex.props(styles.thumbWrap)}>
          <img
            alt={`Map preview for ${lobby.name}`}
            src={awbwSmallMapAssetPath(lobby.mapId)}
            {...stylex.props(styles.thumb)}
          />
        </div>

        <Stack gap="xs" xstyle={styles.rowTitleBlock}>
          <Heading size="lg" xstyle={styles.rowTitle}>
            {lobby.name}
          </Heading>
          <Inline gap="sm" xstyle={styles.rowMetaWrap}>
            <Text size="sm" tone="muted" xstyle={styles.rowMeta}>
              Host {lobby.creatorName}
            </Text>
            <Text size="sm" tone="muted" xstyle={styles.rowMeta}>
              Map {lobby.mapId}
            </Text>
            <Text size="sm" tone="muted" xstyle={styles.rowMeta}>
              {lobby.settings.fogEnabled ? "Fog on" : "Fog off"}
            </Text>
            <Text size="sm" tone="muted" xstyle={styles.rowMeta}>
              {lobby.settings.startingFunds.toLocaleString()} funds
            </Text>
          </Inline>
          <Text size="sm" tone="muted">
            {lobby.joinedPlayerNames.length > 0
              ? `Joined: ${lobby.joinedPlayerNames.join(", ")}`
              : "Joined: No players yet"}
          </Text>
        </Stack>

        <div {...stylex.props(styles.rowStats)}>
          <div {...stylex.props(styles.statBlock)}>
            <Text size="sm" tone="muted" xstyle={styles.statLabel}>
              Players
            </Text>
            <Text tone="strong">
              {lobby.participantCount} / {lobby.maxPlayers}
            </Text>
          </div>
          <div {...stylex.props(styles.statBlock)}>
            <Text size="sm" tone="muted" xstyle={styles.statLabel}>
              Open Slots
            </Text>
            <Text tone="strong">{lobby.openSlotCount}</Text>
          </div>
          <div {...stylex.props(styles.statBlock, styles.rowCreated)}>
            <Text size="sm" tone="muted" xstyle={styles.statLabel}>
              Created
            </Text>
            <Text tone="strong">{formatRelativeTime(lobby.createdAt, relativeTimeBaseMs)}</Text>
          </div>
        </div>
      </div>
    </Link>
  );
}

function parseLoadedAt(iso: string | undefined): number {
  const parsed = iso ? Date.parse(iso) : Number.NaN;
  return Number.isNaN(parsed) ? Date.now() : parsed;
}

function formatRelativeTime(iso: string, relativeToMs: number): string {
  const deltaMs = relativeToMs - Date.parse(iso);
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
  headerCopy: {
    maxWidth: 720,
  },
  stateFrame: {
    minHeight: 200,
    display: "flex",
    alignItems: "center",
  },
  listHeader: {
    display: "flex",
    justifyContent: "space-between",
    gap: tokens.space3,
    flexWrap: "wrap",
    paddingInline: tokens.space4,
    paddingBlock: tokens.space3,
    borderBottomWidth: 2,
    borderBottomStyle: "solid",
    borderBottomColor: tokens.strokeBase,
    backgroundColor: tokens.panelRaised,
  },
  listMeta: {
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  list: {
    display: "flex",
    flexDirection: "column",
  },
  row: {
    display: "block",
    paddingInline: tokens.space4,
    paddingBlock: tokens.space4,
    borderTopWidth: 2,
    borderTopStyle: "solid",
    borderTopColor: tokens.strokeBase,
    textDecoration: "none",
    color: tokens.inkStrong,
    backgroundColor: "transparent",
  },
  rowInteractive: {
    transitionDuration: tokens.transitionFast,
    transitionProperty: "background-color, transform",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
    },
    backgroundColor: {
      default: "transparent",
      ":hover": "rgba(255, 255, 255, 0.18)",
    },
  },
  rowMain: {
    display: "grid",
    gridTemplateColumns: {
      default: "max-content minmax(0, 1.4fr) minmax(260px, 0.9fr)",
      "@media (max-width: 860px)": "1fr",
    },
    gap: tokens.space4,
    alignItems: "center",
  },
  thumbWrap: {
    display: "inline-flex",
    overflow: "hidden",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius2,
    backgroundColor: tokens.panelInset,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    alignSelf: "start",
  },
  thumb: {
    display: "block",
    width: "auto",
    height: "auto",
    maxWidth: "100%",
    imageRendering: "pixelated",
  },
  rowTitleBlock: {
    minWidth: 0,
  },
  rowTitle: {
    color: tokens.inkStrong,
  },
  rowMetaWrap: {
    alignItems: "flex-start",
  },
  rowMeta: {
    paddingInlineEnd: tokens.space2,
  },
  rowStats: {
    display: "grid",
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
    gap: tokens.space3,
    "@media (max-width: 540px)": {
      gridTemplateColumns: "1fr",
    },
  },
  statBlock: {
    display: "grid",
    gap: tokens.space1,
  },
  statLabel: {
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  rowCreated: {
    justifyItems: {
      default: "end",
      "@media (max-width: 860px)": "start",
    },
    textAlign: {
      default: "right",
      "@media (max-width: 860px)": "left",
    },
  },
  pagination: {
    display: "flex",
    justifyContent: "center",
  },
});

const rowClassName = sxClassName(styles.row, styles.rowInteractive);
