import { Button } from "@astryxdesign/core/Button";
import { Dialog } from "@astryxdesign/core/Dialog";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import * as stylex from "@stylexjs/stylex";
import {
  useCallback,
  useEffect,
  useEffectEvent,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { uiAtlasSpriteStyle, unitSpriteStyle } from "#/components/game_sprites.ts";
import type { ProductionOption, ProductionSite, UnitKind } from "#/wasm/awbrn_wasm.js";

/**
 * How the menu is drawn, which follows the input that opened it rather than the
 * viewport alone. A mouse gets the menu where it clicked, the way the source
 * game puts its menu under the cursor. A finger gets a sheet at the bottom
 * edge, because a list under the thumb that opened it is a list you cannot
 * read, and because a sheet is the shape a phone already uses for a choice.
 */
export type BuildMenuPresentation = "board" | "sheet";

/** The gap between the menu and the point it was opened from, and the frame. */
const BOARD_MENU_INSET = 8;
const BOARD_MENU_CURSOR_OFFSET = 12;

interface BuildMenuProps {
  /** Where the board was pressed, in surface pixels. Null when no pointer opened it. */
  anchor: { x: number; y: number } | null;
  /** Why the commands are inert, when they are. */
  disabledReason?: string;
  factionCode: string;
  funds: number | null;
  isEnabled: boolean;
  onBuild: (unit: UnitKind) => void;
  onDismiss: () => void;
  /** Hands the keyboard back to the board once the menu has closed. */
  onRestoreFocus: () => void;
  options: ProductionOption[];
  presentation: BuildMenuPresentation;
  site: ProductionSite;
}

export function BuildMenu(props: BuildMenuProps) {
  return props.presentation === "sheet" ? <BuildSheet {...props} /> : <BoardBuildMenu {...props} />;
}

/**
 * The menu drawn on the battlefield, beside the base it belongs to.
 *
 * It positions itself inside the element it is rendered into — the board frame,
 * which must be a positioned box. Living inside the board rather than over the
 * page is what makes it behave like a window the game itself opened: it travels
 * with the board, it never covers the roster, and it cannot be stranded off
 * screen.
 */
function BoardBuildMenu({
  anchor,
  disabledReason,
  factionCode,
  funds,
  isEnabled,
  onBuild,
  onDismiss,
  onRestoreFocus,
  options,
  site,
}: BuildMenuProps) {
  const menuRef = useRef<HTMLElement>(null);
  const [frame, setFrame] = useState<{ left: number; maxHeight: number; top: number } | null>(null);

  // Placement runs before paint, so the menu is never seen at the raw press
  // point and then moved.
  useLayoutEffect(() => {
    const menu = menuRef.current;
    // The menu positions itself inside the element it was rendered into, which
    // is the board frame. Reading it from the DOM rather than from a ref keeps
    // the placement correct on the very first commit, before a parent ref has
    // been attached.
    const surface = menu?.parentElement;
    if (!menu || !surface) return;

    const place = () => {
      const bounds = surface.getBoundingClientRect();
      const menuBounds = menu.getBoundingClientRect();
      setFrame(placeOnBoard(anchor, bounds, menuBounds));
    };

    place();
    const observer = new ResizeObserver(place);
    observer.observe(surface);
    observer.observe(menu);

    return () => observer.disconnect();
  }, [anchor]);

  // A press anywhere else closes the menu. Presses on the board already reach
  // the engine, which answers with its own decision about what is selected.
  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) {
        onDismiss();
      }
    };

    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [onDismiss]);

  return (
    <VStack
      aria-label={`Build at ${facilityLabel(site)}`}
      gap={0}
      role="dialog"
      // The menu is measured at its natural size on the first pass, so it is
      // held out of view until the frame it must fit inside is known.
      style={
        frame
          ? { insetBlockStart: frame.top, insetInlineStart: frame.left, maxHeight: frame.maxHeight }
          : { opacity: 0 }
      }
      ref={menuRef}
      xstyle={styles.boardMenu}
    >
      <BuildMenuBody
        disabledReason={disabledReason}
        factionCode={factionCode}
        funds={funds}
        isEnabled={isEnabled}
        onBuild={onBuild}
        onDismiss={onDismiss}
        onRestoreFocus={onRestoreFocus}
        options={options}
        site={site}
        spriteScale={1}
        takesCursor
      />
    </VStack>
  );
}

