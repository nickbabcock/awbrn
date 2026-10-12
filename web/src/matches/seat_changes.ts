import type { MatchParticipantSnapshot, MatchSnapshot } from "#/matches/schemas.ts";

export type SeatPatch = Partial<Pick<MatchParticipantSnapshot, "coId" | "factionId" | "ready">>;

type Intent = { patch: SeatPatch; revision: number; error: string | null };
export type SeatChangeState = { intents: ReadonlyMap<number, Intent>; isSaving: boolean };

/** Show local choices without a change to confirmed match data. */
export function projectSeatChanges(
  match: MatchSnapshot,
  userId: string | null,
  intents: SeatChangeState["intents"],
): MatchSnapshot {
  if (userId === null || intents.size === 0) return match;
  return {
    ...match,
    participants: match.participants.map((seat) =>
      seat.userId === userId ? { ...seat, ...intents.get(seat.slotIndex)?.patch } : seat,
    ),
  };
}

/** Save one request at a time. Keep the latest choice until the server accepts it. */
export class SeatChanges {
  private state: SeatChangeState = { intents: new Map(), isSaving: false };
  private listeners = new Set<() => void>();
  private revision = 0;
  private stopped = false;

  constructor(
    private save: (slotIndex: number, patch: SeatPatch) => Promise<void>,
    private refresh: () => Promise<unknown>,
  ) {}

  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  private publish(intents: ReadonlyMap<number, Intent>, isSaving = this.state.isSaving) {
    this.state = { intents, isSaving };
    for (const listener of this.listeners) listener();
  }

  select(slotIndex: number, patch: SeatPatch) {
    if (this.stopped) return;
    const intents = new Map(this.state.intents);
    const previous = intents.get(slotIndex);
    intents.set(slotIndex, {
      patch: { ...previous?.patch, ...patch },
      revision: ++this.revision,
      error: null,
    });
    this.publish(intents);
    void this.drain();
  }

  retry() {
    if (this.stopped) return;
    this.publish(
      new Map(
        Array.from(this.state.intents, ([slot, intent]) => [
          slot,
          {
            ...intent,
            revision: ++this.revision,
            error: null,
          },
        ]),
      ),
    );
    void this.drain();
  }

  reconcile(match: MatchSnapshot, userId: string | null) {
    if (userId === null) return;
    const intents = new Map(this.state.intents);
    for (const [slot, intent] of intents) {
      if (intent.error === null) continue;
      const seat = match.participants.find(
        (value) => value.slotIndex === slot && value.userId === userId,
      );
      if (
        !seat ||
        Object.entries(intent.patch).every(
          ([field, value]) => seat[field as keyof SeatPatch] === value,
        )
      ) {
        intents.delete(slot);
      }
    }
    if (intents.size !== this.state.intents.size) this.publish(intents);
  }

  stop() {
    this.stopped = true;
  }
  start() {
    this.stopped = false;
    void this.drain();
  }

  private async drain() {
    if (this.state.isSaving || this.stopped) return;
    const next = Array.from(this.state.intents).find(([, intent]) => intent.error === null);
    if (!next) return;
    this.publish(this.state.intents, true);
    try {
      let pending: [number, Intent] | undefined = next;
      while (pending && !this.stopped) {
        const [slot, intent] = pending;
        let error: string | null = null;
        try {
          await this.save(slot, intent.patch);
        } catch (cause) {
          error = cause instanceof Error ? cause.message : "Seat update failed.";
        }
        const intents = new Map(this.state.intents);
        if (intents.get(slot)?.revision === intent.revision) {
          if (error === null) intents.delete(slot);
          else intents.set(slot, { ...intent, error });
        }
        this.publish(intents);
        pending = Array.from(intents).find(([, value]) => value.error === null);
      }
    } finally {
      this.publish(this.state.intents, false);
      // Read the server after errors, too. A lost response can hide a saved choice.
      if (!this.stopped) void this.refresh().catch(() => {});
    }
  }
}
