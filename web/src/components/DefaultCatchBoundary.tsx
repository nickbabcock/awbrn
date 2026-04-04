import {
  ErrorComponent,
  Link,
  rootRouteId,
  useMatch,
  useRouter,
  type ErrorComponentProps,
} from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { Button, Frame, Inline, Page, Section, Stack, Text } from "#/ui/primitives.tsx";
import { recoveryLinkStyle } from "#/ui/links.tsx";
import { sxClassName } from "#/ui/stylex.ts";

const styles = stylex.create({
  frame: {
    maxWidth: 800,
  },
});

export function DefaultCatchBoundary({ error }: ErrorComponentProps) {
  const router = useRouter();
  const isRoot = useMatch({
    strict: false,
    select: (state) => state.id === rootRouteId,
  });

  console.error("Route error:", error);

  return (
    <Page width="content">
      <Section>
        <Frame xstyle={styles.frame}>
          <Stack gap="lg">
            <Stack gap="sm">
              <Text tone="muted" size="sm">
                Router
              </Text>
              <Text as="h1" size="lg" tone="strong">
                Route Error
              </Text>
              <div>
                <ErrorComponent error={error} />
              </div>
            </Stack>
            <Inline gap="sm">
              <Button
                tone="brand"
                type="button"
                onClick={() => {
                  void router.invalidate();
                }}
              >
                Try Again
              </Button>
              {isRoot ? (
                <Link className={sxClassName(recoveryLinkStyle)} to="/">
                  Home
                </Link>
              ) : (
                <Button
                  tone="neutral"
                  type="button"
                  variant="outline"
                  onClick={() => {
                    if (window.history.length <= 1) {
                      void router.navigate({ to: "/" });
                      return;
                    }

                    const previousHref = window.location.href;
                    window.history.back();
                    window.setTimeout(() => {
                      if (window.location.href === previousHref) {
                        void router.navigate({ to: "/" });
                      }
                    }, 160);
                  }}
                >
                  Go Back
                </Button>
              )}
            </Inline>
          </Stack>
        </Frame>
      </Section>
    </Page>
  );
}
