import { useEffect, useRef } from "react";
import "./App.css";
import { proxy, transfer, wrap } from "comlink";
import { GameWorker } from "./worker_types";
import { GameEvent } from "awbrn-wasm";
import { useGameStore, useGameActions } from "./store";

let gameInstance: Awaited<ReturnType<GameWorker["createGame"]>> | undefined;

function App() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const currentDay = useGameStore((state) => state.currentDay);
  const { setCurrentDay } = useGameActions();

  const handleReplayFileChange = async (
    event: React.ChangeEvent<HTMLInputElement>,
  ) => {
    const file = event.target.files?.[0];
    if (file && gameInstance) {
      try {
        await gameInstance.newReplay(file);
      } catch (error) {
        console.error("Error loading replay:", error);
      }
    }
  };

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    if (canvas.dataset.transferred === "true") return;

    // Create an OffscreenCanvas from the visible canvas
    const container = document.getElementById("container")!;
    const bounds = container.getBoundingClientRect();

    const snapToDevicePixel = (size: number, ratio: number) => {
      const physicalSize = Math.floor(size * ratio);
      const logical = Math.floor(physicalSize / ratio);
      return { logical, physical: physicalSize };
    };
    const snappedWidth = snapToDevicePixel(
      bounds.width,
      window.devicePixelRatio,
    );
    const snappedHeight = snapToDevicePixel(
      bounds.height,
      window.devicePixelRatio,
    );

    canvas.width = snappedWidth.physical;
    canvas.height = snappedWidth.physical;
    canvas.style.width = `${snappedWidth.logical}px`;
    canvas.style.height = `${snappedHeight.logical}px`;

    const offscreenCanvas = canvas.transferControlToOffscreen();
    canvas.dataset.transferred = "true";

    const webWorker = new Worker(new URL("./worker.ts", import.meta.url), {
      type: "module",
    });

    let abortController = new AbortController();
    let worker = wrap<GameWorker>(webWorker);

    const setupGame = async () => {
      const game = await worker.createGame(
        transfer(offscreenCanvas, [offscreenCanvas]),
        {
          width: snappedWidth.logical,
          height: snappedHeight.logical,
          scaleFactor: window.devicePixelRatio,
        },
        proxy((event: GameEvent) => {
          switch (event.type) {
            case "NewDay": {
              console.log(`New day: ${event.day}`);
              setCurrentDay(event.day);
              break;
            }
            default: {
              break;
            }
          }
        })
      );

      gameInstance = game;

      const ro = new ResizeObserver((_entries) => {
        const bounds = container.getBoundingClientRect();

        const snappedWidth = snapToDevicePixel(
          bounds.width,
          window.devicePixelRatio,
        );
        const snappedHeight = snapToDevicePixel(
          bounds.height,
          window.devicePixelRatio,
        );

        canvas.style.width = `${snappedWidth.logical}px`;
        canvas.style.height = `${snappedHeight.logical}px`;

        game.resize({
          width: snappedWidth.logical,
          height: snappedHeight.logical,
        });
      });
      ro.observe(container);

      document.addEventListener(
        "keydown",
        (event) => {
          game.handleKeyDown({
            key: event.key,
            keyCode: event.code,
            repeat: event.repeat,
          });
        },
        { signal: abortController.signal },
      );

      document.addEventListener("keyup", (event) => {
        game.handleKeyUp({
          key: event.key,
          keyCode: event.code,
          repeat: event.repeat,
        });
      });

      // Pause game when tab is not visible
      const handleVisibilityChange = () => {
        if (document.hidden) {
          game.pause();
        } else {
          game.resume();
        }
      };

      document.addEventListener("visibilitychange", handleVisibilityChange);
    };

    setupGame();

    // return () => {
    //   abortController.abort();
    // };
  }, []);

  return (
    <>
      <div
        id="container"
        style={{
          width: "1000px",
          height: "800px",
          position: "absolute",
          top: "0",
          left: "0",
          right: "0",
          bottom: "0",
        }}
      >
        <canvas ref={canvasRef} width={600} height={400} />
      </div>
      <div
        style={{
          position: "fixed",
          top: "20px",
          left: "20px",
          zIndex: 10,
          backgroundColor: "rgba(0,0,0,0.7)",
          padding: "10px",
          borderRadius: "5px",
          color: "white",
          fontSize: "16px",
          fontWeight: "bold",
        }}
      >
        Day: {currentDay}
      </div>
      <div
        style={{
          position: "fixed",
          bottom: "20px",
          right: "20px",
          zIndex: 10,
          backgroundColor: "rgba(0,0,0,0.7)",
          padding: "10px",
          borderRadius: "5px",
        }}
      >
        <label
          htmlFor="replay-file-input"
          style={{
            color: "white",
            display: "block",
            marginBottom: "5px",
            fontSize: "14px",
          }}
        >
          Load Replay:
        </label>
        <input
          id="replay-file-input"
          type="file"
          accept=".zip"
          onChange={handleReplayFileChange}
          style={{
            color: "white",
            fontSize: "14px",
          }}
        />
      </div>
    </>
  );
}

export default App;
