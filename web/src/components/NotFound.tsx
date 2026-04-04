import { Link } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { Button, Frame, Heading, Inline, Page, Section, Stack, Text } from "../ui/primitives";
import { recoveryLinkStyle } from "../ui/links";
import { sxClassName } from "../ui/stylex";

const styles = stylex.create({
  frame: {
    maxWidth: 720,
  },
});

export function NotFound() {
  return (
    <Page width="content">
      <Section>
        <Frame xstyle={styles.frame}>
          <Stack gap="lg">
            <Stack gap="sm">
              <Text tone="muted" size="sm">
                Route
              </Text>
              <Heading size="display">Not Found</Heading>
              <Text size="lg">The page you are looking for does not exist.</Text>
            </Stack>
            <Inline gap="sm">
              <Button
                tone="brand"
                type="button"
                onClick={() => {
                  window.history.back();
                }}
              >
                Go Back
              </Button>
              <Link className={sxClassName(recoveryLinkStyle)} to="/">
                Start Over
              </Link>
            </Inline>
          </Stack>
        </Frame>
      </Section>
    </Page>
  );
}
