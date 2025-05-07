import { useEffect, useRef } from "react";
import "./App.css";
import { transfer, wrap } from "comlink";
import { GameWorker } from "./worker_types";

function App() {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    if (canvas.dataset.transferred === "true") return;

    // Create an OffscreenCanvas from the visible canvas
    const container = document.getElementById("container")!;
    const bounds = container.getBoundingClientRect();
    canvas.width = bounds.width * window.devicePixelRatio;
    canvas.height = bounds.height * window.devicePixelRatio;
    canvas.style.width = `${bounds.width}px`;
    canvas.style.height = `${bounds.height}px`;

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
          width: bounds.width * window.devicePixelRatio,
          height: bounds.height * window.devicePixelRatio,
        },
      );

      const ro = new ResizeObserver((_entries) => {
        const bounds = container.getBoundingClientRect();
        canvas.style.width = `${container.clientWidth}px`;
        canvas.style.height = `${container.clientHeight}px`;
        game.resize({
          width: bounds.width * window.devicePixelRatio,
          height: bounds.height * window.devicePixelRatio,
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
    };

    setupGame();

    // return () => {
    //   abortController.abort();
    // };
  }, []);

  return (
    <div
      id="container"
      style={{
        width: "100%",
        height: "100%",
        position: "absolute",
        top: "0",
        left: "0",
        right: "0",
        bottom: "0",
      }}
    >
      <canvas ref={canvasRef} width={600} height={400} />
    </div>
  );
}

export default App;
