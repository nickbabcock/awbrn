import { Text } from "@astryxdesign/core/Text";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack, Stack } from "@astryxdesign/core/Stack";
import { radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { resolveAwbwUsername } from "#/awbw/api.ts";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { unitSpriteStyle } from "#/components/game_sprites.ts";
import { useReplayRunner } from "#/engine/runtime_context.tsx";
import { useGameActions, useGameStore } from "#/engine/store.ts";
import { getFactionByCode } from "#/factions.ts";
import type { ArmyView } from "#/matches/components/Armies.tsx";
import { InkIconKey } from "#/matches/components/MatchFrame.tsx";
import type { LogBoundary } from "#/matches/components/turn_log.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { ProblemBanner } from "#/ui/EmptySheet.tsx";
import { IconBack, IconUpload } from "#/ui/icons.tsx";
import { Page } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { readArchiveOutline } from "./archive_outline.ts";
import { ReplayScreen } from "./ReplayScreen.tsx";

interface AwbwMapSummary {
  name: string;
  width: number;
  height: number;
}

/** The AWBW map an archive was played on: its name and size. */
async function fetchAwbwMap(mapId: number): Promise<AwbwMapSummary | null> {
  const response = await fetch(`/api/awbw/map/${mapId}`);
  if (!response.ok) return null;
  const payload = (await response.json()) as {
    Name?: string;
    "Size X"?: number;
    "Size Y"?: number;
  };
  if (typeof payload.Name !== "string") return null;
  return {
    name: payload.Name,
    width: payload["Size X"] ?? 0,
    height: payload["Size Y"] ?? 0,
  };
}

/** The replay viewer for an AWBW .zip or AWBRN .json archive, read on this device. */
export function ReplayPage() {
  const playerRoster = useGameStore((state) => state.playerRoster);
  const boardMap = useGameStore((state) => state.boardView?.map ?? null);
  const gameActions = useGameActions();
  const runner = useReplayRunner();
  const fileRef = useRef<HTMLInputElement>(null);
  const [playerNames, setPlayerNames] = useState<Record<number, string>>({});
  const [replayError, setReplayError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [outline, setOutline] = useState<LogBoundary[] | null>(null);
  const loadCount = useRef(0);
  const mapId = playerRoster?.mapId ?? null;
  const { data: awbwMap } = useQuery({
    queryKey: ["awbw", "map", mapId],
    queryFn: () => fetchAwbwMap(mapId ?? 0),
    enabled: mapId !== null && mapId > 0,
    staleTime: Infinity,
  });

  async function loadFile(file: File | null) {
    if (!file) return;
    gameActions.setPlayerRoster(null);
    setReplayError(null);
    setIsLoading(true);
    setOutline(null);
    // The outline is read beside the engine, not after it, so the day ticks
    // and the log are there as soon as the board is.
    loadCount.current += 1;
    const load = loadCount.current;
    void file
      .arrayBuffer()
      .then((buffer) => readArchiveOutline(new Uint8Array(buffer)))
      .then((read) => {
        if (load === loadCount.current) setOutline(read);
      });
    try {
      await runner.loadReplay(file);
    } catch (error) {
      gameActions.setPlayerRoster(null);
      setReplayError(error instanceof Error ? error.message : "Replay failed to load.");
      console.error("Error loading replay:", error);
    } finally {
      setIsLoading(false);
      if (fileRef.current) fileRef.current.value = "";
    }
  }

  // The archive names seats by AWBW user; the names are looked up once each.
  useEffect(() => {
    let canceled = false;
    if (!playerRoster) {
      setPlayerNames({});
      return () => {
        canceled = true;
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
        if (!username || canceled) return;
        setPlayerNames((previous) =>
          previous[player.userId] === username
            ? previous
            : { ...previous, [player.userId]: username },
        );
      }),
    );

    return () => {
      canceled = true;
    };
  }, [playerRoster]);

  const views = useMemo<ArmyView[]>(
    () =>
      (playerRoster?.players ?? []).map((player) => ({
        army: {
          entry: player,
          hasLiveStats: true,
          isActive: playerRoster?.activePlayerId === player.playerId,
          name: playerNames[player.userId] ?? `Player ${player.turnOrder + 1}`,
        },
        isViewer: false,
        clock: null,
        onFactionChange: (factionId: number) => {
          void runner
            .setPlayerDisplayFaction(
              player.playerId,
              factionId === getFactionByCode(player.actualFactionCode)?.id ? null : factionId,
            )
            .catch((error: unknown) => {
              setReplayError(
                error instanceof Error ? error.message : "Faction depiction failed to update.",
              );
            });
        },
      })),
    [playerNames, playerRoster, runner],
  );

  const fileInput = (
    <input
      accept=".zip,.json"
      aria-label="Load a replay"
      onChange={(event) => void loadFile(event.currentTarget.files?.[0] ?? null)}
      ref={fileRef}
      // The visible keys open it.
      tabIndex={-1}
      type="file"
      {...stylex.props(styles.fileInput)}
    />
  );
  const chooseFile = () => fileRef.current?.click();

  if (playerRoster) {
    const map =
      awbwMap && awbwMap.width > 0 ? { width: awbwMap.width, height: awbwMap.height } : boardMap;
    return (
      <>
        {fileInput}
        <ReplayScreen
          archiveKey={
            <InkIconKey icon={IconUpload} label="Load another replay" onPress={chooseFile} />
          }
          back={
            <Button
              icon={<Icon icon={IconBack} size="lg" />}
              isIconOnly
              label="Close the replay"
              onClick={() => gameActions.setPlayerRoster(null)}
              variant="ghost-on-ink"
              xstyle={styles.back}
            />
          }
          error={replayError}
          locatorSrc={mapId ? `/api/awbw/smallmap/${mapId}` : null}
          map={map}
          outline={outline}
          runner={runner}
          title={awbwMap?.name ?? `Game ${playerRoster.matchId}`}
          views={views}
        />
      </>
    );
  }

  return (
    <Page>
      {fileInput}
      <StandbyBoard runner={runner} />
      {replayError ? (
        <ProblemBanner
          action={<Button label="Choose file" onClick={chooseFile} size="sm" variant="secondary" />}
          description={replayError}
          title="The replay could not be read"
        />
      ) : null}
      <section
        aria-labelledby="load-replay"
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => {
          event.preventDefault();
          void loadFile(event.dataTransfer.files[0] ?? null);
        }}
        {...stylex.props(styles.empty)}
      >
        <HStack as="span" aria-hidden="true" xstyle={styles.art}>
          {/* Raw span: the two units are inline atlas crops. */}
          <span style={unitSpriteStyle("infantry", "os", 2) ?? undefined} />
          <span style={unitSpriteStyle("mech", "bm", 2) ?? undefined} />
        </HStack>
        <h1 id="load-replay" {...stylex.props(styles.title)}>
          Load a replay
        </h1>
        <Text color="inherit" type="inherit" xstyle={styles.formats}>
          .zip · .json
        </Text>
        <Button
          icon={<Icon icon={IconUpload} />}
          isLoading={isLoading}
          label="Choose file"
          onClick={chooseFile}
          size="lg"
          variant="primary"
        />
      </section>
    </Page>
  );
}

