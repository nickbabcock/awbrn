import { Button } from "#/ui/Button.tsx";
import { Popover } from "@astryxdesign/core/Popover";
import { VStack } from "@astryxdesign/core/Stack";
import { ToggleButton, ToggleButtonGroup } from "@astryxdesign/core/ToggleButton";
import { textSizeVars, typographyVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { factions, getFactionByCode } from "#/factions.ts";
import { ModalSheet } from "#/ui/ModalSheet.tsx";
import { useCompactTouch } from "#/ui/useCompactTouch.ts";

const PICKER_LOGO_SIZE = 20;
/** Two named armies to a row, so twenty of them stay one glance tall. */
const PICKER_WIDTH = 156 * 2 + 32;

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
  const isCompactTouch = useCompactTouch();
  const activeFaction = getFactionByCode(factionCode);
  const activeName = activeFaction?.displayName ?? factionCode.toUpperCase();

  if (!onChange) {
    return <FactionLogo factionCode={factionCode} size={size} />;
  }

  // The crest changes at once. The caller writes the choice to the screen first
  // and sends it second, so the picker never waits on the server.
  function handleSelect(value: string | null | string[]) {
    if (typeof value !== "string" || !onChange) return;

    setIsOpen(false);
    void onChange(Number(value));
  }

  const roll = !isOpen ? null : (
    <ToggleButtonGroup
      label="Army"
      onChange={handleSelect}
      size="sm"
      type="single"
      value={activeFaction ? String(activeFaction.id) : null}
      xstyle={styles.grid}
    >
      {factions.map((faction) => (
        <ToggleButton
          icon={<FactionLogo factionCode={faction.code} isLabelHidden size={PICKER_LOGO_SIZE} />}
          key={faction.id}
          label={faction.displayName}
          value={String(faction.id)}
          xstyle={styles.item}
        />
      ))}
    </ToggleButtonGroup>
  );
  const crest = (
    <Button
      icon={<FactionLogo factionCode={factionCode} isLabelHidden size={size} />}
      isDisabled={isDisabled}
      isIconOnly
      label={`${activeName}. Choose a different army.`}
      onClick={isCompactTouch ? () => setIsOpen(true) : undefined}
      size="sm"
      variant="secondary"
    />
  );

  // A phone gets a titled sheet that hugs the roll; Popover's own sheet is
  // untitled and always tall.
  if (isCompactTouch) {
    return (
      <>
        {crest}
        <ModalSheet isOpen={isOpen} onOpenChange={setIsOpen} title="Army">
          <VStack xstyle={styles.sheetBody}>{roll}</VStack>
        </ModalSheet>
      </>
    );
  }

  return (
    <Popover
      alignment="start"
      // Built only while open: a roster of armies would otherwise carry one
      // hidden plate of every army per row.
      content={roll}
      isOpen={isOpen}
      label="Army"
      onOpenChange={setIsOpen}
      placement="below"
      width={PICKER_WIDTH}
    >
      {crest}
    </Popover>
  );
}

const styles = stylex.create({
  // A two-column roll: the crest teaches the mark, the name identifies it.
  grid: {
    display: "grid",
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
  },
  // Army names are proper nouns, not commands, so they take the body voice.
  item: {
    justifyContent: "start",
    fontFamily: typographyVars["--font-family-body"],
    fontSize: textSizeVars["--font-size-sm"],
    fontWeight: "700",
    letterSpacing: "normal",
    textTransform: "none",
  },
  sheetBody: {
    padding: "16px",
  },
});
