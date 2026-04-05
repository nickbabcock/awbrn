import { describe, expect, it } from "vitest";
import {
  SharedCanvasEventAction,
  SharedCanvasEventType,
  SharedCanvasInputReader,
  SharedCanvasInputWriter,
} from "./ring_buffer";
import type { SharedCanvasDecodedEvent, SharedCanvasInputConfig } from "./ring_buffer";

function createTestConfig(capacity: number): SharedCanvasInputConfig {
  return {
    buffer: new SharedArrayBuffer(64 + capacity * 32),
    capacity,
  };
}

describe("SharedCanvasInputWriter", () => {
  it("preserves FIFO ordering across lifecycle events", () => {
    const config = createTestConfig(8);
    const writer = new SharedCanvasInputWriter(config);
    const reader = new SharedCanvasInputReader(config);
    const drained: SharedCanvasDecodedEvent[] = [];

    writer.enqueueVisibility(true, 1);
    writer.enqueueResize({ width: 320, height: 240, scaleFactor: 2 }, 2);
    writer.enqueueBlur(3);

    reader.drain((event) => {
      drained.push(event);
    });

    expect(drained).toEqual([
      {
        type: SharedCanvasEventType.Visibility,
        action: SharedCanvasEventAction.Hidden,
        timestamp: 1,
      },
      {
        type: SharedCanvasEventType.Resize,
        action: SharedCanvasEventAction.Resize,
        width: 320,
        height: 240,
        scaleFactor: 2,
        timestamp: 2,
      },
      {
        type: SharedCanvasEventType.Focus,
        action: SharedCanvasEventAction.Blur,
        timestamp: 3,
      },
    ]);
  });

  it("throws when the buffer is full", () => {
    const config = createTestConfig(4);
    const writer = new SharedCanvasInputWriter(config);

    writer.enqueueVisibility(true, 1);
    writer.enqueueBlur(2);
    writer.enqueueVisibility(false, 3);

    expect(() => writer.enqueueBlur(4)).toThrow("Shared canvas input ring buffer overflowed.");
  });
});

describe("SharedCanvasInputReader.waitForEvents", () => {
  it("resolves immediately when events are already queued", async () => {
    const config = createTestConfig(8);
    const writer = new SharedCanvasInputWriter(config);
    const reader = new SharedCanvasInputReader(config);

    writer.enqueueBlur(1);

    await reader.waitForEvents();

    const drained: SharedCanvasDecodedEvent[] = [];
    reader.drain((e) => drained.push(e));
    expect(drained).toHaveLength(1);
  });

  it("resolves when an event is enqueued after waiting begins", async () => {
    const config = createTestConfig(8);
    const writer = new SharedCanvasInputWriter(config);
    const reader = new SharedCanvasInputReader(config);

    const waitPromise = reader.waitForEvents();
    writer.enqueueBlur(1);
    await waitPromise;

    const drained: SharedCanvasDecodedEvent[] = [];
    reader.drain((e) => drained.push(e));
    expect(drained).toHaveLength(1);
  });

  it("resolves immediately when signal is already aborted", async () => {
    const config = createTestConfig(8);
    const reader = new SharedCanvasInputReader(config);
    const controller = new AbortController();
    controller.abort();

    await reader.waitForEvents(controller.signal);
  });

  it("resolves when signal is aborted while waiting", async () => {
    const config = createTestConfig(8);
    const reader = new SharedCanvasInputReader(config);
    const controller = new AbortController();

    const waitPromise = reader.waitForEvents(controller.signal);
    controller.abort();
    await waitPromise;
  });
});
