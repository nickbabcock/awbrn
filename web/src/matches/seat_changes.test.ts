import { describe, expect, it, vi } from "vitest";
import { projectSeatChanges, SeatChanges, type SeatPatch } from "#/matches/seat_changes.ts";

import type { MatchSnapshot } from "#/matches/schemas.ts";

function deferred() {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

function setup() {
  const requests: { slot: number; patch: SeatPatch; result: ReturnType<typeof deferred> }[] = [];
  const refresh = vi.fn(async () => {});
  const changes = new SeatChanges(async (slot, patch) => {
    const result = deferred();
    requests.push({ slot, patch, result });
    await result.promise;
  }, refresh);
  return { changes, requests, refresh };
}

describe("seat choices", () => {
  it("keeps local choices over refetches and preserves confirmed changes to other seats", async () => {
    const { changes, requests } = setup();
    changes.select(0, { coId: 9, ready: false });
    const polled = {
      phase: "lobby",
      participants: [
        { userId: "me", slotIndex: 0, coId: 3, ready: false },
        { userId: "them", slotIndex: 1, coId: 7, ready: true },
      ],
    } as MatchSnapshot;
    const displayed = projectSeatChanges(polled, "me", changes.getSnapshot().intents);
    expect(displayed.participants[0]!.coId).toBe(9);
    expect(displayed.participants[1]).toBe(polled.participants[1]);
    expect(displayed.participants[1]!.ready).toBe(true);
    expect(polled.participants[0]!.coId).toBe(3);
    expect(
      projectSeatChanges(polled, "them", changes.getSnapshot().intents).participants[0]!.coId,
    ).toBe(3);
    expect(projectSeatChanges(polled, null, changes.getSnapshot().intents)).toBe(polled);
    requests[0]!.result.resolve();
    await settle();
    expect(projectSeatChanges(polled, "me", changes.getSnapshot().intents)).toBe(polled);
  });

  it("keeps the latest choice visible and skips intermediate choices", async () => {
    const { changes, requests, refresh } = setup();
    changes.select(0, { coId: 3, ready: false });
    changes.select(0, { coId: 9, ready: false });
    changes.select(0, { coId: 12, ready: false });
    expect(requests).toHaveLength(1);
    expect(changes.getSnapshot().intents.get(0)?.patch.coId).toBe(12);
    requests[0]!.result.resolve();
    await settle();
    expect(requests).toHaveLength(2);
    expect(requests[1]!.patch.coId).toBe(12);
    expect(changes.getSnapshot().intents.get(0)?.patch.coId).toBe(12);
    expect(refresh).not.toHaveBeenCalled();
    requests[1]!.result.resolve();
    await settle();
    expect(changes.getSnapshot().intents.size).toBe(0);
    expect(changes.getSnapshot().isSaving).toBe(false);
    expect(refresh).toHaveBeenCalledOnce();
  });

  it("saves a newer choice after an earlier request fails", async () => {
    const { changes, requests } = setup();
    changes.select(0, { coId: 3 });
    changes.select(0, { coId: 9 });
    requests[0]!.result.reject(new Error("Lost response"));
    await settle();
    expect(requests[1]!.patch.coId).toBe(9);
    expect(changes.getSnapshot().intents.get(0)?.error).toBeNull();
    requests[1]!.result.resolve();
    await settle();
    expect(changes.getSnapshot().intents.size).toBe(0);
  });

  it("retains the latest failed choice and retries it", async () => {
    const { changes, requests, refresh } = setup();
    changes.select(0, { coId: 3 });
    changes.select(0, { coId: 9 });
    requests[0]!.result.reject(new Error("First failure"));
    await settle();
    requests[1]!.result.reject(new Error("Second failure"));
    await settle();
    expect(changes.getSnapshot().intents.get(0)).toMatchObject({
      patch: { coId: 9 },
      error: "Second failure",
    });
    expect(changes.getSnapshot().isSaving).toBe(false);
    expect(refresh).toHaveBeenCalledOnce();
    changes.retry();
    expect(requests[2]!.patch.coId).toBe(9);
    expect(changes.getSnapshot().intents.get(0)?.error).toBeNull();
    requests[2]!.result.resolve();
    await settle();
    expect(changes.getSnapshot().intents.size).toBe(0);
  });

  it("combines army and CO choices without losing either field", async () => {
    const { changes, requests } = setup();
    changes.select(0, { factionId: 2, ready: false });
    changes.select(0, { coId: 9, ready: false });
    changes.select(0, { factionId: 4, ready: false });
    requests[0]!.result.resolve();
    await settle();
    expect(requests[1]!.patch).toEqual({ coId: 9, factionId: 4, ready: false });
    requests[1]!.result.resolve();
    await settle();
  });

  it("saves hotseat changes in order and keeps a failed seat separate", async () => {
    const { changes, requests } = setup();
    changes.select(0, { coId: 3 });
    changes.select(1, { coId: 9 });
    requests[0]!.result.reject(new Error("Seat zero failed"));
    await settle();
    expect(requests[1]!.slot).toBe(1);
    requests[1]!.result.resolve();
    await settle();
    expect(changes.getSnapshot().intents.size).toBe(1);
    expect(changes.getSnapshot().intents.get(0)?.patch.coId).toBe(3);
  });

  it("keeps a failed Ready action visible until retry succeeds", async () => {
    const { changes, requests } = setup();
    changes.select(0, { ready: true });
    requests[0]!.result.reject(new Error("Lost response"));
    await settle();
    expect(changes.getSnapshot().intents.get(0)?.patch.ready).toBe(true);
    changes.retry();
    requests[1]!.result.resolve();
    await settle();
    expect(changes.getSnapshot().intents.size).toBe(0);
  });

  it("clears a lost-response error when the server confirms the choice", async () => {
    const { changes, requests } = setup();
    changes.select(0, { coId: 9, factionId: 4, ready: false });
    requests[0]!.result.reject(new Error("Lost response"));
    await settle();
    const match = {
      participants: [{ userId: "me", slotIndex: 0, coId: 9, factionId: 2, ready: false }],
    } as MatchSnapshot;
    changes.reconcile(match, "me");
    expect(changes.getSnapshot().intents.size).toBe(1);
    match.participants[0]!.factionId = 4;
    changes.reconcile(match, "me");
    expect(changes.getSnapshot().intents.size).toBe(0);
    expect(requests).toHaveLength(1);
  });

  it("does not clear an active save when a poll shows the desired choice", async () => {
    const { changes, requests } = setup();
    changes.select(0, { coId: 9 });
    changes.reconcile(
      { participants: [{ userId: "me", slotIndex: 0, coId: 9 }] } as MatchSnapshot,
      "me",
    );
    expect(changes.getSnapshot().intents.size).toBe(1);
    requests[0]!.result.resolve();
    await settle();
  });

  it("does not send queued choices after leaving the page", async () => {
    const { changes, requests, refresh } = setup();
    changes.select(0, { coId: 3 });
    changes.select(0, { coId: 9 });
    changes.stop();
    requests[0]!.result.resolve();
    await settle();
    expect(requests).toHaveLength(1);
    expect(refresh).not.toHaveBeenCalled();
  });
});