/**
 * The menu as a sheet on the bottom edge, for the hand that opened it.
 *
 * Every command sits inside thumb reach, the backdrop dims the board rather
 * than leaving two live surfaces, and the sheet keeps the phone's own dismissal
 * habits: press outside, press Cancel, or send Escape from a keyboard.
 */
function BuildSheet({
  disabledReason,
  factionCode,
  funds,
  isEnabled,
  onBuild,
  onDismiss,
  onRestoreFocus,
  options,
  site,
}: BuildMenuProps) {
  const handleOpenChange = useCallback(
    (isOpen: boolean) => {
      if (!isOpen) onDismiss();
    },
    [onDismiss],
  );

  return (
    <Dialog
      aria-label={`Build at ${facilityLabel(site)}`}
      isOpen
      maxHeight="min(72svh, 40rem)"
      onOpenChange={handleOpenChange}
      padding={0}
      position={{ bottom: 0, left: 0, right: 0 }}
      width="100%"
      xstyle={styles.sheet}
    >
      {/* The sheet itself takes the focus a modal must place somewhere. Without
          a target the browser falls to the scrolling list, which then wears a
          focus ring nobody asked for the moment the sheet opens. */}
      <VStack data-autofocus gap={0} tabIndex={-1} xstyle={styles.sheetBody}>
        <BuildMenuBody
          disabledReason={disabledReason}
          factionCode={factionCode}
          funds={funds}
          isEnabled={isEnabled}
          onBuild={onBuild}
          onDismiss={onDismiss}
          onRestoreFocus={onRestoreFocus}
          options={options}
          site={site}
          spriteScale={2}
          takesCursor={false}
        />
        <HStack gap={0} paddingBlock={3} paddingInline={3} xstyle={styles.sheetFooter}>
          <Button
            clickAction={onDismiss}
            label="Cancel"
            size="lg"
            variant="secondary"
            width="100%"
          />
        </HStack>
      </VStack>
    </Dialog>
  );
}

/**
 * The readout and the orders, shared by both presentations.
 *
 * Only the sprite scale and the row height differ between a menu read at arm's
 * length with a mouse and one pressed with a thumb; the content, the order, and
 * the wording are the same command in both.
 */
