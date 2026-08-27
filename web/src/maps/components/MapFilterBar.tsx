/**
 * The console above the map board.
 *
 * A catalog that only answers to a name makes a player who wants "a two
 * player fog map" read every plate. These are the three questions worth
 * asking of a map before its shape: how many armies it seats, how it plays,
 * and how good it is. All three are always visible, because a filter behind a
 * menu is a filter nobody presses.
 */

import { Button } from "#/ui/Button.tsx";
import { Divider } from "@astryxdesign/core/Divider";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { ToggleButton, ToggleButtonGroup } from "@astryxdesign/core/ToggleButton";
import * as stylex from "@stylexjs/stylex";
import { Close as CloseIcon } from "pixelarticons/react/Close";
import { MAP_RANK_FILTERS } from "#/maps/map_taxonomy.ts";
import {
  MAP_PLAYER_COUNT_FILTER_LABELS,
  MAP_PLAYER_COUNT_FILTERS,
  MAP_RANK_FILTER_LABELS,
  MAP_TAG_LABELS,
  MAP_TAGS,
  type MapCatalogFilter,
  type MapPlayerCountFilter,
  type MapRankFilter,
  type MapTag,
} from "#/maps/schemas.ts";

export function MapFilterBar({
  filterCount,
  filters,
  onFiltersChange,
  onSearchChange,
  search,
  summary,
}: {
  /** How many filter buttons are pressed, which decides the reset key. */
  filterCount: number;
  filters: Required<MapCatalogFilter>;
  onFiltersChange: (filters: Required<MapCatalogFilter>) => void;
  onSearchChange: (search: string) => void;
  search: string;
  /** What the board holds right now, in the HUD voice. */
  summary: string;
}) {
  return (
    <VStack gap={4}>
      <HStack align="end" gap={4} justify="between" wrap="wrap">
        <TextInput
          hasClear
          label="Search the map catalog"
          onChange={onSearchChange}
          placeholder="Map name or author"
          startIcon="search"
          value={search}
          width={320}
        />
        <Text type="label">{summary}</Text>
      </HStack>

      <Divider />

      <HStack gap={6} wrap="wrap">
        <FilterRow
          label="Armies"
          options={MAP_PLAYER_COUNT_FILTERS}
          labels={MAP_PLAYER_COUNT_FILTER_LABELS}
          onChange={(playerCounts: MapPlayerCountFilter[]) =>
            onFiltersChange({ ...filters, playerCounts })
          }
          value={filters.playerCounts}
        />
        <FilterRow
          label="Plays as"
          options={MAP_TAGS}
          labels={MAP_TAG_LABELS}
          onChange={(tags: MapTag[]) => onFiltersChange({ ...filters, tags })}
          value={filters.tags}
        />
        <FilterRow
          label="Rank"
          options={MAP_RANK_FILTERS}
          labels={MAP_RANK_FILTER_LABELS}
          onChange={(ranks: MapRankFilter[]) => onFiltersChange({ ...filters, ranks })}
          value={filters.ranks}
        />
        {filterCount > 0 ? (
          <VStack gap={1} justify="end" xstyle={styles.reset}>
            <Button
              clickAction={() => onFiltersChange({ playerCounts: [], ranks: [], tags: [] })}
              icon={<CloseIcon aria-hidden height={14} width={14} />}
              label={`Clear ${filterCount} filter${filterCount === 1 ? "" : "s"}`}
              size="sm"
              variant="ghost"
            />
          </VStack>
        ) : null}
      </HStack>
    </VStack>
  );
}

/**
 * One question and the answers to it.
 *
 * Pressing nothing means every answer, which is why no row carries an "any"
 * button: the row at rest already is it.
 */
function FilterRow<T extends string>({
  label,
  labels,
  onChange,
  options,
  value,
}: {
  label: string;
  labels: Record<T, string>;
  onChange: (value: T[]) => void;
  options: readonly T[];
  value: readonly T[];
}) {
  return (
    <VStack gap={1.5}>
      <Text color="secondary" type="label">
        {label}
      </Text>
      <ToggleButtonGroup
        label={label}
        onChange={(next) => onChange((next as T[]) ?? [])}
        size="sm"
        type="multiple"
        value={[...value]}
        xstyle={styles.keys}
      >
        {options.map((option) => (
          <ToggleButton key={option} label={labels[option]} value={option} />
        ))}
      </ToggleButtonGroup>
    </VStack>
  );
}

const styles = stylex.create({
  // A row of keys wraps rather than running off the panel. At phone width the
  // "plays as" row is wider than the screen, and a filter key that cannot be
  // reached is a filter nobody has.
  keys: {
    flexWrap: "wrap",
  },
  // The reset key lines up with the buttons beside it rather than with the
  // labels above them, so the row still reads as one rule.
  reset: {
    marginBlockStart: "auto",
  },
});
