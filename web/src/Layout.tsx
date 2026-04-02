import type { ReactNode } from "react";
import { Link } from "@tanstack/react-router";
import { signOut, useSession } from "./lib/auth-client";
import type { Session } from "./server/auth";
import "./Layout.css";

export function Layout({
  children,
  serverSession,
}: {
  children: ReactNode;
  serverSession: Session | null;
}) {
  const { data: clientSession } = useSession();
  const session = clientSession ?? serverSession;

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
          <Link
            to="/matches/new"
            className="nav-link"
            activeProps={{ className: "nav-link active" }}
          >
            New Match
          </Link>
          <Link to="/about" className="nav-link" activeProps={{ className: "nav-link active" }}>
            About
          </Link>
        </nav>

        <div className="nav-auth">
          {session ? (
            <>
              <span className="nav-user">{session.user.name}</span>
              <button className="nav-signout" onClick={() => signOut()}>
                Sign Out
              </button>
            </>
          ) : (
            <>
              <Link to="/auth" search={{}} className="nav-signin">
                Sign In
              </Link>
              <Link to="/auth" search={{ mode: "register" }} className="nav-register">
                Register
              </Link>
            </>
          )}
        </div>
      </header>

      <main className="app-main">{children}</main>
    </div>
  );
}
