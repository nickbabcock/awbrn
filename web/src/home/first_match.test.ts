import { describe, expect, it } from "vitest";
import { cpuCardState } from "./first_match.ts";

describe("cpuCardState", () => {
  it("holds Play the CPU while the map loads", () => {
    expect(
      cpuCardState({ hasStartFailed: false, isStarting: false, mapStatus: "pending" }),
    ).toEqual({ isKeyDisabled: true, problem: null });
  });

  it("offers Play the CPU once the map arrives", () => {
    expect(
      cpuCardState({ hasStartFailed: false, isStarting: false, mapStatus: "success" }),
    ).toEqual({ isKeyDisabled: false, problem: null });
  });

  it("names a failed map and keeps Play the CPU pressable", () => {
    expect(cpuCardState({ hasStartFailed: false, isStarting: false, mapStatus: "error" })).toEqual({
      isKeyDisabled: false,
      problem: "map",
    });
  });

  it("holds Play the CPU while a press is out", () => {
    expect(cpuCardState({ hasStartFailed: false, isStarting: true, mapStatus: "error" })).toEqual({
      isKeyDisabled: true,
      problem: "map",
    });
  });

  it("names a failed start over a failed map", () => {
    expect(cpuCardState({ hasStartFailed: true, isStarting: false, mapStatus: "error" })).toEqual({
      isKeyDisabled: false,
      problem: "start",
    });
  });
});
