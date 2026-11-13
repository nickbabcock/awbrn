/** Where the first match's map terrain stands, as React Query reports it. */
export type FirstMatchMapStatus = "pending" | "error" | "success";

/**
 * What the CPU card offers. Play the CPU waits only while the map's terrain
 * loads or a press is out; once the terrain fails it stays pressable and
 * fetches the terrain again. A failed start is named over a failed load, since
 * its Retry presses Play the CPU, which fetches the terrain too.
 */
export function cpuCardState({
  hasStartFailed,
  isStarting,
  mapStatus,
}: {
  hasStartFailed: boolean;
  isStarting: boolean;
  mapStatus: FirstMatchMapStatus;
}): { isKeyDisabled: boolean; problem: "start" | "map" | null } {
  return {
    isKeyDisabled: isStarting || mapStatus === "pending",
    problem: hasStartFailed ? "start" : mapStatus === "error" ? "map" : null,
  };
}
