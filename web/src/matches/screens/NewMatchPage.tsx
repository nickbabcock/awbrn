import { useAnnounce } from "@astryxdesign/core/hooks";
import { Icon } from "@astryxdesign/core/Icon";
import { NumberInput } from "@astryxdesign/core/NumberInput";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Switch } from "@astryxdesign/core/Switch";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { coRoster, getCoById } from "#/co_roster.ts";
import { coTierListFor } from "#/co_tiers.ts";
import { CoStamp } from "#/components/CoStamp.tsx";
import { UiSprite } from "#/components/UiSprite.tsx";
import { defaultFactionIdForSlot, getFactionById } from "#/factions.ts";
import { MapFilterBar } from "#/maps/components/MapFilterBar.tsx";
import {
  MapCardGrid,
  MapLoadingPlate,
  MapSelectPlate,
  mapPostage,
} from "#/maps/components/MapPlate.tsx";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import { MAP_BOARD_LOADING_PLATES, useMapBoardSummary } from "#/maps/map_board.ts";
import { countMapCatalogFilters } from "#/maps/map_taxonomy.ts";
import { importFailureMessage } from "#/maps/map_import.ts";
import { importAwbwMapFn } from "#/maps/maps.functions.ts";
import { mapKeys } from "#/maps/maps.keys.ts";
import {
  mapCatalogQueryOptions,
  mapQueryOptions,
  mapRevisionQueryOptions,
} from "#/maps/maps.queries.ts";
import type { MapCatalogEntry, MapCatalogFilter } from "#/maps/schemas.ts";
import { ClockSettings, validateClock } from "#/matches/components/ClockSettings.tsx";
import { NO_AI_SEATS, SeatRoster } from "#/matches/components/SeatRoster.tsx";
import { createMatchFn, mutateMatchFn } from "#/matches/matches.functions.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { defaultMatchClock, type AiProfileId, type MatchClock } from "#/matches/schemas.ts";
import { MatchThumb } from "#/play/MatchThumb.tsx";
import { Postage } from "#/play/Postage.tsx";
import { setupStyles } from "#/play/setup.tsx";
import { TaskDock } from "#/play/TaskDock.tsx";
import { TierPicker } from "#/play/TierPicker.tsx";
import { clockTerms } from "#/play/terms.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { EmptySheet, ProblemBanner } from "#/ui/EmptySheet.tsx";
import { IconCheck, IconFog, IconLock, IconNext, IconOpenSeat } from "#/ui/icons.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Sheet, SheetBody, SheetFoot } from "#/ui/Sheet.tsx";
import { Terms } from "#/ui/Terms.tsx";

/** How long the board waits after a keystroke before it searches. */
const SEARCH_DEBOUNCE_MS = 250;

/** A board nothing has been asked of yet. Held still so state stays stable. */
const NO_MAP_FILTERS: Required<MapCatalogFilter> = { playerCounts: [], ranks: [], tags: [] };

/** A match that takes no CO away. */
const EMPTY_CO_BANS: ReadonlySet<number> = new Set<number>();

/** How many banned stamps the Banned well shows before it counts the rest. */
const BANNED_STAMPS_SHOWN = 8;

/**
 * The widest map, in tiles, whose card always fits one column of the picker.
 * A card is 8px a tile plus its rule, and the narrowest column (a 1181px
 * window, the rules beside it) is about 210px.
 */
const PICKER_ONE_COLUMN_TILES = 25;

/**
 * In a picker the order is a choice, not a ranking, so a card that may take
 * two columns follows the ones that never do, narrowest first. The grid then
 * leaves no holes, and the page reads (and tabs) in the order it draws.
 */
function pickerOrder(maps: readonly MapCatalogEntry[]): MapCatalogEntry[] {
  const regular = maps.filter((map) => map.width <= PICKER_ONE_COLUMN_TILES);
  const wide = maps
    .filter((map) => map.width > PICKER_ONE_COLUMN_TILES)
    .sort((a, b) => a.width - b.width);
  return [...regular, ...wide];
}

