import { startTransition, useState } from "react";

import "./NewMatchPage.css";

const demoMatchSetup = {
  map: {
    Name: "Starter Strip",
    Author: "awbrn",
    "Player Count": 2,
    "Published Date": "2026-04-01",
    "Size X": 4,
    "Size Y": 1,
    "Terrain Map": [[42], [1], [1], [47]],
    "Predeployed Units": [],
  },
  players: [
    { faction: "OrangeStar", team: null, startingFunds: 1000 },
    { faction: "BlueMoon", team: null, startingFunds: 1000 },
  ],
  fogEnabled: false,
} as const;

export function NewMatchPage() {
  const [isCreating, setIsCreating] = useState(false);
  const [createdMatchId, setCreatedMatchId] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  async function handleCreateDemoMatch(): Promise<void> {
    setIsCreating(true);
    setErrorMessage(null);

    try {
      const response = await fetch("/api/matches", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(demoMatchSetup),
      });

      const body = (await response.json()) as
        | { matchId: string }
        | { error?: { message?: string } };

      if (!response.ok || !("matchId" in body)) {
        const message =
          "error" in body
            ? (body.error?.message ?? "Failed to create demo match")
            : "Failed to create demo match";
        throw new Error(message);
      }

      startTransition(() => {
        setCreatedMatchId(body.matchId);
        setErrorMessage(null);
      });
    } catch (error) {
      startTransition(() => {
        setCreatedMatchId(null);
        setErrorMessage(error instanceof Error ? error.message : "Failed to create demo match");
      });
    } finally {
      setIsCreating(false);
    }
  }

  return (
    <div className="new-match-page">
      <div className="new-match-card">
        <div className="cs-header">
          <span className="cs-icon" aria-hidden="true">
            ⚔
          </span>
          <h1 className="cs-title">New Match</h1>
        </div>
        <p className="cs-body">
          Create a turn-based match from an AWBW map payload. The current flow focuses on setup
          validation and match creation.
        </p>
        <div className="match-actions">
          <button
            className="match-create-button"
            disabled={isCreating}
            onClick={() => {
              void handleCreateDemoMatch();
            }}
            type="button"
          >
            {isCreating ? "Creating Match..." : "Create Demo Match"}
          </button>
          <p className="match-helper">
            Uses a tiny two-player demo setup to hit the new match creation path.
          </p>
        </div>
        {createdMatchId ? (
          <div className="match-feedback match-feedback-success">
            <p className="match-feedback-label">Match Created</p>
            <p className="match-feedback-value">{createdMatchId}</p>
            <p className="match-feedback-note">
              WebSocket path: `/api/matches/{createdMatchId}/ws`
            </p>
          </div>
        ) : null}
        {errorMessage ? (
          <div className="match-feedback match-feedback-error">
            <p className="match-feedback-label">Create Match Failed</p>
            <p className="match-feedback-note">{errorMessage}</p>
          </div>
        ) : null}
        <p className="cs-status">Bootstrap Ready</p>
      </div>
    </div>
  );
}
