/**
 * What every board of maps shares.
 *
 * Two screens deal the same board: the catalog, where a plate opens a map,
 * and the create screen, where a plate is chosen. They are one board and are
 * held to one column rule and one readout, so a map does not change size or
 * change what it says about itself depending on why it is being looked at.
 */

import { useAnnounce } from "@astryxdesign/core/hooks";
import { useEffect, useRef } from "react";
import { normalizeMapSearch } from "#/maps/map_catalog.ts";
import { normalizeMapCatalogFilters } from "#/maps/map_taxonomy.ts";
import type { MapCatalogFilter } from "#/maps/schemas.ts";

/**
 * @deprecated Deal cards onto `MapCardGrid` (maps/components/MapPlate.tsx),
 * which holds the three, two and one column rule and the shared well rows.
 */
export const MAP_BOARD_COLUMNS = { minWidth: 156, max: 7, repeat: "fill" } as const;

/** Plates drawn while the first page of a board is still on its way. */
export const MAP_BOARD_LOADING_PLATES = 8;

/**
 * What a board holds, as a figure: `8 maps`, `1 map`, `24+ maps` while more
 * pages remain. Nothing while the first read is on its way.
 */
export function mapBoardSummary({
  count,
  hasMore,
  isPending,
}: {
  count: number;
  hasMore: boolean;
  isPending: boolean;
}): string {
  if (isPending) return "";
  const plural = count === 1 && !hasMore ? "" : "s";
  return `${count}${hasMore ? "+" : ""} map${plural}`;
}

/**
 * The summary of a board, which is also told to a screen reader.
 *
 * The count changes only on screen, so it is announced each time a search or
 * a filter settles, even when the count is the same as the last one, and each
 * time more maps load. It is also announced when a read succeeds after a
 * failed one, because the error banner goes away without a word. The first
 * count arrives with the page, which already says it, so it is not announced. Nothing is announced while the board says
 * the result in another way: an empty board shows an empty state, and a failed
 * read shows an error banner, which both announce themselves.
 */
export function useMapBoardSummary({
  count,
  filters,
  hasMore,
  isError,
  isPending,
  search,
}: {
  count: number;
  filters: Required<MapCatalogFilter>;
  hasMore: boolean;
  isError: boolean;
  /** @deprecated The count says it alone; a narrowed set no longer says so. */
  isNarrowed?: boolean;
  isPending: boolean;
  /** The search that the catalog query was made with. */
  search: string;
}): string {
  const summary = mapBoardSummary({ count, hasMore, isPending });
  const isAnnouncedElsewhere = isError || (!isPending && count === 0);
  // The catalog query normalizes the search and the filters the same way, so
  // one query has one key here.
  const searchKey = JSON.stringify([
    normalizeMapSearch(search),
    normalizeMapCatalogFilters(filters),
  ]);
  const announce = useAnnounce({ hasReplace: true });
  const last = useRef<{ isError: boolean; searchKey: string; summary: string } | null>(null);

  useEffect(() => {
    if (isPending) return;
    const previous = last.current;
    if (
      previous?.isError === isError &&
      previous.searchKey === searchKey &&
      previous.summary === summary
    ) {
      return;
    }
    last.current = { isError, searchKey, summary };
    if (previous === null || isAnnouncedElsewhere) return;
    announce(summary);
  }, [announce, isAnnouncedElsewhere, isError, isPending, searchKey, summary]);

  return summary;
}