function BuildMenuBody({
  disabledReason,
  factionCode,
  funds,
  isEnabled,
  onBuild,
  onDismiss,
  onRestoreFocus,
  options,
  site,
  spriteScale,
  takesCursor,
}: {
  disabledReason?: string;
  factionCode: string;
  funds: number | null;
  isEnabled: boolean;
  onBuild: (unit: UnitKind) => void;
  onDismiss: () => void;
  onRestoreFocus: () => void;
  options: ProductionOption[];
  site: ProductionSite;
  spriteScale: 1 | 2;
  /** Whether the menu moves the cursor onto its first order when it opens. */
  takesCursor: boolean;
}) {
  const listRef = useRef<HTMLElement>(null);
  const [previewCost, setPreviewCost] = useState<number | null>(null);

  // A board menu takes the cursor when it opens, the way the game's own menu
  // does, so the first order is one key away for anyone not using a pointer. A
  // sheet does not: a finger did not ask for a selection, and a pre-lit row
  // reads as one. Either way the board is a keyboard surface of its own, so it
  // gets the cursor back when the menu closes without handing focus elsewhere.
  const restoreFocus = useEffectEvent(onRestoreFocus);
  useEffect(() => {
    if (takesCursor) {
      listRef.current
        ?.querySelector<HTMLButtonElement>("button:not(:disabled)")
        ?.focus({ preventScroll: true });
    }

    return () => {
      if (document.activeElement === null || document.activeElement === document.body) {
        restoreFocus();
      }
    };
  }, []);

  const handleKeyDown = useCallback(
    (event: KeyboardEvent<HTMLElement>) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onDismiss();
        return;
      }

      const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
      if (step === 0 && event.key !== "Home" && event.key !== "End") return;

      const buttons = Array.from(
        listRef.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [],
      );
      if (buttons.length === 0) return;

      event.preventDefault();
      const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? buttons.length - 1
            : (current + step + buttons.length) % buttons.length;
      buttons[next]?.focus();
    },
    [onDismiss],
  );

  return (
    <VStack gap={0} onKeyDown={handleKeyDown} xstyle={styles.body}>
      {/* One line: what this building is, and what the treasury holds. The
          facility is anchored to the leading edge and the readout to the
          trailing one, so the remainder appearing under the cursor grows into
          the gap between them instead of shifting either. */}
      <HStack
        align="center"
        gap={3}
        justify="between"
        paddingBlock={spriteScale === 2 ? 2 : 1}
        paddingInline={3}
        xstyle={styles.header}
      >
        <Text type="label" xstyle={styles.facility}>
          {facilityLabel(site)}
        </Text>
        <FundsLine funds={funds} previewCost={previewCost} />
      </HStack>

      {options.length === 0 ? (
        <VStack gap={0} paddingBlock={3} paddingInline={3}>
          <Text color="secondary" type="supporting" xstyle={styles.empty}>
            Nothing can be built here under this match's rules.
          </Text>
        </VStack>
      ) : (
        <VStack gap={0} ref={listRef} xstyle={styles.list}>
          {options.map((option) => (
            <BuildRow
              factionCode={factionCode}
              isEnabled={isEnabled}
              key={option.unit}
              onBuild={onBuild}
              onPreview={setPreviewCost}
              option={option}
              spriteScale={spriteScale}
              title={disabledReason}
            />
          ))}
        </VStack>
      )}

      {isEnabled || !disabledReason ? null : (
        <VStack gap={0} paddingBlock={2} paddingInline={3} xstyle={styles.notice}>
          <Text color="secondary" type="supporting">
            {disabledReason}
          </Text>
        </VStack>
      )}
    </VStack>
  );
}

/**
 * The treasury, and what the order under the cursor would leave of it.
 *
 * The remainder is the number a player is actually doing in their head while
 * they read the list, so the menu does the subtraction as the cursor moves.
 */
function FundsLine({ funds, previewCost }: { funds: number | null; previewCost: number | null }) {
  const remaining = funds !== null && previewCost !== null ? funds - previewCost : null;
  const coinStyle = uiAtlasSpriteStyle("Coin.png", 2);

  return (
    <HStack align="center" gap={2} xstyle={styles.fundsLine}>
      <span aria-hidden="true" style={coinStyle ?? undefined} {...stylex.props(styles.coin)} />
      <Text type="label" xstyle={styles.funds}>
        <VisuallyHidden>Funds</VisuallyHidden>
        {funds === null ? "--" : funds.toLocaleString()}
      </Text>
      {remaining === null ? null : (
        <Text type="label" xstyle={styles.fundsAfter}>
          <VisuallyHidden>, leaving</VisuallyHidden>
          <span aria-hidden="true">→ </span>
          {remaining.toLocaleString()}
        </Text>
      )}
    </HStack>
  );
}

/**
 * One order: the unit as the army will build it, its name, and its price.
 *
 * The sprite is the army's own, so the list reads as this player's roster
 * rather than as a catalogue, and the row under the cursor wears the game's
 * orange selection the way a chosen menu key does everywhere else.
 */
