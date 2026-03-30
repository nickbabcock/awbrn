import { useEffect, useRef, useState, type CSSProperties } from "react";
import { resolveAwbwUsername } from "../awbw_usernames";
import { CoPortrait } from "../CoPortrait";
import { loadCoPortraitCatalog, type CoPortraitCatalog } from "../co_portraits";
import { getFactionVisual } from "../faction_visuals";
import { gameRunner } from "../game_runner";
import { useGameActions, useGameStore } from "../store";
import "../App.css";

export function ReplayPage() {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const currentDay = useGameStore((state) => state.currentDay);
  const replayRoster = useGameStore((state) => state.replayRoster);
  const gameActions = useGameActions();
  const [portraitCatalog] = useState<CoPortraitCatalog>(() => loadCoPortraitCatalog());
  const [playerNames, setPlayerNames] = useState<Record<number, string>>({});

  const handleReplayFileChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (file) {
      gameActions.setReplayRoster(null);
      try {
        await gameRunner.loadReplay(file);
        canvasRef.current?.focus({ preventScroll: true });
      } catch (error) {
        gameActions.setReplayRoster(null);
        console.error("Error loading replay:", error);
      }
    }
  };

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    gameRunner.attachCanvas({ canvas, container }).catch((error) => {
      console.error("Error attaching game runner:", error);
    });

    return () => {
      gameRunner.detachCanvas(canvas);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    if (!replayRoster) {
      setPlayerNames({});
      return () => {
        cancelled = true;
      };
    }

    const activeUserIds = new Set(replayRoster.players.map((player) => player.userId));
    setPlayerNames((previous) =>
      Object.fromEntries(
        Object.entries(previous).filter(([userId]) => activeUserIds.has(Number(userId))),
      ),
    );

    void Promise.all(
      replayRoster.players.map(async (player) => {
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
  }, [replayRoster]);

  return (
    <>
      <div className="game-surface" ref={containerRef}>
        <canvas
          className="game-canvas"
          ref={canvasRef}
          width={600}
          height={400}
          tabIndex={0}
          style={{ display: "block" }}
        />
        {!replayRoster && (
          <div className="game-empty-state">
            <h1 className="ges-wordmark" aria-label="AWBRN">
              <span className="ges-os">A</span>
              <span className="ges-bm">W</span>
              <span className="ges-ge">B</span>
              <span className="ges-yc">R</span>
              <span className="ges-bh">N</span>
            </h1>
            <p className="ges-subtitle">Advance Wars Online</p>
            <p className="ges-prompt">Load a replay to begin</p>
          </div>
        )}
      </div>
      <div className="hud-panel day-panel">Day: {currentDay}</div>
      <aside className="hud-panel roster-panel">
        <div className="roster-title">Replay Roster</div>
        {replayRoster ? (
          <>
            <div className="roster-subtitle">
              Game {replayRoster.gameId} · Map {replayRoster.mapId}
            </div>
            <div className="roster-list">
              {replayRoster.players.map((player) => {
                const factionVisual = getFactionVisual(player.factionCode);
                const playerName = playerNames[player.userId] ?? `Player ${player.order}`;
                const playerMeta = [
                  player.team ? `Team ${player.team}` : null,
                  player.eliminated ? "Eliminated" : null,
                ].filter((value): value is string => value !== null);
                const rosterStyle = {
                  "--player-faction": factionVisual.accent,
                  "--player-faction-wash": factionVisual.wash,
                  "--player-name-color": factionVisual.text,
                } as CSSProperties;

                return (
                  <div className="roster-player" key={player.playerId} style={rosterStyle}>
                    <div className="roster-player-header">
                      <div className="roster-player-headline">{playerName}</div>
                      <span
                        className="roster-player-faction-badge"
                        title={player.factionName}
                        aria-label={`Faction: ${player.factionName}`}
                      >
                        <span
                          aria-hidden="true"
                          className="roster-player-logo"
                          style={{
                            backgroundImage: `url(${factionVisual.logoUrl})`,
                            backgroundPosition: factionVisual.logoPosition,
                          }}
                        />
                      </span>
                    </div>
                    <div className="roster-player-portraits">
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
                    <div className="roster-player-copy">
                      {playerMeta.length > 0 ? (
                        <div className="roster-player-meta">{playerMeta.join(" · ")}</div>
                      ) : null}
                    </div>
                  </div>
                );
              })}
            </div>
          </>
        ) : (
          <div className="roster-empty">Load a replay to inspect player CO portraits.</div>
        )}
      </aside>
      <div className="hud-panel file-panel">
        <label className="file-label" htmlFor="replay-file-input">
          Load Replay:
        </label>
        <input id="replay-file-input" type="file" accept=".zip" onChange={handleReplayFileChange} />
      </div>
    </>
  );
}
