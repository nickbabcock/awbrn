import "./AboutPage.css";

const letters = [
  { letter: "A", faction: "os", word: "Advance", cls: "wm-os" },
  { letter: "W", faction: "bm", word: "Wars", cls: "wm-bm" },
  { letter: "B", faction: "ge", word: "By", cls: "wm-ge" },
  { letter: "R", faction: "yc", word: "Rust", cls: "wm-yc" },
  { letter: "N", faction: "bh", word: "(New)", cls: "wm-bh" },
] as const;

export function AboutPage() {
  return (
    <div className="about-page">
      <div className="about-card">
        <div className="about-header">
          <h1 className="about-title">What's in a Name</h1>
        </div>

        <div className="about-body">
          <div className="acronym-grid">
            {letters.map(({ letter, word, cls }) => (
              <div key={letter} className="acronym-row">
                <span className={`acronym-letter ${cls}`}>{letter}</span>
                <span className="acronym-dash">—</span>
                <span className="acronym-word">{word}</span>
              </div>
            ))}
          </div>

          <p className="about-copy">
            Pronounced, auburn, AWBRN is a replay viewer and game toolkit for{" "}
            <strong>Advance Wars By Web</strong> — the browser-based re-implementation of Nintendo's
            Advance Wars series. Load a <code>.zip</code> replay, step through every turn, and
            review your battles in your browser with pixel-perfect CO portraits and terrain
            rendering, powered by a Rust and WebAssembly core.
          </p>
        </div>
      </div>
    </div>
  );
}
