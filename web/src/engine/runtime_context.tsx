import { createContext, useContext, useEffect, useRef, type ReactNode } from "react";
import { useLocation } from "@tanstack/react-router";
import { useGameStore } from "./store";
import { GameRuntimeRegistry, type PreviewRunnerScope } from "./runtime_registry";

const GameRuntimeContext = createContext<GameRuntimeRegistry | null>(null);

export function GameRuntimeProvider({ children }: { children: ReactNode }) {
  const pathname = useLocation({
    select: (location) => location.pathname,
  });
  const registryRef = useRef<GameRuntimeRegistry | null>(null);

  registryRef.current ??= new GameRuntimeRegistry(undefined, {
    onDisposeReplay: () => {
      useGameStore.getState().actions.reset();
    },
  });

  useEffect(() => {
    registryRef.current?.syncPathname(pathname);
  }, [pathname]);

  useEffect(() => {
    return () => {
      registryRef.current?.disposeAll();
    };
  }, []);

  return (
    <GameRuntimeContext.Provider value={registryRef.current}>
      {children}
    </GameRuntimeContext.Provider>
  );
}

function useGameRuntimeRegistry(): GameRuntimeRegistry {
  const registry = useContext(GameRuntimeContext);
  if (!registry) {
    throw new Error("GameRuntimeProvider is required for game runtime hooks.");
  }

  return registry;
}

export function usePreviewRunner(scope: PreviewRunnerScope) {
  return useGameRuntimeRegistry().getPreviewRunner(scope);
}

export function useReplayRunner() {
  return useGameRuntimeRegistry().getReplayRunner();
}
