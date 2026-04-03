import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { CoPortrait } from "../CoPortrait";
import { listCoPortraits, loadCoPortraitCatalog } from "../co_portraits";
import { MapPreviewSurface } from "../components/MapPreviewSurface";
import { defaultFactionIdForSlot, factions, getFactionById } from "../factions";
import { getFactionVisual } from "../faction_visuals";
import { useAppSession } from "../lib/useAppSession";
import type {
  MatchMutationRequest,
  MatchMutationResponse,
  MatchSnapshot,
} from "../server/match_protocol";
import { awbwMapAssetPath, type AwbwMapData } from "../utils/awbw";
import "./MatchLobbyPage.css";

const coOptions = listCoPortraits();

export function MatchLobbyPage({ matchId }: { matchId: string }) {
  const session = useAppSession();
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const [match, setMatch] = useState<MatchSnapshot | null>(null);
  const [mapData, setMapData] = useState<AwbwMapData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [pendingAction, setPendingAction] = useState<string | null>(null);

  const joinSlug =
    typeof window === "undefined" ? null : new URLSearchParams(window.location.search).get("join");

  useEffect(() => {
    void loadMatchSnapshot();
  }, [matchId]);

  useEffect(() => {
    if (!match) {
      return;
    }

    let cancelled = false;
    void (async () => {
      try {
        const response = await fetch(awbwMapAssetPath(match.mapId));
        if (!response.ok) {
          throw new Error("Map metadata could not be loaded.");
        }
        const payload = (await response.json()) as AwbwMapData;
        if (!cancelled) {
          setMapData(payload);
        }
      } catch (nextError) {
        if (!cancelled) {
          setError(
            nextError instanceof Error ? nextError.message : "Map metadata could not be loaded.",
          );
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [match?.mapId]);

  const currentUserId = session?.user.id ?? null;
  const participantsBySlot = useMemo(
    () =>
      new Map(match?.participants.map((participant) => [participant.slotIndex, participant]) ?? []),
    [match],
  );
  const myParticipant =
    currentUserId === null
      ? null
      : (match?.participants.find((participant) => participant.userId === currentUserId) ?? null);

  async function loadMatchSnapshot(): Promise<void> {
    setIsLoading(true);
    setError(null);

    try {
      const search = joinSlug ? `?join=${encodeURIComponent(joinSlug)}` : "";
      const response = await fetch(`/api/matches/${matchId}${search}`);
      const body = (await response.json()) as MatchSnapshot | { error?: { message?: string } };
      if (!response.ok || !("matchId" in body)) {
        throw new Error(
          "error" in body
            ? (body.error?.message ?? "Failed to load the lobby.")
            : "Failed to load the lobby.",
        );
      }
      setMatch(body);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : "Failed to load the lobby.");
    } finally {
      setIsLoading(false);
    }
  }

  async function submitAction(action: MatchMutationRequest, pendingLabel: string): Promise<void> {
    setPendingAction(pendingLabel);
    setError(null);

    try {
      const response = await fetch(`/api/matches/${matchId}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(action),
      });
      const body = (await response.json()) as
        | MatchMutationResponse
        | { error?: { message?: string } };
      if (!response.ok || !("match" in body)) {
        throw new Error(
          "error" in body
            ? (body.error?.message ?? "Lobby update failed.")
            : "Lobby update failed.",
        );
      }
      setMatch(body.match);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : "Lobby update failed.");
    } finally {
      setPendingAction(null);
    }
  }

  const shareUrl =
    match?.isPrivate && match.joinSlug && typeof window !== "undefined"
      ? `${window.location.origin}/matches/${match.matchId}?join=${match.joinSlug}`
      : null;

  return (
    <div className="match-lobby-page">
      {isLoading ? (
        <div className="match-lobby-empty">Loading lobby...</div>
      ) : !match ? (
        <div className="match-lobby-empty">Match not found.</div>
      ) : (
        <div className="match-lobby-layout">
          <section className="match-lobby-preview-card">
            <div className="match-lobby-header">
              <div>
                <p className="match-lobby-kicker">
                  {match.phase === "active" ? "Match Active" : "Lobby"}
                </p>
                <h1 className="match-lobby-title">{match.name}</h1>
              </div>
              <div className="match-lobby-badges">
                <span className="match-phase-pill">{match.phase}</span>
                {match.isPrivate ? (
                  <span className="match-phase-pill match-phase-pill-private">Private</span>
                ) : null}
              </div>
            </div>

            <div className="match-lobby-copy">
              <p>
                Map {match.mapId} · {match.maxPlayers} players · creator {match.creatorName}
              </p>
              <p>
                Settings: {match.settings.fogEnabled ? "Fog On" : "Fog Off"} ·{" "}
                {match.settings.startingFunds.toLocaleString()} funds
              </p>
            </div>

            <MapPreviewSurface className="match-lobby-preview-shell" mapId={match.mapId} />

            {mapData ? (
              <div className="match-lobby-meta">
                <div className="match-lobby-meta-item">
                  <span className="match-lobby-meta-label">Map</span>
                  <span className="match-lobby-meta-value">{mapData.Name}</span>
                </div>
                <div className="match-lobby-meta-item">
                  <span className="match-lobby-meta-label">Author</span>
                  <span className="match-lobby-meta-value">{mapData.Author}</span>
                </div>
                <div className="match-lobby-meta-item">
                  <span className="match-lobby-meta-label">Size</span>
                  <span className="match-lobby-meta-value">
                    {mapData["Size X"]} × {mapData["Size Y"]}
                  </span>
                </div>
              </div>
            ) : null}

            {shareUrl ? (
              <div className="match-lobby-share">
                <p className="match-lobby-share-label">Private Join Link</p>
                <p className="match-lobby-share-value">{shareUrl}</p>
              </div>
            ) : null}
          </section>

          <section className="match-lobby-participants-card">
            <div className="match-lobby-header">
              <div>
                <p className="match-lobby-kicker">Participants</p>
                <h2 className="match-lobby-title">Seats</h2>
              </div>
            </div>

            {error ? <p className="match-lobby-error">{error}</p> : null}
            {!session ? (
              <p className="match-lobby-note">Sign in to claim a seat in the lobby.</p>
            ) : null}
            {match.phase === "starting" ? (
              <p className="match-lobby-note">All players are ready. Starting the match...</p>
            ) : null}
            {match.phase === "active" ? (
              <p className="match-lobby-note">The match is active. Lobby controls are locked.</p>
            ) : null}

            <div className="participant-list">
              {Array.from({ length: match.maxPlayers }, (_, slotIndex) => {
                const participant = participantsBySlot.get(slotIndex) ?? null;
                const isMine = participant?.userId === currentUserId;
                const fallbackFactionId =
                  participant?.factionId ?? defaultFactionIdForSlot(slotIndex);
                const faction = getFactionById(fallbackFactionId);
                const factionVisual = getFactionVisual(faction?.code ?? "os");
                const cardStyle = {
                  "--player-faction": factionVisual.accent,
                  "--player-faction-wash": factionVisual.wash,
                  "--player-name-color": factionVisual.text,
                } as CSSProperties;

                return (
                  <div className="participant-card" key={slotIndex} style={cardStyle}>
                    <div className="participant-card-header">
                      <div>
                        <p className="participant-slot-label">Slot {slotIndex + 1}</p>
                        <h3 className="participant-name">
                          {participant ? participant.userName : "Open Seat"}
                        </h3>
                      </div>
                      {participant ? (
                        <span
                          className={
                            participant.ready ? "participant-ready ready" : "participant-ready"
                          }
                        >
                          {participant.ready ? "Ready" : "Waiting"}
                        </span>
                      ) : null}
                    </div>

                    <div className="participant-portrait-row">
                      <CoPortrait
                        catalog={portraitCatalog}
                        coKey={
                          coOptions.find((portrait) => portrait.awbwId === participant?.coId)
                            ?.key ?? null
                        }
                        fallbackLabel="?"
                      />
                      <div className="participant-details">
                        <p className="participant-detail-line">
                          {faction?.displayName ?? "Unknown Faction"}
                        </p>
                        <p className="participant-detail-line">
                          {participant?.coId
                            ? (coOptions.find((portrait) => portrait.awbwId === participant.coId)
                                ?.displayName ?? `CO ${participant.coId}`)
                            : "No CO selected"}
                        </p>
                      </div>
                    </div>

                    {participant === null ? (
                      <button
                        className="participant-action"
                        disabled={
                          pendingAction !== null ||
                          !session ||
                          myParticipant !== null ||
                          match.phase !== "lobby"
                        }
                        onClick={() => {
                          void submitAction(
                            {
                              action: "join",
                              slotIndex,
                              factionId: defaultFactionIdForSlot(slotIndex),
                              joinSlug,
                            },
                            `join-${slotIndex}`,
                          );
                        }}
                        type="button"
                      >
                        Claim Slot
                      </button>
                    ) : isMine ? (
                      <div className="participant-controls">
                        <label className="participant-field">
                          <span>Faction</span>
                          <select
                            className="participant-select"
                            disabled={pendingAction !== null || match.phase !== "lobby"}
                            value={participant.factionId}
                            onChange={(event) => {
                              void submitAction(
                                {
                                  action: "updateParticipant",
                                  factionId: Number(event.target.value),
                                  joinSlug,
                                },
                                "faction",
                              );
                            }}
                          >
                            {factions.map((option) => (
                              <option key={option.id} value={option.id}>
                                {option.displayName}
                              </option>
                            ))}
                          </select>
                        </label>

                        <label className="participant-field">
                          <span>CO</span>
                          <select
                            className="participant-select"
                            disabled={pendingAction !== null || match.phase !== "lobby"}
                            value={participant.coId ?? ""}
                            onChange={(event) => {
                              void submitAction(
                                {
                                  action: "updateParticipant",
                                  coId:
                                    event.target.value === "" ? null : Number(event.target.value),
                                  joinSlug,
                                },
                                "co",
                              );
                            }}
                          >
                            <option value="">Select CO</option>
                            {coOptions.map((option) => (
                              <option key={option.awbwId} value={option.awbwId}>
                                {option.displayName}
                              </option>
                            ))}
                          </select>
                        </label>

                        <div className="participant-actions">
                          <button
                            className="participant-action"
                            disabled={pendingAction !== null || match.phase !== "lobby"}
                            onClick={() => {
                              void submitAction(
                                {
                                  action: "updateParticipant",
                                  ready: !participant.ready,
                                  joinSlug,
                                },
                                "ready",
                              );
                            }}
                            type="button"
                          >
                            {participant.ready ? "Unready" : "Ready Up"}
                          </button>
                          <button
                            className="participant-action participant-action-secondary"
                            disabled={pendingAction !== null || match.phase !== "lobby"}
                            onClick={() => {
                              void submitAction({ action: "leave" }, "leave");
                            }}
                            type="button"
                          >
                            Leave
                          </button>
                        </div>
                      </div>
                    ) : (
                      <p className="participant-note">Seat claimed.</p>
                    )}
                  </div>
                );
              })}
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
