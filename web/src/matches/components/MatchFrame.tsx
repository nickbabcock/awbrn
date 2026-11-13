import { Text } from "@astryxdesign/core/Text";
import { Icon } from "@astryxdesign/core/Icon";
import { HStack, Stack, VStack } from "@astryxdesign/core/Stack";
import { borderVars, radiusVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useState, type ReactNode, type RefObject } from "react";
import type { GameFullscreenMode } from "#/canvas_courier/index.ts";
import { CancelMark } from "#/components/CoStamp.tsx";
import { UiSprite } from "#/components/UiSprite.tsx";
import { matchLayout, matchType } from "#/matches/components/matchLayout.stylex.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { AirmailBand } from "#/ui/AirmailBand.tsx";
import { arrivalFocusProps } from "#/ui/arrivalFocus.ts";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconEye, IconFullScreen, IconRefresh } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Postmark } from "#/ui/Postmark.tsx";
import { ShellMode } from "#/ui/ShellMode.tsx";

/** Where the match is a phone screen; `pageLayout.phoneMedia` without `@media`. */
const PHONE_QUERY = "(max-width: 720px)";

const INSET_TOP = "env(safe-area-inset-top, 0px)";
const INSET_BOTTOM = "env(safe-area-inset-bottom, 0px)";

/**
 * The match and the replay viewer, as one pushed screen: a stage beside a rail
 * on desktop, a column on a phone. One DOM: the canvas box is never remounted
 * between the two.
 *
 * Each safe-area inset is counted once. AppShell pads its header with the top
 * inset (the navy header stays, empty, over a phone's turn line) and main
 * with the bottom one. On desktop the screen is the window less both, as the
 * paper page is. On a phone the dock (and every dock that takes its place)
 * pads the home indicator itself in ink, so the screen hangs into main's
 * bottom padding rather than leaving a strip of paper under the dock.
 *
 * `surfaceRef` is the element full screen takes, so the rail and the dock come
 * with the board.
 */
export function MatchFrame({
  dock,
  foot,
  fullscreenMode,
  label,
  phoneArmies,
  rail,
  railFoot,
  strip,
  surfaceRef,
  table,
  tileRow,
  turnLine,
}: {
  dock?: ReactNode;
  foot?: ReactNode;
  fullscreenMode: GameFullscreenMode;
  label: string;
  phoneArmies?: ReactNode;
  rail: ReactNode;
  railFoot?: ReactNode;
  strip: ReactNode;
  surfaceRef: RefObject<HTMLElement | null>;
  table: ReactNode;
  tileRow?: ReactNode;
  turnLine: ReactNode;
}) {
  // Each width mounts only its own regions, so a name or a key is in the page
  // once: hidden twins would be found first by search and by tests. Until the
  // page knows its width (the server's render, and the first paint after it)
  // both are mounted and the CSS below shows the one that fits, so a phone
  // opens on its own turn line, Back key and dock rather than a bare table.
  const width = useScreenWidth();
  const showsDesktop = width !== "phone";
  const showsPhone = width !== "desktop";
  return (
    <section
      aria-label={label}
      ref={surfaceRef}
      {...stylex.props(
        styles.screen,
        fullscreenMode !== "off" && styles.fullscreen,
        fullscreenMode === "immersive" && styles.immersive,
        foot === undefined && styles.noFoot,
      )}
    >
      <ShellMode mode="pushed" />
      {showsDesktop ? (
        <section aria-label="Turn" {...arrivalFocusProps} {...stylex.props(styles.strip)}>
          {strip}
        </section>
      ) : null}
      {showsPhone ? (
        <section aria-label="Turn" {...arrivalFocusProps} {...stylex.props(styles.turnLine)}>
          {turnLine}
        </section>
      ) : null}
      {showsPhone ? (
        <section aria-label="Armies" {...stylex.props(styles.phoneArmies)}>
          {phoneArmies}
        </section>
      ) : null}
      <section aria-label="Board" {...stylex.props(styles.table)}>
        {table}
      </section>
      {showsPhone ? (
        <section aria-label="Tile" {...stylex.props(styles.tileRow)}>
          {tileRow}
        </section>
      ) : null}
      {foot !== undefined && showsDesktop ? (
        <footer {...stylex.props(styles.foot)}>{foot}</footer>
      ) : null}
      {showsPhone ? <footer {...stylex.props(styles.dock)}>{dock}</footer> : null}
      {showsDesktop ? (
        <aside aria-label="Armies" {...stylex.props(styles.rail)}>
          <VStack xstyle={styles.railScroll}>{rail}</VStack>
          {railFoot ? (
            <HStack align="center" as="footer" gap={2} xstyle={styles.railFoot}>
              {railFoot}
            </HStack>
          ) : null}
        </aside>
      ) : null}
    </section>
  );
}

