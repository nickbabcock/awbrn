import { Heading } from "@astryxdesign/core/Heading";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { useSuspenseInfiniteQuery, useSuspenseQuery } from "@tanstack/react-query";
import { spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { useId, useMemo } from "react";
import { OPEN_RANKED_POOLS } from "#/matchmaking/ranked_display.ts";
import { headingLooks } from "#/play/headings.ts";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { EmptySheet } from "#/ui/EmptySheet.tsx";
import { Page } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Postmark, usePostmarkDate } from "#/ui/Postmark.tsx";
import { Sheet, SheetHead } from "#/ui/Sheet.tsx";
import { YouChip } from "#/ui/YouChip.tsx";
import { layoutBunting } from "../bunting.ts";
import { Bunting } from "../components/Bunting.tsx";
import { ProfileCos } from "../components/ProfileCos.tsx";
import { ProfileHistory } from "../components/ProfileHistory.tsx";
import { ProfileOngoing } from "../components/ProfileOngoing.tsx";
import { playerHistoryQueryOptions, playerProfileQueryOptions } from "../players.queries.ts";
import type { PlayerProfile } from "../schemas.ts";

/**
 * A player's profile: how good and how active in one look (the bunting), then
 * what they play now, what they played, and whom they play as.
 */
export function PlayerProfilePage({ userId }: { userId: string }) {
  const { data: profile } = useSuspenseQuery(playerProfileQueryOptions(userId));
  const { data: history } = useSuspenseInfiniteQuery(playerHistoryQueryOptions(userId));
  // The loader answers an unknown player with the not-found page.
  if (!profile) return null;
  const hasHistory = (history.pages[0]?.matches.length ?? 0) > 0;
  const hasGames = hasHistory || profile.ongoing.length > 0;

  const ranked = <RankedSheet profile={profile} />;
  const historySlot = hasHistory ? (
    <ProfileHistory isViewer={profile.isViewer} userId={userId} />
  ) : hasGames ? null : (
    <EmptySheet
      action={
        profile.isViewer ? (
          <RouterButton label="Open lobbies" to="/matches" variant="ink" />
        ) : undefined
      }
      title="No games yet"
    />
  );

  // No games and no rating: one centered list, as My games stands.
  if (!hasGames && profile.pools.length === 0) {
    return (
      <Page width="narrow">
        <ProfileHead profile={profile} />
        {ranked}
        {historySlot}
      </Page>
    );
  }

  const ongoing = profile.ongoing.length > 0 ? <ProfileOngoing matches={profile.ongoing} /> : null;

  // Ongoing is placed twice, one copy hidden at each width, so the one shown
  // sits in reading and tab order where it is seen: in the rail beside the
  // main column, between Ranked and History below it.
  return (
    <Page>
      <ProfileHead profile={profile} />
      <VStack xstyle={styles.grid}>
        <VStack xstyle={styles.column}>
          <VStack xstyle={styles.slot}>{ranked}</VStack>
          {ongoing ? <VStack xstyle={[styles.slot, styles.belowRailOnly]}>{ongoing}</VStack> : null}
          {historySlot ? <VStack xstyle={styles.slot}>{historySlot}</VStack> : null}
        </VStack>
        <VStack xstyle={styles.column}>
          {ongoing ? <VStack xstyle={[styles.slot, styles.railOnly]}>{ongoing}</VStack> : null}
          {profile.cos.length > 0 ? (
            <VStack xstyle={styles.slot}>
              <ProfileCos cos={profile.cos} />
            </VStack>
          ) : null}
        </VStack>
      </VStack>
    </Page>
  );
}

/** The name, YOU on your own, and the day they joined as a postmark. */
function ProfileHead({ profile }: { profile: PlayerProfile }) {
  const joined = usePostmarkDate(profile.joinedAt);
  return (
    <HStack as="header" xstyle={styles.head}>
      {/* Focus lands here after a route change (`#/ui/arrivalFocus.ts`). */}
      <Heading level={1} tabIndex={-1} xstyle={styles.name}>
        {profile.name}
      </Heading>
      {profile.isViewer ? <YouChip xstyle={styles.you} /> : null}
      <Postmark
        arcText="JOINED"
        figure={joined.year}
        label={`Joined ${joined.long}`}
        size={64}
        word={joined.month}
        xstyle={styles.joined}
      />
    </HStack>
  );
}

function RankedSheet({ profile }: { profile: PlayerProfile }) {
  const headingId = useId();
  const layout = useMemo(
    () =>
      layoutBunting({
        currentSeason: profile.currentSeason,
        now: profile.loadedAt,
        openPools: OPEN_RANKED_POOLS,
        pools: profile.pools,
        seasons: profile.seasons,
      }),
    [profile],
  );
  if (layout.rows.length === 0) return null;

  return (
    <Sheet aria-labelledby={headingId}>
      <SheetHead>
        <Heading id={headingId} level={2} xstyle={headingLooks.title}>
          Ranked
        </Heading>
      </SheetHead>
      <Bunting layout={layout} />
    </Sheet>
  );
}

const styles = stylex.create({
  head: {
    display: "flex",
    alignItems: "center",
    gap: {
      default: awbrnVars.spacing3_5,
      [pageLayout.phoneMedia]: awbrnVars.spacing2_5,
    },
    minInlineSize: 0,
  },
  // A long name gives way first; the postmark never shrinks.
  name: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    paddingBlockEnd: "0.06em",
  },
  you: {
    marginBlockStart: spacingVars["--spacing-1-5"],
  },
  joined: {
    marginInlineStart: {
      default: spacingVars["--spacing-1-5"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-0-5"],
    },
  },
  // The main column and the rail; below the rail width the sheets fall into
  // one column in the order a visitor asks: how good, now, before, who.
  grid: {
    display: {
      default: "grid",
      [pageLayout.belowRailMedia]: "flex",
    },
    flexDirection: "column",
    gridTemplateColumns: `minmax(0, 1fr) ${pageLayout.profileRailWidth}`,
    alignItems: {
      default: "start",
      [pageLayout.belowRailMedia]: "stretch",
    },
    gap: {
      default: spacingVars["--spacing-6"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
  },
  column: {
    display: {
      default: "flex",
      [pageLayout.belowRailMedia]: "contents",
    },
    flexDirection: "column",
    gap: spacingVars["--spacing-6"],
    minInlineSize: 0,
  },
  slot: {
    display: "block",
    minInlineSize: 0,
  },
  railOnly: {
    display: {
      default: "block",
      [pageLayout.belowRailMedia]: "none",
    },
  },
  belowRailOnly: {
    display: {
      default: "none",
      [pageLayout.belowRailMedia]: "block",
    },
  },
});
