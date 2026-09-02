import type { TurnReadinessChanged } from "#/wasm/awbrn_wasm.js";

/**
 * What ending the turn now would leave behind, `null` when it would leave
 * nothing, or `undefined` when readiness is not known.
 *
 * A turn is ended many times a match, so a question asked every time is one
 * that gets clicked through. This asks only when there is something to say, and
 * says what it is: the prompt is the answer, not a speed bump in front of it.
 */
export function describeTurnResidue(
  readiness: TurnReadinessChanged | undefined,
  isPowerReady: boolean,
): string | null | undefined {
  if (readiness === undefined) {
    return undefined;
  }

  const left: string[] = [];
  const idleUnits = readiness.idleUnits;
  const freeSites = readiness.freeSites;
  if (idleUnits > 0) {
    left.push(idleUnits === 1 ? "1 unit has not moved" : `${idleUnits} units have not moved`);
  }
  if (freeSites > 0) {
    left.push(freeSites === 1 ? "1 base can still build" : `${freeSites} bases can still build`);
  }
  // The power is named last and on its own, because it is the one of the three
  // that is gone rather than merely postponed: a turn ended on a full meter is
  // a power the opponent moves before.
  if (isPowerReady) {
    left.push("your CO power is ready");
  }
  if (left.length === 0) {
    return null;
  }

  const listed =
    left.length === 1
      ? left[0]
      : `${left.slice(0, -1).join(", ")} and ${left[left.length - 1] ?? ""}`;
  return `${listed[0]?.toUpperCase() ?? ""}${listed.slice(1)}. Ending the turn cannot be undone.`;
}
