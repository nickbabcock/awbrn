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
    width: "100%",
    height: "100%",
    imageRendering: "pixelated",
    outline: "none",
  },
});

export function MatchMapPreview({ mapId, xstyle }: { mapId: number | null; xstyle?: XStyle }) {
  const [runner] = useState(() => new GameRunner());

  useEffect(() => () => runner.dispose(), [runner]);

  const { canvasRef, surfaceRef } = useCanvasCourierSurface({
    controller: runner,
  });

  useEffect(() => {
    if (mapId === null) {
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
  }, [mapId, runner]);

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
