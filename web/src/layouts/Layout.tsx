import type { ReactNode } from "react";
import { Link } from "@tanstack/react-router";
import { useNavigate, useRouter } from "@tanstack/react-router";
import { useState } from "react";
import { authClient } from "../auth/client";
import { useAppSession } from "../auth/useAppSession";
import "./Layout.css";

export function Layout({ children }: { children: ReactNode }) {
  const session = useAppSession();
  const navigate = useNavigate();
  const router = useRouter();
  const [isSigningOut, setIsSigningOut] = useState(false);
  const [signOutError, setSignOutError] = useState<string | null>(null);

  async function handleSignOut() {
    if (isSigningOut) {
      return;
    }

    setIsSigningOut(true);
    setSignOutError(null);

    try {
      const result = await authClient.signOut();

      if (result.error) {
        setSignOutError(result.error.message ?? "Sign out failed");
        return;
      }

      await router.invalidate();
      await navigate({ to: "/" });
    } catch (error) {
      setSignOutError(error instanceof Error ? error.message : "Sign out failed");
    } finally {
      setIsSigningOut(false);
    }
  }

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
              {signOutError ? (
                <p className="nav-auth-error" role="alert">
                  {signOutError}
                </p>
              ) : null}
              <button
                className="nav-signout"
                disabled={isSigningOut}
                onClick={() => {
                  void handleSignOut();
                }}
              >
                {isSigningOut ? "Signing Out..." : "Sign Out"}
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
