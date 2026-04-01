import { Link } from "@tanstack/react-router";

export function NotFound() {
  return (
    <div className="about-page">
      <div className="about-card">
        <div className="about-header">
          <h1 className="about-title">Not Found</h1>
        </div>
        <div className="about-body">
          <p className="about-copy">The page you are looking for does not exist.</p>
          <div style={{ display: "flex", gap: "12px", flexWrap: "wrap" }}>
            <button
              type="button"
              onClick={() => {
                window.history.back();
              }}
            >
              Go Back
            </button>
            <Link to="/">Start Over</Link>
          </div>
        </div>
      </div>
    </div>
  );
}
