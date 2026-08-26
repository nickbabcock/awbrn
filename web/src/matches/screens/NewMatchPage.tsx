/*
 * THE MAP BOARD
 *
 * THESIS: creating a match starts with a map you can see, not an id you have
 *   to know. The board of maps AWBRN holds is the screen; the settings form is
 *   what happens after a map is chosen. It refuses the form-first arrangement
 *   this screen used to have, where the battlefield was a number in a field.
 * OWN-WORLD: outlined cream plates cast onto the open sky, map art at native
 *   pixels in a recessed tan well, HUD readouts under every plate, and one
 *   command orange that marks the chosen map and the launch key.
 * STORY: a player sees what AWBRN can play, picks a battlefield by its shape,
 *   reads its briefing, dials the rules, and opens the lobby.
 * FIRST VIEWPORT: the page title, then the board itself: the AWBW import slot
 *   at the head, then every map as a plate. The briefing panel and the create
 *   key follow the board, under the map that was chosen.
 * FORM: the map board, index 5 of the ordered structures, seed key c7f7ecc5.
 * FINISH: unreviewed and undocumented is unfinished; this build ends with the
 *   finish review, the verdict, DESIGN.md, and every shipping raster carrying
 *   its provenance.
 */

import { useInfiniteQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AspectRatio } from "@astryxdesign/core/AspectRatio";
import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Grid, GridSpan } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { NumberInput } from "@astryxdesign/core/NumberInput";
import { Section } from "@astryxdesign/core/Section";
import { SelectableCard } from "@astryxdesign/core/SelectableCard";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { Button } from "#/ui/Button.tsx";
import { RouterTextLink } from "#/ui/astryx-links.tsx";
import { importAwbwMapFn } from "#/maps/maps.functions.ts";
import { mapCatalogQueryOptions } from "#/maps/maps.queries.ts";
import { mapKeys } from "#/maps/maps.keys.ts";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import type { MapCatalogEntry } from "#/maps/schemas.ts";
import { createMatchFn } from "#/matches/matches.functions.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";

/** How long the board waits after a keystroke before it searches. */
const SEARCH_DEBOUNCE_MS = 250;

/** Plates drawn while the first page of the board is still on its way. */
const LOADING_PLATE_COUNT = 8;

/** Open slots drawn beside the import panel while the catalog is empty. */
const OPEN_SLOT_COUNT = 6;

