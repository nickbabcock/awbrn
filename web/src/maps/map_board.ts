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
 * The grid every board of plates is dealt onto.
 *
 * The column count is high for the width because a plate's well cannot grow
 * with it: map art is drawn at a whole multiple of its own pixels, so a well
 * wider than the largest picture at that multiple is tan nothing. Narrow
 * columns keep the art against the edges of its well and fit more of the
 * catalog in one look.
 */
export const MAP_BOARD_COLUMNS = { minWidth: 156, max: 7, repeat: "fill" } as const;

/** Plates drawn while the first page of a board is still on its way. */
export const MAP_BOARD_LOADING_PLATES = 8;

/**
 * What a board holds, in the HUD voice.
 *
 * A narrowed board says what it found; a whole one says what AWBRN holds,
 * because those are two different facts and the count alone does not say
 * which one is on screen.
 */
export function mapBoardSummary({
  count,
  hasMore,
  isNarrowed,
  isPending,
}: {
  count: number;
  hasMore: boolean;
  isNarrowed: boolean;
  isPending: boolean;
}): string {
  if (isPending) return "Reading the catalog";
  const plural = count === 1 && !hasMore ? "" : "s";
  return `${count}${hasMore ? "+" : ""} map${plural} ${isNarrowed ? "found" : "held"}`;
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
  isNarrowed,
  isPending,
  search,
}: {
  count: number;
  filters: Required<MapCatalogFilter>;
  hasMore: boolean;
  isError: boolean;
  isNarrowed: boolean;
  isPending: boolean;
  /** The search that the catalog query was made with. */
  search: string;
}): string {
  const summary = mapBoardSummary({ count, hasMore, isNarrowed, isPending });
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
