import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars, textSizeVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useQuery, useSuspenseInfiniteQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { CoStamp } from "#/components/CoStamp.tsx";
import { unitSpriteStyle } from "#/components/game_sprites.ts";
import { getFactionById } from "#/factions.ts";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import { matchDetailQueryOptions, matchesBrowseQueryOptions } from "#/matches/matches.queries.ts";
import type { MatchBrowseSummary } from "#/matches/schemas.ts";
import { MatchThumb } from "#/play/MatchThumb.tsx";
import { SeatStamp } from "#/play/Opponents.tsx";
import { isSeatedIn } from "#/play/prefetch_rows.ts";
import { seatsFigure } from "#/play/terms.ts";
import { clockTerms } from "#/play/terms.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { EmptySheet, ProblemBanner } from "#/ui/EmptySheet.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconPlus } from "#/ui/icons.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Sheet } from "#/ui/Sheet.tsx";
import { Terms } from "#/ui/Terms.tsx";

/**
 * On desktop every row shares one set of tracks (subgrid), so the terms follow
 * the longest title and the seats take the slack.
 */
export function MatchesBrowsePage() {
  const browseQuery = useSuspenseInfiniteQuery(matchesBrowseQueryOptions());
  const session = useAppSession();
  const [paginationError, setPaginationError] = useState<string | null>(null);
  const lobbies = browseQuery.data.pages.flatMap((page) => page.matches);

  async function handleLoadMore(): Promise<void> {
    if (browseQuery.isFetchingNextPage || !browseQuery.hasNextPage) return;
    setPaginationError(null);
    try {
      await browseQuery.fetchNextPage();
    } catch (nextError) {
      setPaginationError(
        nextError instanceof Error ? nextError.message : "More lobbies failed to load",
      );
    }
  }

  return (
    <Page>
      <PageHeader title="Play" />

      {lobbies.length === 0 ? (
        <EmptySheet
          action={
            <RouterButton
              icon={<Icon icon={IconPlus} />}
              label="New match"
              to="/matches/new"
              variant="ink"
            />
          }
          art={<EmptyArt />}
          title="No open lobbies"
        />
      ) : (
        <Sheet
          aria-label={`${lobbies.length === 1 ? "1 open lobby" : `${lobbies.length} open lobbies`}, newest first`}
        >
          <ul {...stylex.props(styles.rows)}>
            {lobbies.map((lobby, index) => (
              <LobbyRow
                isAboveFold={index < ABOVE_FOLD_ROWS}
                key={lobby.matchId}
                lobby={lobby}
                viewerId={session?.user.id ?? null}
              />
            ))}
          </ul>
        </Sheet>
      )}

      {paginationError ? (
        <ProblemBanner
          action={<Button clickAction={handleLoadMore} label="Retry" size="sm" />}
          title="More lobbies failed to load"
        />
      ) : null}

      {lobbies.length > 0 && browseQuery.hasNextPage ? (
        <HStack justify="center">
          <Button
            clickAction={handleLoadMore}
            isLoading={browseQuery.isFetchingNextPage}
            label="Load more"
          />
        </HStack>
      ) : null}
    </Page>
  );
}

/** How many lobby rows the list shows before the fold. */
const ABOVE_FOLD_ROWS = 4;

