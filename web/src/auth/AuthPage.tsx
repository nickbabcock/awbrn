import { Link, useNavigate, useRouter } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { authClient } from "./client";
import { authSignInSchema, authSignUpSchema } from "./schemas";
import {
  Button,
  Frame,
  Heading,
  Kicker,
  Page,
  Section,
  Stack,
  Text,
  TextField,
} from "#/ui/primitives.tsx";
import { tokens } from "#/ui/theme.stylex.ts";

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
    <Page width="wide">
      <Section>
        <div {...stylex.props(styles.layout)}>
          <Frame xstyle={styles.introFrame}>
            <Stack gap="lg">
              <Kicker xstyle={styles.introKicker}>Access</Kicker>
              <Heading size="display">{isRegister ? "Register" : "Sign In"}</Heading>
              <Text size="lg" tone="strong" xstyle={styles.lead}>
                Use the same field manual language as the rest of the app: clear intent, direct
                actions, no filler.
              </Text>
            </Stack>
          </Frame>
          <Frame xstyle={styles.formFrame}>
            <form onSubmit={handleSubmit}>
              <Stack gap="md">
                {isRegister ? (
                  <TextField
                    autoComplete="name"
                    id="name"
                    label="Name"
                    onChange={(e) => setName(e.target.value)}
                    required
                    type="text"
                    value={name}
                  />
                ) : null}
                <TextField
                  autoComplete="email"
                  id="email"
                  label="Email"
                  onChange={(e) => setEmail(e.target.value)}
                  required
                  type="email"
                  value={email}
                />
                <TextField
                  autoComplete={isRegister ? "new-password" : "current-password"}
                  id="password"
                  label="Password"
                  onChange={(e) => setPassword(e.target.value)}
                  required
                  type="password"
                  value={password}
                />
                {error ? (
                  <Text role="alert" size="sm" tone="danger">
                    {error}
                  </Text>
                ) : null}
                <Button fullWidth tone="success" type="submit" disabled={isPending}>
                  {isPending ? "Working..." : isRegister ? "Create Account" : "Sign In"}
                </Button>
                <Text size="sm" tone="muted">
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
                </Text>
              </Stack>
            </form>
          </Frame>
        </div>
      </Section>
    </Page>
  );
}

const styles = stylex.create({
  layout: {
    display: "grid",
    gap: tokens.space8,
    gridTemplateColumns: {
      default: "minmax(0, 1.1fr) minmax(320px, 420px)",
      "@media (max-width: 860px)": "1fr",
    },
    alignItems: "start",
  },
  lead: {
    maxWidth: 560,
  },
  introFrame: {
    backgroundColor: tokens.panelRaised,
    backgroundImage:
      "linear-gradient(180deg, rgba(255,255,255,0.26), transparent 38%), linear-gradient(135deg, rgba(47, 109, 168, 0.12), transparent 56%)",
  },
  introKicker: {
    color: tokens.infoHover,
  },
  formFrame: {
    backgroundImage:
      "linear-gradient(180deg, rgba(47, 142, 69, 0.16), transparent 42%), linear-gradient(135deg, rgba(29, 37, 50, 0.08), transparent 45%)",
  },
});

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
