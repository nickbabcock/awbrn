import { Badge } from "@astryxdesign/core/Badge";
import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { Center } from "@astryxdesign/core/Center";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { FileInput } from "@astryxdesign/core/FileInput";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useState } from "react";
import { resolveAwbwUsername } from "#/awbw/api.ts";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { FactionSelectionControl } from "#/components/FactionSelectionControl.tsx";
import { loadCoPortraitCatalog, type CoPortraitCatalog } from "#/components/co_portraits.ts";
import { PlayerHeader } from "#/components/PlayerHeader.tsx";
import { useReplayRunner } from "#/engine/runtime_context.tsx";
import { useGameActions, useGameStore } from "#/engine/store.ts";
import { getFactionByCode } from "#/factions.ts";
import { infantrySpriteStyle, uiAtlasSpriteStyle } from "./roster_icons";

const formatMoney = (value: number) => value.toLocaleString();
const formatMaybeMoney = (value: number | null | undefined) =>
  value == null ? "--" : formatMoney(value);
const formatMaybeCount = (value: number | null | undefined) =>
  value == null ? "--" : value.toString();

function StatIcon({
  spriteName,
  factionCode,
  coinOverlay = false,
}: {
  spriteName?: string;
  factionCode?: string;
  coinOverlay?: boolean;
}) {
  const baseStyle = spriteName
    ? uiAtlasSpriteStyle(spriteName)
    : factionCode
      ? infantrySpriteStyle(factionCode)
      : null;
  const coinStyle = coinOverlay ? uiAtlasSpriteStyle("Coin.png") : null;

  return (
    <span aria-hidden="true" {...stylex.props(styles.statIconStack)}>
      <span style={baseStyle ?? undefined} {...stylex.props(styles.statIcon)} />
      {coinStyle ? (
        <span style={coinStyle} {...stylex.props(styles.statIcon, styles.statIconCoin)} />
      ) : null}
    </span>
  );
}