export function NewMatchPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const session = useAppSession();

  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [selectedMap, setSelectedMap] = useState<MapCatalogEntry | null>(null);

  const [matchName, setMatchName] = useState("");
  const [fogEnabled, setFogEnabled] = useState(false);
  const [startingFunds, setStartingFunds] = useState(1000);
  const [isPrivate, setIsPrivate] = useState(false);
  const [hotseatEnabled, setHotseatEnabled] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  // The name follows the map until the player writes their own.
  const autoMatchNameRef = useRef<string | null>(null);

  // A board can be taller than the viewport, so choosing a map has to bring
  // its briefing to the player rather than changing an outline off screen.
  const briefingRef = useRef<HTMLDivElement>(null);
  const revealBriefingRef = useRef(false);

  useEffect(() => {
    if (!revealBriefingRef.current) return;
    revealBriefingRef.current = false;
    briefingRef.current?.scrollIntoView({ block: "nearest" });
  }, [selectedMap]);

  useEffect(() => {
    const timer = setTimeout(() => setSearch(searchInput), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [searchInput]);

  const catalogQuery = useInfiniteQuery(mapCatalogQueryOptions(search));
  const catalogMaps = useMemo(
    () => catalogQuery.data?.pages.flatMap((page) => page.maps) ?? [],
    [catalogQuery.data],
  );
  const isSearching = search.trim().length > 0;

  const boardPictureSize = useMemo(
    () =>
      catalogMaps.reduce(
        (largest, map) => {
          const picture = mapScreenshotSize("small", map.width, map.height);
          return {
            width: Math.max(largest.width, picture.width),
            height: Math.max(largest.height, picture.height),
          };
        },
        { width: 1, height: 1 },
      ),
    [catalogMaps],
  );

  const createMatchMutation = useMutation({
    mutationFn: createMatchFn,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: matchKeys.browse() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.mine() }),
      ]);
    },
  });

  function handleSelectMap(map: MapCatalogEntry): void {
    setSelectedMap(map);
    setCreateError(null);
    revealBriefingRef.current = true;
    if (!matchName.trim() || matchName === autoMatchNameRef.current) {
      autoMatchNameRef.current = map.name;
      setMatchName(map.name);
    }
  }

  async function handleImported(map: MapCatalogEntry): Promise<void> {
    await queryClient.invalidateQueries({ queryKey: mapKeys.all });
    handleSelectMap(map);
  }

  async function handleCreateLobby(): Promise<void> {
    if (!session) {
      setCreateError("Sign in to create a match.");
      return;
    }
    if (!selectedMap) {
      setCreateError("Choose a map from the board first.");
      return;
    }
    if (!Number.isSafeInteger(startingFunds) || startingFunds < 0) {
      setCreateError("Starting funds must be a whole number, zero or above.");
      return;
    }
    if (!matchName.trim()) {
      setCreateError("Name the match before it opens.");
      return;
    }

    setCreateError(null);
    try {
      const match = await createMatchMutation.mutateAsync({
        data: {
          name: matchName.trim(),
          map: { mapId: selectedMap.mapId, revision: selectedMap.revision },
          isPrivate,
          settings: { fogEnabled, startingFunds, hotseatEnabled },
        },
      });
      await navigate({ to: "/matches/$matchId", params: { matchId: match.matchId } });
    } catch (error) {
      setCreateError(error instanceof Error ? error.message : "The lobby could not be opened.");
    }
  }

  const isBoardEmpty = !catalogQuery.isPending && catalogMaps.length === 0;

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={8}>
        <VStack gap={2}>
          <Heading level={1} type="display-2">
            Create match
          </Heading>
          <Text color="secondary" type="large">
            Pick a battlefield from the maps AWBRN holds, or bring one over from AWBW.
          </Text>
        </VStack>

        <VStack gap={4}>
          {isBoardEmpty && !isSearching ? null : (
            <HStack align="end" gap={4} justify="between" wrap="wrap">
              <TextInput
                hasClear
                label="Search the map catalog"
                onChange={setSearchInput}
                placeholder="Map name or author"
                startIcon="search"
                value={searchInput}
                width={320}
              />
              <Text type="label">
                {catalogQuery.isPending
                  ? "Reading the catalog"
                  : `${catalogMaps.length}${catalogQuery.hasNextPage ? "+" : ""} map${
                      catalogMaps.length === 1 && !catalogQuery.hasNextPage ? "" : "s"
                    } ${isSearching ? "found" : "held"}`}
              </Text>
            </HStack>
          )}

          {catalogQuery.isError ? (
            <Banner
              description="The map catalog could not be read. Try again in a moment."
              status="error"
              title="Catalog unavailable"
            />
          ) : null}

          <Grid align="start" columns={{ minWidth: 156, max: 5, repeat: "fill" }} gap={4}>
            {isBoardEmpty && !isSearching ? (
              <GridSpan columns={2}>
                <FirstMapPanel isSignedIn={session !== null} onImported={handleImported} />
              </GridSpan>
            ) : (
              <ImportPlate isSignedIn={session !== null} onImported={handleImported} />
            )}
            {isBoardEmpty && !isSearching
              ? Array.from({ length: OPEN_SLOT_COUNT }, (_, index) => <OpenSlot key={index} />)
              : null}
            {catalogQuery.isPending
              ? Array.from({ length: LOADING_PLATE_COUNT }, (_, index) => (
                  <LoadingPlate index={index} key={index} />
                ))
              : catalogMaps.map((map) => (
                  <MapPlate
                    boardPictureSize={boardPictureSize}
                    isSelected={selectedMap?.mapId === map.mapId}
                    key={map.mapId}
                    map={map}
                    onSelect={handleSelectMap}
                  />
                ))}
          </Grid>

          {isBoardEmpty && isSearching ? (
            <EmptyState
              description={`Nothing in the catalog matches "${search}". Import the map from AWBW and it joins the board.`}
              headingLevel={2}
              isCompact
              title="No map by that name"
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

        {selectedMap ? (
          <MapBriefing
            briefingRef={briefingRef}
            createError={createError}
            fogEnabled={fogEnabled}
            hotseatEnabled={hotseatEnabled}
            isCreating={createMatchMutation.isPending}
            isPrivate={isPrivate}
            map={selectedMap}
            matchName={matchName}
            onCreate={handleCreateLobby}
            onFogChange={setFogEnabled}
            onHotseatChange={setHotseatEnabled}
            onMatchNameChange={(value) => {
              autoMatchNameRef.current = null;
              setMatchName(value);
            }}
            onPrivateChange={setIsPrivate}
            onStartingFundsChange={setStartingFunds}
            session={session}
            startingFunds={startingFunds}
          />
        ) : catalogMaps.length > 0 ? (
          <EmptyState
            description="Choose a map from the board to read its briefing and set the rules."
            headingLevel={2}
            isCompact
            title="No map chosen"
          />
        ) : null}
      </VStack>
    </Section>
  );
}

