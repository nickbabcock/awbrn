import type { ReactNode } from "react";
import { Link } from "@tanstack/react-router";
import "./Layout.css";

export function Layout({ children }: { children: ReactNode }) {
  return (
    <div className="app-shell">
      <header className="nav">
        <Link to="/" className="nav-logo" aria-label="AWBRN home">
          <span className="logo-os">A</span>
          <span className="logo-bm">W</span>
          <span className="logo-ge">B</span>
          <span className="logo-yc">R</span>
          <span className="logo-bh">N</span>
        </Link>

        <nav className="nav-links" aria-label="Main navigation">
          <Link
            to="/"
            className="nav-link"
            activeProps={{ className: "nav-link active" }}
            activeOptions={{ exact: true }}
          >
            Play
          </Link>
          <Link to="/game/new" className="nav-link" activeProps={{ className: "nav-link active" }}>
            New Game
          </Link>
          <Link to="/about" className="nav-link" activeProps={{ className: "nav-link active" }}>
            About
          </Link>
        </nav>

        <div className="nav-auth">
          <Link to="/auth" search={{}} className="nav-signin">
            Sign In
          </Link>
          <Link to="/auth" search={{ mode: "register" }} className="nav-register">
            Register
          </Link>
        </div>
      </header>

      <main className="app-main">{children}</main>
    </div>
  );
}
