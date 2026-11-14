import type { TableMode } from "#/matches/components/BoardControls.tsx";

/**
 * Where the build menu is mounted, for the table's layout. Each width mounts
 * it once: in the docked column beside the board, in the rail's View block
 * when the table has no column, or as a bottom sheet on a phone (which has
 * neither). Null while the table is still measuring.
 */
export type BuildMenuMount = "column" | "rail" | "sheet";

export function buildMenuMount(mode: TableMode): BuildMenuMount | null {
  switch (mode) {
    case "phone":
      return "sheet";
    case "dock":
    case "gather":
      return "column";
    case "none":
      return "rail";
    case "pending":
      return null;
  }
}
