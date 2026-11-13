/**
 * Search and filters over a set of maps. The filters open in a popover, or a
 * bottom sheet on a compact touch screen. Each press narrows the set at once,
 * so the sheet's last key only closes it on the result it already counts.
 */

import { Badge } from "@astryxdesign/core/Badge";
import { Icon } from "@astryxdesign/core/Icon";
import { Popover } from "@astryxdesign/core/Popover";
import { Stack } from "@astryxdesign/core/Stack";
import { TextInput } from "@astryxdesign/core/TextInput";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { MAP_RANK_FILTERS } from "#/maps/map_taxonomy.ts";
import {
  MAP_PLAYER_COUNT_FILTER_LABELS,
  MAP_PLAYER_COUNT_FILTERS,
  MAP_RANK_FILTER_LABELS,
  MAP_TAG_LABELS,
  MAP_TAGS,
  type MapCatalogFilter,
} from "#/maps/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { Chip, ChipGroup } from "#/ui/Chip.tsx";
import { Figure } from "#/ui/Figure.tsx";
import { IconFilter } from "#/ui/icons.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { useCompactTouch } from "#/ui/useCompactTouch.ts";

type Filters = Required<MapCatalogFilter>;

const NO_FILTERS: Filters = { playerCounts: [], ranks: [], tags: [] };

export function MapFilterBar({
  filterCount,
  filters,
  onFiltersChange,
  onSearchChange,
  search,
  summary,
}: {
  filterCount: number;
  filters: Filters;
  onFiltersChange: (filters: Filters) => void;
  onSearchChange: (search: string) => void;
  search: string;
  /** The result count, said as a figure (`8 maps`, `5 of 8`). */
  summary: string;
}) {
  return (
    <Stack direction="horizontal" xstyle={styles.bar}>
      <TextInput
        containerXstyle={styles.search}
        enterKeyHint="search"
        hasAutoCorrect={false}
        hasClear
        isLabelHidden
        label="Search the map catalog"
        onChange={onSearchChange}
        placeholder="Map name or author"
        startIcon="search"
        type="search"
        value={search}
      />
      <FiltersKey
        filterCount={filterCount}
        filters={filters}
        onFiltersChange={onFiltersChange}
        summary={summary}
      />
      {/* useMapBoardSummary announces each settled count, so this is not a live region. */}
      <Figure tone="secondary" xstyle={styles.count}>
        {summary}
      </Figure>
    </Stack>
  );
}

function FiltersKey({
  filterCount,
  filters,
  onFiltersChange,
  summary,
}: {
  filterCount: number;
  filters: Filters;
  onFiltersChange: (filters: Filters) => void;
  summary: string;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const isCompactTouch = useCompactTouch();
  const isOn = filterCount > 0;

  const key = (
    <Button
      endContent={isOn ? <Badge label={String(filterCount)} /> : undefined}
      icon={<Icon icon={IconFilter} />}
      label={isOn ? `Filters, ${filterCount} on` : "Filters"}
      onClick={isCompactTouch ? () => setIsOpen(true) : undefined}
      variant="secondary"
      xstyle={styles.key}
    >
      Filters
    </Button>
  );

  const groups = <FilterGroups filters={filters} onFiltersChange={onFiltersChange} />;
  const clear = isOn ? (
    <Button label="Clear" onClick={() => onFiltersChange(NO_FILTERS)} size="sm" variant="ghost" />
  ) : null;

  if (isCompactTouch) {
    return (
      <>
        {key}
        <ModalSheet
          footer={
            <>
              {clear}
              <Button
                label={summary ? `Show ${summary}` : "Show maps"}
                onClick={() => setIsOpen(false)}
                size="lg"
                variant="ink"
                xstyle={styles.show}
              />
            </>
          }
          isOpen={isOpen}
          onOpenChange={setIsOpen}
          title="Filters"
        >
          <Stack xstyle={styles.sheetBody}>{groups}</Stack>
        </ModalSheet>
      </>
    );
  }

  return (
    <Popover
      alignment="start"
      content={
        <Stack xstyle={styles.popover}>
          {groups}
          {clear ? (
            <Stack direction="horizontal" xstyle={styles.popoverFoot}>
              {clear}
            </Stack>
          ) : null}
        </Stack>
      }
      isOpen={isOpen}
      label="Filters"
      onOpenChange={setIsOpen}
      padding={4}
      width={380}
    >
      {key}
    </Popover>
  );
}

function FilterGroups({
  filters,
  onFiltersChange,
}: {
  filters: Filters;
  onFiltersChange: (filters: Filters) => void;
}) {
  return (
    <Stack xstyle={styles.groups}>
      <ChipRow
        label="Armies"
        labels={MAP_PLAYER_COUNT_FILTER_LABELS}
        onChange={(playerCounts) => onFiltersChange({ ...filters, playerCounts })}
        options={MAP_PLAYER_COUNT_FILTERS}
        value={filters.playerCounts}
      />
      <ChipRow
        label="Plays as"
        labels={MAP_TAG_LABELS}
        onChange={(tags) => onFiltersChange({ ...filters, tags })}
        options={MAP_TAGS}
        value={filters.tags}
      />
      <ChipRow
        label="Rank"
        labels={MAP_RANK_FILTER_LABELS}
        onChange={(ranks) => onFiltersChange({ ...filters, ranks })}
        options={MAP_RANK_FILTERS}
        value={filters.ranks}
      />
    </Stack>
  );
}

/**
 * One question and its answers. Pressing nothing means every answer, so no
 * group carries an "any" chip: the group at rest already is it.
 */
function ChipRow<T extends string>({
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
    <ChipGroup label={label}>
      {options.map((option) => (
        <Chip
          isPressed={value.includes(option)}
          key={option}
          label={labels[option]}
          onPressedChange={(isPressed) =>
            onChange(options.filter((item) => (item === option ? isPressed : value.includes(item))))
          }
        />
      ))}
    </ChipGroup>
  );
}

const styles = stylex.create({
  bar: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    minInlineSize: 0,
  },
  search: {
    flexGrow: 1,
    flexShrink: 1,
    flexBasis: {
      default: "420px",
      [pageLayout.phoneMedia]: "0%",
    },
    maxInlineSize: {
      default: "420px",
      [pageLayout.phoneMedia]: "none",
    },
    minInlineSize: 0,
  },
  key: {
    flexShrink: 0,
    minBlockSize: "44px",
  },
  count: {
    flexShrink: 0,
    paddingInlineStart: "8px",
  },
  groups: {
    display: "grid",
    gap: "18px",
  },
  popover: {
    display: "grid",
    gap: "16px",
  },
  popoverFoot: {
    display: "flex",
    justifyContent: "flex-end",
    paddingBlockStart: "12px",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
  },
  sheetBody: {
    display: "block",
    padding: "16px",
  },
  show: {
    flexGrow: 1,
  },
});
