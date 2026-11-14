/*
 * Home for a player with no matches yet: a match against the CPU to start in
 * one press rather than an empty state.
 */

import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  borderVars,
  colorVars,
  fontWeightVars,
  radiusVars,
  spacingVars,
  typeScaleVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { useId, useState } from "react";
import { cpuCardState } from "#/home/first_match.ts";
import { getCoById } from "#/co_roster.ts";
import { coTierListFor } from "#/co_tiers.ts";
import { UiSprite } from "#/components/UiSprite.tsx";
import { defaultFactionIdForSlot, getFactionById } from "#/factions.ts";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapRevisionQueryOptions } from "#/maps/maps.queries.ts";
import type { MapCatalogEntry } from "#/maps/schemas.ts";
import { aiProfileDisplays, DEFAULT_AI_PROFILE_ID } from "#/matches/ai_profiles.ts";
import { createMatchFn, mutateMatchFn } from "#/matches/matches.functions.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { defaultMatchClock, type AiProfileId, type MatchBrowseSummary } from "#/matches/schemas.ts";
import { MatchThumb } from "#/play/MatchThumb.tsx";
import { FIRST_CO_ID } from "#/play/opening_co.ts";
import { SeatStamp } from "#/play/Opponents.tsx";
import { Postage, mapPostage } from "#/play/Postage.tsx";
import { clockTerms, seatsFigure } from "#/play/terms.ts";
import { TierPicker } from "#/play/TierPicker.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { ProblemBanner } from "#/ui/EmptySheet.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconChevronDown } from "#/ui/icons.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Sheet, SheetBody, SheetHead, SheetRow, SheetRows } from "#/ui/Sheet.tsx";

/** Funds a first match starts with, as New match offers them. */
const FIRST_MATCH_FUNDS = 1000;
/** Nothing is banned in a first match. */
const NO_BANS: ReadonlySet<number> = new Set<number>();
/** The seat the player takes; the CPU takes the other. */
const PLAYER_SLOT = 0;
const CPU_SLOT = 1;

/** The well the map sits in: the map at 1x on desktop, 192px tall on a phone. */
const PHONE_WELL = 192;

export function FirstMatch({
  lobbies,
  map,
}: {
  lobbies: readonly MatchBrowseSummary[];
  map: MapCatalogEntry | null;
}) {
  const headingId = useId();
  return (
    <Sheet aria-labelledby={headingId}>
      <SheetHead>
        <Heading id={headingId} level={1}>
          Your first match
        </Heading>
      </SheetHead>
      <SheetBody>
        {map ? (
          <CpuCard map={map} />
        ) : (
          <RouterButton label="New match" size="lg" to="/matches/new" variant="ink" />
        )}
      </SheetBody>
      {lobbies.length > 0 ? (
        <SheetRows>
          {lobbies.map((lobby) => (
            <SheetRow key={lobby.matchId}>
              <LobbyRow lobby={lobby} />
            </SheetRow>
          ))}
        </SheetRows>
      ) : null}
    </Sheet>
  );
}

