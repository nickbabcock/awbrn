import {
  rootRouteId,
  useMatch,
  useRouter,
  useRouterState,
  type ErrorComponentProps,
} from "@tanstack/react-router";
import { HStack } from "@astryxdesign/core/Stack";
import { NotFound } from "#/components/NotFound.tsx";
import { Button } from "#/ui/Button.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { ProblemBanner } from "#/ui/EmptySheet.tsx";
import { Page } from "#/ui/Page.tsx";
import { sectionForPath } from "#/ui/site_sections.ts";

/**
 * A screen that failed to load: the problem as the title and the key that
 * recovers.
 *
 * A thing that does not exist ("match not found", "Map not found.") says so
 * and leads to its section, since trying again never finds it. Anything else
 * may be passing, so it keeps Try again, with the way out beside it.
 */
export function DefaultCatchBoundary({ error }: ErrorComponentProps) {
  const router = useRouter();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const isRoot = useMatch({
    strict: false,
    select: (state) => state.id === rootRouteId,
  });

  console.error("Route error:", error);

  const message = error instanceof Error ? error.message.trim() : "";
  if (/\bnot found\.?$/i.test(message)) {
    const section = sectionForPath(pathname);
    const title = message.charAt(0).toUpperCase() + message.slice(1).replace(/\.$/, "");
    return section ? (
      <NotFound label={section.label} title={title} to={section.to} />
    ) : (
      <NotFound title={title} />
    );
  }

  function handleGoBack() {
    if (!router.history.canGoBack()) {
      void router.navigate({ to: "/" });
      return;
    }

    router.history.back();
  }

  return (
    <Page width="narrow">
      <ProblemBanner
        action={
          <HStack gap={2} wrap="wrap">
            <Button clickAction={() => router.invalidate()} label="Try again" size="sm" />
            {isRoot ? (
              <RouterButton label="Home" size="sm" to="/" variant="ghost" />
            ) : (
              <Button label="Go back" onClick={handleGoBack} size="sm" variant="ghost" />
            )}
          </HStack>
        }
        title={message ? `This page failed to load: ${message}` : "This page failed to load"}
      />
    </Page>
  );
}
