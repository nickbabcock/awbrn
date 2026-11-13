import { Card } from "@astryxdesign/core/Card";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { borderVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { AirmailBand } from "#/ui/AirmailBand.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/**
 * A panel on paper; never put a sheet inside a sheet. `isOwed` marks what
 * waits on the viewer. Compose with `SheetHead`, `SheetBody`, `SheetRows` and
 * `SheetFoot`, which carry the sheet's gutters.
 */
export function Sheet({
  "aria-label": ariaLabel,
  "aria-labelledby": ariaLabelledBy,
  children,
  isOwed = false,
  tone = "sheet",
  xstyle,
}: {
  "aria-label"?: string;
  "aria-labelledby"?: string;
  children: ReactNode;
  isOwed?: boolean;
  tone?: "sheet" | "well";
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    <section aria-label={ariaLabel} aria-labelledby={ariaLabelledBy} {...stylex.props(styles.host)}>
      <Card
        padding={0}
        variant={tone === "well" ? "well" : isOwed ? "owed" : "default"}
        xstyle={[styles.sheet, xstyle]}
      >
        {isOwed ? <AirmailBand /> : null}
        {children}
      </Card>
    </section>
  );
}

export function SheetHead({ actions, children }: { actions?: ReactNode; children: ReactNode }) {
  return (
    <HStack align="center" gap={4} justify="between" xstyle={styles.head}>
      {children}
      {actions ? (
        <HStack align="center" gap={2} xstyle={styles.headEnd}>
          {actions}
        </HStack>
      ) : null}
    </HStack>
  );
}

export function SheetBody({
  children,
  xstyle,
}: {
  children: ReactNode;
  xstyle?: stylex.StyleXStyles;
}) {
  return <VStack xstyle={[styles.body, xstyle]}>{children}</VStack>;
}

/** `hasTopRule` draws a rule above the first row (under a head). */
export function SheetRows({
  children,
  hasTopRule = true,
}: {
  children: ReactNode;
  hasTopRule?: boolean;
}) {
  return <ul {...stylex.props(styles.rows, hasTopRule && styles.rowsTopRule)}>{children}</ul>;
}

export function SheetRow({ children }: { children: ReactNode }) {
  return <li {...stylex.props(styles.row)}>{children}</li>;
}

export function SheetFoot({ children }: { children: ReactNode }) {
  return (
    <HStack align="center" gap={3} justify="between" wrap="wrap" xstyle={styles.foot}>
      {children}
    </HStack>
  );
}

const rule = {
  borderBlockStartColor: awbrnVars.rule,
  borderBlockStartStyle: "solid",
  borderBlockStartWidth: borderVars["--border-width"],
} as const;

const gutter = {
  default: spacingVars["--spacing-5"],
  [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
} as const;

const styles = stylex.create({
  host: {
    display: "block",
    minInlineSize: 0,
  },
  sheet: {
    overflow: "clip",
  },
  head: {
    paddingBlockStart: {
      default: awbrnVars.spacing4_5,
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
    paddingBlockEnd: {
      default: awbrnVars.spacing3_5,
      [pageLayout.phoneMedia]: spacingVars["--spacing-3"],
    },
    paddingInline: gutter,
  },
  headEnd: {
    flexShrink: 0,
  },
  body: {
    paddingBlockEnd: gutter,
    paddingInline: gutter,
  },
  rows: {
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  rowsTopRule: rule,
  row: {
    ...rule,
    borderBlockStartWidth: {
      default: borderVars["--border-width"],
      ":first-child": 0,
    },
  },
  foot: {
    ...rule,
    paddingBlock: {
      default: awbrnVars.spacing3_5,
      [pageLayout.phoneMedia]: spacingVars["--spacing-3"],
    },
    paddingInline: gutter,
  },
});
