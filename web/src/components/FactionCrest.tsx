import { Button } from "#/ui/Button.tsx";
import { Popover } from "@astryxdesign/core/Popover";
import { ToggleButton, ToggleButtonGroup } from "@astryxdesign/core/ToggleButton";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { factions, getFactionByCode } from "#/factions.ts";

const PICKER_LOGO_SIZE = 20;
/** Two named armies to a row, so twenty of them stay one glance tall. */
const PICKER_ITEM_WIDTH = 156;
const PICKER_WIDTH = PICKER_ITEM_WIDTH * 2 + 32;

interface FactionCrestProps {
  factionCode: string;
  /** Omit to render the crest as a plain mark rather than a control. */
  onChange?: (factionId: number) => void | Promise<void>;
  isDisabled?: boolean;
  size?: number;
}

/**
 * The army's crest, and the way its army is chosen.
 *
 * The mark is the control: a seat shows only its insignia, and clicking it
 * opens the roll of armies. Every army is named there, so this is where the
 * crest is learned and why the surrounding row does not have to repeat it.
 */
export function FactionCrest({
  factionCode,
  isDisabled = false,
  onChange,
  size = 24,
}: FactionCrestProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isPending, setIsPending] = useState(false);
  const activeFaction = getFactionByCode(factionCode);
  const activeName = activeFaction?.displayName ?? factionCode.toUpperCase();

  if (!onChange) {
    return <FactionLogo factionCode={factionCode} size={size} />;
  }

  async function handleSelect(value: string | null | string[]) {
    if (typeof value !== "string" || !onChange) return;

    setIsOpen(false);
    setIsPending(true);
    try {
      await onChange(Number(value));
    } finally {
      setIsPending(false);
    }
  }

  return (
    <Popover
      alignment="start"
      // Built only while open: a roster of armies would otherwise carry one
      // hidden plate of every army per row.
      content={
        !isOpen ? null : (
          <ToggleButtonGroup
            label="Army"
            onChange={(value) => void handleSelect(value)}
            size="sm"
            type="single"
            value={activeFaction ? String(activeFaction.id) : null}
            xstyle={styles.grid}
          >
            {factions.map((faction) => (
              <ToggleButton
                icon={
                  <FactionLogo
                    factionCode={faction.code}
                    isFramed={false}
                    isLabelHidden
                    size={PICKER_LOGO_SIZE}
                  />
                }
                key={faction.id}
                label={faction.displayName}
                value={String(faction.id)}
                xstyle={styles.item}
              />
            ))}
          </ToggleButtonGroup>
        )
      }
      isOpen={isOpen}
      label="Army"
      onOpenChange={setIsOpen}
      placement="below"
      width={PICKER_WIDTH}
    >
      <Button
        icon={<FactionLogo factionCode={factionCode} isFramed={false} isLabelHidden size={size} />}
        isDisabled={isDisabled}
        isIconOnly
        isLoading={isPending}
        label={`${activeName}. Choose a different army.`}
        size="sm"
        variant="secondary"
      />
    </Popover>
  );
}

const styles = stylex.create({
  // A two-column roll: the crest teaches the mark, the name identifies it.
  grid: {
    flexWrap: "wrap",
  },
  // Army names are proper nouns, not commands, so they take the body voice.
  // The HUD face is too wide to fit "Purple Lightning" beside its crest.
  item: {
    width: PICKER_ITEM_WIDTH,
    justifyContent: "start",
    fontFamily: "var(--font-family-body)",
    fontSize: "var(--font-size-sm)",
    fontWeight: "700",
    letterSpacing: "normal",
    textTransform: "none",
  },
});
