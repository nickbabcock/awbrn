import * as stylex from "@stylexjs/stylex";

/**
 * Typed references to the Airmail tokens that Astryx has no name for.
 *
 * The values live in `awbrnTheme.ts`; these are `var()` references, so a
 * StyleX rule reads the theme rather than a copy of it. Use the Astryx token
 * sets (`colorVars`, `spacingVars`, ...) first and reach here for the ink
 * table, the band, the postmark and the shadows DESIGN.md names.
 */
export const awbrnVars = stylex.defineConsts({
  // Ink: the table, bars, rail, raised keys, board frames.
  ink950: "var(--color-ink-950)",
  ink900: "var(--color-ink-900)",
  ink850: "var(--color-ink-850)",
  ink800: "var(--color-ink-800)",
  ink700: "var(--color-ink-700)",
  ink600: "var(--color-ink-600)",
  ink300: "var(--color-ink-300)",
  ink200: "var(--color-ink-200)",
  ink100: "var(--color-ink-100)",
  // Paper: the ground, sheets, wells and rules.
  paper: "var(--color-paper)",
  paperWell: "var(--color-paper-well)",
  sheet: "var(--color-sheet)",
  rule: "var(--color-rule)",
  ruleStrong: "var(--color-rule-strong)",
  // Text beyond primary and secondary.
  textTertiary: "var(--color-text-tertiary)",
  textOnInk: "var(--color-text-on-ink)",
  textOnInkSecondary: "var(--color-text-on-ink-secondary)",
  /** Pure white on ink: the current section, a key's label, a running clock. */
  textOnInkStrong: "var(--color-text-on-ink-strong)",
  // Airmail.
  red: "var(--color-airmail-red)",
  redHover: "var(--color-airmail-red-hover)",
  redWash: "var(--color-airmail-red-wash)",
  blue: "var(--color-airmail-blue)",
  blueOnInk: "var(--color-airmail-blue-on-ink)",
  violet: "var(--color-stamp-violet)",
  violetOnInk: "var(--color-stamp-violet-on-ink)",
  // Status.
  ok: "var(--color-ok)",
  okOnInk: "var(--color-ok-on-ink)",
  warn: "var(--color-warn)",
  warnFill: "var(--color-warn-fill)",
  danger: "var(--color-danger)",
  /** The ground of an Out tag (text in `danger`). */
  dangerTag: "var(--color-danger-tag)",
  gold: "var(--color-power-gold)",
  goldDim: "var(--color-power-gold-dim)",
  /** A power meter on paper: the charge, and the track it fills. */
  goldOnPaper: "var(--color-power-gold-on-paper)",
  goldTrackOnPaper: "var(--color-power-gold-track-on-paper)",
  // The band, the shadows, the hatch and the stepped timing.
  band: "var(--airmail-band)",
  shadowSheet: "var(--shadow-sheet)",
  shadowOwed: "var(--shadow-owed)",
  shadowFloat: "var(--shadow-float)",
  shadowBoard: "var(--shadow-board)",
  /** The Board shadow as a `filter`, for a map picture whose sprites rise above its first row. */
  shadowBoardPicture: "var(--shadow-board-picture)",
  /** A `backgroundImage` for an empty well on paper (the seeking slot). */
  hatchWell: "var(--hatch-well)",
  shadowStamp: "var(--shadow-stamp)",
  /** The pennant's lift off a map plate, as a `filter`. */
  shadowPennant: "var(--shadow-pennant)",
  /** A soft ink lift under a slot that holds what is owed; pair it with an inset rule. */
  shadowSlot: "var(--shadow-slot)",
  /** A sheet docked at the screen's foot, cast upward. */
  shadowDock: "var(--shadow-dock)",
  // Hairlines and the stamp's paper, as colors for an inset rule or a fill.
  /** #ffffff: a stamp's perforated margin. */
  stampWhite: "var(--color-stamp-white)",
  /** Ink at 14% on paper: the rule around a stamp's field. */
  inkHairline: "var(--color-ink-hairline)",
  /** White at 6% on ink: the edge of a minimap in its well. */
  wellHairline: "var(--color-well-hairline)",
  easeStep: "var(--ease-step)",
  // Half steps of the spacing scale (Astryx `spacingVars` holds the rest).
  /** 10px: a menu head's block, a menu row's start. */
  spacing2_5: "var(--spacing-2-5)",
  /** 14px: a menu head's start, a sheet foot's block, a menu row's end. */
  spacing3_5: "var(--spacing-3-5)",
  /** 18px: a sheet head's top. */
  spacing4_5: "var(--spacing-4-5)",
  /** 22px: a dialog's gutter. */
  spacing5_5: "var(--spacing-5-5)",
  // Type under the Astryx scale.
  /** 11.5px: the tab bar's label and the nav count. */
  textNav: "var(--text-nav-size)",
  /** 11px, the floor: the count on a tab. */
  textMicro: "var(--text-micro-size)",
  // Sizes by their pixels, for a screen whose size is not one of the named
  // styles (`typeScaleVars`). 11px is `textMicro`.
  text11_5: "var(--text-11-5-size)",
  text12: "var(--text-12-size)",
  text12_5: "var(--text-12-5-size)",
  text13: "var(--text-13-size)",
  text13_5: "var(--text-13-5-size)",
  text14: "var(--text-14-size)",
  text14_5: "var(--text-14-5-size)",
  text15: "var(--text-15-size)",
  text15_5: "var(--text-15-5-size)",
  text16: "var(--text-16-size)",
  text17: "var(--text-17-size)",
  text26: "var(--text-26-size)",
  /** 3px: the current section's mark (its thickness and its rounded ends). */
  currentMark: "var(--size-current-mark)",
  // Families, for a primitive that names its face outright.
  fontUi: "var(--font-family-body)",
  fontFigure: "var(--font-family-code)",

  /** @deprecated The old soft rule. Use `rule`. */
  colorBorderSoft: "var(--color-border-soft)",
  /** @deprecated The old disabled outline. Use `rule`. */
  colorBorderDisabled: "var(--color-border-disabled)",
  /** @deprecated Airmail keys do not move when pressed; this is 0px. */
  offsetControlPressed: "var(--offset-control-pressed)",
});

/** Match report dimensions that do not map to the Astryx core scale. */
export const matchHistoryVars = stylex.defineConsts({
  briefWidth: "300px",
});