function BuildRow({
  factionCode,
  isEnabled,
  onBuild,
  onPreview,
  option,
  spriteScale,
  title,
}: {
  factionCode: string;
  isEnabled: boolean;
  onBuild: (unit: UnitKind) => void;
  onPreview: (cost: number | null) => void;
  option: ProductionOption;
  spriteScale: 1 | 2;
  title?: string;
}) {
  const spriteStyle = unitSpriteStyle(option.unit, factionCode, spriteScale);
  const isOrderable = isEnabled && option.affordable;

  return (
    <button
      disabled={!isOrderable}
      aria-label={
        option.affordable
          ? undefined
          : `${option.name}, ${option.cost.toLocaleString()}, more than your funds`
      }
      onBlur={() => onPreview(null)}
      onClick={() => onBuild(option.unit)}
      onFocus={() => onPreview(option.cost)}
      // The cursor follows the pointer rather than doubling it: entering a row
      // moves the one cursor there, so hover and keyboard never light two rows.
      onPointerEnter={(event) => event.currentTarget.focus({ preventScroll: true })}
      title={isEnabled ? undefined : title}
      type="button"
      {...stylex.props(
        styles.row,
        spriteScale === 2 && styles.rowSpacious,
        !isEnabled && styles.rowInert,
        !option.affordable && styles.rowUnaffordable,
      )}
    >
      <span aria-hidden="true" style={spriteStyle ?? undefined} {...stylex.props(styles.sprite)} />
      <span {...stylex.props(styles.rowName, !option.affordable && styles.rowStruck)}>
        {option.name}
      </span>
      <span {...stylex.props(styles.rowCost, !option.affordable && styles.rowStruck)}>
        <VisuallyHidden>costs</VisuallyHidden>
        {option.cost.toLocaleString()}
      </span>
    </button>
  );
}

/** Where the menu sits so that it clears the press and stays inside the board. */
function placeOnBoard(
  anchor: { x: number; y: number } | null,
  surface: DOMRect,
  menu: DOMRect,
): { left: number; maxHeight: number; top: number } {
  const maxHeight = Math.max(surface.height - BOARD_MENU_INSET * 2, 0);
  const height = Math.min(menu.height, maxHeight);
  const limitLeft = Math.max(surface.width - menu.width - BOARD_MENU_INSET, BOARD_MENU_INSET);
  const limitTop = Math.max(surface.height - height - BOARD_MENU_INSET, BOARD_MENU_INSET);

  if (!anchor) {
    return {
      left: Math.max((surface.width - menu.width) / 2, BOARD_MENU_INSET),
      maxHeight,
      top: Math.max((surface.height - height) / 2, BOARD_MENU_INSET),
    };
  }

  // The menu opens away from the press, and folds back across it only when the
  // board runs out on that side, so the tile stays visible either way.
  const preferredLeft = anchor.x + BOARD_MENU_CURSOR_OFFSET;
  const left =
    preferredLeft > limitLeft
      ? Math.max(anchor.x - BOARD_MENU_CURSOR_OFFSET - menu.width, BOARD_MENU_INSET)
      : Math.max(preferredLeft, BOARD_MENU_INSET);
  const preferredTop = anchor.y - height / 2;

  return {
    left,
    maxHeight,
    top: Math.min(Math.max(preferredTop, BOARD_MENU_INSET), limitTop),
  };
}

