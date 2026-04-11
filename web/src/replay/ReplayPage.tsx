import { useEffect, useState } from "react";
import * as stylex from "@stylexjs/stylex";
import type { CSSProperties } from "react";
import { resolveAwbwUsername } from "#/awbw/api.ts";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { FactionSelectionControl } from "#/components/FactionSelectionControl.tsx";
import { loadCoPortraitCatalog, type CoPortraitCatalog } from "#/components/co_portraits.ts";
import { PlayerHeader } from "#/components/PlayerHeader.tsx";
import { useReplayRunner } from "#/engine/runtime_context.tsx";
import { useGameActions, useGameStore } from "#/engine/store.ts";
import { getFactionVisual } from "#/faction_visuals.ts";
import { getFactionByCode } from "#/factions.ts";
import { Badge, Text, Wordmark } from "#/ui/primitives.tsx";
import { tokens, media } from "#/ui/theme.stylex.ts";
import { infantrySpriteStyle, uiAtlasSpriteStyle } from "./roster_icons";

const formatMoney = (value: number) => value.toLocaleString();
const formatMaybeMoney = (value: number | null | undefined) =>
  value == null ? "--" : formatMoney(value);
const formatMaybeCount = (value: number | null | undefined) =>
  value == null ? "--" : value.toString();

const rosterPlayerSurface = (wash: string): CSSProperties => ({
  backgroundImage: `linear-gradient(115deg, ${wash} 0%, rgba(255,255,255,0) 42%), linear-gradient(180deg, rgba(245, 232, 192, 0.94), rgba(252, 246, 230, 0.96))`,
});

const cursorBlink = stylex.keyframes({
  "0%, 49%": { opacity: 1 },
  "50%, 100%": { opacity: 0.25 },
});

