import { Card } from "@astryxdesign/core/Card";
import { Section } from "@astryxdesign/core/Section";
import * as stylex from "@stylexjs/stylex";
import { useEffect } from "react";
import { useCanvasCourierSurface } from "#/canvas_courier/index.ts";
import type { GameRunner } from "#/engine/game_runner.ts";

const styles = stylex.create({
  viewport: {
    overflowX: "auto",
  },
  surface: {
    flex: "0 0 auto",
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

export function MatchMapPreview({ mapId, runner }: { mapId: number | null; runner: GameRunner }) {
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
    <Card padding={4} width="100%" xstyle={styles.viewport}>
      <Section
        height={400}
        padding={0}
        ref={surfaceRef}
        variant="muted"
        width={600}
        xstyle={styles.surface}
      >
        <canvas
          ref={canvasRef}
          width={600}
          height={400}
          tabIndex={-1}
          {...stylex.props(styles.canvas)}
        />
      </Section>
    </Card>
  );
}
