import { useEffect, useRef, useState } from "react";
import * as stylex from "@stylexjs/stylex";
import type { CSSProperties } from "react";
import { resolveAwbwUsername } from "#/awbw/api.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { loadCoPortraitCatalog, type CoPortraitCatalog } from "#/components/co_portraits.ts";
import { GameRunner } from "#/engine/game_runner.ts";
import { useGameActions, useGameStore } from "#/engine/store.ts";
import { getFactionVisual } from "#/faction_visuals.ts";
import { Badge, Text, Wordmark } from "#/ui/primitives.tsx";
import { tokens } from "#/ui/theme.stylex.ts";
import { infantrySpriteStyle, uiAtlasSpriteStyle } from "./roster_icons";

const formatMoney = (value: number) => value.toLocaleString();
const formatMaybeMoney = (value: number | null | undefined) =>
  value == null ? "--" : formatMoney(value);
const formatMaybeCount = (value: number | null | undefined) =>
  value == null ? "--" : value.toString();

const rosterPlayerSurface = (wash: string): CSSProperties => ({
  backgroundImage: `linear-gradient(115deg, ${wash} 0%, rgba(255,255,255,0) 42%), linear-gradient(180deg, rgba(245, 232, 192, 0.94), rgba(252, 246, 230, 0.96))`,
});

const rosterPlayerHeaderSurface = (accent: string, text: string): CSSProperties => ({
  backgroundImage: `linear-gradient(135deg, rgba(58, 35, 21, 0.28), rgba(58, 35, 21, 0.1)), linear-gradient(135deg, ${accent} 0%, ${text} 100%)`,
});

const rosterPlayerHeadlineShadow = (color: string): CSSProperties => ({
  textShadow: `0 1px 0 ${color}`,
});

const cursorBlink = stylex.keyframes({
  "0%, 49%": { opacity: 1 },
  "50%, 100%": { opacity: 0.25 },
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
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const runnerRef = useRef<GameRunner | null>(null);
  const currentDay = useGameStore((state) => state.currentDay);
  const playerRoster = useGameStore((state) => state.playerRoster);
  const gameActions = useGameActions();
  const [portraitCatalog] = useState<CoPortraitCatalog>(() => loadCoPortraitCatalog());
  const [playerNames, setPlayerNames] = useState<Record<number, string>>({});

  if (runnerRef.current === null) {
    runnerRef.current = new GameRunner();
  }

  const handleReplayFileChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    const runner = runnerRef.current;
    if (file) {
      gameActions.setPlayerRoster(null);
      try {
        if (!runner) {
          throw new Error("Game runner was not initialized.");
        }
        await runner.loadReplay(file);
        canvasRef.current?.focus({ preventScroll: true });
      } catch (error) {
        gameActions.setPlayerRoster(null);
        console.error("Error loading replay:", error);
      }
    }
  };

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    const runner = runnerRef.current;
    if (!canvas || !container || !runner) return;

    runner.attachCanvas({ canvas, container }).catch((error) => {
      console.error("Error attaching game runner:", error);
    });

    return () => {
      runner.dispose();
      runnerRef.current = null;
    };
  }, []);

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

  return (
    <div {...stylex.props(styles.root)}>
      <div ref={containerRef} {...stylex.props(styles.gameSurface)}>
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
                const factionVisual = getFactionVisual(player.factionCode);
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
                    icon: <StatIcon factionCode={player.factionCode} />,
                  },
                  {
                    key: "value",
                    label: "Value",
                    value: formatMaybeMoney(player.stats.unitValue),
                    icon: <StatIcon factionCode={player.factionCode} coinOverlay />,
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
                    <div
                      style={rosterPlayerHeaderSurface(factionVisual.accent, factionVisual.text)}
                      {...stylex.props(styles.rosterPlayerHeader)}
                    >
                      <div {...stylex.props(styles.rosterPlayerHeading)}>
                        <div
                          style={rosterPlayerHeadlineShadow(factionVisual.text)}
                          {...stylex.props(styles.rosterPlayerHeadline)}
                        >
                          {playerName}
                        </div>
                        {isActivePlayer ? <Badge tone="brand">Turn</Badge> : null}
                      </div>
                      <div {...stylex.props(styles.rosterPlayerHeaderBadges)}>
                        <span
                          title={player.factionName}
                          aria-label={`Faction: ${player.factionName}`}
                          {...stylex.props(styles.factionBadge)}
                        >
                          <span
                            aria-hidden="true"
                            style={{
                              backgroundImage: `url(${factionVisual.logoUrl})`,
                              backgroundPosition: factionVisual.logoPosition,
                            }}
                            {...stylex.props(styles.factionLogo)}
                          />
                        </span>
                      </div>
                    </div>
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
  },
  gameCanvas: {
    display: "block",
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
    top: `calc(${tokens.navHeight} + 16px)`,
    right: 16,
    width: {
      default: 320,
      "@media (max-width: 640px)": "calc(100vw - 32px)",
    },
    maxHeight: `calc(100vh - ${tokens.navHeight} - 88px)`,
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
    display: "flex",
    justifyContent: "space-between",
    gap: tokens.space2,
    alignItems: "center",
    marginInline: -8,
    padding: "5px 8px",
    minHeight: 32,
  },
  rosterPlayerHeading: {
    display: "flex",
    gap: tokens.space2,
    alignItems: "center",
    minWidth: 0,
  },
  rosterPlayerHeadline: {
    color: "#fff7eb",
    fontFamily: tokens.fontBody,
    fontSize: 17,
    fontWeight: 800,
  },
  rosterPlayerHeaderBadges: {
    display: "flex",
    alignItems: "center",
    gap: tokens.space2,
  },
  factionBadge: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 24,
    height: 24,
    padding: 3,
    borderRadius: tokens.radius1,
    backgroundColor: "rgba(255,255,255,0.16)",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: "rgba(255,255,255,0.24)",
  },
  factionLogo: {
    display: "block",
    width: 14,
    height: 14,
    backgroundSize: "140px 28px",
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
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
    left: 16,
    bottom: 16,
    display: "grid",
    gap: tokens.space2,
    padding: tokens.space3,
    minWidth: 220,
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