const popIn = stylex.keyframes({
  "0%": { opacity: 0, transform: "translateY(-4px)" },
  "100%": { opacity: 1, transform: "translateY(0)" },
});

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
  const runner = useReplayRunner();

  const { canvasRef, focus, surfaceRef } = useCanvasCourierSurface({
    controller: runner,
  });

  const handleReplayFileChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (file) {
      gameActions.setPlayerRoster(null);
      try {
        if (!runner) {
          throw new Error("Game runner was not initialized.");
        }
        await runner.loadReplay(file);
        focus();
      } catch (error) {
        gameActions.setPlayerRoster(null);
        console.error("Error loading replay:", error);
      }
    }
  };

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
        if (!username || cancelled) {
          return;
        }

        setPlayerNames((previous) => {
          if (previous[player.userId] === username) {
            return previous;
          }

          return { ...previous, [player.userId]: username };
        });
      }),
    );

    return () => {
      cancelled = true;
    };
  }, [playerRoster]);

  const handlePlayerDisplayFactionChange = async (playerId: number, factionId: number | null) => {
    if (!runner) {
      throw new Error("Game runner was not initialized.");
    }

    await runner.setPlayerDisplayFaction(playerId, factionId);
  };

  return (
    <div {...stylex.props(styles.root)}>
      <div ref={surfaceRef} {...stylex.props(styles.gameSurface)}>
        <canvas
          ref={canvasRef}
          width={600}
          height={400}
          tabIndex={0}
          {...stylex.props(styles.gameCanvas)}
        />
        {!playerRoster && (
          <div {...stylex.props(styles.emptyState)}>
            <Wordmark shadow />
            <Text size="lg" tone="inverse" xstyle={styles.subtitle}>
              Advance Wars Online
            </Text>
            <Text size="sm" tone="inverseMuted" xstyle={styles.prompt}>
              Load a replay to begin
            </Text>
          </div>
        )}
      </div>
      <div {...stylex.props(styles.hudPanel, styles.dayPanel)}>Day: {currentDay}</div>
      <aside {...stylex.props(styles.hudPanel, styles.rosterPanel)}>
        <div {...stylex.props(styles.panelTitle)}>Replay Roster</div>
        {playerRoster ? (
          <>
            <div {...stylex.props(styles.panelSubtitle)}>
              Game {playerRoster.matchId} · Map {playerRoster.mapId}
            </div>
            <div {...stylex.props(styles.rosterList)}>
              {playerRoster.players.map((player) => {
                const factionVisual = getFactionVisual(player.displayFactionCode);
                const playerName = playerNames[player.userId] ?? `Player ${player.turnOrder}`;
                const isActivePlayer = playerRoster.activePlayerId === player.playerId;
                const playerMeta = [
                  player.team ? `Team ${player.team}` : null,
                  player.eliminated ? "Eliminated" : null,
                ].filter((value): value is string => value !== null);
                const playerStats = [
                  {
                    key: "funds",
                    label: "Funds",
                    value: formatMaybeMoney(player.stats.funds),
                    icon: <StatIcon spriteName="Coin.png" />,
                  },
                  {
                    key: "units",
                    label: "Units",
                    value: formatMaybeCount(player.stats.unitCount),
                    icon: <StatIcon factionCode={player.displayFactionCode} />,
                  },
                  {
                    key: "value",
                    label: "Value",
                    value: formatMaybeMoney(player.stats.unitValue),
                    icon: <StatIcon factionCode={player.displayFactionCode} coinOverlay />,
                  },
                  {
                    key: "income",
                    label: "Income",
                    value: formatMaybeMoney(player.stats.income),
                    icon: <StatIcon spriteName="BuildingsCaptured.png" />,
                  },
                ];

                return (
                  <div
                    key={player.playerId}
                    style={rosterPlayerSurface(factionVisual.wash)}
                    {...stylex.props(styles.rosterPlayer)}
                  >
                    <PlayerHeader
                      factionCode={player.displayFactionCode}
                      name={playerName}
                      xstyle={styles.rosterPlayerHeader}
                      trailing={
                        <>
                          {isActivePlayer ? <Badge tone="brand">Turn</Badge> : null}
                          <FactionSelectionControl
                            align="end"
                            sideOffset={8}
                            disabled={false}
                            factionCode={player.displayFactionCode}
                            onDark
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
                    <div {...stylex.props(styles.rosterPlayerPortraits)}>
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
                    </div>
                    <div {...stylex.props(styles.rosterPlayerCopy)}>
                      {playerMeta.length > 0 ? (
                        <div {...stylex.props(styles.rosterPlayerMeta)}>
                          {playerMeta.join(" · ")}
                        </div>
                      ) : null}
                      <div {...stylex.props(styles.rosterPlayerStats)}>
                        {playerStats.map((stat) => (
                          <div
                            aria-label={`${stat.label}: ${stat.value}`}
                            key={stat.key}
                            title={stat.label}
                            {...stylex.props(styles.rosterPlayerStat)}
                          >
                            {stat.icon}
                            <span {...stylex.props(styles.rosterPlayerStatValue)}>
                              {stat.value}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </>
        ) : (
          <div {...stylex.props(styles.rosterEmpty)}>
            Load a replay to inspect player CO portraits.
          </div>
        )}
      </aside>
      <div {...stylex.props(styles.hudPanel, styles.filePanel)}>
        <Text
          as="label"
          htmlFor="replay-file-input"
          size="sm"
          tone="muted"
          xstyle={styles.fileLabel}
        >
          Load Replay
        </Text>
        <input
          id="replay-file-input"
          type="file"
          accept=".zip"
          onChange={handleReplayFileChange}
          {...stylex.props(styles.fileInput)}
        />
      </div>
    </div>
  );
}

const styles = stylex.create({
  root: {
    position: "relative",
    minHeight: `calc(100vh - ${tokens.navHeight})`,
  },
  gameSurface: {
    position: "absolute",
    inset: 0,
    overflow: "hidden",
  },
  gameCanvas: {
    display: "block",
    width: "100%",
    height: "100%",
    outline: "none",
  },
  emptyState: {
    position: "absolute",
    inset: 0,
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    gap: tokens.space3,
    pointerEvents: "none",
  },
  subtitle: {
    color: "rgba(255, 245, 220, 0.82)",
    fontWeight: 800,
    letterSpacing: "0.1em",
    textTransform: "uppercase",
  },
  prompt: {
    fontFamily: tokens.fontPixel,
    letterSpacing: "0.08em",
    textTransform: "uppercase",
    animationDuration: "1.4s",
    animationIterationCount: "infinite",
    animationTimingFunction: "step-end",
    animationName: cursorBlink,
  },
  hudPanel: {
    position: "fixed",
    zIndex: 10,
    borderWidth: 3,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius3,
    backgroundColor: "rgba(248, 237, 201, 0.98)",
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardLg}`,
    overflow: "hidden",
  },
  dayPanel: {
    top: `calc(${tokens.navHeight} + 16px)`,
    left: 16,
    minHeight: 46,
    paddingInline: tokens.space4,
    display: "inline-flex",
    alignItems: "center",
    backgroundColor: tokens.brand,
    borderColor: tokens.strokeHeavy,
    color: tokens.onDarkStrong,
    fontFamily: tokens.fontPixel,
    fontSize: 10,
    letterSpacing: "0.08em",
    textTransform: "uppercase",
  },
  rosterPanel: {
    top: {
      default: `calc(${tokens.navHeight} + 16px)`,
      [media.compact]: "auto",
    },
    right: {
      default: 16,
      [media.compact]: 8,
    },
    bottom: {
      default: "auto",
      [media.compact]: 92,
    },
    left: {
      default: "auto",
      [media.compact]: 8,
    },
    width: {
      default: 320,
      [media.compact]: "auto",
    },
    maxHeight: {
      default: `calc(100vh - ${tokens.navHeight} - 88px)`,
      [media.compact]: "42vh",
    },
    overflow: "auto",
    padding: tokens.space3,
  },
  panelTitle: {
    margin: "calc(-1 * 12px) calc(-1 * 12px) 8px",
    padding: "12px 12px",
    backgroundColor: tokens.chromeBg,
    borderBottomWidth: 3,
    borderBottomStyle: "solid",
    borderBottomColor: tokens.chromeBorder,
    color: tokens.onDarkStrong,
    fontFamily: tokens.fontPixel,
    fontSize: 10,
    letterSpacing: "0.08em",
    textTransform: "uppercase",
  },
  panelSubtitle: {
    color: tokens.ink,
    fontFamily: tokens.fontBody,
    fontSize: 14,
    marginBottom: tokens.space2,
  },
  rosterList: {
    display: "grid",
    gap: tokens.space2,
  },
  rosterPlayer: {
    display: "grid",
    gap: tokens.space2,
    gridTemplateAreas: '"header header" "portraits copy"',
    gridTemplateColumns: "auto 1fr",
    padding: "0 8px 8px",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
  },
  rosterPlayerHeader: {
    gridArea: "header",
    marginInline: -8,
    paddingInline: 8,
  },
  factionBadgeButton: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 24,
    height: 24,
    borderRadius: tokens.radius1,
    backgroundColor: "rgba(255, 255, 255, 0.16)",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: "rgba(255, 255, 255, 0.24)",
    cursor: "pointer",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, opacity",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
      ":disabled": "translateY(0)",
    },
    opacity: {
      default: 1,
      ":disabled": 0.55,
    },
  },
  factionBadgeLogo: {
    display: "block",
    width: 14,
    height: 14,
    // Keep this in sync with the logos atlas geometry in `faction_visuals.ts`
    // where `LOGO_COLUMNS = 10` and `LOGO_TILE_SIZE = 14`.
    backgroundSize: "140px 28px",
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  },
  pickerPopup: {
    borderWidth: 3,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius3,
    backgroundColor: tokens.panelRaised,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardLg}`,
    padding: tokens.space3,
    animationDuration: "140ms",
    animationFillMode: "both",
    animationName: popIn,
  },
  factionPopup: {
    width: "min(420px, calc(100vw - 32px))",
  },
  factionPickerIntro: {
    display: "grid",
    gap: 4,
    paddingBottom: tokens.space3,
  },
  selectorLabel: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  dropdownSubtitle: {
    color: tokens.inkMuted,
    fontFamily: tokens.fontBody,
    fontSize: tokens.textSm,
    lineHeight: tokens.leadingBody,
  },
  factionGrid: {
    display: "grid",
    gap: tokens.space1,
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
  },
  factionTile: (wash: string) => ({
    display: "flex",
    alignItems: "center",
    gap: tokens.space2,
    width: "100%",
    padding: tokens.space1,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    backgroundColor: wash,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    cursor: "pointer",
    textAlign: "left",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, border-color, background-color",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
    },
  }),
  factionTileSelected: {
    borderColor: tokens.strokeHeavy,
    backgroundColor: tokens.brandSoft,
  },
  factionTileLogoWrap: {
    flex: "0 0 auto",
    width: 14,
    height: 14,
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
  },
  factionTileLogo: {
    width: 14,
    height: 14,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  },
  factionTileCopy: {
    display: "grid",
    gap: 2,
  },
  tileTitle: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontBody,
    fontSize: tokens.textSm,
    fontWeight: 800,
  },
  tileMeta: {
    color: tokens.inkMuted,
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  factionResetBadge: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 14,
    height: 14,
    color: tokens.inkMuted,
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  rosterPlayerPortraits: {
    display: "flex",
    gap: 4,
    gridArea: "portraits",
  },
  rosterPlayerCopy: {
    gridArea: "copy",
    minWidth: 0,
  },
  rosterPlayerMeta: {
    color: tokens.inkMuted,
    fontFamily: tokens.fontBody,
    fontSize: 13,
    marginBottom: tokens.space2,
  },
  rosterPlayerStats: {
    display: "grid",
    gap: 6,
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
  },
  rosterPlayerStat: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    minWidth: 0,
    padding: "5px 6px",
    borderRadius: tokens.radius1,
    backgroundColor: "rgba(255,255,255,0.38)",
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
  },
  rosterPlayerStatValue: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontBody,
    fontSize: 12,
    fontWeight: 800,
    fontVariantNumeric: "lining-nums tabular-nums",
    letterSpacing: "0.01em",
    lineHeight: 1,
    whiteSpace: "nowrap",
  },
  rosterEmpty: {
    color: tokens.inkMuted,
    fontFamily: tokens.fontBody,
    fontSize: 14,
    lineHeight: 1.5,
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
  filePanel: {
    left: {
      default: 16,
      [media.compact]: 8,
    },
    right: {
      default: "auto",
      [media.compact]: 8,
    },
    bottom: {
      default: 16,
      [media.compact]: 8,
    },
    display: "grid",
    gap: tokens.space2,
    padding: tokens.space3,
    minWidth: {
      default: 220,
      [media.compact]: 0,
    },
  },
  fileLabel: {
    fontFamily: tokens.fontPixel,
    fontSize: 9,
    letterSpacing: "0.08em",
    textTransform: "uppercase",
  },
  fileInput: {
    color: tokens.inkStrong,
  },
});