function MapPlate({
  boardPictureSize,
  isSelected,
  map,
  onSelect,
}: {
  /** The largest picture on this board, which sets the multiple for all of them. */
  boardPictureSize: { width: number; height: number };
  isSelected: boolean;
  map: MapCatalogEntry;
  onSelect: (map: MapCatalogEntry) => void;
}) {
  const picture = mapScreenshotSize("small", map.width, map.height);

  return (
    <SelectableCard
      isSelected={isSelected}
      label={`${map.name} by ${map.author}, ${map.playerCount} players, ${map.width} by ${map.height}`}
      onChange={() => onSelect(map)}
      padding={2}
    >
      <VStack gap={2}>
        <MapPicture
          alt=""
          ratio={1}
          scaleFrom={boardPictureSize}
          sourceHeight={picture.height}
          sourceWidth={picture.width}
          src={map.screenshot.small}
        />
        <VStack gap={0.5}>
          <Text maxLines={1} weight="bold">
            {map.name}
          </Text>
          <Text maxLines={1} type="label">
            {map.playerCount}P · {map.width}×{map.height}
          </Text>
        </VStack>
      </VStack>
    </SelectableCard>
  );
}

function LoadingPlate({ index }: { index: number }) {
  return (
    <Card padding={2}>
      <VStack gap={2}>
        <AspectRatio ratio={1} xstyle={styles.plateWell}>
          <Skeleton index={index} radius="none" />
        </AspectRatio>
        <Skeleton height={16} index={index} radius={0} />
        <Skeleton height={12} index={index} radius={0} width="60%" />
      </VStack>
    </Card>
  );
}

/**
 * A place on the board that no map holds yet.
 *
 * The catalog starts empty, and a board of open slots says what the import
 * panel beside it is for better than another sentence would.
 */
function OpenSlot() {
  return (
    <AspectRatio ratio={1} xstyle={styles.openSlot}>
      <VStack />
    </AspectRatio>
  );
}

/**
 * The slot at the head of the board.
 *
 * A map enters the catalog here and nowhere else, so this keeps a plate's
 * footprint on the board rather than hiding behind a menu.
 */
