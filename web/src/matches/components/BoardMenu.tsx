import { spacingDefaults } from "@astryxdesign/core";
import { Button } from "#/ui/Button.tsx";
import { Dialog } from "@astryxdesign/core/Dialog";
import { VStack } from "@astryxdesign/core/Stack";
import {
  borderVars,
  colorVars,
  radiusVars,
  shadowVars,
  spacingVars,
  textSizeVars,
  typographyVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import {
  useCallback,
  useEffect,
  useEffectEvent,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { boardMenuLayout } from "./boardMenuLayout.stylex.ts";

/**
 * How a menu the board opened is drawn, which follows the input that opened it
 * rather than the viewport alone. A mouse gets the menu where it clicked, the
 * way the source game puts its menu under the cursor. A finger gets a sheet at
 * the bottom edge, because a list under the thumb that opened it is a list you
 * cannot read, and because a sheet is the shape a phone already uses for a
 * choice.
 */
export type BoardMenuPresentation = "board" | "sheet";

/** The gap between the menu and the point it was opened from, and the frame. */
const BOARD_MENU_INSET = Number.parseFloat(spacingDefaults["--spacing-2"]);
const BOARD_MENU_CURSOR_OFFSET = Number.parseFloat(spacingDefaults["--spacing-3"]);

export interface BoardMenuShellProps {
  /** Where the board was pressed, in surface pixels. Null when no pointer opened it. */
  anchor: { x: number; y: number } | null;
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
   * menu read at arm's length with a mouse and one pressed with a thumb, and
   * `takesCursor` says whether the first order should be lit when it opens.
   */
  children: (context: { spriteScale: 1 | 2; takesCursor: boolean }) => ReactNode;
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
  return props.presentation === "sheet" ? (
    <MenuSheet {...props} />
  ) : (
    <BoardAnchoredMenu {...props} />
  );
}

/**
 * The menu drawn on the battlefield, beside the tile it belongs to.
 *
 * It positions itself inside the element it is rendered into — the board frame,
 * which must be a positioned box. Living inside the board rather than over the
 * page is what makes it behave like a window the game itself opened: it travels
 * with the board, it never covers the roster, and it cannot be stranded off
 * screen.
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
      const next = placeOnBoard(anchor, bounds, menuBounds);
      // The observer watches the menu, and the frame decides the menu's size,
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
      aria-label={label}
      gap={0}
      role="dialog"
      // The menu is measured at its natural size on the first pass, so it is
      // held out of view until the frame it must fit inside is known.
      style={{
        ...(inlineSize ? { inlineSize } : {}),
        ...(frame
          ? { insetBlockStart: frame.top, insetInlineStart: frame.left, maxHeight: frame.maxHeight }
          : { opacity: 0 }),
      }}
      ref={menuRef}
      xstyle={styles.boardMenu}
    >
      <MenuKeyboardScope onDismiss={onDismiss} onRestoreFocus={onRestoreFocus} takesCursor>
        {children({ spriteScale: 1, takesCursor: true })}
      </MenuKeyboardScope>
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
function MenuSheet({ children, footer, label, onDismiss, onRestoreFocus }: BoardMenuShellProps) {
  const handleOpenChange = useCallback(
    (isOpen: boolean) => {
      if (!isOpen) onDismiss();
    },
    [onDismiss],
  );

  return (
    <Dialog
      aria-label={label}
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
      <MenuKeyboardScope
        footer={
          footer ?? (
            <VStack gap={0} paddingBlock={3} paddingInline={3} xstyle={styles.sheetFooter}>
              <Button
                clickAction={onDismiss}
                label="Cancel"
                size="lg"
                variant="secondary"
                width="100%"
              />
            </VStack>
          )
        }
        onDismiss={onDismiss}
        onRestoreFocus={onRestoreFocus}
        takesCursor={false}
      >
        {children({ spriteScale: 2, takesCursor: false })}
      </MenuKeyboardScope>
    </Dialog>
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

export const boardMenuStyles = stylex.create({
  body: {
    minBlockSize: 0,
  },
  // The readout strip at the head of a menu.
  header: {
    borderBlockEndWidth: borderVars["--border-width"],
    borderBlockEndStyle: "solid",
    borderBlockEndColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-muted"],
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
    gap: spacingVars["--spacing-2"],
    inlineSize: "100%",
    minBlockSize: boardMenuLayout.buildRowMinBlockSize,
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-3"],
    margin: 0,
    borderWidth: 0,
    borderBlockEndStyle: "solid",
    borderBlockEndColor: awbrnVars.colorBorderSoft,
    borderBlockEndWidth: { default: borderVars["--border-width"], ":last-child": 0 },
    // The cursor: the same orange fill a chosen tab wears, flush on the panel
    // rather than raised above it.
    backgroundColor: { default: "transparent", ":focus": colorVars["--color-accent"] },
    color: colorVars["--color-text-primary"],
    cursor: "pointer",
    outline: "none",
    textAlign: "start",
  },
  rowSpacious: {
    minBlockSize: boardMenuLayout.buildRowSpaciousMinBlockSize,
    paddingBlock: spacingVars["--spacing-1-5"],
  },
  // A command that cannot be sent stays legible and stays a key on the menu; it
  // simply refuses the cursor.
  rowInert: {
    backgroundColor: "transparent",
    cursor: "not-allowed",
    opacity: 0.45,
  },
  sprite: {
    display: "block",
    flex: "0 0 auto",
  },
  rowName: {
    flex: "1 1 auto",
    minInlineSize: 0,
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  notice: {
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: awbrnVars.colorBorderSoft,
    backgroundColor: colorVars["--color-background-muted"],
  },
});

const styles = stylex.create({
  scope: {
    minBlockSize: 0,
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
  sheet: {
    // A sheet takes the whole bottom edge; the stock dialog width cap would
    // leave it floating in the middle of the screen instead.
    maxWidth: "100%",
    borderRadius: boardMenuLayout.sheetBorderRadius,
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
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: awbrnVars.colorBorderSoft,
  },
});