export function NewMatchPage({ chosenMapId }: { chosenMapId?: string }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const session = useAppSession();

  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [filters, setFilters] = useState<Required<MapCatalogFilter>>(NO_MAP_FILTERS);
  // The map the player chose. Until they choose, the form stands on a default.
  const [pickedMap, setPickedMap] = useState<MapCatalogEntry | null>(null);
  const [isPickerOpen, setIsPickerOpen] = useState(false);
  const [isBansOpen, setIsBansOpen] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);
  const fundsRef = useRef<HTMLInputElement>(null);

  // The name the player wrote; null while the name follows the map.
  const [ownName, setOwnName] = useState<string | null>(null);
  const [fogEnabled, setFogEnabled] = useState(false);
  const [startingFunds, setStartingFunds] = useState(1000);
  const [isPrivate, setIsPrivate] = useState(false);
  const [hotseatEnabled, setHotseatEnabled] = useState(false);
  const [clock, setClock] = useState<MatchClock>(defaultMatchClock);
  const [bannedCoIds, setBannedCoIds] = useState<ReadonlySet<number>>(EMPTY_CO_BANS);
  // The opponent in each seat, by slot index. A seat with no entry is open.
  const [aiSeats, setAiSeats] = useState<ReadonlyMap<number, AiProfileId>>(NO_AI_SEATS);
  const [createError, setCreateError] = useState<string | null>(null);

  useEffect(() => {
    const timer = setTimeout(() => setSearch(searchInput), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [searchInput]);

  const catalogQuery = useInfiniteQuery(mapCatalogQueryOptions(search, filters));
  const catalogMaps = useMemo(
    () => catalogQuery.data?.pages.flatMap((page) => page.maps) ?? [],
    [catalogQuery.data],
  );
  const pickerMaps = useMemo(() => pickerOrder(catalogMaps), [catalogMaps]);
  const filterCount = countMapCatalogFilters(filters);
  // The board is narrowed when a search or a filter is on it, which is what
  // separates "nothing matches" from "the catalog is empty".
  const isNarrowed = search.trim().length > 0 || filterCount > 0;

  // A map chosen on its own page arrives in the address. It is read on its
  // own rather than waited for on the board, because the board is paged and
  // the map may be twenty cards down it.
  const chosenMapQuery = useQuery({
    ...mapQueryOptions(chosenMapId ?? ""),
    enabled: chosenMapId !== undefined,
  });
  const handedOverMap = chosenMapQuery.data ?? null;

  function chooseMap(map: MapCatalogEntry): void {
    setPickedMap(map);
    setCreateError(null);
    // The roster is about one map's seats. Another map has its own, and
    // carrying a seat over would put an opponent in a slot nobody chose.
    setAiSeats(NO_AI_SEATS);
    // A name the player cleared follows the map again.
    if (!ownName?.trim()) setOwnName(null);
  }

  // The form always has a map, from the first paint (the route loads both):
  // the one handed over from its page, or else the first card on the board.
  // It is derived rather than set after mount, so the seats and the dock are
  // drawn with the page instead of a moment after it.
  const firstMap = pickerMaps[0] ?? null;
  const isWaitingForHandOver = chosenMapId !== undefined && chosenMapQuery.isPending;
  const [defaultMap, setDefaultMap] = useState<MapCatalogEntry | null>(null);
  const fallbackMap = isWaitingForHandOver
    ? null
    : (handedOverMap ?? (isNarrowed ? null : firstMap));
  // The default holds once found, so a search that narrows the board does not
  // take away the map the form was standing on.
  if (defaultMap === null && fallbackMap !== null) setDefaultMap(fallbackMap);
  const selectedMap = pickedMap ?? defaultMap ?? fallbackMap;
  const matchName = ownName ?? selectedMap?.name ?? "";

  const createMatchMutation = useMutation({ mutationFn: createMatchFn });

  async function handleImported(map: MapCatalogEntry): Promise<void> {
    await queryClient.invalidateQueries({ queryKey: mapKeys.all });
    chooseMap(map);
    setSearchInput("");
    setFilters(NO_MAP_FILTERS);
  }

  async function handleCreateLobby(): Promise<void> {
    if (!session) {
      setCreateError("Sign in to create a match.");
      return;
    }
    if (!selectedMap) {
      setCreateError("Choose a map first.");
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
    if (bannedCoIds.size >= coRoster.length) {
      setCreateError("Leave at least one CO for the players to choose.");
      return;
    }
    if (aiSeats.size >= selectedMap.playerCount) {
      setCreateError("Leave a seat open for yourself.");
      return;
    }
    const clockError = validateClock(clock);
    if (clockError) {
      setCreateError(clockError);
      return;
    }

    setCreateError(null);
    try {
      const match = await createMatchMutation.mutateAsync({
        data: {
          name: matchName.trim(),
          map: { mapId: selectedMap.mapId, revision: selectedMap.revision },
          isPrivate,
          settings: {
            fogEnabled,
            startingFunds,
            hotseatEnabled,
            bannedCoIds: [...bannedCoIds],
            clock,
          },
          aiSeats: [...aiSeats].map(([slotIndex, profileId]) => ({ slotIndex, profileId })),
        },
      });
      // The roster put the host in seat 1. Should taking it fail, the lobby
      // still offers the seat, so the match is opened either way.
      await queryClient
        .fetchQuery(mapRevisionQueryOptions(selectedMap.mapId, selectedMap.revision))
        .then(({ slotFactionIds }) =>
          mutateMatchFn({
            data: {
              matchId: match.matchId,
              action: {
                action: "join",
                slotIndex: 0,
                factionId: slotFactionIds[0],
                joinSlug: match.joinSlug,
              },
            },
          }),
        )
        .catch(() => undefined);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: matchKeys.browse() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.mine() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.awaiting() }),
      ]);
      await navigate({ to: "/matches/$matchId", params: { matchId: match.matchId } });
    } catch (error) {
      setCreateError(error instanceof Error ? error.message : "The lobby could not be opened.");
    }
  }

  const isBoardEmpty = !catalogQuery.isPending && !catalogQuery.isError && catalogMaps.length === 0;
  const summary = useMapBoardSummary({
    count: catalogMaps.length,
    filters,
    hasMore: catalogQuery.hasNextPage,
    isError: catalogQuery.isError,
    isPending: catalogQuery.isPending,
    search,
  });
  const isSignedIn = session !== null;

  const filterBar = (
    <MapFilterBar
      filterCount={filterCount}
      filters={filters}
      onFiltersChange={setFilters}
      onSearchChange={setSearchInput}
      search={searchInput}
      summary={summary}
    />
  );
  const emptyBoard = isBoardEmpty ? (
    <EmptyBoard
      filterCount={filterCount}
      isNarrowed={isNarrowed}
      isSignedIn={isSignedIn}
      onClearFilters={() => setFilters(NO_MAP_FILTERS)}
      onImported={handleImported}
    />
  ) : null;
  const catalogError = catalogQuery.isError ? (
    <ProblemBanner
      action={<Button label="Retry" onClick={() => void catalogQuery.refetch()} size="sm" />}
      title="The map catalog could not be read"
    />
  ) : null;
  const moreMaps = catalogQuery.hasNextPage ? (
    <Button
      clickAction={async () => {
        await catalogQuery.fetchNextPage();
      }}
      isLoading={catalogQuery.isFetchingNextPage}
      label="More maps"
    />
  ) : null;

  const createKey = (isDock: boolean) =>
    isSignedIn ? (
      <Button
        clickAction={handleCreateLobby}
        isDisabled={createMatchMutation.isPending || selectedMap === null}
        isLoading={createMatchMutation.isPending}
        label="Create lobby"
        size="lg"
        variant="primary"
        xstyle={isDock ? setupStyles.dockKey : undefined}
      />
    ) : (
      <RouterButton
        label="Sign in"
        search={{ mode: undefined }}
        size="lg"
        to="/auth"
        variant="primary"
        xstyle={isDock ? setupStyles.dockKey : undefined}
      />
    );

  return (
    <Page>
      <PageHeader title="Play" />

      <VStack xstyle={styles.grid}>
        <section aria-label="Map" {...stylex.props(styles.maps)}>
          {filterBar}
          {catalogError}
          {emptyBoard ?? (
            <MapCardGrid density="compact">
              {catalogQuery.isPending
                ? Array.from({ length: MAP_BOARD_LOADING_PLATES }, (_, index) => (
                    <MapLoadingPlate index={index} key={index} />
                  ))
                : pickerMaps.map((map) => (
                    <MapSelectPlate
                      isSelected={selectedMap?.mapId === map.mapId}
                      key={map.mapId}
                      map={map}
                      onSelect={chooseMap}
                    />
                  ))}
            </MapCardGrid>
          )}
          {moreMaps ? <HStack xstyle={styles.more}>{moreMaps}</HStack> : null}
        </section>

        <VStack xstyle={styles.side}>
          <ChosenMapKey map={selectedMap} onOpen={() => setIsPickerOpen(true)} />

          <Sheet aria-label="Rules" xstyle={styles.rules}>
            <SheetBody xstyle={[styles.form, styles.rulesBody]}>
              <VStack xstyle={styles.field}>
                <FieldName control={nameRef}>Name</FieldName>
                <TextInput
                  isLabelHidden
                  label="Name"
                  onChange={setOwnName}
                  placeholder="Riverside Duel"
                  ref={nameRef}
                  value={matchName}
                />
              </VStack>
              <ClockSettings clock={clock} onChange={setClock} />
              <VStack xstyle={[styles.field, styles.funds]}>
                <FieldName control={fundsRef}>Funds</FieldName>
                <NumberInput
                  formatValue={(value) => value.toLocaleString("en-US")}
                  isIntegerOnly
                  isLabelHidden
                  label="Funds"
                  min={0}
                  onChange={setStartingFunds}
                  ref={fundsRef}
                  startIcon={CoinIcon}
                  step={1000}
                  value={startingFunds}
                />
              </VStack>
              {selectedMap ? (
                <Seats
                  aiSeats={aiSeats}
                  map={selectedMap}
                  onChange={(seats) => {
                    setCreateError(null);
                    setAiSeats(seats);
                  }}
                  viewerName={session?.user.name ?? null}
                />
              ) : null}
              <BannedWell bannedCoIds={bannedCoIds} onEdit={() => setIsBansOpen(true)} />
              <VStack xstyle={styles.switches}>
                <SwitchRow
                  icon={IconFog}
                  label="Fog"
                  name="Fog of war"
                  onChange={setFogEnabled}
                  value={fogEnabled}
                />
                <SwitchRow
                  icon={IconLock}
                  label="Private"
                  name="Private: only players holding the link can join"
                  onChange={setIsPrivate}
                  value={isPrivate}
                />
                <SwitchRow
                  icon={IconOpenSeat}
                  label="Hotseat"
                  name="Hotseat: one signed-in player may claim more than one army"
                  onChange={setHotseatEnabled}
                  value={hotseatEnabled}
                />
              </VStack>
              {createError ? (
                <VStack aria-live="assertive">
                  <ProblemBanner title={createError} />
                </VStack>
              ) : null}
            </SheetBody>
            <VStack xstyle={[setupStyles.phoneHidden, styles.rulesFoot]}>
              <SheetFoot>
                <VStack as="span" />
                {createKey(false)}
              </SheetFoot>
            </VStack>
          </Sheet>
        </VStack>
      </VStack>

      {/* The dock stays while no map is on the form, so Create lobby never
          leaves the phone. */}
      <TaskDock
        keys={
          selectedMap === null && catalogQuery.isError ? (
            <>
              <Button
                clickAction={async () => {
                  await catalogQuery.refetch();
                }}
                label="Retry"
                size="lg"
                xstyle={setupStyles.dockKey}
              />
              {createKey(true)}
            </>
          ) : (
            createKey(true)
          )
        }
        label="Create"
        line={
          selectedMap ? (
            <>
              <MatchThumb map={selectedMap} size={48} />
              <VStack as="span" xstyle={styles.dockText}>
                <b {...stylex.props(styles.dockName)}>{selectedMap.name}</b>
                <Terms
                  clock={clockTerms(clock)}
                  funds={Number.isFinite(startingFunds) ? startingFunds : null}
                  hasFog={fogEnabled}
                />
              </VStack>
            </>
          ) : (
            <>
              <VStack as="span" aria-hidden="true" xstyle={styles.dockWell} />
              <VStack as="span" xstyle={styles.dockText}>
                <b {...stylex.props(styles.dockName)}>
                  {catalogQuery.isError ? "Maps could not be read" : "Choose a map"}
                </b>
              </VStack>
            </>
          )
        }
      />

      <ModalSheet isOpen={isPickerOpen} onOpenChange={setIsPickerOpen} title="Map" width={560}>
        {isPickerOpen ? (
          <>
            <VStack xstyle={styles.pickerBody}>
              {filterBar}
              {catalogError}
            </VStack>
            {emptyBoard ? (
              <VStack xstyle={styles.pickerBody}>{emptyBoard}</VStack>
            ) : (
              <ul {...stylex.props(styles.pickerRows)}>
                {pickerMaps.map((map) => (
                  <li key={map.mapId}>
                    <MapRowKey
                      isChosen={selectedMap?.mapId === map.mapId}
                      map={map}
                      onChoose={() => {
                        chooseMap(map);
                        setIsPickerOpen(false);
                      }}
                    />
                  </li>
                ))}
              </ul>
            )}
            {moreMaps ? <VStack xstyle={styles.pickerBody}>{moreMaps}</VStack> : null}
          </>
        ) : null}
      </ModalSheet>

      <ModalSheet isOpen={isBansOpen} onOpenChange={setIsBansOpen} title="Banned" width={760}>
        {isBansOpen ? (
          <VStack xstyle={styles.bansBody}>
            <TierPicker
              bannedCoIds={bannedCoIds}
              mode="ban"
              onSetTierBanned={(coIds, isBanned) => {
                setCreateError(null);
                setBannedCoIds((banned) => {
                  const next = new Set(banned);
                  for (const coId of coIds) {
                    if (isBanned) next.add(coId);
                    else next.delete(coId);
                  }
                  return next;
                });
              }}
              onToggleBan={(coId) => {
                setCreateError(null);
                setBannedCoIds((banned) => {
                  const next = new Set(banned);
                  if (!next.delete(coId)) next.add(coId);
                  return next;
                });
              }}
              tierList={coTierListFor({
                fogEnabled,
                playerCount: selectedMap?.playerCount ?? 2,
              })}
            />
          </VStack>
        ) : null}
      </ModalSheet>
    </Page>
  );
}

