import { Link } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { startTransition, useEffect, useRef, useState } from "react";
import {
  Button,
  ButtonLink,
  EmptyState,
  Frame,
  Heading,
  Inline,
  Kicker,
  Notice,
  Page,
  Section,
  Stack,
  Text,
} from "#/ui/primitives.tsx";
import { awbwSmallMapAssetPath } from "#/awbw/paths.ts";
import { sxClassName } from "#/ui/stylex.ts";
import { tokens } from "#/ui/theme.stylex.ts";
import { listMatchesFn } from "#/matches/matches.functions.ts";
import type { MatchBrowseSummary } from "#/matches/schemas.ts";

export function MatchesBrowsePage() {
  const [matches, setMatches] = useState<MatchBrowseSummary[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [hasNextPage, setHasNextPage] = useState(false);
  const [isLoadingInitial, setIsLoadingInitial] = useState(true);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloadNonce, setReloadNonce] = useState(0);
  const requestIdRef = useRef(0);

  useEffect(() => {
    const requestId = ++requestIdRef.current;
    setIsLoadingInitial(true);
    setIsLoadingMore(false);
    setError(null);

    void (async () => {
      try {
        const nextData = await listMatchesFn({ data: {} });
        if (requestIdRef.current !== requestId) {
          return;
        }

        startTransition(() => {
          setMatches(nextData.matches);
          setNextCursor(nextData.nextCursor);
          setHasNextPage(nextData.hasNextPage);
        });
      } catch (nextError) {
        if (requestIdRef.current !== requestId) {
          return;
        }

        startTransition(() => {
          setMatches([]);
          setNextCursor(null);
          setHasNextPage(false);
        });
        setError(nextError instanceof Error ? nextError.message : "Lobby browser failed to load.");
      } finally {
        if (requestIdRef.current === requestId) {
          setIsLoadingInitial(false);
        }
      }
    })();
  }, [reloadNonce]);

  async function handleLoadMore(): Promise<void> {
    if (isLoadingMore || !hasNextPage || !nextCursor) {
      return;
    }

    const requestId = ++requestIdRef.current;
    setIsLoadingMore(true);
    setError(null);

    try {
      const nextData = await listMatchesFn({ data: { cursor: nextCursor } });
      if (requestIdRef.current !== requestId) {
        return;
      }

      startTransition(() => {
        setMatches((current) => [...current, ...nextData.matches]);
        setNextCursor(nextData.nextCursor);
        setHasNextPage(nextData.hasNextPage);
      });
    } catch (nextError) {
      if (requestIdRef.current !== requestId) {
        return;
      }

      setError(nextError instanceof Error ? nextError.message : "More lobbies failed to load.");
    } finally {
      if (requestIdRef.current === requestId) {
        setIsLoadingMore(false);
      }
    }
  }

  return (
    <Page width="wide">
      <Section>
        <Stack gap="lg">
          {error ? (
            <Notice tone="danger">
              <Stack gap="sm">
                <Text tone="strong">Open lobbies failed to load.</Text>
                <Text>{error}</Text>
                <Inline gap="sm">
                  <Button
                    size="sm"
                    tone="neutral"
                    variant="outline"
                    onClick={() => {
                      setReloadNonce((current) => current + 1);
                    }}
                  >
                    Retry
                  </Button>
                </Inline>
              </Stack>
            </Notice>
          ) : null}

          {!error && isLoadingInitial ? (
            <Frame xstyle={styles.stateFrame}>
              <Stack gap="sm">
                <Kicker>Loading</Kicker>
                <Heading size="lg">Fetching open lobbies...</Heading>
              </Stack>
            </Frame>
          ) : null}

          {!error && !isLoadingInitial && matches.length === 0 ? (
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

          {!error && !isLoadingInitial && matches.length > 0 ? (
            <Frame padding="none">
              <div {...stylex.props(styles.listHeader)}>
                <Text size="sm" tone="muted" xstyle={styles.listMeta}>
                  Public open lobbies
                </Text>
              </div>

              <div {...stylex.props(styles.list)}>
                {matches.map((lobby) => (
                  <LobbyRow key={lobby.matchId} lobby={lobby} />
                ))}
              </div>
            </Frame>
          ) : null}

          {!error && !isLoadingInitial && matches.length > 0 && hasNextPage ? (
            <div {...stylex.props(styles.pagination)}>
              <Button
                size="sm"
                tone="neutral"
                variant="outline"
                loading={isLoadingMore}
                onClick={() => {
                  void handleLoadMore();
                }}
              >
                {isLoadingMore ? "Loading..." : "Load More"}
              </Button>
            </div>
          ) : null}
        </Stack>
      </Section>
    </Page>
  );
}

function LobbyRow({ lobby }: { lobby: MatchBrowseSummary }) {
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
            <Text tone="strong">{formatRelativeTime(lobby.createdAt)}</Text>
          </div>
        </div>
      </div>
    </Link>
  );
}

function formatRelativeTime(iso: string): string {
  const deltaMs = Date.now() - Date.parse(iso);
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
