import * as stylex from "@stylexjs/stylex";
import { useEffect, useState } from "react";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import { GameRunner } from "#/engine/game_runner.ts";
import { tokens } from "#/ui/theme.stylex.ts";
import type { XStyle } from "#/ui/stylex.ts";

const styles = stylex.create({
  root: {
    width: "100%",
  },
  frame: {
    display: "flex",
    justifyContent: "flex-start",
    overflow: "auto",
    minHeight: 240,
    borderWidth: 3,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius3,
    backgroundImage:
      "linear-gradient(180deg, rgba(23, 28, 40, 0.88), rgba(8, 11, 18, 0.96)), radial-gradient(circle at top, rgba(255, 255, 255, 0.08), transparent 55%)",
    boxShadow: `${tokens.highlightInsetChrome}, ${tokens.shadowHardLg}`,
    padding: tokens.space4,
  },
  surface: {
    flex: "0 0 auto",
    width: 600,
    height: 400,
    overflow: "hidden",
  },
  canvas: {
    display: "block",
    imageRendering: "pixelated",
    outline: "none",
  },
});

export function MatchMapPreview({ mapId, xstyle }: { mapId: number | null; xstyle?: XStyle }) {
  const [runner, setRunner] = useState<GameRunner | null>(null);

  useEffect(() => {
    const nextRunner = new GameRunner();
    setRunner(nextRunner);

    return () => {
      nextRunner.dispose();
      setRunner((current) => (current === nextRunner ? null : current));
    };
  }, []);

  const { canvasRef, status, surfaceRef } = useCanvasCourierSurface({
    controller: runner,
    onError: (error) => {
      console.error("Error attaching preview surface:", error);
    },
  });

  useEffect(() => {
    if (!runner || mapId === null || !status.attached) {
      return;
    }

    let cancelled = false;

    void Promise.resolve()
      .then(async () => {
        if (!cancelled) {
          await runner.loadMapPreview(mapId);
        }
      })
      .catch((error) => {
        console.error("Error loading map preview:", error);
      });

    return () => {
      cancelled = true;
    };
  }, [mapId, status.attached]);

  return (
    <div {...stylex.props(styles.root, xstyle)}>
      <div {...stylex.props(styles.frame)}>
        <div ref={surfaceRef} {...stylex.props(styles.surface)}>
          <canvas
            ref={canvasRef}
            width={600}
            height={400}
            tabIndex={-1}
            {...stylex.props(styles.canvas)}
          />
        </div>
      </div>
    </div>
  );
}