/**
 * The board's canvas, held off screen while there is no archive, so the engine
 * is ready by the time a file is chosen.
 */
function StandbyBoard({ runner }: { runner: ReturnType<typeof useReplayRunner> }) {
  const { canvasContainerRef } = useCanvasCourierSurface({
    host: runner.host,
    canvasClassName: stylex.props(styles.standbyCanvas).className ?? "",
  });
  return <Stack aria-hidden="true" ref={canvasContainerRef} xstyle={styles.standby} />;
}

const styles = stylex.create({
  standby: {
    position: "fixed",
    insetBlockStart: 0,
    insetInlineStart: "-10000px",
    inlineSize: "960px",
    blockSize: "640px",
    visibility: "hidden",
    pointerEvents: "none",
  },
  standbyCanvas: {
    display: "block",
    width: "100%",
    height: "100%",
  },
  fileInput: {
    position: "absolute",
    inlineSize: "1px",
    blockSize: "1px",
    overflow: "hidden",
    clipPath: "inset(50%)",
    whiteSpace: "nowrap",
  },
  back: {
    inlineSize: "44px",
    minInlineSize: "44px",
    blockSize: "44px",
    paddingInline: 0,
    color: awbrnVars.textOnInkStrong,
  },
  empty: {
    display: "grid",
    justifyItems: "center",
    alignContent: "center",
    gap: spacingVars["--spacing-3"],
    minBlockSize: {
      default: "260px",
      [pageLayout.phoneMedia]: "220px",
    },
    // Centered in the page column.
    inlineSize: "100%",
    maxInlineSize: "560px",
    marginInline: "auto",
    paddingBlock: "40px",
    paddingInline: spacingVars["--spacing-6"],
    borderColor: awbrnVars.ink600,
    borderRadius: radiusVars["--radius-container"],
    borderStyle: "dashed",
    borderWidth: "1.5px",
    backgroundColor: awbrnVars.ink950,
    color: awbrnVars.textOnInk,
    textAlign: "center",
  },
  art: {
    alignItems: "flex-end",
    gap: spacingVars["--spacing-4"],
  },
  title: {
    margin: 0,
    color: awbrnVars.textOnInkStrong,
    fontSize: awbrnVars.text26,
    fontStretch: "68%",
    fontWeight: 800,
    lineHeight: 1.05,
    wordSpacing: "0.1em",
  },
  formats: {
    color: awbrnVars.textOnInkSecondary,
    fontFamily: awbrnVars.fontFigure,
    fontSize: awbrnVars.text12_5,
    fontStretch: "87.5%",
  },
});
