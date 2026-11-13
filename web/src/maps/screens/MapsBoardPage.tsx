// Every narrowing lives in the address, so a filtered set is a link somebody can send.

import { Icon } from "@astryxdesign/core/Icon";
import { Stack } from "@astryxdesign/core/Stack";
import * as stylex from "@stylexjs/stylex";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { useActor } from "#/auth/useActor.ts";
import { MapFilterBar } from "#/maps/components/MapFilterBar.tsx";
import { MapImportKey } from "#/maps/components/MapImportSheet.tsx";
import { MapCardGrid, MapLinkPlate, MapLoadingPlate } from "#/maps/components/MapPlate.tsx";
import { MAP_BOARD_LOADING_PLATES, useMapBoardSummary } from "#/maps/map_board.ts";
import {
  mapBoardAddress,
  mapBoardFilters,
  mapBoardSearchText,
  type MapBoardSearch,
} from "#/maps/map_board_search.ts";
import { countMapCatalogFilters } from "#/maps/map_taxonomy.ts";
import { mapCatalogQueryOptions } from "#/maps/maps.queries.ts";
import type { MapCatalogFilter } from "#/maps/schemas.ts";
import { Button } from "#/ui/Button.tsx";
import { EmptySheet, ProblemBanner } from "#/ui/EmptySheet.tsx";
import { IconRefresh } from "#/ui/icons.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";

const SEARCH_DEBOUNCE_MS = 250;

export function MapsBoardPage({ search }: { search: MapBoardSearch }) {
  const navigate = useNavigate();
  const actor = useActor();

  const text = mapBoardSearchText(search);
  const filters = useMemo(() => mapBoardFilters(search), [search]);

  // The field runs ahead of the address so typing stays immediate; the
  // address catches up once the typing stops.
  const [typed, setTyped] = useState(text);
  useEffect(() => setTyped(text), [text]);

  useEffect(() => {
    if (typed.trim() === text) return;
    const timer = setTimeout(() => {
      void navigate({ search: mapBoardAddress(typed, filters), to: "/maps" });
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [filters, navigate, text, typed]);

  function handleFiltersChange(next: Required<MapCatalogFilter>): void {
    void navigate({ search: mapBoardAddress(typed, next), to: "/maps" });
  }

  const catalogQuery = useInfiniteQuery(mapCatalogQueryOptions(text, filters));
  const maps = useMemo(
    () => catalogQuery.data?.pages.flatMap((page) => page.maps) ?? [],
    [catalogQuery.data],
  );

  const filterCount = countMapCatalogFilters(filters);
  const isNarrowed = text.length > 0 || filterCount > 0;
  // A failed read holds nothing, which is not the same as a catalog that
  // holds nothing. The banner says why, so the empty state stays out of it.
  const isEmpty = !catalogQuery.isPending && !catalogQuery.isError && maps.length === 0;
  const summary = useMapBoardSummary({
    count: maps.length,
    filters,
    hasMore: catalogQuery.hasNextPage,
    isError: catalogQuery.isError,
    isPending: catalogQuery.isPending,
    search: text,
  });

  const mayImport = actor?.can({ map: ["import"] }) ?? false;

  return (
    <Page>
      {/* The filters sit under the head at the head's own 16px, not the page's 24px. */}
      <Stack xstyle={styles.flow}>
        <PageHeader actions={mayImport ? <MapImportKey /> : undefined} title="Maps" />
        <MapFilterBar
          filterCount={filterCount}
          filters={filters}
          onFiltersChange={handleFiltersChange}
          onSearchChange={setTyped}
          search={typed}
          summary={summary}
        />
      </Stack>

      {catalogQuery.isError ? (
        <ProblemBanner
          action={
            <Button
              clickAction={async () => {
                await catalogQuery.refetch();
              }}
              icon={<Icon icon={IconRefresh} />}
              label="Retry"
              size="sm"
            />
          }
          title="Maps failed to load"
        />
      ) : null}

      {isEmpty ? (
        isNarrowed ? (
          <EmptySheet
            action={
              <Button
                label="Clear"
                onClick={() => void navigate({ search: {}, to: "/maps" })}
                variant="secondary"
              />
            }
            title="No maps match"
          />
        ) : (
          <EmptySheet title="No maps yet" />
        )
      ) : (
        <MapCardGrid>
          {catalogQuery.isPending
            ? Array.from({ length: MAP_BOARD_LOADING_PLATES }, (_, index) => (
                <MapLoadingPlate index={index} key={index} />
              ))
            : maps.map((map) => <MapLinkPlate key={map.mapId} map={map} />)}
        </MapCardGrid>
      )}

      {catalogQuery.hasNextPage ? (
        <Stack direction="horizontal" xstyle={styles.more}>
          <Button
            clickAction={async () => {
              await catalogQuery.fetchNextPage();
            }}
            isLoading={catalogQuery.isFetchingNextPage}
            label="More maps"
            variant="secondary"
          />
        </Stack>
      ) : null}
    </Page>
  );
}

const styles = stylex.create({
  flow: {
    display: "block",
  },
  more: {
    display: "flex",
    justifyContent: "center",
  },
});
