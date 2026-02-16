import { useEffect, useMemo, useRef } from "react";
import "./App.css";
import type { GameEvent } from "./game_events";
import { createRuntime, isDesktopRuntime, type GameRuntime } from "./runtime";
import { useGameActions, useGameStore } from "./store";

function App() {
  const isDesktop = useMemo(() => isDesktopRuntime(), []);
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const runtimeRef = useRef<GameRuntime | null>(null);
  const currentDay = useGameStore((state) => state.currentDay);
  const { setCurrentDay } = useGameActions();

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    let disposed = false;

    const onEvent = (event: GameEvent) => {
      if (event.type === "NewDay") {
        setCurrentDay(event.day);
      }
    };

    void (async () => {
      try {
        const runtime = await createRuntime();
        if (disposed) {
          runtime.dispose();
          return;
        }

        runtimeRef.current = runtime;
        await runtime.bootstrap({
          container,
          canvas: canvasRef.current,
          onEvent,
        });
      } catch (error) {
        console.error("Failed to bootstrap runtime", error);
      }
    })();

    return () => {
      disposed = true;
      runtimeRef.current?.dispose();
      runtimeRef.current = null;
    };
  }, [setCurrentDay]);

  const handleReplayFileChange = async (
    event: React.ChangeEvent<HTMLInputElement>,
  ) => {
    const file = event.target.files?.[0];
    if (!file || !runtimeRef.current) {
      return;
    }

    try {
      const data = new Uint8Array(await file.arrayBuffer());
      await runtimeRef.current.loadReplay(data);
    } catch (error) {
      console.error("Error loading replay:", error);
    } finally {
      event.target.value = "";
    }
  };

  return (
    <>
      <div id="container" ref={containerRef}>
        {!isDesktop && <canvas ref={canvasRef} width={600} height={400} />}
      </div>

      <div id="day-badge">Day: {currentDay}</div>

      <div id="replay-loader" data-map-input-stop="true">
        <label htmlFor="replay-file-input">Load Replay:</label>
        <input
          id="replay-file-input"
          type="file"
          accept=".zip"
          onChange={handleReplayFileChange}
        />
      </div>
    </>
  );
}

export default App;
