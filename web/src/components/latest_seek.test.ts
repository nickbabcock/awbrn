import { describe, expect, it } from "vitest";
import { LatestSeek } from "./latest_seek";

function host(isOpen = () => true) {
  const sent: (number | null)[] = [];
  const seek = new LatestSeek<number | null>((target) => {
    if (!isOpen()) return false;
    sent.push(target);
    return true;
  });
  return { seek, sent };
}

describe("pacing a dragged scrubber's seeks", () => {
  it("keeps one seek in flight and sends only the newest target behind it", () => {
    const { seek, sent } = host();
    seek.request(3);
    seek.request(4);
    seek.request(5);
    seek.request(6);
    expect(sent).toEqual([3]);

    seek.settle();
    expect(sent).toEqual([3, 6]);

    seek.settle();
    expect(sent).toEqual([3, 6]);
  });

  it("sends nothing more when the board already went where the thumb is", () => {
    const { seek, sent } = host();
    seek.request(3);
    seek.request(5);
    seek.request(3);
    seek.settle();
    expect(sent).toEqual([3]);
    expect(seek.flying).toBeNull();
  });

  it("lets a return to the end replace a moment still waiting", () => {
    const { seek, sent } = host();
    seek.request(3);
    seek.request(5);
    seek.request(null);
    seek.settle();
    expect(sent).toEqual([3, null]);
  });

  it("sends nothing waiting once it has been reset", () => {
    const { seek, sent } = host();
    seek.request(3);
    seek.request(5);
    seek.reset();
    seek.settle();
    expect(sent).toEqual([3]);
  });

  it("says when it stops having a seek out, so a let-go thumb knows to stop waiting", () => {
    const heard: boolean[] = [];
    const seek = new LatestSeek<number>(
      () => true,
      (inFlight) => heard.push(inFlight),
    );
    seek.request(3);
    seek.request(5);
    seek.settle();
    expect(heard.at(-1)).toBe(true);
    seek.settle();
    expect(heard.at(-1)).toBe(false);
    seek.request(7);
    seek.reset();
    expect(heard.at(-1)).toBe(false);
  });

  it("keeps a window of overlapping seeks out, and waits for all of them before one that cannot overlap", () => {
    const sent: (number | null)[] = [];
    const seek = new LatestSeek<number | null>(
      (target) => {
        sent.push(target);
        return true;
      },
      () => {},
      { window: 3, overlaps: (target) => target !== null },
    );
    seek.request(1);
    seek.request(2);
    seek.request(3);
    seek.request(4);
    seek.request(null);
    expect(sent).toEqual([1, 2, 3]);
    expect(seek.flying).toEqual({ target: 1 });

    seek.settle();
    seek.settle();
    expect(sent).toEqual([1, 2, 3]);
    seek.settle();
    expect(sent).toEqual([1, 2, 3, null]);
  });

  it("is not left waiting on a seek that never got out", () => {
    let isOpen = false;
    const { seek, sent } = host(() => isOpen);
    seek.request(3);
    expect(seek.flying).toBeNull();

    isOpen = true;
    seek.request(4);
    expect(sent).toEqual([4]);
  });
});
