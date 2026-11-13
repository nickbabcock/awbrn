import { spacingDefaults } from "@astryxdesign/core";
import { Button } from "#/ui/Button.tsx";
import { overlayMotion } from "#/ui/overlayMotion.ts";
import { BottomSheet } from "@astryxdesign/core/BottomSheet";
import { Icon } from "@astryxdesign/core/Icon";
import { VStack } from "@astryxdesign/core/Stack";
import {
  borderVars,
  colorVars,
  radiusVars,
  shadowVars,
  spacingVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { StyleXStyles } from "@stylexjs/stylex";
import type { PointerEvent as ReactPointerEvent } from "react";
import {
  useCallback,
  useEffect,
  useEffectEvent,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { IconClose } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { boardMenuLayout, menuRowMarker } from "./boardMenuLayout.stylex.ts";

export { menuRowMarker };

/**
 * How a menu the board opened is drawn, which follows the input that opened it
 * rather than the viewport alone. A mouse gets the menu where it clicked, the
 * way the source game puts its menu under the cursor. A finger gets a sheet at
 * the bottom edge, because a list under the thumb that opened it is a list you
 * cannot read, and because a sheet is the shape a phone already uses for a
 * choice.
 */
export type BoardMenuPresentation = "board" | "sheet" | "docked";

/** The gap between the menu and the point it was opened from, and the frame. */
const BOARD_MENU_INSET = Number.parseFloat(spacingDefaults["--spacing-2"]);
const BOARD_MENU_CURSOR_OFFSET = Number.parseFloat(spacingDefaults["--spacing-3"]);

export interface BoardMenuShellProps {
  /** Where the board was pressed, in surface pixels. Null when no pointer opened it. */
  anchor: { x: number; y: number } | null;
  /**
   * The sheet only: false while it leaves, so its exit plays. The parent keeps
   * it mounted that long (`useLeaving`). The other shapes cut and ignore it.
   */
  isOpen?: boolean;
  /** Names the menu for assistive technology. */
  label: string;
  onDismiss: () => void;
  /** Hands the keyboard back to the board once the menu has closed. */
  onRestoreFocus: () => void;
  presentation: BoardMenuPresentation;
  /**
   * What the sheet offers at the bottom edge, when the default way out is not
   * the only command that belongs in thumb reach. A menu that asks a question
   * answers it here, because a second Cancel below the answers is the sheet
   * disagreeing with itself. The node carries its own padding and rules: a
   * footer that follows an empty body needs no divider of its own.
   */
  footer?: ReactNode;
  /**
   * How wide the board-anchored menu is drawn. It sits over the tile the player
   * is deciding about, so each menu asks for the width its own content needs
   * rather than sharing one.
   */
  inlineSize?: string;
  /**
   * The menu's contents. `spriteScale` is the only thing that differs between a
   * menu read at arm's length with a mouse and one pressed with a thumb.
   * `takesCursor` says whether the first order should be lit when it opens, and
   * `isSheet` says whether Astryx owns the scroll area.
   */
  children: (context: { isSheet: boolean; spriteScale: 1 | 2; takesCursor: boolean }) => ReactNode;
}

/**
 * The shell every menu the board opens shares: where it sits, how it is
 * dismissed, and how it is walked with a keyboard.
 *
 * Both the production menu and the destination menu are the same object in the
 * source game — a small window the board opened, over the tile it belongs to —
 * so they are one component here, differing only in what they list.
 */
export function BoardMenuShell(props: BoardMenuShellProps) {
  if (props.presentation === "docked") return <DockedMenu {...props} />;
  return props.presentation === "sheet" ? (
    <MenuSheet {...props} />
  ) : (
    <BoardAnchoredMenu {...props} />
  );
}

/**
 * The menu docked in the table's column beside the board: a letter on the
 * table, paper with an ink head. It never stands over a tile, so the board
 * stays tappable for another target; the engine answers presses on the board.
 * It lands with the selection in two frames (90ms, stepped); the page keys it
 * by the selection so each new one lands.
 */
function DockedMenu({ children, footer, label, onDismiss, onRestoreFocus }: BoardMenuShellProps) {
  return (
    <VStack aria-label={label} gap={0} role="dialog" xstyle={styles.dockedMenu}>
      <MenuKeyboardScope onDismiss={onDismiss} onRestoreFocus={onRestoreFocus} takesCursor>
        {children({ isSheet: false, spriteScale: 1, takesCursor: true })}
      </MenuKeyboardScope>
      {footer}
    </VStack>
  );
}

/**
 * The menu drawn on the battlefield, beside the tile it belongs to.
 *
 * It is a board panel that takes a keyboard and a press: the frame and the
 * placement are the panel's, and what is added here is the way out of it.
 */
function BoardAnchoredMenu({
  anchor,
  children,
  inlineSize,
  label,
  onDismiss,
  onRestoreFocus,
}: BoardMenuShellProps) {
  const menuRef = useRef<HTMLElement>(null);

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
    <BoardAnchoredPanel
      anchor={anchor}
      inlineSize={inlineSize}
      label={label}
      ref={menuRef}
      role="dialog"
    >
      <MenuKeyboardScope onDismiss={onDismiss} onRestoreFocus={onRestoreFocus} takesCursor>
        {children({ isSheet: false, spriteScale: 1, takesCursor: true })}
      </MenuKeyboardScope>
    </BoardAnchoredPanel>
  );
}

export interface BoardAnchoredPanelProps {
  /** Where on the board the panel belongs, in surface pixels. */
  anchor: { x: number; y: number } | null;
  children: ReactNode;
  inlineSize?: string;
  /** Names the panel for assistive technology. */
  label?: string;
  ref?: RefObject<HTMLElement | null>;
  /** What the panel is. A panel that only reports something is nothing. */
  role?: string;
  xstyle?: StyleXStyles;
}

/**
 * The window the board opens beside a tile: the frame, and where it sits.
 *
 * It positions itself inside the element it is rendered into — the board frame,
 * which must be a positioned box. Living inside the board rather than over the
 * page is what makes it behave like a window the game itself opened: it travels
 * with the board, it never covers the roster, and it cannot be stranded off
 * screen.
 *
 * The menu is one thing drawn this way and the attack forecast is another, so
 * the frame is its own component. What is inside it decides whether it takes a
 * keyboard, a press, or neither.
 */
export function BoardAnchoredPanel({
  anchor,
  children,
  inlineSize,
  label,
  ref,
  role,
  xstyle,
}: BoardAnchoredPanelProps) {
  const fallbackRef = useRef<HTMLElement>(null);
  const panelRef = ref ?? fallbackRef;
  const [frame, setFrame] = useState<{ left: number; maxHeight: number; top: number } | null>(null);

  // Placement runs before paint, so the panel is never seen at the raw press
  // point and then moved.
  useLayoutEffect(() => {
    const panel = panelRef.current;
    // The panel positions itself inside the element it was rendered into, which
    // is the board frame. Reading it from the DOM rather than from a ref keeps
    // the placement correct on the very first commit, before a parent ref has
    // been attached.
    const surface = panel?.parentElement;
    if (!panel || !surface) return;

    const place = () => {
      const bounds = surface.getBoundingClientRect();
      const panelBounds = panel.getBoundingClientRect();
      const next = placeOnBoard(anchor, bounds, panelBounds);
      // The observer watches the panel, and the frame decides the panel's size,
      // so a placement that lands where the last one did must return the same
      // object or the two feed each other a re-render per tick.
      setFrame((previous) =>
        previous &&
        previous.left === next.left &&
        previous.top === next.top &&
        previous.maxHeight === next.maxHeight
          ? previous
          : next,
      );
    };

    place();
    const observer = new ResizeObserver(place);
    observer.observe(surface);
    observer.observe(panel);

    return () => observer.disconnect();
  }, [anchor, panelRef]);

  return (
    <VStack
      aria-label={label}
      gap={0}
      role={role}
      // The panel is measured at its natural size on the first pass, so it is
      // held out of view until the frame it must fit inside is known.
      ref={panelRef}
      xstyle={[
        styles.boardMenu,
        xstyle,
        inlineSize ? styles.panelInlineSize(inlineSize) : null,
        frame ? styles.panelFrame(frame.top, frame.left, frame.maxHeight) : styles.panelUnplaced,
      ]}
    >
      {children}
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
function MenuSheet({
  children,
  footer,
  isOpen = true,
  label,
  onDismiss,
  onRestoreFocus,
}: BoardMenuShellProps) {
  const handleOpenChange = useCallback(
    (next: boolean) => {
      if (!next) onDismiss();
    },
    [onDismiss],
  );

  return (
    <BottomSheet
      height="capped"
      isOpen={isOpen}
      label={label}
      onOpenChange={handleOpenChange}
      purpose="info"
      xstyle={[styles.sheet, overlayMotion.sheet, !isOpen && overlayMotion.sheetLeaving]}
      padding={0}
    >
      {/* The sheet itself takes the focus a modal must place somewhere. Without
          a target the browser falls to the scrolling list, which then wears a
          focus ring nobody asked for the moment the sheet opens. */}
      <Button
        icon={<Icon icon={IconClose} />}
        isIconOnly
        label="Close"
        onClick={onDismiss}
        variant="ghost-on-ink"
        xstyle={styles.sheetClose}
      />
      <MenuKeyboardScope
        footer={footer}
        onDismiss={onDismiss}
        onRestoreFocus={onRestoreFocus}
        takesCursor={false}
      >
        {children({ isSheet: true, spriteScale: 2, takesCursor: false })}
      </MenuKeyboardScope>
    </BottomSheet>
  );
}

/**
 * Walking the orders, leaving them, and giving the board its keyboard back.
 *
 * A board menu takes the cursor when it opens, the way the game's own menu
 * does, so the first order is one key away for anyone not using a pointer. A
 * sheet does not: a finger did not ask for a selection, and a pre-lit row reads
 * as one. Either way the board is a keyboard surface of its own, so it gets the
 * cursor back when the menu closes without handing focus elsewhere.
 */
function MenuKeyboardScope({
  children,
  footer,
  onDismiss,
  onRestoreFocus,
  takesCursor,
}: {
  children: ReactNode;
  footer?: ReactNode;
  onDismiss: () => void;
  onRestoreFocus: () => void;
  takesCursor: boolean;
}) {
  const listRef = useRef<HTMLElement>(null);

  const restoreFocus = useEffectEvent(onRestoreFocus);
  useEffect(() => {
    if (takesCursor) {
      focusPreferredOrder(listRef.current);
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
            : current === -1
              ? step > 0
                ? 0
                : buttons.length - 1
              : (current + step + buttons.length) % buttons.length;
      buttons[next]?.focus();
    },
    [onDismiss],
  );

  if (!takesCursor) {
    return (
      <VStack
        data-autofocus
        gap={0}
        onKeyDown={handleKeyDown}
        tabIndex={-1}
        xstyle={styles.sheetBody}
      >
        <VStack gap={0} ref={listRef} xstyle={styles.scope}>
          {children}
        </VStack>
        {footer}
      </VStack>
    );
  }

  return (
    <VStack gap={0} onKeyDown={handleKeyDown} ref={listRef} xstyle={styles.scope}>
      {children}
    </VStack>
  );
}

/**
 * Light the order the menu opened on.
 *
 * A menu may nominate one — a drag released on an enemy opens on Fire, because
 * the player has already said what they meant. Otherwise the first order takes
 * the cursor.
 */
function focusPreferredOrder(root: HTMLElement | null) {
  const preferred = root?.querySelector<HTMLButtonElement>(
    "button[data-preselected]:not(:disabled)",
  );
  (preferred ?? root?.querySelector<HTMLButtonElement>("button:not(:disabled)"))?.focus({
    preventScroll: true,
  });
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

/**
 * Move the menu's one cursor to the row under the pointer.
 *
 * Bound to pointer movement rather than to entering the row. A menu opens
 * wherever the board says it belongs, which is under whatever the mouse was
 * last left on, and `pointerenter` fires for a row that merely appears beneath
 * a parked pointer. That took the cursor off the order the menu opened on and
 * put it on whichever row happened to land under the mouse — on a menu pushed
 * up off the bottom edge, often the last one, which is Delete. A player who had
 * not touched the mouse then had Enter aimed at deleting the unit.
 *
 * Movement is what a player using the pointer does to reach a row, so nothing
 * is lost by asking for it.
 */
export function followPointerCursor(event: ReactPointerEvent<HTMLButtonElement>) {
  if (document.activeElement === event.currentTarget) return;
  event.currentTarget.focus({ preventScroll: true });
}

export const boardMenuStyles = stylex.create({
  body: {
    minBlockSize: 0,
  },
  // The ink head of a letter on the table: what the menu is for, and a figure.
  header: {
    minBlockSize: "44px",
    paddingBlock: awbrnVars.spacing2_5,
    paddingInlineStart: awbrnVars.spacing3_5,
    paddingInlineEnd: awbrnVars.spacing3_5,
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInkStrong,
    "--color-text-primary": awbrnVars.textOnInkStrong,
    "--color-text-secondary": awbrnVars.textOnInkSecondary,
  },
  list: {
    minBlockSize: 0,
    overflowY: "auto",
    overscrollBehavior: "contain",
  },
  // One order: 48px (56px under a thumb), a rule above, the red wash under
  // the cursor.
  row: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    inlineSize: "100%",
    minBlockSize: "48px",
    paddingBlock: spacingVars["--spacing-1-5"],
    paddingInlineStart: awbrnVars.spacing2_5,
    paddingInlineEnd: awbrnVars.spacing3_5,
    margin: 0,
    borderWidth: 0,
    borderBlockStartStyle: "solid",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartWidth: { default: "1px", ":first-child": 0 },
    backgroundColor: {
      default: awbrnVars.sheet,
      // Under a pointer that hovers only: a tap in the sheet never leaves a
      // row lit as the cursor, which the sheet opens without.
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.redWash },
      ":focus": awbrnVars.redWash,
      ":active": awbrnVars.redWash,
    },
    color: colorVars["--color-text-primary"],
    fontFamily: "inherit",
    fontSize: awbrnVars.text15_5,
    fontWeight: 650,
    cursor: "pointer",
    outline: "none",
    textAlign: "start",
    transitionDuration: {
      default: "90ms",
      [pageLayout.reducedMotionMedia]: "0s",
    },
    transitionProperty: "background-color",
    transitionTimingFunction: awbrnVars.easeStep,
  },
  rowSpacious: {
    minBlockSize: "56px",
    fontSize: awbrnVars.text16,
  },
  // A command that cannot be sent stays legible and stays a key on the menu; it
  // simply refuses the cursor.
  rowInert: {
    backgroundColor: awbrnVars.sheet,
    cursor: "not-allowed",
    color: awbrnVars.textTertiary,
  },
  // The red caret on the row under the cursor.
  caret: {
    flexShrink: 0,
    inlineSize: "16px",
    blockSize: "16px",
    color: awbrnVars.red,
    opacity: {
      default: 0,
      [stylex.when.ancestor(":focus", menuRowMarker)]: 1,
      [pageLayout.hoverMedia]: {
        default: 0,
        [stylex.when.ancestor(":hover", menuRowMarker)]: 1,
      },
    },
  },
  sprite: {
    display: "block",
    flex: "0 0 auto",
  },
  rowName: {
    flex: "1 1 auto",
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  notice: {
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: awbrnVars.rule,
    backgroundColor: awbrnVars.paper,
  },
});

/** The red caret a chosen order wears. */
export function MenuCaret() {
  return (
    <svg
      aria-hidden="true"
      fill="currentColor"
      viewBox="0 0 16 16"
      {...stylex.props(boardMenuStyles.caret)}
    >
      <path d="M5 3l6 5-6 5z" />
    </svg>
  );
}

/** Two visible frames: halfway, then there. */
const land = stylex.keyframes({
  from: { opacity: 0 },
  to: { opacity: 1 },
});

const styles = stylex.create({
  scope: {
    minBlockSize: 0,
  },
  panelInlineSize: (inlineSize: string) => ({ inlineSize }),
  panelFrame: (top: number, left: number, maxHeight: number) => ({
    insetBlockStart: `${top}px`,
    insetInlineStart: `${left}px`,
    maxBlockSize: `${maxHeight}px`,
  }),
  panelUnplaced: {
    opacity: 0,
  },
  // The menu is a window the board opened: the standard panel, lifted to the
  // level reserved for content that overlays other content.
  boardMenu: {
    position: "absolute",
    display: "flex",
    margin: 0,
    padding: 0,
    inlineSize: boardMenuLayout.buildInlineSize,
    maxInlineSize: boardMenuLayout.buildMaxInlineSize,
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-container"],
    backgroundColor: colorVars["--color-background-surface"],
    boxShadow: shadowVars["--shadow-high"],
    color: colorVars["--color-text-primary"],
    zIndex: 2,
    overflow: "hidden",
  },
  dockedMenu: {
    animationName: {
      default: land,
      [pageLayout.reducedMotionMedia]: "none",
    },
    animationDuration: "90ms",
    animationTimingFunction: awbrnVars.easeStep,
    flexShrink: 1,
    minBlockSize: "120px",
    inlineSize: "100%",
    maxInlineSize: "296px",
    overflow: "hidden",
    borderRadius: radiusVars["--radius-container"],
    backgroundColor: awbrnVars.sheet,
    boxShadow: awbrnVars.shadowSheet,
    color: colorVars["--color-text-primary"],
  },
  sheetClose: {
    position: "absolute",
    insetBlockStart: "calc(var(--spacing-6) + 4px)",
    insetInlineEnd: "6px",
    zIndex: 2,
    inlineSize: "44px",
    minInlineSize: "44px",
    blockSize: "44px",
    paddingInline: 0,
    color: awbrnVars.textOnInkStrong,
  },
  sheet: {
    // A sheet takes the whole bottom edge; the stock dialog width cap would
    // leave it floating in the middle of the screen instead.
    maxWidth: "100%",
    backgroundColor: awbrnVars.ink900,
    borderRadius: boardMenuLayout.sheetBorderRadius,
    borderBlockEndWidth: 0,
  },
  sheetBody: {
    minBlockSize: 0,
    outline: "none",
    // The BottomSheet handle is out of flow. Reserve its space above the menu.
    paddingBlockStart: spacingVars["--spacing-6"],
  },
  sheetFooter: {
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: awbrnVars.colorBorderSoft,
  },
});
