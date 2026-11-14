import { Banner } from "@astryxdesign/core/Banner";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Stack } from "@astryxdesign/core/Stack";
import { TextInput } from "@astryxdesign/core/TextInput";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import * as stylex from "@stylexjs/stylex";
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useRouter } from "@tanstack/react-router";
import { useState } from "react";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapCatalogQueryOptions } from "#/maps/maps.queries.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Sheet, SheetBody } from "#/ui/Sheet.tsx";
import { authKeys } from "./auth.keys";
import { authClient } from "./client";
import { authSignInSchema, authSignUpSchema } from "./schemas";

type Mode = "sign-in" | "register";

/** A failed attempt: the problem, and whether switching to sign in fixes it. */
interface AuthProblem {
  title: string;
  isSignInFix: boolean;
}

export function AuthPage({ isRegister }: { isRegister: boolean }) {
  const navigate = useNavigate();
  const router = useRouter();
  const queryClient = useQueryClient();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [problem, setProblem] = useState<AuthProblem | null>(null);
  const [isPending, setIsPending] = useState(false);
  const mode: Mode = isRegister ? "register" : "sign-in";

  function changeMode(next: Mode): void {
    setProblem(null);
    void navigate({ search: { mode: next === "register" ? "register" : undefined }, to: "/auth" });
  }

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    setProblem(null);
    setIsPending(true);

    try {
      const result = await submitAuthRequest(isRegister, { email, password, name });
      if (result.error) {
        setProblem(describeFailure(result.error, isRegister));
        return;
      }

      queryClient.removeQueries({ queryKey: matchKeys.mine() });
      queryClient.removeQueries({ queryKey: matchKeys.completed() });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: authKeys.all }),
        queryClient.invalidateQueries({ queryKey: matchKeys.details() }),
      ]);
      // Drop loads cached while signed out, but do not reload this page: its
      // guard would redirect home too, holding the form up for a second load.
      router.clearCache();
      await navigate({ to: "/" });
    } catch (nextError) {
      setProblem({
        isSignInFix: false,
        title: nextError instanceof Error ? nextError.message : "Something went wrong",
      });
    } finally {
      setIsPending(false);
    }
  }

  const action = isRegister ? "Create account" : "Sign in";

  return (
    <Stack xstyle={styles.auth}>
      <BoardArt />
      <Stack aria-labelledby="auth-title" as="section" xstyle={styles.panel}>
        <VisuallyHidden as="h1" id="auth-title">
          {action}
        </VisuallyHidden>
        <Sheet xstyle={styles.form}>
          <SheetBody xstyle={styles.formBody}>
            {/* <form> has no Astryx component (FormLayout says to wrap it in a native one).
                It posts, so a submit before hydration keeps the password out of the URL. */}
            <form method="post" noValidate onSubmit={handleSubmit} {...stylex.props(styles.fields)}>
              <SegmentedControl
                label="Account"
                layout="fill"
                onChange={(next) => changeMode(next as Mode)}
                value={mode}
                xstyle={styles.fill}
              >
                <SegmentedControlItem label="Sign in" value="sign-in" />
                <SegmentedControlItem label="Create account" value="register" />
              </SegmentedControl>
              {problem ? (
                <Banner
                  endContent={
                    problem.isSignInFix ? (
                      <Button
                        label="Sign in instead"
                        onClick={() => changeMode("sign-in")}
                        size="sm"
                        variant="secondary"
                      />
                    ) : undefined
                  }
                  role="alert"
                  status="error"
                  title={problem.title}
                />
              ) : null}
              {isRegister ? (
                <TextInput
                  autoComplete="name"
                  enterKeyHint="next"
                  htmlName="name"
                  id="name"
                  label="Name"
                  onChange={setName}
                  type="text"
                  value={name}
                  containerXstyle={styles.field}
                />
              ) : null}
              <TextInput
                autoComplete="username"
                enterKeyHint="next"
                hasSpellCheck={false}
                htmlName="email"
                id="email"
                label="Email"
                onChange={setEmail}
                type="email"
                value={email}
                containerXstyle={styles.field}
              />
              <TextInput
                autoComplete={isRegister ? "new-password" : "current-password"}
                enterKeyHint="done"
                htmlName="password"
                id="password"
                label="Password"
                onChange={setPassword}
                type="password"
                value={password}
                containerXstyle={styles.field}
              />
              <Button
                isLoading={isPending}
                label={action}
                size="lg"
                type="submit"
                variant="primary"
                width="100%"
              />
            </form>
          </SheetBody>
        </Sheet>
      </Stack>
    </Stack>
  );
}