function CoinIcon() {
  return <UiSprite name="Coin.png" />;
}

/** On a phone the form opens with the chosen map as one row, which opens the map picker. */
function ChosenMapKey({ map, onOpen }: { map: MapCatalogEntry | null; onOpen: () => void }) {
  return (
    <button
      aria-haspopup="dialog"
      aria-label={map ? `Map: ${map.name}. Change map` : "Choose a map"}
      onClick={onOpen}
      type="button"
      {...stylex.props(styles.mapKey)}
    >
      {map ? (
        <MapKeyBody map={map} />
      ) : (
        <VStack as="span" xstyle={styles.mapKeyText}>
          <b {...stylex.props(styles.mapKeyName)}>Choose a map</b>
        </VStack>
      )}
      <Icon icon={IconNext} size="md" xstyle={styles.mapKeyChevron} />
    </button>
  );
}

function MapRowKey({
  isChosen,
  map,
  onChoose,
}: {
  isChosen: boolean;
  map: MapCatalogEntry;
  onChoose: () => void;
}) {
  return (
    <button
      aria-pressed={isChosen}
      onClick={onChoose}
      type="button"
      {...stylex.props(styles.mapRow, isChosen && styles.mapRowChosen)}
    >
      <MapKeyBody map={map} />
      {isChosen ? (
        <VStack as="span" aria-hidden="true" xstyle={styles.picked}>
          <Icon icon={IconCheck} size="sm" />
        </VStack>
      ) : null}
    </button>
  );
}

