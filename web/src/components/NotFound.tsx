import { RouterButton } from "#/ui/astryx-links.tsx";
import { EmptySheet } from "#/ui/EmptySheet.tsx";
import { Page } from "#/ui/Page.tsx";

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
      <EmptySheet action={<RouterButton label={label} to={to} variant="ink" />} title={title} />
    </Page>
  );
}