function CpuCard({ map }: { map: MapCatalogEntry }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const revisionOptions = mapRevisionQueryOptions(map.mapId, map.revision);
  const revisionQuery = useQuery(revisionOptions);
  const document = revisionQuery.data;
  const [coId, setCoId] = useState(FIRST_CO_ID);
  const [tier, setTier] = useState<AiProfileId>(DEFAULT_AI_PROFILE_ID);
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [hasStartFailed, setHasStartFailed] = useState(false);
  const [isStarting, setIsStarting] = useState(false);

  const playerFactionId = playerFactionFor(document);
  const factionCode = getFactionById(playerFactionId)?.code ?? null;
  const co = getCoById(coId);
  const card = cpuCardState({
    hasStartFailed,
    isStarting,
    mapStatus: revisionQuery.status,
  });

  /**
   * Creates the match with the CPU seated, then takes and readies the other
   * seat, which starts it. A failure after the match exists opens its lobby,
   * where the seat can be finished by hand, rather than leaving a second
   * match behind a retry.
   */
  async function playTheCpu(): Promise<void> {
    setHasStartFailed(false);
    setIsStarting(true);
    let revision: { slotFactionIds: readonly number[] };
    try {
      revision = document ?? (await queryClient.fetchQuery(revisionOptions));
    } catch {
      setIsStarting(false);
      return;
    }
    const factionId = playerFactionFor(revision);
    // Home offers this card until the player has a match, so a stale list
    // would offer a second first match on the way back.
    const refreshLists = () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: matchKeys.mine() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.browse() }),
      ]);
    let matchId: string | null = null;
    try {
      const created = await createMatchFn({
        data: {
          name: map.name,
          map: { mapId: map.mapId, revision: map.revision },
          isPrivate: true,
          settings: {
            fogEnabled: false,
            startingFunds: FIRST_MATCH_FUNDS,
            hotseatEnabled: false,
            bannedCoIds: [],
            clock: defaultMatchClock,
          },
          aiSeats: [{ slotIndex: CPU_SLOT, profileId: tier }],
        },
      });
      matchId = created.matchId;
      const joinSlug = created.joinSlug;
      const seat = (action: Parameters<typeof mutateMatchFn>[0]["data"]["action"]) =>
        mutateMatchFn({ data: { matchId: created.matchId, action } });
      await seat({ action: "join", slotIndex: PLAYER_SLOT, factionId, joinSlug });
      await seat({ action: "updateParticipant", slotIndex: PLAYER_SLOT, coId, joinSlug });
      await seat({ action: "updateParticipant", slotIndex: PLAYER_SLOT, ready: true, joinSlug });
      await Promise.all([
        refreshLists(),
        queryClient.invalidateQueries({
          queryKey: matchDetailQueryOptions(matchId, null).queryKey,
        }),
      ]);
      await navigate({ to: "/matches/$matchId", params: { matchId } });
    } catch {
      setIsStarting(false);
      if (matchId !== null) {
        await refreshLists();
        await navigate({ to: "/matches/$matchId", params: { matchId } });
        return;
      }
      setHasStartFailed(true);
    }
  }

  const full = mapScreenshotSize("full", map.width, map.height);
  const small = mapScreenshotSize("small", map.width, map.height);
  const phoneScale = Math.max(
    1,
    Math.floor((PHONE_WELL - 32) / Math.max(small.width, small.height)),
  );

  return (
    <>
      <article {...stylex.props(styles.card)}>
        <VStack as="span" xstyle={styles.well}>
          <img
            alt={`The ${map.name} battlefield`}
            height={full.height}
            src={map.screenshot.full}
            width={full.width}
            {...stylex.props(styles.picture, styles.notPhone)}
          />
          <img
            alt=""
            height={small.height}
            src={map.screenshot.small}
            width={small.width}
            {...stylex.props(
              styles.picture,
              styles.phoneOnly,
              styles.phonePicture(small.width * phoneScale, small.height * phoneScale),
            )}
          />
        </VStack>
        <VStack xstyle={styles.body}>
          <HStack as="span" xstyle={styles.title}>
            <Heading level={2} xstyle={styles.mapName}>
              {map.name}
            </Heading>
            <MapRankMedal rank={map.rank} size="sm" />
          </HStack>
          <Postage>{mapPostage(map)}</Postage>
          <HStack as="span" xstyle={[figureStyles.figure, styles.funds]}>
            <UiSprite name="Coin.png" />
            <Text color="inherit" type="inherit">
              <VisuallyHidden>Funds </VisuallyHidden>
              {FIRST_MATCH_FUNDS.toLocaleString("en-US")}
            </Text>
          </HStack>
        </VStack>
        <VStack xstyle={styles.act}>
          <VStack xstyle={styles.pick}>
            <Text color="inherit" type="inherit" aria-hidden="true" xstyle={styles.pickKey}>
              You
            </Text>
            <Button
              label={`Your CO: ${co?.displayName ?? "None"}. Change CO`}
              onClick={() => setIsPickerOpen(true)}
              tooltip="Change CO"
              xstyle={styles.coKey}
            >
              <HStack as="span" xstyle={styles.coKeyBody}>
                <SeatStamp coId={coId} factionCode={factionCode} isDecorative />
                <b>{co?.displayName}</b>
                <Icon icon={IconChevronDown} size="sm" xstyle={styles.coKeyChevron} />
              </HStack>
            </Button>
            <Text color="inherit" type="inherit" aria-hidden="true" xstyle={styles.pickKey}>
              CPU
            </Text>
            <SegmentedControl
              label="CPU"
              onChange={(value) => setTier(value as AiProfileId)}
              value={tier}
              xstyle={styles.tier}
            >
              {aiProfileDisplays.map((profile) => (
                <SegmentedControlItem
                  key={profile.id}
                  label={profile.label}
                  value={profile.id}
                  xstyle={styles.tierItem}
                />
              ))}
            </SegmentedControl>
          </VStack>
          <HStack as="span" xstyle={styles.keys}>
            <RouterButton
              label="Other map"
              to="/matches/new"
              variant="ghost"
              xstyle={styles.other}
            />
            <Button
              clickAction={playTheCpu}
              isDisabled={card.isKeyDisabled}
              isLoading={isStarting}
              label="Play the CPU"
              size="lg"
              variant="primary"
              xstyle={styles.go}
            />
          </HStack>
        </VStack>
      </article>
      {card.problem === "start" ? (
        <ProblemBanner
          action={<Button clickAction={playTheCpu} label="Retry" size="sm" />}
          title="The match could not be created"
        />
      ) : card.problem === "map" ? (
        <ProblemBanner
          action={
            <Button
              clickAction={async () => {
                await revisionQuery.refetch();
              }}
              label="Retry"
              size="sm"
            />
          }
          title="The map could not be loaded"
        />
      ) : null}
      <ModalSheet
        isOpen={isPickerOpen}
        onOpenChange={setIsPickerOpen}
        title="Change CO"
        width={720}
      >
        <VStack xstyle={styles.picker}>
          <TierPicker
            bannedCoIds={NO_BANS}
            chosenCoId={coId}
            factionCode={factionCode}
            mode="pick"
            onPick={(picked) => {
              setCoId(picked);
              setIsPickerOpen(false);
            }}
            tierList={coTierListFor({ fogEnabled: false, playerCount: map.playerCount })}
          />
        </VStack>
      </ModalSheet>
    </>
  );
}