/**
 * Which width the screen is laid out for, or null until the page has read it
 * (on the server, and in the first paint that hydrates its markup).
 */
function useScreenWidth(): "phone" | "desktop" | null {
  const [width, setWidth] = useState<"phone" | "desktop" | null>(null);
  useEffect(() => {
    const query = window.matchMedia(PHONE_QUERY);
    const read = () => setWidth(query.matches ? "phone" : "desktop");
    read();
    query.addEventListener("change", read);
    return () => query.removeEventListener("change", read);
  }, []);
  return width;
}

/** Whether the browser can hand an element the screen (iOS Safari cannot). */
export function useFullscreenEnabled(): boolean {
  const [isEnabled, setIsEnabled] = useState(false);
  useEffect(() => {
    setIsEnabled(document.fullscreenEnabled === true);
  }, []);
  return isEnabled;
}

/** Full screen, and back out of it. */
export function FullscreenKey({
  isFullscreen,
  onEnter,
  onExit,
}: {
  isFullscreen: boolean;
  onEnter: () => void;
  onExit: () => void;
}) {
  return (
    <Button
      aria-pressed={isFullscreen}
      icon={<Icon icon={IconFullScreen} />}
      isIconOnly
      label={isFullscreen ? "Exit full screen" : "Full screen"}
      onClick={isFullscreen ? onExit : onEnter}
      tooltip={isFullscreen ? "Exit full screen (Esc)" : "Full screen"}
      variant="on-ink"
      xstyle={styles.iconKey}
    />
  );
}

/**
 * The match connection: the first one, a live one, one that dropped, a first
 * one the server refused, or none, because the viewer is signed out.
 */
export type MatchConnection = "connected" | "connecting" | "reconnecting" | "refused" | "signedOut";

/**
 * The live connection, as a dot, and a word when it is not live. On a phone
 * the dock says the word, so the turn line gives the word to assistive tech
 * alone. Warn means a connection that dropped: nothing has gone wrong with a
 * first one yet, and a refused first one is still being tried.
 */
export function ConnectionState({ status }: { status: MatchConnection }) {
  if (status === "signedOut") return null;
  if (status === "connected") {
    return (
      <Stack as="span" aria-label="Connected" role="img" xstyle={[styles.dot, styles.dotLive]} />
    );
  }
  const isFirst = status !== "reconnecting";
  return (
    <HStack
      as="span"
      role="status"
      xstyle={[styles.connection, isFirst ? styles.quiet : styles.warn]}
    >
      <Stack
        as="span"
        aria-hidden="true"
        xstyle={[styles.dot, isFirst ? styles.dotQuiet : styles.dotWarn]}
      />
      <Text color="inherit" type="inherit" xstyle={styles.connectionWord}>
        {isFirst ? "Connecting" : "Reconnecting"}
      </Text>
    </HStack>
  );
}

/** A spectator's tag: they are watching, not playing. */
export function WatchingTag({ hasFog }: { hasFog: boolean }) {
  // Raw span: Astryx components drop `title`, and this tooltip is its label.
  return (
    <span
      title={hasFog ? "Fog: you see only what is public" : "The whole board is visible"}
      {...stylex.props(styles.watching)}
    >
      <Icon icon={IconEye} size="xsm" />
      Watching
    </span>
  );
}