function ImportPlate({
  isSignedIn,
  onImported,
}: {
  isSignedIn: boolean;
  onImported: (map: MapCatalogEntry) => void | Promise<void>;
}) {
  const importer = useAwbwImport(onImported);

  return (
    <Card padding={2} variant="muted" xstyle={styles.importPlate}>
      <VStack gap={2}>
        <VStack gap={2} padding={2} xstyle={styles.importWell}>
          <NumberInput
            isDisabled={!isSignedIn}
            isIntegerOnly
            isLabelHidden
            label="AWBW map ID"
            min={1}
            onChange={importer.setSourceMapId}
            placeholder="162795"
            size="sm"
            value={importer.sourceMapId}
            width="100%"
          />
          <Button
            clickAction={importer.run}
            isDisabled={!isSignedIn}
            isLoading={importer.isPending}
            label="Import"
            size="sm"
            variant="secondary"
            width="100%"
          />
        </VStack>
        <VStack gap={0.5}>
          <Text weight="bold">Import from AWBW</Text>
          <Text maxLines={2} type="label">
            {importer.error ?? (isSignedIn ? "By map id" : "Sign in first")}
          </Text>
        </VStack>
      </VStack>
    </Card>
  );
}

/**
 * What the board is before anybody has imported anything.
 *
 * The catalog starts empty, so the first visit is the import step and says so
 * at full size instead of leaving a lone plate on an empty board.
 */
function FirstMapPanel({
  isSignedIn,
  onImported,
}: {
  isSignedIn: boolean;
  onImported: (map: MapCatalogEntry) => void | Promise<void>;
}) {
  const importer = useAwbwImport(onImported);

  return (
    <Card maxWidth={720} padding={6}>
      <VStack gap={4}>
        <Heading level={2}>The catalog is empty</Heading>
        <Text color="secondary">
          Every map AWBRN plays is imported once and then held for everybody. Bring the first one
          over with its AWBW map id, the number in the map&rsquo;s address on AWBW.
        </Text>
        <HStack align="end" gap={2} wrap="wrap">
          <NumberInput
            isDisabled={!isSignedIn}
            isIntegerOnly
            label="AWBW map ID"
            min={1}
            onChange={importer.setSourceMapId}
            placeholder="162795"
            value={importer.sourceMapId}
          />
          <Button
            clickAction={importer.run}
            isDisabled={!isSignedIn}
            isLoading={importer.isPending}
            label="Import map"
            variant="primary"
          />
        </HStack>
        {!isSignedIn ? (
          <Text weight="medium">
            <RouterTextLink search={{ mode: undefined }} to="/auth">
              Sign in
            </RouterTextLink>{" "}
            to import a map.
          </Text>
        ) : null}
        {importer.error ? (
          <Banner description={importer.error} status="error" title="Import failed" />
        ) : null}
      </VStack>
    </Card>
  );
}

