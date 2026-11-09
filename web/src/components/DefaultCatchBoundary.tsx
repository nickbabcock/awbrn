import {
  ErrorComponent,
  rootRouteId,
  useMatch,
  useRouter,
  type ErrorComponentProps,
} from "@tanstack/react-router";
import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "#/ui/Button.tsx";
import { Card } from "@astryxdesign/core/Card";
import { Center } from "@astryxdesign/core/Center";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { Page } from "#/ui/Page.tsx";

export function DefaultCatchBoundary({ error }: ErrorComponentProps) {
  const router = useRouter();
  const isRoot = useMatch({
    strict: false,
    select: (state) => state.id === rootRouteId,
  });

  console.error("Route error:", error);

  function handleGoBack() {
    if (!router.history.canGoBack()) {
      void router.navigate({ to: "/" });
      return;
    }

    router.history.back();
  }

  return (
    <Page width="narrow">
      <Center axis="horizontal" width="100%">
        <Card maxWidth={800} padding={8} width="100%">
          <VStack gap={4}>
            <Text color="secondary" type="supporting" weight="bold">
              Router
            </Text>
            <Banner
              description={<ErrorComponent error={error} />}
              status="error"
              title="Route error"
            />
            <HStack gap={2} wrap="wrap">
              <Button clickAction={() => router.invalidate()} label="Try again" variant="primary" />
              {isRoot ? (
                <RouterButton label="Home" to="/" variant="secondary" />
              ) : (
                <Button label="Go back" onClick={handleGoBack} variant="secondary" />
              )}
            </HStack>
          </VStack>
        </Card>
      </Center>
    </Page>
  );
}
