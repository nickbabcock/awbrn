import { describe, expect, it } from "vitest";
import { authKeys } from "./auth.keys";

describe("auth query keys", () => {
  it("keeps a stable session key", () => {
    expect(authKeys.session()).toEqual(["auth", "session"]);
  });
});