function MapKeyBody({ map }: { map: MapCatalogEntry }) {
  return (
    <>
      <MatchThumb map={map} size={64} />
      <VStack as="span" xstyle={styles.mapKeyText}>
        <b {...stylex.props(styles.mapKeyName)}>{map.name}</b>
        <Postage>{mapPostage(map)}</Postage>
      </VStack>
      {map.rank ? <MapRankMedal rank={map.rank} size="sm" /> : null}
    </>
  );
}

/** The seats of the chosen map, each with its army from the map. */
function Seats({
  aiSeats,
  map,
  onChange,
  viewerName,
}: {
  aiSeats: ReadonlyMap<number, AiProfileId>;
  map: MapCatalogEntry;
  onChange: (aiSeats: ReadonlyMap<number, AiProfileId>) => void;
  viewerName: string | null;
}) {
  const { data: revision } = useQuery(mapRevisionQueryOptions(map.mapId, map.revision));
  const factionCodes = Array.from({ length: map.playerCount }, (_, slotIndex) => {
    const factionId = revision?.slotFactionIds?.[slotIndex] ?? defaultFactionIdForSlot(slotIndex);
    return getFactionById(factionId)?.code ?? "os";
  });
  return (
    <SeatRoster
      aiSeats={aiSeats}
      factionCodes={factionCodes}
      onChange={onChange}
      viewerName={viewerName}
    />
  );
}

