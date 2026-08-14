import { inputWrapperStyles } from "@astryxdesign/core/Field";
import { Popover } from "@astryxdesign/core/Popover";
import {
  borderVars,
  colorVars,
  radiusVars,
  spacingVars,
  textSizeVars,
  typographyVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { ChevronDown as ChevronDownIcon } from "pixelarticons/react/ChevronDown";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { spritePickerLayout } from "./spritePickerLayout.stylex.ts";

/**
 * A choice made from the art the game already draws.
 *
 * A unit, a commander and a tile are things a player recognises before they
 * read: an Advance Wars player knows the Mega Tank silhouette and the mountain
 * cell without either being named, and has known them since long before they
 * opened this panel. A dropdown throws that away and asks them to read
 * twenty-five names in one column instead, which is the slowest way to answer
 * a question they could have answered by looking.
 *
 * So the choice is a grid of the real sprites, in the army's own colours, the
 * way the game's own build menu asks it. The names stay under the art rather
 * than being replaced by it, because a grid of unlabelled sprites is a memory
 * test, and the figure that decides the choice — what a unit costs, what a
 * tile shelters — is on the cell beside them.
 */

export interface SpritePickerOption {
  value: string;
  /** What the thing is called. Also what typing jumps to. */
  label: string;
  /**
   * The one figure that decides between two options a player is torn over: a
   * unit's cost, a tile's defense. Absent where there is no such figure.
   */
  detail?: string;
  art: ReactNode;
  /** The heading this option files under, for a grid worth dividing. */
  group?: string;
}

interface SpritePickerProps {
  /** The accessible name of the choice, e.g. "Target unit". */
  label: string;
  onChange: (value: string) => void;
  options: SpritePickerOption[];
  /** How wide one cell is, which is set by the art and the longest name. */
  shape: keyof typeof cellStyles;
  /** What the trigger shows: the art of the current choice, and its name. */
  triggerArt: ReactNode;
  /**
   * What to call the current value when the grid does not offer it.
   *
   * A panel seeded from a real board can hold a state the player is not being
   * asked to choose. The trigger reports it rather than showing the raw value
   * the engine keyed it by.
   */
  triggerLabel?: string;
  /** How wide the closed trigger is, so a row of them lines up down a column. */
  triggerXstyle?: stylex.StyleXStyles;
  value: string;
}

/** How long a typed run keeps counting as one word. */
const TYPEAHEAD_WINDOW_MS = 600;

export function SpritePicker({
  label,
  onChange,
  options,
  shape,
  triggerArt,
  triggerLabel,
  triggerXstyle,
  value,
}: SpritePickerProps) {
  const [isOpen, setIsOpen] = useState(false);
  const name = triggerLabel ?? options.find((option) => option.value === value)?.label ?? value;

  return (
    <Popover
      alignment="start"
      // Built when it opens, not held hidden behind a closed trigger. The grid
      // puts the keyboard on the current choice as it mounts, and a grid that
      // mounted with the panel would have done that once, to nobody, before
      // the player had chosen anything.
      content={
        isOpen ? (
          <SpriteGrid
            label={label}
            onChange={(next) => {
              onChange(next);
              setIsOpen(false);
            }}
            options={options}
            shape={shape}
            value={value}
          />
        ) : null
      }
      hasAutoFocus={false}
      isOpen={isOpen}
      label={label}
      onOpenChange={setIsOpen}
      placement="below"
      width={spritePickerLayout.panelInlineSize}
    >
      {(trigger) => (
        <button
          {...trigger}
          aria-label={`${label}: ${name}`}
          type="button"
          {...stylex.props(inputWrapperStyles.base, styles.trigger, triggerXstyle)}
        >
          <span {...stylex.props(styles.triggerArt)}>{triggerArt}</span>
          <span {...stylex.props(styles.triggerLabel)}>{name}</span>
          <ChevronDownIcon aria-hidden height={14} width={14} />
        </button>
      )}
    </Popover>
  );
}

/**
 * The open grid.
 *
 * It is one listbox however many headings divide it, so the arrow keys walk
 * the whole catalogue: a player moving right off the last ground unit lands on
 * the first air unit rather than stopping at a heading they did not put there.
 */
function SpriteGrid({
  label,
  onChange,
  options,
  shape,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  options: SpritePickerOption[];
  shape: keyof typeof cellStyles;
  value: string;
}) {
  const selectedIndex = Math.max(
    0,
    options.findIndex((option) => option.value === value),
  );
  const [activeIndex, setActiveIndex] = useState(selectedIndex);
  const cellRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const typed = useRef<{ at: number; text: string }>({ at: 0, text: "" });

  // The grid opens on the current choice rather than on its first cell, so the
  // keyboard starts where the player already is and one arrow key means "the
  // next unit" rather than "back to Infantry".
  useLayoutEffect(() => {
    cellRefs.current[selectedIndex]?.focus();
    // Only on open. Moving the selection afterwards closes the grid.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    cellRefs.current[activeIndex]?.focus();
  }, [activeIndex]);

  const columns = useGridColumns(cellRefs);

  const move = (to: number) => {
    setActiveIndex(Math.max(0, Math.min(options.length - 1, to)));
  };

  return (
    <div
      aria-label={label}
      onKeyDown={(event) => {
        // Escape is the browser's: the grid is a native popover and closes
        // itself. What the panel behind it does about the same key is the
        // panel's business, and it decides by looking at where the key landed.
        const step: Record<string, number> = {
          ArrowRight: 1,
          ArrowLeft: -1,
          ArrowDown: columns,
          ArrowUp: -columns,
        };
        if (event.key in step) {
          event.preventDefault();
          move(activeIndex + (step[event.key] ?? 0));
          return;
        }
        if (event.key === "Home" || event.key === "End") {
          event.preventDefault();
          move(event.key === "Home" ? 0 : options.length - 1);
          return;
        }
        if (event.key.length === 1 && /\S/.test(event.key)) {
          const now = Date.now();
          const text =
            (now - typed.current.at > TYPEAHEAD_WINDOW_MS ? "" : typed.current.text) + event.key;
          typed.current = { at: now, text };
          const match = options.findIndex((option) =>
            option.label.toLowerCase().startsWith(text.toLowerCase()),
          );
          if (match >= 0) {
            event.preventDefault();
            move(match);
          }
        }
      }}
      role="listbox"
      {...stylex.props(styles.grid)}
    >
      {options.map((option, index) => (
        <Cell
          index={index}
          isActive={index === activeIndex}
          isSelected={option.value === value}
          key={option.value}
          onChange={onChange}
          option={option}
          previousGroup={options[index - 1]?.group}
          ref={(element) => {
            cellRefs.current[index] = element;
          }}
          shape={shape}
        />
      ))}
    </div>
  );
}

function Cell({
  index,
  isActive,
  isSelected,
  onChange,
  option,
  previousGroup,
  ref,
  shape,
}: {
  index: number;
  isActive: boolean;
  isSelected: boolean;
  onChange: (value: string) => void;
  option: SpritePickerOption;
  previousGroup: string | undefined;
  ref: (element: HTMLButtonElement | null) => void;
  shape: keyof typeof cellStyles;
}) {
  // A heading is a row of its own inside the grid rather than a wrapper around
  // a nested one, so every cell in the picker keeps the same column track and
  // the sprites stay in line down the whole catalogue.
  const heading =
    option.group !== undefined && option.group !== previousGroup ? (
      <span
        aria-hidden="true"
        {...stylex.props(styles.groupHeading, index > 0 && styles.groupHeadingLater)}
      >
        {option.group}
      </span>
    ) : null;

  return (
    <>
      {heading}
      <button
        aria-selected={isSelected}
        onClick={() => onChange(option.value)}
        ref={ref}
        role="option"
        tabIndex={isActive ? 0 : -1}
        type="button"
        {...stylex.props(styles.cell, cellStyles[shape], isSelected && styles.cellSelected)}
      >
        <span {...stylex.props(styles.cellArt)}>{option.art}</span>
        <span {...stylex.props(styles.cellLabel)}>{option.label}</span>
        {option.detail === undefined ? null : (
          <span {...stylex.props(styles.cellDetail, isSelected && styles.cellDetailSelected)}>
            {option.detail}
          </span>
        )}
      </button>
    </>
  );
}

/**
 * How many cells the grid fits on a line, measured rather than assumed.
 *
 * The grid fills to whatever the popover is given, so the number of columns is
 * a fact about the rendered page and not a constant this file could hold. Down
 * and up arrows have to agree with what the player sees.
 */
function useGridColumns(cellRefs: React.RefObject<(HTMLButtonElement | null)[]>): number {
  const [columns, setColumns] = useState(1);

  useEffect(() => {
    const cells = cellRefs.current.filter((cell): cell is HTMLButtonElement => cell !== null);
    const first = cells[0];
    if (!first) return;

    const measure = () => {
      const top = first.offsetTop;
      const count = cells.filter((cell) => cell.offsetTop === top).length;
      setColumns(Math.max(1, count));
    };
    measure();

    const observer = new ResizeObserver(measure);
    observer.observe(first);
    return () => {
      observer.disconnect();
    };
  }, [cellRefs]);

  return columns;
}

const styles = stylex.create({
  // The closed picker wears the same frame as the fields beside it, because it
  // is one of them: a control holding a value the player set.
  trigger: {
    minInlineSize: 0,
    cursor: "pointer",
    color: colorVars["--color-text-primary"],
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    textAlign: "start",
  },
  triggerArt: {
    display: "flex",
    flex: "0 0 auto",
    alignItems: "flex-end",
    justifyContent: "center",
  },
  triggerLabel: {
    flex: "1 1 auto",
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  grid: {
    display: "grid",
    // The popover sizes itself; the grid takes all of it, or auto-fill counts
    // its columns against a shrink-to-fit width and lays out one short of what
    // the panel could hold.
    inlineSize: "100%",
    gridTemplateColumns: `repeat(auto-fill, minmax(${spritePickerLayout.cellMinInlineSize}, 1fr))`,
    gap: spacingVars["--spacing-1"],
    maxBlockSize: spritePickerLayout.gridMaxBlockSize,
    overflowY: "auto",
    overscrollBehavior: "contain",
  },
  groupHeading: {
    gridColumn: "1 / -1",
    paddingBlockEnd: spacingVars["--spacing-1"],
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
    borderBlockEndWidth: borderVars["--border-width"],
    borderBlockEndStyle: "solid",
    borderBlockEndColor: colorVars["--color-border-emphasized"],
  },
  groupHeadingLater: {
    marginBlockStart: spacingVars["--spacing-2"],
  },
  // One key on the menu. It takes the outline every piece of chrome in this
  // system wears and no shadow: twenty-five raised keys in a grid is a texture,
  // not a set of choices.
  cell: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: spacingVars["--spacing-1"],
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-1"],
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: {
      default: colorVars["--color-background-surface"],
      ":hover": { "@media (hover: hover)": colorVars["--color-background-muted"] },
    },
    color: colorVars["--color-text-primary"],
    cursor: "pointer",
    // The cursor moves onto a key; the key does not move under the cursor.
    outline: {
      default: null,
      ":focus-visible": `2px solid ${colorVars["--color-accent"]}`,
    },
    outlineOffset: { default: null, ":focus-visible": "2px" },
  },
  // The game's own cursor: an orange fill sitting flush on the chrome.
  cellSelected: {
    backgroundColor: {
      default: colorVars["--color-accent"],
      ":hover": { "@media (hover: hover)": colorVars["--color-accent"] },
    },
  },
  cellArt: {
    display: "flex",
    flex: "1 1 auto",
    alignItems: "flex-end",
    justifyContent: "center",
  },
  cellLabel: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    textAlign: "center",
    // Two-word names wrap between their words; the cell is sized so that no
    // single word has to. A bitmap face broken mid-word stops being a name and
    // becomes two pieces of texture.
    overflowWrap: "normal",
    hyphens: "none",
  },
  cellDetail: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    fontVariantNumeric: "tabular-nums",
    color: colorVars["--color-text-secondary"],
  },
  // On the orange cursor the secondary ink loses its contrast, so the figure
  // joins the name at full strength rather than being tinted to survive.
  cellDetailSelected: {
    color: colorVars["--color-text-primary"],
  },
});

/**
 * How tall a cell's art is, per catalogue.
 *
 * A terrain cell is twice the height of a unit cell because a tile carries
 * what rises above it — a mountain peak, an HQ roof — and cropping that away
 * would leave a player picking mountains from a picture of grass.
 */
const cellStyles = stylex.create({
  unit: { minBlockSize: spritePickerLayout.unitCellBlockSize },
  terrain: { minBlockSize: spritePickerLayout.terrainCellBlockSize },
  commander: { minBlockSize: spritePickerLayout.commanderCellBlockSize },
});