/**
 * The desktop turn strip. `isDayNew` lands the postmark: the live day just
 * changed. `isLive` cancels the postmark: the live match's day is being
 * played; a replay or a review shows the postmark alone.
 */
export function TurnStrip({
  clock,
  day,
  isDayNew = false,
  isLive = false,
  isOwed,
  keys,
  sub,
  title,
}: {
  clock?: ReactNode;
  day: number | null;
  isDayNew?: boolean;
  isLive?: boolean;
  isOwed: boolean;
  keys?: ReactNode;
  sub?: ReactNode;
  title: string;
}) {
  const ringText = useRingDate();
  return (
    <header {...stylex.props(styles.turnStrip)}>
      {isOwed ? <AirmailBand isOverlay thickness={5} /> : null}
      {day === null ? null : (
        <HStack as="span" xstyle={styles.postmarkGroup}>
          <Postmark
            figure={day}
            isNew={isDayNew}
            key={day}
            label={`Day ${day}`}
            ringText={ringText ?? undefined}
            tone="ink"
            word="DAY"
          />
          {isLive ? <CancelMark xstyle={styles.cancel} /> : null}
        </HStack>
      )}
      <HStack as="span" xstyle={styles.who}>
        <h1 {...stylex.props(styles.title, styles.stripTitle)}>{title}</h1>
        {sub ? (
          <HStack as="span" xstyle={styles.sub}>
            {sub}
          </HStack>
        ) : null}
      </HStack>
      <HStack as="span" xstyle={styles.stripEnd}>
        {clock ? (
          <HStack as="span" xstyle={styles.stripClock}>
            {clock}
          </HStack>
        ) : null}
        {keys}
      </HStack>
    </header>
  );
}

export function StripClock({
  label,
  pressure,
  value,
}: {
  label: string;
  pressure: "steady" | "low" | "critical";
  value: string;
}) {
  return (
    <HStack as="span" aria-label={label} role="timer" xstyle={styles.clock}>
      <UiSprite name="Clock.png" />
      <Text
        color="inherit"
        type="inherit"
        aria-hidden="true"
        xstyle={[figureStyles.figure, styles.clockFigure, pressure !== "steady" && styles.clockLow]}
      >
        {value}
      </Text>
    </HStack>
  );
}

export function TurnLine({
  back,
  day,
  isDayNew = false,
  isTitleDim = false,
  keys,
  sub,
  title,
}: {
  back: ReactNode;
  day: number | null;
  /** The live day just changed: the postmark lands. */
  isDayNew?: boolean;
  isTitleDim?: boolean;
  keys?: ReactNode;
  sub?: ReactNode;
  title: string;
}) {
  return (
    <header {...stylex.props(styles.turnLineBar)}>
      {back}
      {day === null ? null : (
        <Postmark
          figure={day}
          isNew={isDayNew}
          key={day}
          label={`Day ${day}`}
          size={44}
          tone="ink"
        />
      )}
      <HStack as="span" xstyle={[styles.who, styles.lineWho]}>
        <Text
          color="inherit"
          type="inherit"
          xstyle={[styles.title, styles.lineTitle, isTitleDim && styles.lineTitleDim]}
        >
          {title}
        </Text>
        {sub ? (
          <HStack as="span" xstyle={styles.lineSub}>
            {sub}
          </HStack>
        ) : null}
      </HStack>
      {keys ? (
        <HStack as="span" xstyle={styles.lineKeys}>
          {keys}
        </HStack>
      ) : null}
    </header>
  );
}

/** The postmark's ring date: today, `NOV 12 · 2026`. Client only. */
function useRingDate(): string | null {
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    const now = new Date();
    const month = now.toLocaleString("en-US", { month: "short" }).toUpperCase();
    setText(`${month} ${now.getDate()} · ${now.getFullYear()}`);
  }, []);
  return text;
}