function BannedWell({
  bannedCoIds,
  onEdit,
}: {
  bannedCoIds: ReadonlySet<number>;
  onEdit: () => void;
}) {
  const banned = [...bannedCoIds].map((coId) => getCoById(coId)).filter((co) => co !== null);
  const shown = banned.slice(0, BANNED_STAMPS_SHOWN);
  return (
    <VStack aria-label="Banned" role="group" xstyle={styles.field}>
      <Text color="inherit" type="inherit" xstyle={styles.fieldLabel}>
        Banned
      </Text>
      <HStack xstyle={styles.banned}>
        <HStack as="span" xstyle={styles.bannedStamps}>
          {banned.length === 0 ? (
            <Text color="secondary">None</Text>
          ) : (
            shown.map((co) => <CoStamp coKey={co.key} key={co.key} xstyle={styles.bannedStamp} />)
          )}
          {banned.length > shown.length ? (
            <Text color="secondary">+{banned.length - shown.length}</Text>
          ) : null}
        </HStack>
        <Button label="Edit" onClick={onEdit} size="sm" />
      </HStack>
    </VStack>
  );
}

/**
 * A field's name in the form's label style, drawn here because the input's
 * own label is the lighter Astryx default. The input's hidden label stays its
 * accessible name, so this one is hidden from assistive tech.
 */
