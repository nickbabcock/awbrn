/**
 * Ranking a map revision and tagging a map. A rank is a moderator's judgment,
 * signed with a reason. Nobody ranks their own map. An author tags their own
 * map with no record kept; a moderator retagging somebody else's map signs the
 * change with a reason.
 */

import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Stack } from "@astryxdesign/core/Stack";
import { TextArea } from "@astryxdesign/core/TextArea";
import * as stylex from "@stylexjs/stylex";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import type { Actor } from "#/auth/actor.ts";
import { mapRankGrant, mapTagGrant } from "#/maps/map_authz.ts";
import { MAP_RANK_FILTERS, sortMapTags } from "#/maps/map_taxonomy.ts";
import { setMapRankFn, setMapTagsFn } from "#/maps/maps.functions.ts";
import { mapKeys } from "#/maps/maps.keys.ts";
import {
  MAP_RANK_FILTER_LABELS,
  MAP_TAG_LABELS,
  MAP_TAGS,
  MAP_UNRANKED_FILTER,
  type MapCatalogEntry,
  type MapRank,
  type MapRankFilter,
  type MapTag,
} from "#/maps/schemas.ts";
import { moderationKeys } from "#/moderation/moderation.keys.ts";
import {
  MODERATION_REASON_MAX_LENGTH,
  MODERATION_REASON_MIN_LENGTH,
} from "#/moderation/schemas.ts";
import { Button } from "#/ui/Button.tsx";
import { Chip, ChipGroup } from "#/ui/Chip.tsx";
import { ProblemBanner } from "#/ui/EmptySheet.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";
import { Sheet, SheetBody } from "#/ui/Sheet.tsx";

export function MapCurationPanel({ actor, map }: { actor: Actor | null; map: MapCatalogEntry }) {
  const rankGrant = mapRankGrant(map, actor);
  // A moderator who wrote the map holds the role and not the act. That is the
  // one refusal worth drawing, because it is a rule and not an absence.
  const isOwnWork = actor !== null && map.authorUserId === actor.userId;
  const isRankRefused = rankGrant === null && isOwnWork && actor.can({ map: ["rank"] });

  if (rankGrant === null && !isRankRefused) return null;

  return (
    <Sheet aria-label="Curation">
      <SheetBody xstyle={styles.curationBody}>
        {rankGrant !== null ? (
          <RankBench
            key={`${map.mapId}:${map.revision}`}
            mapId={map.mapId}
            rank={map.rank}
            revision={map.revision}
          />
        ) : (
          <ProblemBanner status="info" title="Another moderator ranks the maps you wrote" />
        )}
      </SheetBody>
    </Sheet>
  );
}

function RankBench({
  mapId,
  rank,
  revision,
}: {
  mapId: string;
  rank: MapRank | null;
  revision: number;
}) {
  const queryClient = useQueryClient();
  const current: MapRankFilter = rank ?? MAP_UNRANKED_FILTER;
  // The control holds the rank just set until the refetched map catches up.
  const [chosen, setChosen] = useState<MapRankFilter>(current);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: setMapRankFn,
    onSuccess: async () => {
      setReason("");
      setError(null);
      await invalidateMap(queryClient);
    },
  });

  const isChanged = chosen !== current;

  async function handleSubmit(): Promise<void> {
    if (!isChanged) return;
    if (reason.trim().length < MODERATION_REASON_MIN_LENGTH) {
      setError("Give a reason; it is kept with the rank");
      return;
    }
    setError(null);
    try {
      await mutation.mutateAsync({
        data: {
          map: { mapId, revision },
          rank: chosen === MAP_UNRANKED_FILTER ? null : chosen,
          reason: reason.trim(),
        },
      });
    } catch (rankError) {
      setError(writeFailure(rankError, "The rank was not set"));
    }
  }

  return (
    <Stack xstyle={styles.stack}>
      <SegmentedControl
        label="Rank"
        onChange={(next) => {
          setError(null);
          setChosen(next as MapRankFilter);
        }}
        value={chosen}
        xstyle={styles.seg}
      >
        {MAP_RANK_FILTERS.map((option) => (
          <SegmentedControlItem
            key={option}
            label={MAP_RANK_FILTER_LABELS[option]}
            value={option}
          />
        ))}
      </SegmentedControl>
      <TextArea
        isLabelHidden
        label="Reason"
        onChange={(value) => {
          setError(null);
          setReason(value.slice(0, MODERATION_REASON_MAX_LENGTH));
        }}
        placeholder="Reason"
        rows={2}
        value={reason}
      />
      {error ? <ProblemBanner title={error} /> : null}
      <Stack direction="horizontal" xstyle={styles.end}>
        <Button
          clickAction={handleSubmit}
          isDisabled={!isChanged}
          isLoading={mutation.isPending}
          label="Set rank"
          variant="ink"
        />
      </Stack>
    </Stack>
  );
}

