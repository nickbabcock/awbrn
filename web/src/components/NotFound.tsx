import { RouterButton } from "#/ui/astryx-links.tsx";
import { EmptySheet } from "#/ui/EmptySheet.tsx";
import { Page } from "#/ui/Page.tsx";

/**
 * A page, or the thing a page shows, that does not exist: the problem as the
 * title and one key to somewhere that does (home, unless the caller names the
 * page's own section).
 */
export function NotFound({
  title = "Page not found",
  to = "/",
  label = "Home",
}: {
  title?: string;
  to?: string;
  label?: string;
}) {
  return (
    <Page width="narrow">
      <EmptySheet
        action={<RouterButton label={label} to={to} variant="ink" />}
        headingLevel={2}
        title={title}
      />
    </Page>
  );
}