function FieldName({
  children,
  control,
}: {
  children: string;
  control: RefObject<HTMLInputElement | null>;
}) {
  return (
    <label
      aria-hidden="true"
      onClick={() => control.current?.focus()}
      {...stylex.props(styles.fieldLabel)}
    >
      {children}
    </label>
  );
}

function SwitchRow({
  icon,
  label,
  name,
  onChange,
  value,
}: {
  icon: typeof IconFog;
  label: string;
  /** What assistive tech hears, when the word alone is too short. */
  name: string;
  onChange: (value: boolean) => void;
  value: boolean;
}) {
  return (
    <HStack xstyle={styles.switchRow}>
      <HStack as="span" aria-hidden="true" xstyle={styles.switchLabel}>
        <Icon icon={icon} size="sm" xstyle={styles.switchIcon} />
        {label}
      </HStack>
      <Switch isLabelHidden label={name} onChange={onChange} value={value} />
    </HStack>
  );
}

/** A map enters the catalog by its AWBW id, so the import is here. */
function EmptyBoard({
  filterCount,
  isNarrowed,
  isSignedIn,
  onClearFilters,
  onImported,
}: {
  filterCount: number;
  isNarrowed: boolean;
  isSignedIn: boolean;
  onClearFilters: () => void;
  onImported: (map: MapCatalogEntry) => void | Promise<void>;
}) {
  return (
    <VStack xstyle={styles.emptyBoard}>
      <EmptySheet
        action={
          filterCount > 0 ? (
            <Button label="Clear filters" onClick={onClearFilters} size="sm" />
          ) : undefined
        }
        title={
          !isNarrowed
            ? "No maps yet"
            : filterCount > 0
              ? "No map fits those filters"
              : "No map by that name"
        }
      />
      <AwbwImport isSignedIn={isSignedIn} onImported={onImported} />
    </VStack>
  );
}

