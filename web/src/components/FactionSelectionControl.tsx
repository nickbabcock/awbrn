import { Popover } from "@base-ui/react/popover";
import { ScrollArea } from "@base-ui/react/scroll-area";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { factions } from "#/factions.ts";
import { getFactionVisual } from "#/faction_visuals.ts";
import { tokens } from "#/ui/theme.stylex.ts";

const popIn = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(8px) scale(0.98)" },
  to: { opacity: 1, transform: "translateY(0) scale(1)" },
});

export function FactionSelectionControl({
  factionCode,
  disabled,
  onDark = false,
  align = "start",
  sideOffset = 10,
  onChange,
}: {
  factionCode: string;
  disabled: boolean;
  onDark?: boolean;
  align?: "start" | "end";
  sideOffset?: number;
  onChange: (nextValue: number) => void | Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const activeVisual = getFactionVisual(factionCode);
  const title = "Faction depiction";

  const handleSelect = async (value: number) => {
    setPending(true);
    try {
      await onChange(value);
      setOpen(false);
    } finally {
      setPending(false);
    }
  };

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger
        aria-label={title}
        disabled={disabled || pending}
        title={title}
        {...stylex.props(
          onDark
            ? styles.factionBadgeDarkButton
            : [
                styles.factionBadge(activeVisual.accentSoft, activeVisual.accent),
                styles.factionBadgeButton,
              ],
          (disabled || pending) && styles.triggerDisabled,
        )}
      >
        <FactionLogo factionCode={factionCode} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner align={align} sideOffset={sideOffset}>
          <Popover.Popup
            initialFocus={false}
            {...stylex.props(styles.pickerPopup, styles.factionPopup)}
          >
            <ScrollArea.Root>
              <ScrollArea.Viewport {...stylex.props(styles.pickerViewport)}>
                <ScrollArea.Content>
                  <div {...stylex.props(styles.factionPickerIntro)}>
                    <span {...stylex.props(styles.selectorLabel)}>Display only</span>
                  </div>
                  <div {...stylex.props(styles.factionGrid)}>
                    {factions.map((option) => {
                      const optionVisual = getFactionVisual(option.code);
                      return (
                        <button
                          key={option.code}
                          onClick={() => {
                            void handleSelect(option.id);
                          }}
                          type="button"
                          disabled={pending}
                          {...stylex.props(
                            styles.factionTile(optionVisual.wash),
                            option.code === factionCode && styles.factionTileSelected,
                          )}
                        >
                          <FactionLogo factionCode={option.code} />
                          <span {...stylex.props(styles.factionTileCopy)}>
                            <span {...stylex.props(styles.tileTitle)}>{option.displayName}</span>
                          </span>
                        </button>
                      );
                    })}
                  </div>
                </ScrollArea.Content>
              </ScrollArea.Viewport>
            </ScrollArea.Root>
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}

function FactionLogo({ factionCode }: { factionCode: string }) {
  const visual = getFactionVisual(factionCode);

  return (
    <span aria-hidden="true" {...stylex.props(styles.factionLogoWrap)}>
      <span
        style={{
          backgroundImage: `url(${visual.logoUrl})`,
          backgroundPosition: visual.logoPosition,
        }}
        {...stylex.props(styles.factionLogo)}
      />
    </span>
  );
}

const styles = stylex.create({
  triggerDisabled: {
    opacity: 0.55,
    cursor: "not-allowed",
  },
  factionBadge: (accentSoft: string, accent: string) => ({
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 24,
    height: 24,
    borderRadius: tokens.radius1,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: accent,
    backgroundColor: accentSoft,
  }),
  factionBadgeButton: {
    cursor: "pointer",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, opacity",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
      ":disabled": "translateY(0)",
    },
    boxShadow: {
      default: tokens.shadowHardSm,
      ":hover": tokens.shadowHardMd,
      ":active": "none",
    },
  },
  factionBadgeDarkButton: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 24,
    height: 24,
    borderRadius: tokens.radius1,
    backgroundColor: "rgba(255, 255, 255, 0.16)",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: "rgba(255, 255, 255, 0.24)",
    cursor: "pointer",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, opacity",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
      ":disabled": "translateY(0)",
    },
  },
  pickerPopup: {
    borderWidth: 3,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius3,
    backgroundColor: tokens.panelRaised,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardLg}`,
    padding: tokens.space3,
    animationDuration: "140ms",
    animationFillMode: "both",
    animationName: popIn,
  },
  factionPopup: {
    width: "min(420px, calc(100vw - 32px))",
  },
  pickerViewport: {
    maxHeight: "min(420px, 60vh)",
  },
  factionPickerIntro: {
    display: "grid",
    gap: 4,
    paddingBottom: tokens.space3,
  },
  selectorLabel: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  factionGrid: {
    display: "grid",
    gap: tokens.space1,
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
  },
  factionTile: (wash: string) => ({
    display: "flex",
    alignItems: "center",
    gap: tokens.space2,
    width: "100%",
    padding: tokens.space1,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    backgroundColor: wash,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    cursor: "pointer",
    textAlign: "left",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, border-color, background-color",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
    },
  }),
  factionTileSelected: {
    borderColor: tokens.strokeHeavy,
    backgroundColor: tokens.brandSoft,
  },
  factionLogoWrap: {
    flex: "0 0 auto",
    width: 14,
    height: 14,
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
  },
  factionLogo: {
    width: 14,
    height: 14,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  },
  factionTileCopy: {
    display: "grid",
    gap: 2,
  },
  tileTitle: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontBody,
    fontSize: tokens.textSm,
    fontWeight: 800,
  },
});