function LobbyRow({
  isAboveFold,
  lobby,
  viewerId,
}: {
  isAboveFold: boolean;
  lobby: MatchBrowseSummary;
  viewerId: string | null;
}) {
  const { data: map } = useQuery(mapCatalogEntryQueryOptions(lobby.mapId, lobby.mapRevision));
  const isMine = isSeatedIn(lobby, viewerId);
  // The list carries the seats; the lobby seals every commander but the
  // viewer's own, which only their match record holds.
  const { data: detail } = useQuery({
    ...matchDetailQueryOptions(lobby.matchId, null),
    enabled: isMine,
  });
  const ownCoId = detail?.participants.find((seat) => seat.userId === viewerId)?.coId ?? null;
  const names = lobby.seats.map((seat) => seat.userName);

  return (
    <li {...stylex.props(styles.row)}>
      <HStack as="span" xstyle={styles.thumb}>
        <MatchThumb
          isEager={isAboveFold}
          mapId={lobby.mapId}
          phoneSize={64}
          revision={lobby.mapRevision}
        />
      </HStack>
      <VStack as="span" xstyle={styles.main}>
        <HStack as="span" xstyle={styles.titleLine}>
          <b {...stylex.props(styles.title)}>{lobby.name}</b>
          {lobby.settings.hotseatEnabled ? (
            <HStack as="span" xstyle={styles.tag}>
              Hotseat
            </HStack>
          ) : null}
        </HStack>
        {map ? (
          <Text color="inherit" type="inherit" xstyle={styles.mapLine}>
            {map.name} · {map.width}×{map.height}
          </Text>
        ) : null}
      </VStack>
      <HStack as="span" xstyle={styles.terms}>
        <Terms
          clock={clockTerms(lobby.settings.clock)}
          funds={lobby.settings.startingFunds}
          hasFog={lobby.settings.fogEnabled}
        />
      </HStack>
      <HStack as="span" xstyle={styles.seats}>
        <SeatStamps lobby={lobby} ownCoId={ownCoId} viewerId={viewerId} />
        <HStack as="span" xstyle={styles.who}>
          {names.length > 0 ? <b {...stylex.props(styles.whoNames)}>{names.join(", ")}</b> : null}
          <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.whoCount]}>
            {seatsFigure(lobby.participantCount, lobby.maxPlayers)}
          </Text>
        </HStack>
      </HStack>
      <RouterButton
        label={isMine ? "Open" : "Join"}
        params={{ matchId: lobby.matchId }}
        to="/matches/$matchId"
        xstyle={styles.key}
      />
    </li>
  );
}

/** A taken seat shows the viewer's own commander; the lobby seals the rest. */
function SeatStamps({
  lobby,
  ownCoId,
  viewerId,
}: {
  lobby: MatchBrowseSummary;
  ownCoId: number | null;
  viewerId: string | null;
}) {
  const taken = new Set(lobby.seats.map((seat) => seat.slotIndex));
  const open = Array.from({ length: lobby.maxPlayers }, (_, slotIndex) => slotIndex).filter(
    (slotIndex) => !taken.has(slotIndex),
  );

  return (
    <HStack as="span" aria-hidden="true" xstyle={styles.stamps}>
      {lobby.seats.map((seat) => (
        <SeatStamp
          coId={viewerId !== null && seat.userId === viewerId ? ownCoId : null}
          factionCode={getFactionById(seat.factionId)?.code ?? null}
          isDecorative
          key={seat.slotIndex}
          xstyle={styles.takenStamp}
        />
      ))}
      {/* The summary does not carry the map's armies, so an open seat shows
          none rather than a guess. */}
      {open.map((slotIndex) => (
        <CoStamp isDecorative isOpenSeat key={slotIndex} xstyle={styles.openStamp} />
      ))}
    </HStack>
  );
}

/** Two infantry, facing off over an empty list. */
function EmptyArt() {
  const left = unitSpriteStyle("infantry", "os", 2);
  const right = unitSpriteStyle("infantry", "bm", 2);
  return (
    <HStack as="span" aria-hidden="true" xstyle={styles.art}>
      {/* Crops of the sprite atlas, placed by an inline style object that no
          Astryx component takes. */}
      {left ? <span style={left} /> : null}
      {right ? <span style={right} /> : null}
    </HStack>
  );
}

/**
 * Below the rail breakpoint but wider than a phone, in two bands: from 1024px
 * the five tracks still leave a title 300px, and below it the terms and seats
 * drop under the title. Ranges rather than `max-width`, so the phone's tracks
 * never lose to them.
 */
const TABLET_MEDIA = "@media (min-width: 721px) and (max-width: 1180px)";
const NARROW_TABLET_MEDIA = "@media (min-width: 721px) and (max-width: 1023px)";
const WIDE_TABLET_MEDIA = "@media (min-width: 1024px) and (max-width: 1180px)";

