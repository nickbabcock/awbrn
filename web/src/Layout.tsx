import { NavLink, Outlet } from "react-router-dom";
import "./Layout.css";

export function Layout() {
  return (
    <div className="app-shell">
      <header className="nav">
        <NavLink to="/" className="nav-logo" aria-label="AWBRN home">
          <span className="logo-os">A</span>
          <span className="logo-bm">W</span>
          <span className="logo-ge">B</span>
          <span className="logo-yc">R</span>
          <span className="logo-bh">N</span>
        </NavLink>

        <nav className="nav-links" aria-label="Main navigation">
          <NavLink to="/" end className={({ isActive }) => `nav-link${isActive ? " active" : ""}`}>
            Play
          </NavLink>
          <NavLink
            to="/game/new"
            className={({ isActive }) => `nav-link${isActive ? " active" : ""}`}
          >
            New Game
          </NavLink>
          <NavLink to="/about" className={({ isActive }) => `nav-link${isActive ? " active" : ""}`}>
            About
          </NavLink>
        </nav>

        <div className="nav-auth">
          <NavLink to="/auth" className="nav-signin">
            Sign In
          </NavLink>
          <NavLink to="/auth?mode=register" className="nav-register">
            Register
          </NavLink>
        </div>
      </header>

      <main className="app-main">
        <Outlet />
      </main>
    </div>
  );
}
