import { Button } from "#/ui/Button.tsx";
import { Card } from "@astryxdesign/core/Card";
import { Center } from "@astryxdesign/core/Center";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { HStack } from "@astryxdesign/core/Stack";
import { useRouter } from "@tanstack/react-router";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { Page } from "#/ui/Page.tsx";

export function NotFound() {
  const router = useRouter();

  function handleGoBack() {
    if (router.history.canGoBack()) {
      router.history.back();
    } else {
      void router.navigate({ to: "/" });
    }
  }

  return (
    <Page width="narrow">
      <Center axis="horizontal" width="100%">
        <Card maxWidth={720} padding={8} width="100%">
          <EmptyState
            actions={
              <HStack gap={2} justify="center" wrap="wrap">
                <Button label="Go back" onClick={handleGoBack} variant="primary" />
                <RouterButton label="Start over" to="/" variant="secondary" />
              </HStack>
            }
            description="The page you are looking for does not exist."
            headingLevel={1}
            title="Not found"
          />
        </Card>
      </Center>
    </Page>
  );
}
