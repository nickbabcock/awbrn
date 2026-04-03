import { Link, useNavigate } from "@tanstack/react-router";
import { startTransition, useMemo, useState } from "react";
import { useAppSession } from "../../auth/useAppSession";
import { awbwMapAssetPath } from "../../awbw/paths";
import type { AwbwMapData } from "../../awbw/schemas";
import { MatchMapPreview } from "../components/MatchMapPreview";
import { createMatchFn } from "../matches.functions";
import "./NewMatchPage.css";

export function NewMatchPage() {
  const navigate = useNavigate();
  const session = useAppSession();
  const [matchName, setMatchName] = useState("");
  const [mapIdInput, setMapIdInput] = useState("162795");
  const [mapData, setMapData] = useState<AwbwMapData | null>(null);
  const [fogEnabled, setFogEnabled] = useState(false);
  const [startingFunds, setStartingFunds] = useState("1000");
  const [isPrivate, setIsPrivate] = useState(false);
  const [isLoadingMap, setIsLoadingMap] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [mapError, setMapError] = useState<string | null>(null);
  const [createError, setCreateError] = useState<string | null>(null);

  const parsedMapId = useMemo(() => {
    const value = Number(mapIdInput);
    return Number.isSafeInteger(value) && value > 0 ? value : null;
  }, [mapIdInput]);

  const parsedStartingFunds = useMemo(() => {
    const value = Number(startingFunds);
    return Number.isSafeInteger(value) && value >= 0 ? value : null;
  }, [startingFunds]);

  async function handleLoadMap(): Promise<void> {
    if (parsedMapId === null) {
      setMapData(null);
      setMapError("Enter a valid AWBW map id.");
      return;
    }

    setIsLoadingMap(true);
    setMapError(null);

    try {
      const response = await fetch(awbwMapAssetPath(parsedMapId));
      if (!response.ok) {
        throw new Error(response.status === 404 ? "Map not found." : "Map preview failed to load.");
      }

      const nextMap = (await response.json()) as AwbwMapData;
      startTransition(() => {
        setMapData(nextMap);
        setMapError(null);
        if (!matchName.trim()) {
          setMatchName(nextMap.Name);
        }
      });
    } catch (error) {
      startTransition(() => {
        setMapData(null);
        setMapError(error instanceof Error ? error.message : "Map preview failed to load.");
      });
    } finally {
      setIsLoadingMap(false);
    }
  }

  async function handleCreateLobby(): Promise<void> {
    if (!session) {
      setCreateError("Sign in to create a match.");
      return;
    }
    if (parsedMapId === null || mapData === null) {
      setCreateError("Load a map before creating the lobby.");
      return;
    }
    if (parsedStartingFunds === null) {
      setCreateError("Starting funds must be a non-negative whole number.");
      return;
    }
    if (!matchName.trim()) {
      setCreateError("Match name is required.");
      return;
    }

    setIsCreating(true);
    setCreateError(null);

    try {
      const match = await createMatchFn({
        data: {
          name: matchName.trim(),
          mapId: parsedMapId,
          isPrivate,
          settings: { fogEnabled, startingFunds: parsedStartingFunds },
        },
      });

      await navigate({ to: "/matches/$matchId", params: { matchId: match.matchId } });
    } catch (error) {
      setCreateError(error instanceof Error ? error.message : "Failed to create the lobby.");
    } finally {
      setIsCreating(false);
    }
  }

  return (
    <div className="new-match-page">
      <div className="new-match-layout">
        <section className="new-match-card">
          <div className="cs-header">
            <span className="cs-icon" aria-hidden="true">
              ⚔
            </span>
            <h1 className="cs-title">Create Match</h1>
          </div>
          <p className="cs-body">
            Load an AWBW map, inspect the battlefield, and dial in the initial rules before the
            lobby goes live.
          </p>

          <div className="new-match-form">
            <label className="match-field">
              <span className="match-field-label">Match Name</span>
              <input
                className="match-input"
                type="text"
                value={matchName}
                onChange={(event) => setMatchName(event.target.value)}
                placeholder="Riverside Duel"
              />
            </label>

            <div className="match-row">
              <label className="match-field">
                <span className="match-field-label">AWBW Map ID</span>
                <input
                  className="match-input"
                  inputMode="numeric"
                  type="text"
                  value={mapIdInput}
                  onChange={(event) => {
                    setMapIdInput(event.target.value);
                    setMapData(null);
                    setMapError(null);
                  }}
                />
              </label>
              <button
                className="match-create-button"
                disabled={isLoadingMap}
                onClick={() => {
                  void handleLoadMap();
                }}
                type="button"
              >
                {isLoadingMap ? "Loading..." : "Load Map"}
              </button>
            </div>

            <div className="match-row">
              <label className="match-field">
                <span className="match-field-label">Starting Funds</span>
                <input
                  className="match-input"
                  inputMode="numeric"
                  type="text"
                  value={startingFunds}
                  onChange={(event) => setStartingFunds(event.target.value)}
                />
              </label>

              <label className="match-toggle">
                <input
                  checked={fogEnabled}
                  onChange={(event) => setFogEnabled(event.target.checked)}
                  type="checkbox"
                />
                <span>Fog Enabled</span>
              </label>
            </div>

            <label className="match-toggle">
              <input
                checked={isPrivate}
                onChange={(event) => setIsPrivate(event.target.checked)}
                type="checkbox"
              />
              <span>Private match</span>
            </label>

            {!session ? (
              <p className="match-helper">
                <Link to="/auth" search={{}}>
                  Sign in
                </Link>{" "}
                to create a lobby.
              </p>
            ) : (
              <p className="match-helper">Lobby creator: {session.user.name}</p>
            )}

            {mapError ? <p className="match-error">{mapError}</p> : null}
            {createError ? <p className="match-error">{createError}</p> : null}

            <button
              className="match-submit"
              disabled={isCreating || mapData === null || !session}
              onClick={() => {
                void handleCreateLobby();
              }}
              type="button"
            >
              {isCreating ? "Creating Lobby..." : "Create Lobby"}
            </button>
          </div>
        </section>

        <section className="new-match-preview-card">
          <div className="preview-header">
            <h2 className="preview-title">Map Preview</h2>
            {mapData ? (
              <p className="preview-subtitle">
                {mapData.Name} · {mapData.Author}
              </p>
            ) : (
              <p className="preview-subtitle">Load a map to inspect its terrain.</p>
            )}
          </div>

          {mapData && parsedMapId !== null ? (
            <>
              <MatchMapPreview className="preview-shell" mapId={parsedMapId} />
              <div className="preview-metadata">
                <div className="preview-stat">
                  <span className="preview-stat-label">Players</span>
                  <span className="preview-stat-value">{mapData["Player Count"]}</span>
                </div>
                <div className="preview-stat">
                  <span className="preview-stat-label">Size</span>
                  <span className="preview-stat-value">
                    {mapData["Size X"]} × {mapData["Size Y"]}
                  </span>
                </div>
                <div className="preview-stat">
                  <span className="preview-stat-label">Published</span>
                  <span className="preview-stat-value">{mapData["Published Date"]}</span>
                </div>
              </div>
            </>
          ) : (
            <div className="preview-empty">No map loaded.</div>
          )}
        </section>
      </div>
    </div>
  );
}
