import { describe, expect, it } from "vitest";
import { moveSeat, splitTeams, teamChoices, teamFromId, teamShape, teamsProblem } from "./teams.ts";

describe("teams", () => {
  it("accepts a free-for-all or a packed team match of two sides or more", () => {
    expect(teamsProblem([], 5)).toBeNull();
    expect(teamsProblem([1, 1, 1, 1, 2], 5)).toBeNull();
    expect(teamsProblem([1, 2, 1, 2, 3], 5)).toBeNull();
  });

  it("refuses teams that miss a seat, leave one side or every seat alone, skip a letter or pass H", () => {
    expect(teamsProblem([1, 1, 2], 5)).not.toBeNull();
    expect(teamsProblem([1, 1, 1], 3)).not.toBeNull();
    expect(teamsProblem([1, 1, 3], 3)).not.toBeNull();
    expect(teamsProblem([1, 2, 3, 4, 5, 6, 7, 8, 9], 9)).not.toBeNull();
    expect(teamsProblem([1, 2, 3], 3)).not.toBeNull();
    expect(teamsProblem([1, 2], 2)).not.toBeNull();
  });

  it("splits the seats in turn order, the larger half on A", () => {
    expect(splitTeams(5)).toEqual([1, 1, 1, 2, 2]);
    expect(teamShape(splitTeams(5))).toBe("3v2");
  });

  it("will not let the lone seat of a two-side match leave its side", () => {
    const teams = [1, 1, 1, 1, 2];
    expect(teamChoices(teams, 4).find((choice) => choice.team === 1)?.isDisabled).toBe(true);
    expect(moveSeat(teams, 4, 1)).toBeNull();
  });

  it("offers a new side only to a seat that shares one", () => {
    expect(teamChoices([1, 1, 1, 1, 2], 4).some((choice) => choice.isNew)).toBe(false);
    expect(teamChoices([1, 1, 1, 1, 2], 0).at(-1)).toMatchObject({ team: 3, isNew: true });
  });

  it("offers no new side that would leave every seat alone", () => {
    expect(teamChoices([1, 1, 2], 0).some((choice) => choice.isNew)).toBe(false);
    expect(moveSeat([1, 1, 2], 0, 3)).toBeNull();
  });

  it("moves letters down when a side empties", () => {
    expect(moveSeat([1, 2, 3, 3], 1, 3)).toEqual([1, 2, 2, 2]);
    expect(teamShape([1, 2, 2, 2])).toBe("1v3");
  });

  it("reads a side from an engine team id or an AWBW letter", () => {
    expect(teamFromId("team-2")).toBe(2);
    expect(teamFromId("B")).toBe(2);
    expect(teamFromId("K")).toBe(11);
    expect(teamFromId("team-9")).toBeNull();
    expect(teamFromId("player-3")).toBeNull();
  });
});
