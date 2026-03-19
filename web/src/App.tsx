import { useEffect, useRef } from "react";
import "./App.css";
import { gameRunner } from "./game_runner";
import { useGameStore } from "./store";

function App() {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const currentDay = useGameStore((state) => state.currentDay);

  const handleReplayFileChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (file) {
      try {
        await gameRunner.loadReplay(file);
        canvasRef.current?.focus({ preventScroll: true });
      } catch (error) {
        console.error("Error loading replay:", error);
      }
    }
  };

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    gameRunner.attachCanvas({ canvas, container }).catch((error) => {
      console.error("Error attaching game runner:", error);
    });

    return () => {
      gameRunner.detachCanvas(canvas);
    };
  }, []);

  return (
    <>
      <div
        ref={containerRef}
        style={{
          position: "absolute",
          top: "0",
          left: "0",
        }}
      >
        <canvas
          className="game-canvas"
          ref={canvasRef}
          width={600}
          height={400}
          tabIndex={0}
          style={{ display: "block" }}
        />
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
