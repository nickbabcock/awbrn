import "./NewGamePage.css";

export function NewGamePage() {
  return (
    <div className="new-game-page">
      <div className="coming-soon-card">
        <div className="cs-header">
          <span className="cs-icon" aria-hidden="true">
            ⚔
          </span>
          <h1 className="cs-title">New Game</h1>
        </div>
        <p className="cs-body">
          Challenge opponents on any AWBW map — CO selection, team setup, and direct play from your
          browser. No client required.
        </p>
        <p className="cs-status">Under Construction</p>
      </div>
    </div>
  );
}
