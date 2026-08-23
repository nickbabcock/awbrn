import * as stylex from "@stylexjs/stylex";

/** Typed references to AWBRN tokens that Astryx does not provide. */
export const awbrnVars = stylex.defineConsts({
  colorBorderDisabled: "var(--color-border-disabled)",
  colorBorderSoft: "var(--color-border-soft)",
  offsetControlPressed: "var(--offset-control-pressed)",
});

/** Match report dimensions that do not map to the Astryx core scale. */
export const matchHistoryVars = stylex.defineConsts({
  briefWidth: "300px",
  titleFluidSize: "13vw",
  titleMaximumSize: "3.375rem",
  titleMinimumSize: "1.75rem",
});
