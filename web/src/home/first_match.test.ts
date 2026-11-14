import { describe, expect, it } from "vitest";
import { cpuCardState } from "./first_match.ts";

describe("cpuCardState", () => {
  it.each([
    ["holds the key while the map loads", false, false, "pending", true, null],
    ["offers the key once the map arrives", false, false, "success", false, null],
    ["names a failed map and keeps the key pressable", false, false, "error", false, "map"],
    ["holds the key while a press is out", false, true, "error", true, "map"],
    ["names a failed start over a failed map", true, false, "error", false, "start"],
  ] as const)("%s", (_, hasStartFailed, isStarting, mapStatus, isKeyDisabled, problem) => {
    expect(cpuCardState({ hasStartFailed, isStarting, mapStatus })).toEqual({
      isKeyDisabled,
      problem,
    });
  });
});