/** The first S-ranked map, as decoration beside the form on desktop. */
function BoardArt() {
  const { data } = useInfiniteQuery(mapCatalogQueryOptions("", { ranks: ["S"] }));
  const map = data?.pages[0]?.maps[0];
  const picture = map ? mapScreenshotSize("full", map.width, map.height) : null;

  return (
    <Stack aria-hidden="true" as="section" xstyle={styles.art}>
      {map && picture ? (
        <Stack xstyle={styles.board}>
          <MapPicture
            alt=""
            maxScale={2}
            sourceHeight={picture.height}
            sourceWidth={picture.width}
            src={map.screenshot.full}
          />
        </Stack>
      ) : null}
    </Stack>
  );
}

function describeFailure(
  error: { code?: string; message?: string; status?: number },
  isRegister: boolean,
): AuthProblem {
  const said = `${error.code ?? ""} ${error.message ?? ""}`;
  if (isRegister && /already|exists/i.test(said)) {
    return { isSignInFix: true, title: "That email already has an account" };
  }
  if (/invalid.*(email|password)|credential/i.test(said)) {
    return { isSignInFix: false, title: "Wrong email or password" };
  }
  if (/password.*short|too short/i.test(said)) {
    return { isSignInFix: false, title: "That password is too short" };
  }
  if (error.status === 429 || /too many/i.test(said)) {
    return { isSignInFix: false, title: "Too many tries, wait a minute" };
  }
  return { isSignInFix: false, title: error.message || "Something went wrong" };
}

async function submitAuthRequest(
  isRegister: boolean,
  payload: { email: string; password: string; name: string },
) {
  if (isRegister) {
    const parsed = authSignUpSchema.safeParse(payload);
    if (!parsed.success) throw new Error(fieldProblem(parsed.error.issues[0]?.path[0], payload));
    return authClient.signUp.email(parsed.data);
  }

  const parsed = authSignInSchema.safeParse({
    email: payload.email,
    password: payload.password,
  });
  if (!parsed.success) throw new Error(fieldProblem(parsed.error.issues[0]?.path[0], payload));
  return authClient.signIn.email(parsed.data);
}

/** The field a submitted form is missing or got wrong, said as the problem. */
function fieldProblem(field: PropertyKey | undefined, payload: { email: string }): string {
  if (field === "name") return "Enter your name";
  if (field === "email") return payload.email ? "That email is not valid" : "Enter your email";
  if (field === "password") return "Enter your password";
  return "Something went wrong";
}

const styles = stylex.create({
  auth: {
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(0, 1.1fr) minmax(0, 1fr)",
      [pageLayout.phoneMedia]: "minmax(0, 1fr)",
    },
    minBlockSize: {
      default: `calc(100dvh - ${pageLayout.topBarHeight})`,
      [pageLayout.phoneMedia]: 0,
    },
  },
  art: {
    display: {
      default: "grid",
      [pageLayout.phoneMedia]: "none",
    },
    placeItems: "center",
    padding: "40px",
    overflow: "hidden",
    backgroundColor: awbrnVars.ink950,
  },
  board: {
    inlineSize: "100%",
    filter: awbrnVars.shadowBoardPicture,
  },
  panel: {
    display: "flex",
    flexDirection: "column",
    justifyContent: "center",
    maxInlineSize: "620px",
    paddingBlock: {
      default: "56px",
      [pageLayout.phoneMedia]: "20px 32px",
    },
    paddingInline: {
      default: "64px",
      [pageLayout.phoneMedia]: pageLayout.gutterPhone,
    },
  },
  form: {
    maxInlineSize: "440px",
  },
  formBody: {
    paddingBlockStart: "20px",
  },
  fields: {
    display: "grid",
    gap: {
      default: "16px",
      [pageLayout.phoneMedia]: "12px",
    },
    // On a phone the account segment (the form's only md control; a text
    // input sets its own) takes 44px items, and the submit key (its only lg
    // control) the 44px target rather than the dock's 52px.
    "--size-element-md": {
      default: null,
      [pageLayout.phoneMedia]: "52px",
    },
    "--size-element-lg": {
      default: null,
      [pageLayout.phoneMedia]: "44px",
    },
  },
  fill: {
    inlineSize: "100%",
  },
  field: {
    gap: {
      default: null,
      [pageLayout.phoneMedia]: "6px",
    },
  },
});
