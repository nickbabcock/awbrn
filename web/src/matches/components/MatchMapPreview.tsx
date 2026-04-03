import { useEffect, useRef } from "react";
import { GameRunner } from "../../engine/game_runner";
import "./MatchMapPreview.css";

export function MatchMapPreview({
  mapId,
  className,
}: {
  mapId: number | null;
  className?: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const attachPromiseRef = useRef<Promise<void> | null>(null);
  const runnerRef = useRef<GameRunner | null>(null);

  if (runnerRef.current === null) {
    runnerRef.current = new GameRunner();
  }

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    const runner = runnerRef.current;
    if (!canvas || !container || !runner) {
      return;
    }

    attachPromiseRef.current = runner.attachCanvas({ canvas, container }).catch((error) => {
      console.error("Error attaching preview surface:", error);
    });

    return () => {
      attachPromiseRef.current = null;
      runner.detachCanvas(canvas);
    };
  }, []);

  useEffect(() => {
    const runner = runnerRef.current;
    if (!runner || mapId === null) {
      return;
    }

    let cancelled = false;

    void (attachPromiseRef.current ?? Promise.resolve())
      .then(async () => {
        if (cancelled) {
          return;
        }

        await runner.loadMapPreview(mapId);
      })
      .catch((error) => {
        console.error("Error loading map preview:", error);
      });

    return () => {
      cancelled = true;
    };
  }, [mapId]);

  return (
    <div className={className}>
      <div className="map-preview-frame">
        <div className="map-preview-surface" ref={containerRef}>
          <canvas
            className="map-preview-canvas"
            ref={canvasRef}
            width={600}
            height={400}
            tabIndex={-1}
          />
        </div>
      </div>
    </div>
  );
}