export function MatchNotice({
  action,
  description,
  title,
  tone = "danger",
}: {
  action?: ReactNode;
  description?: string;
  title: string;
  tone?: "danger" | "info";
}) {
  return (
    <HStack
      align="center"
      gap={2}
      role={tone === "danger" ? "alert" : "status"}
      wrap="wrap"
      xstyle={[styles.notice, tone === "info" && styles.noticeInfo]}
    >
      <Stack as="span" xstyle={styles.noticeText}>
        <b {...stylex.props(styles.noticeTitle)}>{title}</b>
        {description ? (
          <Text color="inherit" type="inherit">
            {description}
          </Text>
        ) : null}
      </Stack>
      {action}
    </HStack>
  );
}

export function ReconnectKey({ onPress }: { onPress: () => void }) {
  return (
    <Button
      icon={<Icon icon={IconRefresh} />}
      label="Reconnect"
      onClick={onPress}
      variant="on-ink"
    />
  );
}

/**
 * Sign in, for a signed-out viewer of a match: the recovery key of the
 * desktop notice, or filling the phone dock where the connection would be.
 */
export function SignInKey({ size }: { size: "notice" | "dock" }) {
  return size === "dock" ? (
    <RouterButton
      label="Sign in"
      search={{ mode: undefined }}
      size="lg"
      to="/auth"
      variant="primary"
      xstyle={styles.dockFill}
    />
  ) : (
    <RouterButton label="Sign in" search={{ mode: undefined }} to="/auth" variant="on-ink" />
  );
}

export function DockKey({
  icon,
  isDisabled,
  label,
  onPress,
}: {
  icon: typeof IconEye;
  isDisabled?: boolean;
  label: string;
  onPress: () => void;
}) {
  return (
    <Button
      icon={<Icon icon={icon} xstyle={styles.dockGlyph} />}
      isDisabled={isDisabled}
      isIconOnly
      label={label}
      onClick={onPress}
      tooltip={label}
      variant="on-ink"
      xstyle={styles.dockKey}
    />
  );
}

/**
 * An icon key on ink (rail foot, turn line). With `href` it is a link to a
 * file (Download the replay archive) rather than a command.
 */
export function InkIconKey({
  href,
  icon,
  isDisabled,
  label,
  onPress,
}: {
  icon: typeof IconEye;
  isDisabled?: boolean;
  label: string;
} & ({ href: string; onPress?: never } | { href?: never; onPress: () => void })) {
  const link = href === undefined ? {} : ({ as: "a", href } as const);
  return (
    <Button
      {...link}
      icon={<Icon icon={icon} />}
      isDisabled={isDisabled}
      isIconOnly
      label={label}
      onClick={onPress}
      tooltip={label}
      variant="on-ink"
      xstyle={styles.iconKey}
    />
  );
}

export function DockRow({ children }: { children: ReactNode }) {
  return (
    <HStack align="stretch" gap={2} xstyle={styles.dockRow}>
      {children}
    </HStack>
  );
}

/** Their turn, in the dock: who is moving and their clock. */
export function WaitCell({
  clock,
  line,
  name,
}: {
  clock?: ReactNode;
  /**
   * The connection, not a player, is what is being waited on: the first one
   * (Text on ink 2) or one that dropped (warn).
   */
  line?: "connecting" | "reconnecting";
  name: string;
}) {
  return (
    <HStack as="span" role={line ? "status" : undefined} xstyle={styles.waitCell}>
      <HStack
        as="span"
        xstyle={[
          styles.waitName,
          line === "connecting" && styles.quiet,
          line === "reconnecting" && styles.warn,
        ]}
      >
        <Stack
          as="span"
          aria-hidden="true"
          xstyle={[
            styles.dot,
            line === undefined
              ? styles.dotLive
              : line === "connecting"
                ? styles.dotQuiet
                : styles.dotWarn,
          ]}
        />
        <Text color="inherit" type="inherit" xstyle={styles.waitNameText}>
          {name}
        </Text>
      </HStack>
      {clock}
    </HStack>
  );
}