/** The army the player's seat flies on the map, or the seat's default before the map arrives. */
function playerFactionFor(document: { slotFactionIds: readonly number[] } | undefined): number {
  return document?.slotFactionIds[PLAYER_SLOT] ?? defaultFactionIdForSlot(PLAYER_SLOT);
}

function LobbyRow({ lobby }: { lobby: MatchBrowseSummary }) {
  const host = lobby.seats.find((seat) => seat.isHost) ?? null;
  return (
    <VStack xstyle={styles.lobby}>
      <MatchThumb isEager mapId={lobby.mapId} phoneSize={64} revision={lobby.mapRevision} />
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
      <RouterButton
        label="Join"
        params={{ matchId: lobby.matchId }}
        to="/matches/$matchId"
        xstyle={styles.join}
      />
    </VStack>
  );
}

const styles = stylex.create({
  card: {
    display: "grid",
    gridTemplateColumns: {
      default: "auto minmax(0, 1fr)",
      [pageLayout.phoneMedia]: "minmax(0, 1fr)",
    },
    gridTemplateAreas: {
      default: '"board body" "board act"',
      [pageLayout.phoneMedia]: '"board" "body" "act"',
    },
    gridTemplateRows: {
      default: "auto minmax(0, 1fr)",
      [pageLayout.phoneMedia]: "auto",
    },
    columnGap: spacingVars["--spacing-6"],
    rowGap: {
      default: 0,
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
  },
  well: {
    gridArea: "board",
    display: "grid",
    placeItems: "center",
    alignSelf: "start",
    boxSizing: "border-box",
    overflow: "hidden",
    inlineSize: {
      default: "auto",
      [pageLayout.phoneMedia]: "100%",
    },
    blockSize: {
      default: "auto",
      [pageLayout.phoneMedia]: `${PHONE_WELL}px`,
    },
    // No top padding: the full picture's top row is the terrain overhang,
    // transparent where nothing rises into it, so it stands in for it.
    paddingBlockEnd: {
      default: spacingVars["--spacing-4"],
      [pageLayout.phoneMedia]: 0,
    },
    paddingInline: {
      default: spacingVars["--spacing-4"],
      [pageLayout.phoneMedia]: 0,
    },
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink900,
    boxShadow: `inset 0 0 0 ${borderVars["--border-width"]} ${awbrnVars.ink700}`,
  },
  picture: {
    maxInlineSize: "none",
    imageRendering: "pixelated",
  },
  phonePicture: (width: number, height: number) => ({
    inlineSize: `${width}px`,
    blockSize: `${height}px`,
  }),
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
  body: {
    gridArea: "body",
    alignSelf: "start",
    display: "grid",
    justifyItems: "start",
    gap: spacingVars["--spacing-1-5"],
    paddingBlockStart: {
      default: spacingVars["--spacing-1"],
      [pageLayout.phoneMedia]: 0,
    },
    minInlineSize: 0,
  },
  title: {
    alignItems: "center",
    gap: spacingVars["--spacing-3"],
    maxInlineSize: "100%",
    minInlineSize: 0,
  },
  mapName: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  funds: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1"],
    marginBlockStart: spacingVars["--spacing-1"],
    fontSize: typeScaleVars["--text-supporting-size"],
    fontWeight: fontWeightVars["--font-weight-semibold"],
  },
  act: {
    gridArea: "act",
    alignSelf: "start",
    alignItems: "stretch",
    gap: {
      default: spacingVars["--spacing-7"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-3"],
    },
    marginBlockStart: {
      default: spacingVars["--spacing-7"],
      [pageLayout.phoneMedia]: 0,
    },
    minInlineSize: 0,
  },
  pick: {
    display: "grid",
    gridTemplateColumns: {
      default: "40px auto",
      [pageLayout.phoneMedia]: "40px minmax(0, 1fr)",
    },
    justifyContent: "start",
    justifyItems: "start",
    alignItems: "center",
    gap: spacingVars["--spacing-3"],
  },
  pickKey: {
    color: colorVars["--color-text-secondary"],
    fontSize: typeScaleVars["--text-label-size"],
    fontWeight: typeScaleVars["--text-label-weight"],
  },
  coKey: {
    blockSize: "44px",
    paddingInlineStart: spacingVars["--spacing-1"],
    paddingInlineEnd: awbrnVars.spacing2_5,
    backgroundColor: awbrnVars.paperWell,
  },
  coKeyBody: {
    display: "inline-flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    fontSize: typeScaleVars["--text-body-size"],
  },
  coKeyChevron: {
    color: colorVars["--color-text-secondary"],
  },
  tier: {
    justifySelf: {
      default: "start",
      [pageLayout.phoneMedia]: "stretch",
    },
    inlineSize: {
      default: "auto",
      [pageLayout.phoneMedia]: "100%",
    },
  },
  // Desktop: each segment hugs its label, so no label is cut short.
  // Phone: the segments share the row.
  tierItem: {
    flex: {
      default: null,
      [pageLayout.phoneMedia]: "1",
    },
  },
  keys: {
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
  },
  other: {
    flexShrink: 0,
    blockSize: {
      default: null,
      [pageLayout.phoneMedia]: "52px",
    },
  },
  go: {
    flexGrow: 1,
  },
  picker: {
    padding: spacingVars["--spacing-4"],
  },
  lobby: {
    display: "grid",
    gridTemplateColumns: "auto minmax(0, 1fr) auto",
    alignItems: "center",
    columnGap: {
      default: spacingVars["--spacing-4"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-3"],
    },
    paddingBlock: spacingVars["--spacing-4"],
    paddingInline: {
      default: spacingVars["--spacing-5"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
  },
  lobbyMain: {
    display: "grid",
    gap: spacingVars["--spacing-1-5"],
    minInlineSize: 0,
  },
  lobbyTitle: {
    fontSize: awbrnVars.text16,
    fontWeight: fontWeightVars["--font-weight-bold"],
    lineHeight: 1.25,
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  lobbyMeta: {
    flexWrap: "wrap",
    alignItems: "center",
    columnGap: awbrnVars.spacing2_5,
    rowGap: spacingVars["--spacing-1"],
  },
  lobbyHost: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1-5"],
    fontSize: awbrnVars.text13_5,
    fontWeight: fontWeightVars["--font-weight-semibold"],
  },
  inlinePostage: {
    display: "inline",
  },
  join: {
    flexShrink: 0,
    paddingInline: awbrnVars.spacing4_5,
  },
});