function MapBriefing({
  briefingRef,
  createError,
  fogEnabled,
  hotseatEnabled,
  isCreating,
  isPrivate,
  map,
  matchName,
  onCreate,
  onFogChange,
  onHotseatChange,
  onMatchNameChange,
  onPrivateChange,
  onStartingFundsChange,
  session,
  startingFunds,
}: {
  briefingRef: RefObject<HTMLDivElement | null>;
  createError: string | null;
  fogEnabled: boolean;
  hotseatEnabled: boolean;
  isCreating: boolean;
  isPrivate: boolean;
  map: MapCatalogEntry;
  matchName: string;
  onCreate: () => void | Promise<void>;
  onFogChange: (value: boolean) => void;
  onHotseatChange: (value: boolean) => void;
  onMatchNameChange: (value: string) => void;
  onPrivateChange: (value: boolean) => void;
  onStartingFundsChange: (value: number) => void;
  session: ReturnType<typeof useAppSession>;
  startingFunds: number;
}) {
  return (
    <Card padding={6} ref={briefingRef}>
      <VStack gap={6}>
        <VStack gap={1}>
          <Heading level={2}>{map.name}</Heading>
          <Text type="label">By {map.author}</Text>
        </VStack>

        <Grid
          align="start"
          columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
          gap={6}
        >
          <MapPicture
            alt={`The battlefield of ${map.name}`}
            sourceHeight={mapScreenshotSize("full", map.width, map.height).height}
            sourceWidth={mapScreenshotSize("full", map.width, map.height).width}
            src={map.screenshot.full}
          />

          <VStack gap={4}>
            <MetadataList columns={3} label={{ position: "top" }}>
              <MetadataListItem label="Players">{map.playerCount}</MetadataListItem>
              <MetadataListItem label="Size">
                {map.width} × {map.height}
              </MetadataListItem>
              <MetadataListItem label="Source">
                {map.origin ? `AWBW ${map.origin.sourceMapId}` : "AWBRN"}
              </MetadataListItem>
            </MetadataList>

            <TextInput
              isRequired
              label="Match name"
              onChange={onMatchNameChange}
              placeholder="Riverside Duel"
              value={matchName}
            />

            <NumberInput
              isIntegerOnly
              isRequired
              label="Starting funds"
              min={0}
              onChange={onStartingFundsChange}
              value={startingFunds}
            />

            <VStack gap={2}>
              <CheckboxInput label="Fog of war" onChange={onFogChange} value={fogEnabled} />
              <CheckboxInput
                description="Only players holding the link can join."
                label="Private match"
                onChange={onPrivateChange}
                value={isPrivate}
              />
              <CheckboxInput
                description="Let one signed-in player claim more than one army."
                label="Hotseat"
                onChange={onHotseatChange}
                value={hotseatEnabled}
              />
            </VStack>

            {session ? (
              <Text type="label">Host {session.user.name}</Text>
            ) : (
              <Text weight="medium">
                <RouterTextLink search={{ mode: undefined }} to="/auth">
                  Sign in
                </RouterTextLink>{" "}
                to open a lobby.
              </Text>
            )}

            {createError ? (
              <Banner description={createError} status="error" title="The lobby did not open" />
            ) : null}

            <Button
              clickAction={onCreate}
              isDisabled={isCreating || !session}
              isLoading={isCreating}
              label="Create lobby"
              variant="primary"
              width="100%"
            />
          </VStack>
        </Grid>
      </VStack>
    </Card>
  );
}

/** Shared behavior of the two places a map is imported. */
function useAwbwImport(onImported: (map: MapCatalogEntry) => void | Promise<void>) {
  const [sourceMapId, setSourceMapId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: importAwbwMapFn,
    onSuccess: async (map) => {
      setError(null);
      await onImported(map);
    },
  });

  async function run(): Promise<void> {
    if (sourceMapId === null || sourceMapId <= 0) {
      setError("Enter the map's AWBW id.");
      return;
    }
    setError(null);
    try {
      await mutation.mutateAsync({ data: { sourceMapId } });
    } catch (importError) {
      setError(formatImportError(importError));
    }
  }

  return { error, isPending: mutation.isPending, run, setSourceMapId, sourceMapId };
}

function formatImportError(error: unknown): string {
  const message = error instanceof Error ? error.message : "";
  if (/not found|404/i.test(message)) return "AWBW has no map with that id.";
  if (/signed in/i.test(message)) return "Sign in to import a map.";
  return "The map could not be imported. Try again in a moment.";
}

const styles = stylex.create({
  // A well is recessed into the plate it sits in, so it takes the road-tan
  // fill and no outline of its own.
  plateWell: {
    backgroundColor: colorVars["--color-background-muted"],
    borderRadius: "var(--radius-element)",
  },
  importWell: {
    backgroundColor: colorVars["--color-background-surface"],
    borderRadius: "var(--radius-element)",
  },
  // The slot the board is waiting to fill: a dashed rule around a recessed
  // well, which is what this system gives an empty region inside a panel.
  importPlate: {
    borderStyle: "dashed",
  },
  openSlot: {
    backgroundColor: colorVars["--color-background-muted"],
    borderColor: awbrnVars.colorBorderSoft,
    borderRadius: "var(--radius-element)",
    borderStyle: "dashed",
    borderWidth: "var(--border-width)",
  },
});
