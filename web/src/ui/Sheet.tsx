import { Card } from "@astryxdesign/core/Card";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { borderVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { AirmailBand } from "#/ui/AirmailBand.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

/**
 * A sheet: a panel laid on paper (10px corners, 1px rule, the Sheet shadow).
 * Lists and forms lift onto sheets; never put a sheet inside a sheet.
 *
 * - `tone="sheet"` (default), `"well"` (one step darker than paper, flat: the
 *   Waiting row, insets), `"ink"` (a panel on the ink table).
 * - `isOwed`: what waits on the viewer. The airmail band runs along the top and
 *   the sheet lifts one step higher. Band Means Owed: use it for nothing else.
 *
 * Compose with `SheetHead`, `SheetBody`, `SheetRows` and `SheetFoot`, which
 * carry the sheet's gutters (20px, 16px on a phone):
 *
 *   <Sheet aria-labelledby="your-move" isOwed>
 *     <SheetHead><Heading id="your-move" level={1}>Your move</Heading></SheetHead>
 *     <SheetBody>...</SheetBody>
 *     <SheetRows>{matches.map((m) => <SheetRow key={m.id}>...</SheetRow>)}</SheetRows>
 *   </Sheet>
 */
export function Sheet({
  "aria-label": ariaLabel,
  "aria-labelledby": ariaLabelledBy,
  as = "section",
  children,
  isOwed = false,
  tone = "sheet",
  xstyle,
}: {
  "aria-label"?: string;
  "aria-labelledby"?: string;
  as?: "section" | "article" | "aside" | "div";
  children: ReactNode;
  isOwed?: boolean;
  tone?: "sheet" | "well" | "ink";
  xstyle?: stylex.StyleXStyles;
}) {
  const Element = as;
  return (
    <Element aria-label={ariaLabel} aria-labelledby={ariaLabelledBy} {...stylex.props(styles.host)}>
      <Card
        padding={0}
        variant={tone === "well" ? "well" : tone === "ink" ? "ink" : isOwed ? "owed" : "default"}
        xstyle={[styles.sheet, xstyle]}
      >
        {isOwed ? <AirmailBand /> : null}
        {children}
      </Card>
    </Element>
  );
}

/** The head row of a sheet: its heading, and keys or a count at the end. */
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

/** The sheet's padded body. */
export function SheetBody({
  children,
  xstyle,
}: {
  children: ReactNode;
  xstyle?: stylex.StyleXStyles;
}) {
  return <VStack xstyle={[styles.body, xstyle]}>{children}</VStack>;
}

/**
 * Rows on a sheet: a list whose `SheetRow` items are split by 1px rules.
 * `hasTopRule` draws a rule above the first row (under a head).
 */
export function SheetRows({
  "aria-label": ariaLabel,
  children,
  hasTopRule = true,
}: {
  "aria-label"?: string;
  children: ReactNode;
  hasTopRule?: boolean;
}) {
  return (
    <ul aria-label={ariaLabel} {...stylex.props(styles.rows, hasTopRule && styles.rowsTopRule)}>
      {children}
    </ul>
  );
}

/** One row of `SheetRows`; rows after the first take a 1px rule above. */
export function SheetRow({
  children,
  xstyle,
}: {
  children: ReactNode;
  xstyle?: stylex.StyleXStyles;
}) {
  return <li {...stylex.props(styles.row, xstyle)}>{children}</li>;
}

/** The foot of a sheet: a rule, then its keys. */
export function SheetFoot({ children }: { children: ReactNode }) {
  return (
    <HStack align="center" gap={3} justify="between" wrap="wrap" xstyle={styles.foot}>
      {children}
    </HStack>
  );
}

/** A rule between parts of a sheet. */
const rule = {
  borderBlockStartColor: awbrnVars.rule,
  borderBlockStartStyle: "solid",
  borderBlockStartWidth: borderVars["--border-width"],
} as const;

/** The sheet's gutter: 20px, 16px on a phone. */
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
