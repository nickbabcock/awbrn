import { useEffect, useRef, useState } from "react";
import { CoPortrait } from "../CoPortrait";
import { loadCoPortraitCatalog, type CoPortraitCatalog } from "../co_portraits";
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
              {replayRoster.players.map((player) => (
                <div className="roster-player" key={player.playerId}>
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
                    <div className="roster-player-headline">
                      P{player.order} · {player.coName ?? "Unknown CO"}
                    </div>
                    <div className="roster-player-meta">
                      {player.factionName}
                      {player.team ? ` · Team ${player.team}` : ""}
                      {player.eliminated ? " · Eliminated" : ""}
                    </div>
                    {player.tagCoName ? (
                      <div className="roster-player-meta">Tag: {player.tagCoName}</div>
                    ) : null}
                  </div>
                </div>
              ))}
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