export function ReplayPage() {
  const currentDay = useGameStore((state) => state.currentDay);
  const playerRoster = useGameStore((state) => state.playerRoster);
  const gameActions = useGameActions();
  const [portraitCatalog] = useState<CoPortraitCatalog>(() => loadCoPortraitCatalog());
  const [playerNames, setPlayerNames] = useState<Record<number, string>>({});
  const [replayFile, setReplayFile] = useState<File | null>(null);
  const [replayError, setReplayError] = useState<string | null>(null);
  const runner = useReplayRunner();
  const { canvasRef, focus, surfaceRef } = useCanvasCourierSurface({ controller: runner });

  async function handleReplayFileChange(files: File | File[] | null) {
    const file = Array.isArray(files) ? files[0] : files;
    if (!file) {
      setReplayFile(null);
      gameActions.setPlayerRoster(null);
      setReplayError(null);
      return;
    }

    gameActions.setPlayerRoster(null);
    setReplayError(null);
    try {
      if (!runner) throw new Error("Game runner was not initialized.");
      await runner.loadReplay(file);
      focus();
    } catch (error) {
      gameActions.setPlayerRoster(null);
      setReplayError(error instanceof Error ? error.message : "Replay failed to load.");
      console.error("Error loading replay:", error);
    }
  }

  useEffect(() => {
    let cancelled = false;

    if (!playerRoster) {
      setPlayerNames({});
      return () => {
        cancelled = true;
      };
    }

    const activeUserIds = new Set(playerRoster.players.map((player) => player.userId));
    setPlayerNames((previous) =>
      Object.fromEntries(
        Object.entries(previous).filter(([userId]) => activeUserIds.has(Number(userId))),
      ),
    );

    void Promise.all(
      playerRoster.players.map(async (player) => {
        const username = await resolveAwbwUsername(player.userId);
        if (!username || cancelled) return;

        setPlayerNames((previous) =>
          previous[player.userId] === username
            ? previous
            : { ...previous, [player.userId]: username },
        );
      }),
    );

    return () => {
      cancelled = true;
    };
  }, [playerRoster]);

  async function handlePlayerDisplayFactionChange(playerId: number, factionId: number | null) {
    setReplayError(null);
    try {
      if (!runner) throw new Error("Game runner was not initialized.");
      await runner.setPlayerDisplayFaction(playerId, factionId);
    } catch (error) {
      setReplayError(
        error instanceof Error ? error.message : "Faction depiction failed to update.",
      );
      console.error("Error updating replay faction depiction:", error);
    }
  }

  return (
    <Section padding={4} variant="transparent">
      <VStack gap={4}>
        <HStack align="center" gap={3} justify="between" wrap="wrap">
          <VStack gap={1}>
            <Text color="accent" type="supporting" weight="bold">
              Replay viewer
            </Text>
            <Heading level={1}>Battle review</Heading>
          </VStack>
          <Badge label={`Day ${currentDay}`} variant="info" />
        </HStack>

        {replayError ? (
          <Banner description={replayError} status="error" title="Replay failed to load" />
        ) : null}

        <Grid align="start" columns={{ minWidth: 300, max: 2, repeat: "fit" }} gap={4}>
          <Card padding={0} width="100%">
            <Section
              height={640}
              padding={0}
              ref={surfaceRef}
              variant="muted"
              xstyle={styles.gameSurface}
            >
              <canvas
                ref={canvasRef}
                width={960}
                height={640}
                tabIndex={0}
                {...stylex.props(styles.gameCanvas, !replayFile && styles.gameCanvasHidden)}
              />
              {!playerRoster ? (
                <Center height="100%" width="100%" xstyle={styles.emptyOverlay}>
                  <EmptyState
                    description="Choose an AWBW .zip replay to begin."
                    headingLevel={2}
                    title="Load a replay"
                  />
                </Center>
              ) : null}
            </Section>
          </Card>

          <VStack gap={4}>
            <Card padding={4} width="100%">
              <FileInput
                accept=".zip"
                changeAction={handleReplayFileChange}
                description="AWBW replay archives in .zip format"
                label="Load replay"
                mode="dropzone"
                onChange={(files) =>
                  setReplayFile(Array.isArray(files) ? (files[0] ?? null) : files)
                }
                value={replayFile}
                width="100%"
              />
            </Card>

            <Section padding={4} variant="section" xstyle={styles.rosterPanel}>
              <VStack gap={3}>
                <VStack gap={1}>
                  <Heading level={2}>Replay roster</Heading>
                  {playerRoster ? (
                    <Text color="secondary" type="supporting">
                      Game {playerRoster.matchId} · Map {playerRoster.mapId}
                    </Text>
                  ) : null}
                </VStack>

                {playerRoster ? (
                  <VStack gap={3}>
                    {playerRoster.players.map((player) => {
                      const playerName = playerNames[player.userId] ?? `Player ${player.turnOrder}`;
                      const isActivePlayer = playerRoster.activePlayerId === player.playerId;
                      const playerMeta = [
                        player.team ? `Team ${player.team}` : null,
                        player.eliminated ? "Eliminated" : null,
                      ].filter((value): value is string => value !== null);

                      return (
                        <Section key={player.playerId} padding={0} variant="muted">
                          <VStack gap={2}>
                            <PlayerHeader
                              factionCode={player.displayFactionCode}
                              name={playerName}
                              trailing={
                                <>
                                  {isActivePlayer ? (
                                    <StatusDot label="Active turn" variant="accent" />
                                  ) : null}
                                  <FactionSelectionControl
                                    disabled={false}
                                    factionCode={player.displayFactionCode}
                                    onChange={(factionId) =>
                                      handlePlayerDisplayFactionChange(
                                        player.playerId,
                                        factionId === getFactionByCode(player.actualFactionCode)?.id
                                          ? null
                                          : factionId,
                                      )
                                    }
                                  />
                                </>
                              }
                            />
                            <Section padding={3} variant="transparent">
                              <VStack gap={3}>
                                <HStack align="center" gap={2}>
                                  <CoPortrait
                                    catalog={portraitCatalog}
                                    coKey={player.coKey}
                                    fallbackLabel={player.coName ?? "?"}
                                  />
                                  {player.tagCoKey ? (
                                    <CoPortrait
                                      catalog={portraitCatalog}
                                      coKey={player.tagCoKey}
                                      fallbackLabel={player.tagCoName ?? "?"}
                                    />
                                  ) : null}
                                  {playerMeta.length > 0 ? (
                                    <Text color="secondary" type="supporting">
                                      {playerMeta.join(" · ")}
                                    </Text>
                                  ) : null}
                                </HStack>
                                <MetadataList columns="multi" label={{ position: "top" }}>
                                  <MetadataListItem
                                    icon={<StatIcon spriteName="Coin.png" />}
                                    label="Funds"
                                  >
                                    {formatMaybeMoney(player.stats.funds)}
                                  </MetadataListItem>
                                  <MetadataListItem
                                    icon={<StatIcon factionCode={player.displayFactionCode} />}
                                    label="Units"
                                  >
                                    {formatMaybeCount(player.stats.unitCount)}
                                  </MetadataListItem>
                                  <MetadataListItem
                                    icon={
                                      <StatIcon
                                        coinOverlay
                                        factionCode={player.displayFactionCode}
                                      />
                                    }
                                    label="Value"
                                  >
                                    {formatMaybeMoney(player.stats.unitValue)}
                                  </MetadataListItem>
                                  <MetadataListItem
                                    icon={<StatIcon spriteName="BuildingsCaptured.png" />}
                                    label="Income"
                                  >
                                    {formatMaybeMoney(player.stats.income)}
                                  </MetadataListItem>
                                  <MetadataListItem
                                    icon={<StatIcon spriteName="NormalPower.png" />}
                                    label="Power"
                                  >
                                    {formatMaybeCount(player.powerCharge)}
                                  </MetadataListItem>
                                </MetadataList>
                              </VStack>
                            </Section>
                          </VStack>
                        </Section>
                      );
                    })}
                  </VStack>
                ) : (
                  <EmptyState
                    description="Load a replay to inspect player CO portraits and public stats."
                    headingLevel={3}
                    isCompact
                    title="No roster loaded"
                  />
                )}
              </VStack>
            </Section>
          </VStack>
        </Grid>
      </VStack>
    </Section>
  );
}

const styles = stylex.create({
  gameSurface: {
    position: "relative",
    overflow: "hidden",
  },
  gameCanvas: {
    display: "block",
    width: "100%",
    height: "100%",
    outline: "none",
    imageRendering: "pixelated",
  },
  gameCanvasHidden: {
    visibility: "hidden",
  },
  emptyOverlay: {
    position: "absolute",
    inset: 0,
    pointerEvents: "none",
  },
  rosterPanel: {
    maxHeight: 760,
    overflowY: "auto",
  },
  statIconStack: {
    position: "relative",
    display: "inline-flex",
    width: 18,
    height: 18,
    flex: "0 0 auto",
  },
  statIcon: {
    display: "block",
    width: 16,
    height: 16,
    imageRendering: "pixelated",
    backgroundRepeat: "no-repeat",
  },
  statIconCoin: {
    position: "absolute",
    right: -2,
    bottom: -2,
    width: 10,
    height: 10,
  },
});
