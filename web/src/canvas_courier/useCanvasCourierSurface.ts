import { useCallback, useLayoutEffect, useRef } from "react";
import type { CanvasCourierHost } from "./host";
import { useGameFullscreen } from "./useGameFullscreen";

export function useCanvasCourierSurface({
  host,
  canvasClassName,
}: {
  host: CanvasCourierHost;
  canvasClassName: string;
}) {
  const surfaceRef = useRef<HTMLElement>(null);
  const canvasContainerRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    host.setClassName(canvasClassName);
  }, [host, canvasClassName]);

  useLayoutEffect(() => {
    const container = canvasContainerRef.current;
    if (!container) return;

    host.attach(container);
    return () => host.detach(container);
  }, [host]);

  const focus = useCallback(() => {
    host.canvas?.focus({ preventScroll: true });
  }, [host]);

  const blur = useCallback(() => {
    host.canvas?.blur();
  }, [host]);

  const { enterFullscreen, exitFullscreen, isFullscreen, mode } = useGameFullscreen({
    focusSurface: focus,
    surfaceRef,
  });

  return {
    surfaceRef,
    canvasContainerRef,
    focus,
    blur,
    enterFullscreen,
    exitFullscreen,
    fullscreenMode: mode,
    isFullscreen,
  };
}
