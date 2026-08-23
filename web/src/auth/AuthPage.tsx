import { useQueryClient } from "@tanstack/react-query";
import { useNavigate, useRouter } from "@tanstack/react-router";
import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "#/ui/Button.tsx";
import { Card } from "@astryxdesign/core/Card";
import { Center } from "@astryxdesign/core/Center";
import { FormLayout } from "@astryxdesign/core/FormLayout";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { useState } from "react";
import { authClient } from "./client";
import { authSignInSchema, authSignUpSchema } from "./schemas";
import { authKeys } from "./auth.keys";
import { matchKeys } from "#/matches/matches.keys.ts";
import { RouterTextLink } from "#/ui/astryx-links.tsx";
import { TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";

export function AuthPage({ isRegister }: { isRegister: boolean }) {
  const navigate = useNavigate();
  const router = useRouter();
  const queryClient = useQueryClient();
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

      queryClient.removeQueries({ queryKey: matchKeys.mine() });
      queryClient.removeQueries({ queryKey: matchKeys.completed() });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: authKeys.all }),
        queryClient.invalidateQueries({ queryKey: matchKeys.details() }),
      ]);
      await router.invalidate();
      await navigate({ to: "/" });
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : "Authentication failed");
    } finally {
      setIsPending(false);
    }
  }

  return (
    <Section padding={6} variant="transparent">
      <Center axis="horizontal" width="100%">
        <Grid
          align="start"
          columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
          gap={8}
          maxWidth={1200}
          width="100%"
        >
          <Section padding={6} variant="muted">
            <VStack gap={3}>
              <Heading level={1} type="display-2">
                {isRegister ? "Register" : "Sign In"}
              </Heading>
              <Text type="large" weight="medium">
                Use the same field manual language as the rest of the app: clear intent, direct
                actions, no filler.
              </Text>
            </VStack>
          </Section>
          <Card padding={6} width="100%">
            <form onSubmit={handleSubmit}>
              <VStack gap={4}>
                <FormLayout>
                  {isRegister ? (
                    <TextInput
                      autoComplete="name"
                      id="name"
                      isRequired
                      label="Name"
                      onChange={setName}
                      type="text"
                      value={name}
                    />
                  ) : null}
                  <TextInput
                    autoComplete="email"
                    id="email"
                    isRequired
                    label="Email"
                    onChange={setEmail}
                    type="email"
                    value={email}
                  />
                  <TextInput
                    autoComplete={isRegister ? "new-password" : "current-password"}
                    id="password"
                    isRequired
                    label="Password"
                    onChange={setPassword}
                    type="password"
                    value={password}
                  />
                </FormLayout>
                {error ? (
                  <Banner status="error" title="Authentication failed" description={error} />
                ) : null}
                <Button
                  isDisabled={isPending}
                  isLoading={isPending}
                  label={isRegister ? "Create account" : "Sign in"}
                  type="submit"
                  variant="primary"
                  width="100%"
                />
                <Text color="secondary">
                  {isRegister ? (
                    <>
                      Already have an account?{" "}
                      <RouterTextLink to="/auth" search={{ mode: undefined }}>
                        Sign in →
                      </RouterTextLink>
                    </>
                  ) : (
                    <>
                      New here?{" "}
                      <RouterTextLink to="/auth" search={{ mode: "register" }}>
                        Create an account →
                      </RouterTextLink>
                    </>
                  )}
                </Text>
              </VStack>
            </form>
          </Card>
        </Grid>
      </Center>
    </Section>
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
