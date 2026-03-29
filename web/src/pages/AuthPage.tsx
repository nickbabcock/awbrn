import { useSearchParams, Link } from "react-router-dom";
import "./AuthPage.css";

export function AuthPage() {
  const [params] = useSearchParams();
  const isRegister = params.get("mode") === "register";

  return (
    <div className="auth-page">
      <div className="auth-card">
        <div className="auth-header">
          <h1 className="auth-title">{isRegister ? "Register" : "Sign In"}</h1>
        </div>

        <form className="auth-form" onSubmit={(e) => e.preventDefault()}>
          {isRegister && (
            <div className="auth-field">
              <label className="auth-label" htmlFor="username">
                Username
              </label>
              <input className="auth-input" id="username" type="text" autoComplete="username" />
            </div>
          )}

          <div className="auth-field">
            <label className="auth-label" htmlFor="email">
              Email
            </label>
            <input className="auth-input" id="email" type="email" autoComplete="email" />
          </div>

          <div className="auth-field">
            <label className="auth-label" htmlFor="password">
              Password
            </label>
            <input
              className="auth-input"
              id="password"
              type="password"
              autoComplete={isRegister ? "new-password" : "current-password"}
            />
          </div>

          <button className="auth-submit" type="submit">
            {isRegister ? "Create Account" : "Sign In"}
          </button>
        </form>

        <p className="auth-switch">
          {isRegister ? (
            <>
              Already have an account? <Link to="/auth">Sign in →</Link>
            </>
          ) : (
            <>
              New here? <Link to="/auth?mode=register">Create an account →</Link>
            </>
          )}
        </p>
      </div>
    </div>
  );
}
