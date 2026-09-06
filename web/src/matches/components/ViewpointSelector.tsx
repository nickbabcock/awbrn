import { Selector } from "@astryxdesign/core/Selector";
import type { MatchArmy } from "#/matches/match_armies.ts";
import type { ReplayViewpointChanged } from "#/wasm/awbrn_wasm.js";

/** Watching the whole board rather than any one seat. */
const EVERYONE = "everyone";
/** Watching through whoever holds the turn, seat by seat as it passes. */
const TURN_HOLDER = "turnHolder";

/**
 * Whose eyes a finished match is watched through.
 *
 * A fogged match is two games until it ends: each player spent it reading a
 * board the other could not see. Once it is over there is nothing left to
 * protect, so every seat can be looked through, and the losing side's view is
 * usually the one worth reading — it holds the moment the game turned, which
 * the winner never saw.
 *
 * The value is the engine's, not this control's. The board's own keys move the
 * viewpoint as well, so what is selected here is whatever the engine last
 * reported rather than whatever was last pressed.
 */
export function ViewpointSelector({
  armies,
  onChange,
  viewpoint,
}: {
  armies: MatchArmy[];
  onChange: (playerId: number | null, followsActivePlayer: boolean) => void;
  viewpoint: ReplayViewpointChanged | null;
}) {
  const options = [
    { value: EVERYONE, label: "Everyone", description: "The whole board, fog lifted" },
    { value: TURN_HOLDER, label: "Turn holder", description: "Follows the seat that is playing" },
    ...armies.map((army) => ({
      value: String(army.entry.playerId),
      label: army.name,
    })),
  ];

  return (
    <Selector
      label="Watching as"
      onChange={(value) => {
        if (value === TURN_HOLDER) return onChange(null, true);
        if (value === EVERYONE) return onChange(null, false);
        onChange(Number(value), false);
      }}
      options={options}
      size="sm"
      value={selectedValue(viewpoint)}
      width={200}
    />
  );
}

function selectedValue(viewpoint: ReplayViewpointChanged | null): string {
  if (viewpoint === null) return EVERYONE;
  if (viewpoint.followsActivePlayer) return TURN_HOLDER;
  return viewpoint.playerId === null ? EVERYONE : String(viewpoint.playerId);
}
