import * as stylex from "@stylexjs/stylex";

export type XStyle = stylex.StyleXStyles;

export function sx(...styles: XStyle[]) {
  return stylex.props(...styles);
}

export function sxClassName(...styles: XStyle[]) {
  return stylex.props(...styles).className || undefined;
}
