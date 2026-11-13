import { Text } from "@astryxdesign/core/Text";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import type { ReactNode } from "react";
import { figureStyles } from "#/ui/Figure.tsx";

export { mapPostage } from "#/maps/components/MapPlate.tsx";

/**
 * The postage line: a row's facts franked in mono, mixed case
 * (`2P · 22×18 · cookielamb3`, `Live 5m +2m · 1 / 2`).
 */
export function Postage({
  children,
  xstyle,
}: {
  children: ReactNode;
  xstyle?: stylex.StyleXStyles;
}) {
  return (
    <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.postage, xstyle]}>
      {children}
    </Text>
  );
}

const styles = stylex.create({
  postage: {
    display: "block",
    fontSize: awbrnVars.text11_5,
    fontWeight: 500,
    lineHeight: 1.35,
    letterSpacing: "0.02em",
    whiteSpace: "normal",
    overflowWrap: "anywhere",
    color: colorVars["--color-text-secondary"],
  },
});
