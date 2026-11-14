import { useEffect, useRef, useState } from "react";

export interface LatestSeekOptions<T> {
  /** When a waiting target is where the board already went. */
  isSame?: (a: T, b: T) => boolean;
  /**
   * How many seeks may be out at once, as long as each of them `overlaps`.
   * Answers come back in the order they were asked, so a longer window shows
   * more of a drag when the wait is the trip rather than the work.
   */
  window?: number;
  overlaps?: (target: T) => boolean;
}

/**
 * Seeks paced by their answers: one in flight, and only the newest target
 * waiting behind it.
 *
 * A dragged scrubber asks for a new moment on every move, faster than the
 * board can show them. Sending each one would queue a backlog the board works
 * through long after the thumb has stopped, so a newer target replaces the
 * one waiting, and the board stays one answer behind the thumb.
 */
export class LatestSeek<T> {
  private readonly flight: T[] = [];
  private waiting: { target: T } | null = null;
  private sent: { target: T } | null = null;
  private readonly isSame: (a: T, b: T) => boolean;
  private readonly window: number;
  private readonly overlaps: (target: T) => boolean;

  /**
   * `send` reports whether the seek got out, and so whether an answer is
   * coming. `onInFlight` hears each time that changes.
   */
  constructor(
    private readonly send: (target: T) => boolean,
    private readonly onInFlight: (inFlight: boolean) => void = () => {},
    { isSame = Object.is, window = 1, overlaps = () => false }: LatestSeekOptions<T> = {},
  ) {
    this.isSame = isSame;
    this.window = window;
    this.overlaps = overlaps;
  }

  /** The oldest seek that is out, whose answer comes back next. */
  get flying(): { target: T } | null {
    return this.flight.length > 0 ? { target: this.flight[0]! } : null;
  }

  request(target: T): void {
    if (!this.canSend(target)) {
      this.waiting = { target };
      return;
    }
    if (!this.send(target)) return;
    this.flight.push(target);
    this.sent = { target };
    if (this.flight.length === 1) this.onInFlight(true);
  }

  /** The oldest seek out was answered: send the target waiting, unless it is where the board went. */
  settle(): void {
    if (this.flight.shift() !== undefined && this.flight.length === 0) this.onInFlight(false);
    const waiting = this.waiting;
    if (waiting === null || !this.canSend(waiting.target)) return;
    this.waiting = null;
    if (this.sent === null || !this.isSame(waiting.target, this.sent.target)) {
      this.request(waiting.target);
    }
  }

  /** Drop everything, for an answer that is never coming back. */
  reset(): void {
    const wasInFlight = this.flight.length > 0;
    this.flight.length = 0;
    this.waiting = null;
    this.sent = null;
    if (wasInFlight) this.onInFlight(false);
  }

  private canSend(target: T): boolean {
    return (
      this.flight.length === 0 ||
      (this.flight.length < this.window &&
        this.overlaps(target) &&
        this.flight.every((out) => this.overlaps(out)))
    );
  }
}

/**
 * A {@link LatestSeek} for one page, and whether it has a seek out. It sends
 * through the newest `send`, and nothing waiting is sent once the page has
 * gone.
 */
export function useLatestSeek<T>(
  send: (target: T) => boolean,
  options?: LatestSeekOptions<T>,
): [LatestSeek<T>, boolean] {
  const sendRef = useRef(send);
  sendRef.current = send;
  const [isSeeking, setIsSeeking] = useState(false);
  const [seek] = useState(
    () => new LatestSeek<T>((target) => sendRef.current(target), setIsSeeking, options),
  );
  useEffect(() => () => seek.reset(), [seek]);
  return [seek, isSeeking];
}
