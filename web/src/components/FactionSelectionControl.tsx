import { Selector } from "@astryxdesign/core/Selector";
import { useState } from "react";
import { FactionBadge } from "#/components/PlayerHeader.tsx";
import { factions, getFactionByCode } from "#/factions.ts";

interface FactionSelectionControlProps {
  factionCode: string;
  disabled: boolean;
  onChange: (nextValue: number) => void | Promise<void>;
}

const factionOptions = factions.map((faction) => ({
  icon: <FactionBadge factionCode={faction.code} isLabelHidden title={faction.displayName} />,
  label: faction.displayName,
  value: String(faction.id),
}));

export function FactionSelectionControl({
  factionCode,
  disabled,
  onChange,
}: FactionSelectionControlProps) {
  const [pending, setPending] = useState(false);
  const [selectionError, setSelectionError] = useState<string | null>(null);
  const activeFaction = getFactionByCode(factionCode);

  async function handleSelect(value: string) {
    setPending(true);
    setSelectionError(null);
    try {
      await onChange(Number(value));
    } catch (error) {
      setSelectionError(
        error instanceof Error ? error.message : "Faction depiction failed to update.",
      );
    } finally {
      setPending(false);
    }
  }

  return (
    <Selector
      disabledMessage={pending ? "Updating faction depiction" : undefined}
      isDisabled={disabled || pending}
      isLabelHidden
      label="Faction depiction"
      onChange={(value) => void handleSelect(value)}
      options={factionOptions}
      size="sm"
      status={selectionError ? { type: "error", message: selectionError } : undefined}
      value={activeFaction ? String(activeFaction.id) : ""}
      width={160}
    />
  );
}
