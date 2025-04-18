import { useEffect, useRef, useState } from "react";
import reactLogo from "./assets/react.svg";
import viteLogo from "/vite.svg";
import "./App.css";
import { transfer, wrap } from "comlink";
import { GameWorker } from "./worker_types";

let worker: GameWorker | null = null;

function App() {
  const [count, setCount] = useState(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (worker || !canvas) return; // Prevent multiple transfers

    // Create an OffscreenCanvas from the visible canvas
    const container = document.getElementById("container")!;
    const bounds = container.getBoundingClientRect();
    canvas.width = bounds.width * window.devicePixelRatio;
    canvas.height = bounds.height * window.devicePixelRatio;
    canvas.style.width = `${bounds.width}px`;
    canvas.style.height = `${bounds.height}px`;

    const offscreenCanvas = canvas.transferControlToOffscreen();
    const webWorker = new Worker(new URL("./worker.ts", import.meta.url), {
      type: "module",
    });

    let abc = (worker = wrap(webWorker));

    const fnnn = async () => {
      const game = await abc.createGame(
        transfer(offscreenCanvas, [offscreenCanvas]),
        {
          width: bounds.width * window.devicePixelRatio,
          height: bounds.height * window.devicePixelRatio,
        },
      );

      let resiveObserverAF = 0;
      const ro = new ResizeObserver((_entries) => {
        // Why resive observer has RAF: https://stackoverflow.com/a/58701523
        cancelAnimationFrame(resiveObserverAF);
        resiveObserverAF = requestAnimationFrame(() => {
          const bounds = container.getBoundingClientRect();
          canvas.style.width = `${container.clientWidth}px`;
          canvas.style.height = `${container.clientHeight}px`;
          game.resize({
            width: bounds.width * window.devicePixelRatio,
            height: bounds.height * window.devicePixelRatio,
          });
        });
      });
      ro.observe(container);
    };

    fnnn();
  }, []);

  return (
    <>
      <div>
        <a href="https://vite.dev" target="_blank">
          <img src={viteLogo} className="logo" alt="Vite logo" />
        </a>
        <a href="https://react.dev" target="_blank">
          <img src={reactLogo} className="logo react" alt="React logo" />
        </a>
      </div>
      <h1>Vite + React</h1>
      <div className="card">
        <button onClick={() => setCount((count) => count + 1)}>
          count is {count}
        </button>
        <p>
          Edit <code>src/App.tsx</code> and save to test HMR
        </p>
      </div>
      <p className="read-the-docs">
        Click on the Vite and React logos to learn more
      </p>
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
    </>
  );
}

export default App;
