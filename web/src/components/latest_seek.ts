import { useEffect, useRef, useState } from "react";

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
  private inFlight = false;
  private waiting: { target: T } | null = null;
  private sent: { target: T } | null = null;

  /**
   * `send` reports whether the seek got out, and so whether an answer is
   * coming. `onInFlight` hears each time that changes.
   */
  constructor(
    private readonly send: (target: T) => boolean,
    private readonly onInFlight: (inFlight: boolean) => void = () => {},
  ) {}

  /** True while a seek is out and its answer has not come back. */
  get isInFlight(): boolean {
    return this.inFlight;
  }

  request(target: T): void {
    if (this.inFlight) {
      this.waiting = { target };
      return;
    }
    this.setInFlight(this.send(target));
    if (this.inFlight) this.sent = { target };
  }

  /** The seek in flight was answered: send the target waiting, unless it is where the board went. */
  settle(): void {
    this.setInFlight(false);
    const waiting = this.waiting;
    this.waiting = null;
    if (waiting !== null && waiting.target !== this.sent?.target) this.request(waiting.target);
  }

  /** Drop everything, for an answer that is never coming back. */
  reset(): void {
    this.setInFlight(false);
    this.waiting = null;
    this.sent = null;
  }

  private setInFlight(inFlight: boolean): void {
    this.inFlight = inFlight;
    this.onInFlight(inFlight);
  }
}

/**
 * A {@link LatestSeek} for one page, and whether it has a seek out. It sends
 * through the newest `send`, and nothing waiting is sent once the page has
 * gone.
 */
export function useLatestSeek<T>(send: (target: T) => boolean): [LatestSeek<T>, boolean] {
  const sendRef = useRef(send);
  sendRef.current = send;
  const [isSeeking, setIsSeeking] = useState(false);
  const [seek] = useState(
    () => new LatestSeek<T>((target) => sendRef.current(target), setIsSeeking),
  );
  useEffect(() => () => seek.reset(), [seek]);
  return [seek, isSeeking];
}
