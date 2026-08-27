/*
 * THE MAP CATALOG
 *
 * THESIS: the maps AWBRN holds are a place, not a step inside creating a
 *   match. A player who wants to know what can be played here, or a moderator
 *   who wants to find the maps nobody has graded yet, arrives at a board and
 *   presses on it. Every board it can be narrowed to has its own address, so
 *   "the unranked two-player fog maps" is a link somebody can send.
 * OWN-WORLD: the same board of outlined cream plates on the open sky the
 *   create screen deals, with each map's grade stamped on its corner the way
 *   the game stamps a battle report.
 * STORY: a player reads the console, presses the keys that describe the game
 *   they want, and opens the map that looks right.
 * FIRST VIEWPORT: the title, the console, then plates.
 */

import { useInfiniteQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { useEffect, useMemo, useState } from "react";
import { Button } from "#/ui/Button.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { MapFilterBar } from "#/maps/components/MapFilterBar.tsx";
import { MapLinkPlate, MapLoadingPlate, boardPictureSize } from "#/maps/components/MapPlate.tsx";
import {
  mapBoardAddress,
  mapBoardFilters,
  mapBoardSearchText,
  type MapBoardSearch,
} from "#/maps/map_board_search.ts";
import { mapCatalogQueryOptions } from "#/maps/maps.queries.ts";
import { countMapCatalogFilters } from "#/maps/map_taxonomy.ts";
import { MAP_BOARD_COLUMNS, MAP_BOARD_LOADING_PLATES, mapBoardSummary } from "#/maps/map_board.ts";
import type { MapCatalogFilter } from "#/maps/schemas.ts";

/** How long the console waits after a keystroke before it writes the address. */
const SEARCH_DEBOUNCE_MS = 250;

export function MapsBoardPage({ search }: { search: MapBoardSearch }) {
  const navigate = useNavigate();

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
  const boardPicture = useMemo(() => boardPictureSize(maps), [maps]);

  const filterCount = countMapCatalogFilters(filters);
  const isNarrowed = text.length > 0 || filterCount > 0;
  // A failed read holds nothing, which is not the same as a board that holds
  // nothing. The banner above says why, so the empty state stays out of it.
  const isBoardEmpty = !catalogQuery.isPending && !catalogQuery.isError && maps.length === 0;

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={8}>
        <HStack align="end" gap={4} justify="between" wrap="wrap">
          <VStack gap={2}>
            <Heading level={1} type="display-2">
              Maps
            </Heading>
            <Text color="secondary" type="large">
              Every battlefield AWBRN holds. Open one to read its record, or start a match on it.
            </Text>
          </VStack>
          <RouterButton label="New match" to="/matches/new" variant="primary" />
        </HStack>

        <VStack gap={4}>
          <Card padding={4}>
            <MapFilterBar
              filterCount={filterCount}
              filters={filters}
              onFiltersChange={handleFiltersChange}
              onSearchChange={setTyped}
              search={typed}
              summary={mapBoardSummary({
                count: maps.length,
                hasMore: catalogQuery.hasNextPage,
                isNarrowed,
                isPending: catalogQuery.isPending,
              })}
            />
          </Card>

          {catalogQuery.isError ? (
            <Banner
              description="The map catalog could not be read. Try again in a moment."
              status="error"
              title="Catalog unavailable"
            />
          ) : null}

          {isBoardEmpty ? null : (
            <Grid columns={MAP_BOARD_COLUMNS} gap={4}>
              {catalogQuery.isPending
                ? Array.from({ length: MAP_BOARD_LOADING_PLATES }, (_, index) => (
                    <MapLoadingPlate index={index} key={index} />
                  ))
                : maps.map((map) => (
                    <MapLinkPlate boardPicture={boardPicture} key={map.mapId} map={map} />
                  ))}
            </Grid>
          )}

          {isBoardEmpty ? (
            <EmptyState
              actions={
                isNarrowed ? (
                  <Button
                    clickAction={() => void navigate({ search: {}, to: "/maps" })}
                    label="Clear the board"
                    size="sm"
                    variant="secondary"
                  />
                ) : (
                  <RouterButton label="Import a map" to="/matches/new" variant="primary" />
                )
              }
              description={
                isNarrowed
                  ? "No map the catalog holds answers all of it. Widen the console, or bring the map over from AWBW."
                  : "Every map AWBRN plays is imported once and then held for everybody. The board fills as maps arrive."
              }
              headingLevel={2}
              title={isNarrowed ? "No map fits that brief" : "The catalog is empty"}
            />
          ) : null}

          {catalogQuery.hasNextPage ? (
            <HStack justify="center">
              <Button
                clickAction={async () => {
                  await catalogQuery.fetchNextPage();
                }}
                isLoading={catalogQuery.isFetchingNextPage}
                label="More maps"
                size="sm"
                variant="secondary"
              />
            </HStack>
          ) : null}
        </VStack>
      </VStack>
    </Section>
  );
}