const styles = stylex.create({
  rows: {
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(0, 1fr)",
      "@supports (grid-template-columns: subgrid)": {
        default: null,
        [pageLayout.desktopMedia]: "80px fit-content(340px) auto minmax(0, 1fr) 96px",
      },
    },
    columnGap: "32px",
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  row: {
    display: "grid",
    gridColumn: "1 / -1",
    gridTemplateColumns: {
      default: "80px minmax(0, 340px) 176px minmax(0, 1fr) 96px",
      "@supports (grid-template-columns: subgrid)": {
        default: null,
        [pageLayout.desktopMedia]: "subgrid",
      },
      [WIDE_TABLET_MEDIA]: "80px minmax(0, 1fr) 160px 200px 96px",
      [NARROW_TABLET_MEDIA]: "80px auto minmax(0, 1fr) 96px",
      [pageLayout.phoneMedia]: "64px minmax(0, 1fr) 96px",
    },
    gridTemplateAreas: {
      default: '"thumb main terms seats key"',
      [NARROW_TABLET_MEDIA]: '"thumb main main key" "thumb terms seats key"',
      [pageLayout.phoneMedia]: '"thumb main main" "terms terms terms" "seats seats key"',
    },
    alignItems: "center",
    columnGap: {
      default: "32px",
      [TABLET_MEDIA]: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
    rowGap: {
      default: "12px",
      [pageLayout.phoneMedia]: "10px",
    },
    paddingBlock: {
      default: "16px",
      [pageLayout.phoneMedia]: "14px",
    },
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: {
      default: "1px",
      ":first-child": 0,
    },
  },
  thumb: {
    gridArea: "thumb",
    display: "flex",
  },
  main: {
    gridArea: "main",
    display: "grid",
    gap: "4px",
    minInlineSize: 0,
  },
  titleLine: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: "8px",
  },
  title: {
    fontSize: awbrnVars.text16,
    fontWeight: 700,
    lineHeight: 1.25,
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  tag: {
    display: "inline-flex",
    alignItems: "center",
    blockSize: "22px",
    paddingInline: "8px",
    borderRadius: "4px",
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ruleStrong}`,
    color: colorVars["--color-text-secondary"],
    fontSize: awbrnVars.text12,
    fontWeight: 650,
    letterSpacing: "0.02em",
  },
  mapLine: {
    color: colorVars["--color-text-secondary"],
    fontSize: textSizeVars["--font-size-sm"],
    lineHeight: 1.3,
  },
  terms: {
    gridArea: "terms",
    display: "flex",
    alignItems: "center",
    minInlineSize: 0,
  },
  // The stamps beside who sits and the count. On a phone, beside the Join key,
  // the names and the count keep to one line and drop under the stamps only
  // when the two do not fit side by side.
  seats: {
    gridArea: "seats",
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    columnGap: "12px",
    rowGap: {
      default: "6px",
      [pageLayout.phoneMedia]: "4px",
    },
    minInlineSize: 0,
  },
  stamps: {
    display: "inline-flex",
    flexWrap: "wrap",
    alignItems: "center",
    rowGap: "4px",
  },
  takenStamp: {
    marginInlineStart: {
      default: "-6px",
      ":first-child": 0,
    },
  },
  openStamp: {
    marginInlineStart: {
      default: "4px",
      ":first-child": 0,
    },
  },
  who: {
    display: "flex",
    flexDirection: {
      default: "column",
      [pageLayout.phoneMedia]: "row",
    },
    flexWrap: "wrap",
    alignItems: {
      default: "flex-start",
      [pageLayout.phoneMedia]: "baseline",
    },
    columnGap: "8px",
    rowGap: "1px",
    minInlineSize: 0,
    fontSize: textSizeVars["--font-size-sm"],
  },
  whoNames: {
    minInlineSize: 0,
    fontWeight: 650,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  whoCount: {
    fontSize: awbrnVars.text12_5,
    fontWeight: 600,
    color: colorVars["--color-text-secondary"],
  },
  key: {
    gridArea: "key",
    inlineSize: "96px",
    blockSize: {
      default: null,
      [pageLayout.phoneMedia]: "44px",
    },
    justifySelf: "end",
  },
  art: {
    display: "flex",
    gap: "4px",
  },
});