const styles = stylex.create({
  screen: {
    display: "grid",
    gridTemplateColumns: {
      default: `minmax(0, 1fr) ${matchLayout.railWidth}`,
      [pageLayout.phoneMedia]: "minmax(0, 1fr)",
    },
    gridTemplateRows: {
      default: `${matchLayout.stripHeight} minmax(0, 1fr) ${matchLayout.footHeight}`,
      [pageLayout.phoneMedia]: "auto auto minmax(0, 1fr) auto auto",
    },
    gridTemplateAreas: {
      default: '"strip rail" "table rail" "foot rail"',
      [pageLayout.phoneMedia]: '"turnline" "armies" "table" "tilerow" "dock"',
    },
    blockSize: {
      default: `calc(100dvh - ${pageLayout.topBarHeight} - ${INSET_TOP} - ${INSET_BOTTOM})`,
      [pageLayout.phoneMedia]: `calc(100dvh - ${INSET_TOP})`,
    },
    marginBlockEnd: {
      default: 0,
      [pageLayout.phoneMedia]: `calc(-1 * ${INSET_BOTTOM})`,
    },
    minBlockSize: {
      default: matchLayout.screenMinHeight,
      [pageLayout.phoneMedia]: matchLayout.screenMinHeightPhone,
    },
    inlineSize: "100%",
    // A phone in landscape gets this grid; keep it clear of the notch and corners.
    paddingInline: "env(safe-area-inset-left) env(safe-area-inset-right)",
    overflow: "hidden",
    backgroundColor: awbrnVars.ink950,
    color: awbrnVars.textOnInk,
  },
  noFoot: {
    gridTemplateRows: {
      default: `${matchLayout.stripHeight} minmax(0, 1fr)`,
      [pageLayout.phoneMedia]: "auto auto minmax(0, 1fr) auto auto",
    },
    gridTemplateAreas: {
      default: '"strip rail" "table rail"',
      [pageLayout.phoneMedia]: '"turnline" "armies" "table" "tilerow" "dock"',
    },
  },
  // Out of the shell, nothing above pads the top inset; the screen does.
  fullscreen: {
    blockSize: "100dvh",
    marginBlockEnd: 0,
    paddingBlockStart: INSET_TOP,
  },
  immersive: {
    position: "fixed",
    inset: 0,
    zIndex: 100,
    paddingBlock: `${INSET_TOP} 0`,
  },
  strip: {
    gridArea: "strip",
    display: {
      default: "block",
      [pageLayout.phoneMedia]: "none",
    },
    minInlineSize: 0,
    borderInlineEndColor: awbrnVars.ink800,
    borderInlineEndStyle: "solid",
    borderInlineEndWidth: borderVars["--border-width"],
    // The ring on arrival (`arrivalFocusProps`) is drawn inside the screen.
    outlineOffset: "-2px",
  },
  turnLine: {
    gridArea: "turnline",
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "block",
    },
    outlineOffset: "-2px",
    backgroundColor: awbrnVars.ink900,
  },
  phoneArmies: {
    gridArea: "armies",
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "block",
    },
    minInlineSize: 0,
  },
  table: {
    gridArea: "table",
    position: "relative",
    display: "grid",
    minBlockSize: 0,
    minInlineSize: 0,
    borderInlineEndColor: awbrnVars.ink800,
    borderInlineEndStyle: "solid",
    borderInlineEndWidth: {
      default: borderVars["--border-width"],
      [pageLayout.phoneMedia]: 0,
    },
  },
  tileRow: {
    gridArea: "tilerow",
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "block",
    },
    minInlineSize: 0,
  },
  foot: {
    gridArea: "foot",
    display: {
      default: "block",
      [pageLayout.phoneMedia]: "none",
    },
    minInlineSize: 0,
    borderInlineEndColor: awbrnVars.ink800,
    borderInlineEndStyle: "solid",
    borderInlineEndWidth: borderVars["--border-width"],
  },
  dock: {
    gridArea: "dock",
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "block",
    },
    minInlineSize: 0,
  },
  rail: {
    gridArea: "rail",
    display: {
      default: "flex",
      [pageLayout.phoneMedia]: "none",
    },
    flexDirection: "column",
    minBlockSize: 0,
    backgroundColor: awbrnVars.ink900,
  },
  railScroll: {
    display: "flex",
    flexDirection: "column",
    flexGrow: 1,
    minBlockSize: 0,
    overflowY: "auto",
    overscrollBehavior: "contain",
    scrollbarColor: `${awbrnVars.ink700} transparent`,
    scrollbarWidth: "thin",
  },
  railFoot: {
    paddingBlock: `${awbrnVars.spacing3_5} ${spacingVars["--spacing-4"]}`,
    paddingInline: spacingVars["--spacing-4"],
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
    backgroundColor: awbrnVars.ink900,
  },
  turnStrip: {
    position: "relative",
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-4"],
    blockSize: "100%",
    paddingBlock: spacingVars["--spacing-3"],
    paddingInlineStart: spacingVars["--spacing-4"],
    paddingInlineEnd: spacingVars["--spacing-5"],
    backgroundColor: awbrnVars.ink900,
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  postmarkGroup: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    flexShrink: 0,
  },
  cancel: {
    inlineSize: matchLayout.cancelWidth,
    blockSize: matchLayout.cancelHeight,
    color: awbrnVars.violetOnInk,
    opacity: 0.85,
  },
  who: {
    display: "flex",
    flexDirection: "column",
    gap: spacingVars["--spacing-0-5"],
    minInlineSize: 0,
  },
  title: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: awbrnVars.textOnInkStrong,
    fontStretch: "62%",
    fontWeight: 820,
    lineHeight: 1,
    wordSpacing: "0.12em",
  },
  stripTitle: {
    margin: 0,
    fontSize: matchType.stripTitle,
  },
  sub: {
    display: "flex",
    alignItems: "center",
    flexWrap: "wrap",
    gap: `${spacingVars["--spacing-1-5"]} ${awbrnVars.spacing2_5}`,
    minInlineSize: 0,
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.detail,
  },
  stripEnd: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    flexShrink: 0,
    marginInlineStart: "auto",
  },
  stripClock: {
    display: "inline-flex",
    marginInlineEnd: spacingVars["--spacing-2"],
  },
  clock: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
  },
  clockFigure: {
    color: awbrnVars.textOnInkStrong,
    fontSize: matchType.clock,
    fontWeight: 600,
    letterSpacing: "-0.02em",
  },
  clockLow: {
    color: awbrnVars.warnFill,
  },
  iconKey: {
    inlineSize: matchLayout.target,
    minInlineSize: matchLayout.target,
    blockSize: matchLayout.target,
    paddingInline: 0,
  },
  dot: {
    display: "inline-block",
    flexShrink: 0,
    inlineSize: matchLayout.dot,
    blockSize: matchLayout.dot,
    borderRadius: "50%",
  },
  dotLive: {
    backgroundColor: awbrnVars.okOnInk,
    boxShadow: `0 0 0 ${spacingVars["--spacing-1"]} color-mix(in srgb, ${awbrnVars.okOnInk} 18%, transparent)`,
  },
  dotWarn: {
    backgroundColor: awbrnVars.warnFill,
  },
  dotQuiet: {
    backgroundColor: awbrnVars.textOnInkSecondary,
  },
  /** Said by the dock on a phone; kept for assistive tech there. */
  connectionWord: {
    position: {
      default: "static",
      [pageLayout.phoneMedia]: "absolute",
    },
    inlineSize: {
      default: "auto",
      [pageLayout.phoneMedia]: "1px",
    },
    blockSize: {
      default: "auto",
      [pageLayout.phoneMedia]: "1px",
    },
    overflow: {
      default: "visible",
      [pageLayout.phoneMedia]: "hidden",
    },
    clipPath: {
      default: "none",
      [pageLayout.phoneMedia]: "inset(50%)",
    },
    whiteSpace: "nowrap",
  },
  connection: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1-5"],
    fontWeight: 650,
  },
  quiet: {
    color: awbrnVars.textOnInkSecondary,
  },
  warn: {
    color: awbrnVars.warnFill,
  },
  watching: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1-5"],
    blockSize: matchLayout.tagHeight,
    paddingInline: spacingVars["--spacing-2"],
    borderRadius: radiusVars["--radius-inner"],
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink600}`,
    color: awbrnVars.textOnInk,
    fontSize: matchType.label,
    fontWeight: 700,
  },
  turnLineBar: {
    display: "flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    minBlockSize: matchLayout.footHeight,
    paddingBlock: spacingVars["--spacing-1-5"],
    paddingInlineStart: spacingVars["--spacing-2"],
    paddingInlineEnd: spacingVars["--spacing-3"],
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  lineWho: {
    flexGrow: 1,
  },
  lineTitle: {
    fontSize: matchType.lineTitle,
  },
  lineTitleDim: {
    color: awbrnVars.textOnInkSecondary,
  },
  lineSub: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1-5"],
    minInlineSize: 0,
    overflow: "hidden",
    whiteSpace: "nowrap",
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.figure,
  },
  lineKeys: {
    display: "inline-flex",
    gap: spacingVars["--spacing-1-5"],
    flexShrink: 0,
  },
  notice: {
    display: "flex",
    alignItems: "center",
    flexWrap: "wrap",
    gap: awbrnVars.spacing2_5,
    paddingBlock: awbrnVars.spacing2_5,
    paddingInline: awbrnVars.spacing3_5,
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
    backgroundColor: awbrnVars.ink950,
    boxShadow: `inset 3px 0 0 ${awbrnVars.danger}`,
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.note,
    lineHeight: 1.35,
  },
  noticeInfo: {
    boxShadow: `inset 3px 0 0 ${awbrnVars.blueOnInk}`,
  },
  noticeText: {
    display: "grid",
    gap: spacingVars["--spacing-0-5"],
    flexGrow: 1,
    flexBasis: `calc(5 * ${spacingVars["--spacing-8"]})`,
    minInlineSize: 0,
  },
  noticeTitle: {
    color: awbrnVars.textOnInkStrong,
    fontSize: matchType.small,
    fontWeight: 700,
  },
  dockRow: {
    paddingBlock: awbrnVars.spacing2_5,
    paddingInline: spacingVars["--spacing-3"],
    paddingBlockEnd: `calc(${spacingVars["--spacing-3"]} + ${INSET_BOTTOM})`,
    backgroundColor: awbrnVars.ink900,
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
  },
  /** A dock key that takes the room the connection or the mover would. */
  dockFill: {
    flexGrow: 1,
    minInlineSize: 0,
    blockSize: matchLayout.dockKeyHeight,
  },
  dockKey: {
    flexShrink: 0,
    inlineSize: matchLayout.rowHeight,
    minInlineSize: matchLayout.dockKeyWidth,
    blockSize: matchLayout.dockKeyHeight,
    paddingInline: 0,
  },
  dockGlyph: {
    width: matchLayout.dockGlyph,
    height: matchLayout.dockGlyph,
  },
  waitCell: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: spacingVars["--spacing-2"],
    flexGrow: 1,
    minInlineSize: 0,
    blockSize: matchLayout.dockKeyHeight,
    paddingInline: awbrnVars.spacing3_5,
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink950,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink700}`,
    fontSize: matchType.detail,
  },
  waitName: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
    color: awbrnVars.textOnInk,
    fontWeight: 650,
  },
  waitNameText: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
});
