import { Banner } from "@astryxdesign/core/Banner";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Icon } from "@astryxdesign/core/Icon";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { IconRefresh } from "#/ui/icons.tsx";

export function EmptySheet({
  action,
  art,
  headingLevel = 2,
  title,
}: {
  action?: ReactNode;
  art?: ReactNode;
  headingLevel?: 2 | 3 | 4;
  title: string;
}) {
  return (
    <EmptyState
      actions={action}
      headingLevel={headingLevel}
      icon={art}
      title={title}
      xstyle={styles.empty}
    />
  );
}

export function ProblemBanner({
  action,
  icon,
  status = "error",
  title,
}: {
  action?: ReactNode;
  icon?: ReactNode;
  status?: "error" | "warning" | "success" | "info";
  title: ReactNode;
}) {
  return (
    <Banner
      endContent={action}
      icon={icon ?? (status === "error" ? <Icon icon={IconRefresh} /> : undefined)}
      status={status}
      title={title}
    />
  );
}

const styles = stylex.create({
  empty: {
    alignItems: "flex-start",
    textAlign: "start",
    padding: "28px",
  },
});