export function MapTagsKey({ actor, map }: { actor: Actor | null; map: MapCatalogEntry }) {
  const grant = mapTagGrant(map, actor);
  const [isOpen, setIsOpen] = useState(false);
  // The bench stays mounted so the sheet leaves with it, and starts again
  // from the map's tags on each opening.
  const [opening, setOpening] = useState(0);
  if (grant === null) return null;

  return (
    <>
      <Button
        label="Edit tags"
        onClick={() => {
          setOpening((count) => count + 1);
          setIsOpen(true);
        }}
        size="sm"
        variant="secondary"
      />
      <ModalSheet isOpen={isOpen} onOpenChange={setIsOpen} purpose="form" title="Tags">
        <TagBench
          grant={grant}
          key={opening}
          mapId={map.mapId}
          onSaved={() => setIsOpen(false)}
          tags={map.tags}
        />
      </ModalSheet>
    </>
  );
}

function TagBench({
  grant,
  mapId,
  onSaved,
  tags,
}: {
  grant: "owner" | "moderator";
  mapId: string;
  onSaved: () => void;
  tags: readonly MapTag[];
}) {
  const queryClient = useQueryClient();
  const [chosen, setChosen] = useState<MapTag[]>([...tags]);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: setMapTagsFn,
    onSuccess: async () => {
      await invalidateMap(queryClient);
      onSaved();
    },
  });

  const isChanged = !sameTags(chosen, tags);
  const isReasonNeeded = grant === "moderator";

  async function handleSubmit(): Promise<void> {
    if (!isChanged) return;
    if (isReasonNeeded && reason.trim().length < MODERATION_REASON_MIN_LENGTH) {
      setError("Give a reason; it is kept with the change");
      return;
    }
    setError(null);
    try {
      await mutation.mutateAsync({
        data: {
          mapId,
          tags: sortMapTags(chosen),
          ...(isReasonNeeded ? { reason: reason.trim() } : {}),
        },
      });
    } catch (tagError) {
      setError(writeFailure(tagError, "The tags were not saved"));
    }
  }

  return (
    <Stack xstyle={[styles.stack, styles.sheetPad]}>
      <ChipGroup label="Tags">
        {MAP_TAGS.map((tag) => (
          <Chip
            isPressed={chosen.includes(tag)}
            key={tag}
            label={MAP_TAG_LABELS[tag]}
            onPressedChange={(isPressed) => {
              setError(null);
              setChosen(
                sortMapTags(isPressed ? [...chosen, tag] : chosen.filter((item) => item !== tag)),
              );
            }}
          />
        ))}
      </ChipGroup>
      {isReasonNeeded ? (
        <TextArea
          isLabelHidden
          label="Reason"
          onChange={(value) => {
            setError(null);
            setReason(value.slice(0, MODERATION_REASON_MAX_LENGTH));
          }}
          placeholder="Reason"
          rows={2}
          value={reason}
        />
      ) : null}
      {error ? <ProblemBanner title={error} /> : null}
      <Stack direction="horizontal" xstyle={styles.end}>
        <Button
          clickAction={handleSubmit}
          isDisabled={!isChanged}
          isLoading={mutation.isPending}
          label="Save tags"
          variant="ink"
        />
      </Stack>
    </Stack>
  );
}

/** A rank changes which filtered sets hold this map, so the whole catalog is dropped. */
async function invalidateMap(queryClient: ReturnType<typeof useQueryClient>): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: mapKeys.all }),
    queryClient.invalidateQueries({ queryKey: moderationKeys.all }),
  ]);
}

function sameTags(left: readonly MapTag[], right: readonly MapTag[]): boolean {
  return left.length === right.length && left.every((tag, index) => tag === right[index]);
}

function writeFailure(error: unknown, fallback: string): string {
  const message = error instanceof Error ? error.message : "";
  if (/forbidden|403/i.test(message)) return "That is not yours to change";
  if (/reason/i.test(message)) return "The change needs a reason";
  return fallback;
}

const styles = stylex.create({
  curationBody: {
    paddingBlockStart: "18px",
  },
  stack: {
    display: "grid",
    gap: "16px",
  },
  sheetPad: {
    padding: "20px",
  },
  seg: {
    inlineSize: "100%",
  },
  end: {
    display: "flex",
    justifyContent: "flex-end",
  },
});