function AwbwImport({
  isSignedIn,
  onImported,
}: {
  isSignedIn: boolean;
  onImported: (map: MapCatalogEntry) => void | Promise<void>;
}) {
  const [sourceMapId, setSourceMapId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const announce = useAnnounce();
  const mutation = useMutation({
    mutationFn: importAwbwMapFn,
    onSuccess: async (map) => {
      setError(null);
      await onImported(map);
    },
  });

  function fail(message: string): void {
    setError(message);
    announce(message, "assertive");
  }

  async function run(): Promise<void> {
    if (sourceMapId === null || sourceMapId <= 0) {
      fail("Enter the map's AWBW id");
      return;
    }
    setError(null);
    try {
      await mutation.mutateAsync({ data: { sourceMapId } });
    } catch (importError) {
      fail(importFailureMessage(importError));
    }
  }

  return (
    <HStack xstyle={styles.import}>
      <NumberInput
        disabledMessage="Sign in to import a map"
        isDisabled={!isSignedIn}
        isIntegerOnly
        label="AWBW map ID"
        min={1}
        onChange={setSourceMapId}
        placeholder="162795"
        status={error ? { type: "error", message: error } : undefined}
        value={sourceMapId}
      />
      <Button
        clickAction={run}
        isDisabled={!isSignedIn}
        isLoading={mutation.isPending}
        label="Import"
      />
    </HStack>
  );
}

const styles = stylex.create({
  grid: {
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(0, 1fr) 420px",
      [pageLayout.belowRailMedia]: "minmax(0, 1fr)",
    },
    alignItems: "start",
    gap: {
      default: "24px",
      [pageLayout.phoneMedia]: "16px",
    },
  },
  maps: {
    display: {
      default: "grid",
      [pageLayout.phoneMedia]: "none",
    },
    gap: "16px",
    minInlineSize: 0,
  },
  more: {
    justifyContent: "center",
  },
  // Beside a long catalog the choice so far and Create lobby stay in reach:
  // the rules hold a gutter under the top bar while the maps scroll past.
  side: {
    display: "grid",
    gap: "16px",
    minInlineSize: 0,
    position: {
      default: null,
      [pageLayout.desktopMedia]: "sticky",
    },
    insetBlockStart: {
      default: null,
      [pageLayout.desktopMedia]: `calc(${pageLayout.topBarHeight} + ${pageLayout.gutterDesktop})`,
    },
  },
  // A window shorter than the rules scrolls the form inside the sheet, and
  // Create lobby stays at its foot.
  rules: {
    display: {
      default: null,
      [pageLayout.desktopMedia]: "flex",
    },
    flexDirection: "column",
    maxBlockSize: {
      default: null,
      [pageLayout.desktopMedia]: `calc(100dvh - ${pageLayout.topBarHeight} - 2 * ${pageLayout.gutterDesktop})`,
    },
  },
  rulesBody: {
    minBlockSize: 0,
    overflowY: {
      default: null,
      [pageLayout.desktopMedia]: "auto",
    },
  },
  rulesFoot: {
    flexShrink: 0,
  },
  form: {
    display: "grid",
    gap: "24px",
    paddingBlockStart: "20px",
  },
  funds: {
    maxInlineSize: {
      default: "220px",
      [pageLayout.phoneMedia]: "none",
    },
  },
  field: {
    display: "grid",
    gap: "6px",
  },
  fieldLabel: {
    fontSize: awbrnVars.text14,
    fontWeight: 650,
  },
  banned: {
    alignItems: "center",
    gap: "12px",
    minBlockSize: "56px",
    paddingBlock: "8px",
    paddingInline: "10px",
    borderRadius: "6px",
    backgroundColor: awbrnVars.paper,
  },
  bannedStamps: {
    display: "inline-flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: "4px",
    flexGrow: 1,
    minInlineSize: 0,
  },
  bannedStamp: {
    filter: "grayscale(1)",
    opacity: 0.8,
  },
  switches: {
    display: "grid",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
  },
  switchRow: {
    alignItems: "center",
    justifyContent: "space-between",
    gap: "16px",
    minBlockSize: "44px",
    paddingBlock: "6px",
    borderBlockEndColor: awbrnVars.rule,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "1px",
  },
  switchLabel: {
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
    fontWeight: 700,
  },
  switchIcon: {
    color: colorVars["--color-text-secondary"],
  },
  mapKey: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "flex",
    },
    alignItems: "center",
    gap: "12px",
    inlineSize: "100%",
    minBlockSize: "80px",
    paddingBlock: "12px",
    paddingInline: "16px",
    borderColor: awbrnVars.rule,
    borderRadius: "10px",
    borderStyle: "solid",
    borderWidth: "1px",
    backgroundColor: awbrnVars.sheet,
    boxShadow: awbrnVars.shadowSheet,
    color: colorVars["--color-text-primary"],
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  mapKeyText: {
    display: "grid",
    gap: "4px",
    flexGrow: 1,
    minInlineSize: 0,
  },
  mapKeyName: {
    overflow: "hidden",
    fontSize: awbrnVars.text16,
    fontWeight: 700,
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  mapKeyChevron: {
    flexShrink: 0,
    color: awbrnVars.textTertiary,
  },
  pickerBody: {
    display: "grid",
    gap: "12px",
    padding: "16px",
  },
  pickerRows: {
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  mapRow: {
    display: "flex",
    alignItems: "center",
    gap: "12px",
    inlineSize: "100%",
    minBlockSize: "80px",
    paddingBlock: "8px",
    paddingInline: "16px",
    borderStyle: "none",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
    backgroundColor: {
      default: "transparent",
      ":hover": { default: null, [pageLayout.hoverMedia]: awbrnVars.paper },
      ":active": awbrnVars.paper,
    },
    color: colorVars["--color-text-primary"],
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  mapRowChosen: {
    backgroundColor: awbrnVars.paperWell,
  },
  picked: {
    display: "grid",
    placeItems: "center",
    flexShrink: 0,
    inlineSize: "28px",
    blockSize: "28px",
    borderRadius: "50%",
    backgroundColor: colorVars["--color-background-card"],
    boxShadow: awbrnVars.shadowSheet,
    color: awbrnVars.ink900,
  },
  bansBody: {
    padding: "16px 20px 20px",
  },
  dockText: {
    display: "grid",
    gap: "3px",
    minInlineSize: 0,
  },
  dockName: {
    overflow: "hidden",
    fontSize: awbrnVars.text15_5,
    fontWeight: 700,
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  dockWell: {
    flexShrink: 0,
    inlineSize: "48px",
    blockSize: "48px",
    borderRadius: "6px",
    backgroundColor: awbrnVars.paperWell,
  },
  emptyBoard: {
    display: "grid",
    gap: "16px",
  },
  import: {
    alignItems: "flex-end",
    flexWrap: "wrap",
    gap: "8px",
  },
});