/** "Base", "Airport", "Port" — the facility as the game names it. */
function facilityLabel(site: ProductionSite): string {
  return site.facility
    .split("-")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

const styles = stylex.create({
  // The menu is a window the board opened: the standard panel, lifted to the
  // level reserved for content that overlays other content.
  boardMenu: {
    position: "absolute",
    display: "flex",
    margin: 0,
    padding: 0,
    inlineSize: "var(--size-build-menu)",
    maxInlineSize: `calc(100% - ${BOARD_MENU_INSET * 2}px)`,
    borderWidth: "var(--border-width)",
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: "var(--radius-container)",
    backgroundColor: "var(--color-background-surface)",
    boxShadow: "var(--shadow-high)",
    color: "var(--color-text-primary)",
    zIndex: 2,
    overflow: "hidden",
  },
  sheet: {
    // A sheet takes the whole bottom edge; the stock dialog width cap would
    // leave it floating in the middle of the screen instead.
    maxWidth: "100%",
    borderRadius: "var(--radius-container) var(--radius-container) 0 0",
    borderBlockEndWidth: 0,
    // The board behind a sheet is dimmed, not defocused. A blurred backdrop is
    // the one soft edge this system does not have anywhere else.
    "::backdrop": { backdropFilter: "none" },
  },
  sheetBody: {
    minBlockSize: 0,
    outline: "none",
    // The sheet ends at the bottom edge of the device, not at the bottom edge
    // of the screen.
    paddingBlockEnd: "env(safe-area-inset-bottom)",
  },
  sheetFooter: {
    borderBlockStartWidth: "var(--border-width)",
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border-soft)",
  },
  body: {
    minBlockSize: 0,
  },
  // The readout strip: which building this is, and what the army can spend.
  header: {
    borderBlockEndWidth: "var(--border-width)",
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--color-border-emphasized)",
    backgroundColor: "var(--color-background-muted)",
  },
  facility: {
    color: "var(--color-text-secondary)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  fundsLine: {
    flex: "0 0 auto",
    minBlockSize: "var(--size-build-funds-line)",
  },
  coin: {
    display: "block",
    flex: "0 0 auto",
  },
  funds: {
    color: "var(--color-text-primary)",
    fontVariantNumeric: "tabular-nums",
  },
  fundsAfter: {
    color: "var(--color-text-accent)",
    fontVariantNumeric: "tabular-nums",
  },
  list: {
    minBlockSize: 0,
    overflowY: "auto",
    overscrollBehavior: "contain",
  },
  // The order itself. Rows run edge to edge and divide with the same soft rule
  // the roster uses; the row under the cursor wears the orange selection.
  row: {
    display: "flex",
    alignItems: "center",
    gap: "var(--spacing-2)",
    inlineSize: "100%",
    minBlockSize: "var(--size-build-row)",
    paddingBlock: "var(--spacing-1)",
    paddingInline: "var(--spacing-3)",
    margin: 0,
    borderWidth: 0,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--color-border-soft)",
    borderBlockEndWidth: { default: "var(--border-width)", ":last-child": 0 },
    // The cursor: the same orange fill a chosen tab wears, flush on the panel
    // rather than raised above it.
    backgroundColor: { default: "transparent", ":focus": "var(--color-accent)" },
    color: "var(--color-text-primary)",
    cursor: "pointer",
    outline: "none",
    textAlign: "start",
  },
  rowSpacious: {
    minBlockSize: "var(--size-build-row-spacious)",
    paddingBlock: "var(--spacing-1-5)",
  },
  // A command that cannot be sent stays legible and stays a key on the menu;
  // it simply refuses the cursor.
  rowInert: {
    backgroundColor: "transparent",
    cursor: "not-allowed",
    opacity: 0.45,
  },
  // A price beyond the treasury is struck through rather than hidden, which is
  // how the source game says it and how a player reads what one more turn of
  // income would buy. The sprite keeps its colors: the unit is still the thing
  // being identified, and only the order is unavailable.
  rowUnaffordable: {
    cursor: "not-allowed",
    color: "var(--color-text-secondary)",
  },
  rowStruck: {
    textDecorationLine: "line-through",
    textDecorationThickness: "var(--border-width)",
  },
  sprite: {
    display: "block",
    flex: "0 0 auto",
  },
  rowName: {
    flex: "1 1 auto",
    minInlineSize: 0,
    fontFamily: "var(--font-family-code)",
    fontSize: "var(--font-size-sm)",
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  rowCost: {
    flex: "0 0 auto",
    fontFamily: "var(--font-family-code)",
    fontSize: "var(--font-size-sm)",
    letterSpacing: "0.06em",
    fontVariantNumeric: "tabular-nums",
  },
  empty: {
    // An empty state inside a panel is a recessed well, never a second outline.
    padding: "var(--spacing-3)",
    borderWidth: "var(--border-width)",
    borderStyle: "dashed",
    borderColor: "var(--color-border-disabled)",
    borderRadius: "var(--radius-element)",
    backgroundColor: "var(--color-background-muted)",
  },
  notice: {
    borderBlockStartWidth: "var(--border-width)",
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border-soft)",
    backgroundColor: "var(--color-background-muted)",
  },
});
