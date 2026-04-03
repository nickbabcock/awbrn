import { Link, useNavigate, useRouter } from "@tanstack/react-router";
import { useState } from "react";
import { authClient } from "./client";
import { authSignInSchema, authSignUpSchema } from "./schemas";
import "./AuthPage.css";

export function AuthPage({ isRegister }: { isRegister: boolean }) {
  const navigate = useNavigate();
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isPending, setIsPending] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);

    setIsPending(true);

    try {
      const result = await submitAuthRequest(isRegister, { email, password, name });

      if (result.error) {
        throw new Error(result.error.message ?? "Authentication failed");
      }

      await router.invalidate();
      await navigate({ to: "/" });
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : "Authentication failed");
    } finally {
      setIsPending(false);
    }
  }

  return (
    <div className="auth-page">
      <div className="auth-card">
        <div className="auth-header">
          <h1 className="auth-title">{isRegister ? "Register" : "Sign In"}</h1>
        </div>

        <form className="auth-form" onSubmit={handleSubmit}>
          {isRegister && (
            <div className="auth-field">
              <label className="auth-label" htmlFor="name">
                Name
              </label>
              <input
                className="auth-input"
                id="name"
                type="text"
                autoComplete="name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                required
              />
            </div>
          )}

          <div className="auth-field">
            <label className="auth-label" htmlFor="email">
              Email
            </label>
            <input
              className="auth-input"
              id="email"
              type="email"
              autoComplete="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              required
            />
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
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
            />
          </div>

          {error && (
            <p className="auth-error" role="alert">
              {error}
            </p>
          )}

          <button className="auth-submit" type="submit" disabled={isPending}>
            {isPending ? "..." : isRegister ? "Create Account" : "Sign In"}
          </button>
        </form>

        <p className="auth-switch">
          {isRegister ? (
            <>
              Already have an account? <Link to="/auth">Sign in →</Link>
            </>
          ) : (
            <>
              New here?{" "}
              <Link to="/auth" search={{ mode: "register" }}>
                Create an account →
              </Link>
            </>
          )}
        </p>
      </div>
    </div>
  );
}

async function submitAuthRequest(
  isRegister: boolean,
  payload: { email: string; password: string; name: string },
) {
  if (isRegister) {
    const parsed = authSignUpSchema.safeParse(payload);

    if (!parsed.success) {
      throw new Error(parsed.error.issues[0]?.message ?? "Authentication failed");
    }

    return authClient.signUp.email(parsed.data);
  }

  const parsed = authSignInSchema.safeParse({
    email: payload.email,
    password: payload.password,
  });

  if (!parsed.success) {
    throw new Error(parsed.error.issues[0]?.message ?? "Authentication failed");
  }

  return authClient.signIn.email(parsed.data);
}
