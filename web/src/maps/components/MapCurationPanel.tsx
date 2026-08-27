/**
 * The bench a map is graded and tagged at.
 *
 * Tags and ranks are two different acts by two different rules, and the panel
 * says so rather than putting them behind one save. An author tags the map
 * they wrote and no record is kept, because that is authorship. A moderator
 * tagging somebody else's map, or grading any map, is judgement about the
 * catalog, so it is signed with a reason and written to the log.
 *
 * Nobody grades their own work. When a moderator opens the map they wrote,
 * the rank bench is not hidden: it says why it is closed, because a control
 * that vanishes teaches nobody the rule.
 *
 * The keys here are the keys of the filter console, deliberately. Choosing a
 * rank and filtering by a rank are the same vocabulary, so they are the same
 * row of keys.
 */

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { Divider } from "@astryxdesign/core/Divider";
import { Heading } from "@astryxdesign/core/Heading";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";

import { ToggleButton, ToggleButtonGroup } from "@astryxdesign/core/ToggleButton";
import { useState } from "react";
import type { Actor } from "#/auth/actor.ts";
import { TextArea } from "@astryxdesign/core/TextArea";
import { Button } from "#/ui/Button.tsx";
import { mapRankGrant, mapTagGrant } from "#/maps/map_authz.ts";
import { MAP_RANK_DESCRIPTIONS, MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import { setMapRankFn, setMapTagsFn } from "#/maps/maps.functions.ts";
import { mapKeys } from "#/maps/maps.keys.ts";
import { MAP_RANK_FILTERS, sortMapTags } from "#/maps/map_taxonomy.ts";
import { moderationKeys } from "#/moderation/moderation.keys.ts";
import {
  MODERATION_REASON_MAX_LENGTH,
  MODERATION_REASON_MIN_LENGTH,
} from "#/moderation/schemas.ts";
import {
  MAP_RANK_FILTER_LABELS,
  MAP_TAG_LABELS,
  MAP_TAGS,
  MAP_UNRANKED_FILTER,
  type MapCatalogEntry,
  type MapRank,
  type MapTag,
} from "#/maps/schemas.ts";

export function MapCurationPanel({ actor, map }: { actor: Actor | null; map: MapCatalogEntry }) {
  const tagGrant = mapTagGrant(map, actor);
  const rankGrant = mapRankGrant(map, actor);
  // A moderator who wrote the map holds the role and not the act. That is the
  // one refusal worth drawing, because it is a rule and not an absence.
  const isOwnWork = actor !== null && map.authorUserId === actor.userId;
  const isRankRefused = rankGrant === null && isOwnWork && actor.can({ map: ["rank"] });

  if (tagGrant === null && rankGrant === null && !isRankRefused) return null;

  return (
    <Card padding={6}>
      <VStack gap={6}>
        <VStack gap={1}>
          <Heading level={2}>Curation</Heading>
          <Text color="secondary">
            {rankGrant === "moderator"
              ? "A rank is this site's judgement of the map and is kept with the reason you give for it. Tags describe how the map plays."
              : "Tags describe how the map plays. They belong to the map, so they carry across every revision of it."}
          </Text>
        </VStack>

        {rankGrant !== null ? (
          <RankBench
            key={`${map.mapId}:${map.revision}`}
            mapId={map.mapId}
            rank={map.rank}
            revision={map.revision}
          />
        ) : null}

        {isRankRefused ? (
          <Banner
            description="Ranking is this site's judgement of a map, so it is never the author's to make. Another moderator grades this one."
            status="info"
            title="You wrote this map"
          />
        ) : null}

        {rankGrant !== null || isRankRefused ? tagGrant !== null ? <Divider /> : null : null}

        {tagGrant !== null ? (
          <TagBench grant={tagGrant} key={map.mapId} mapId={map.mapId} tags={map.tags} />
        ) : null}
      </VStack>
    </Card>
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
  const [chosen, setChosen] = useState<MapRank | null>(rank);
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

  const isChanged = chosen !== rank;
  const isReasonGiven = reason.trim().length >= MODERATION_REASON_MIN_LENGTH;

  async function handleSubmit(): Promise<void> {
    if (!isChanged) return;
    if (!isReasonGiven) {
      setError("Say why this map earns that grade. The reason is kept with it.");
      return;
    }
    setError(null);
    try {
      await mutation.mutateAsync({
        data: { map: { mapId, revision }, rank: chosen, reason: reason.trim() },
      });
    } catch (rankError) {
      setError(writeFailure(rankError, "The rank could not be written."));
    }
  }

  return (
    <VStack gap={4}>
      <VStack gap={1.5}>
        <Text color="secondary" type="label">
          Rank of revision {revision}
        </Text>
        <HStack align="center" gap={4} wrap="wrap">
          <MapRankMedal rank={chosen} size="md" />
          <ToggleButtonGroup
            label="Rank"
            onChange={(next) => {
              setError(null);
              setChosen(next === MAP_UNRANKED_FILTER || next === null ? null : (next as MapRank));
            }}
            size="sm"
            type="single"
            value={chosen ?? MAP_UNRANKED_FILTER}
          >
            {MAP_RANK_FILTERS.map((option) => (
              <ToggleButton key={option} label={MAP_RANK_FILTER_LABELS[option]} value={option} />
            ))}
          </ToggleButtonGroup>
        </HStack>
        <Text color="secondary">
          {chosen === null
            ? "Held in the catalog without a grade. A player filtering for unranked maps finds it."
            : MAP_RANK_DESCRIPTIONS[chosen]}
        </Text>
      </VStack>

      <TextArea
        description="Kept in the moderation log with your name against it."
        isRequired
        label="Reason"
        maxLength={MODERATION_REASON_MAX_LENGTH}
        onChange={(value) => {
          setError(null);
          setReason(value);
        }}
        placeholder="Why this map earns that grade"
        rows={2}
        value={reason}
      />

      {error ? <Banner description={error} status="error" title="Rank not written" /> : null}

      <HStack gap={3} wrap="wrap">
        <Button
          clickAction={handleSubmit}
          isDisabled={!isChanged || mutation.isPending}
          isLoading={mutation.isPending}
          label={chosen === null ? "Take the rank away" : `Set rank ${chosen}`}
          variant="primary"
        />
        {isChanged ? (
          <Button
            clickAction={() => {
              setChosen(rank);
              setError(null);
            }}
            label="Undo"
            variant="ghost"
          />
        ) : null}
      </HStack>
    </VStack>
  );
}

function TagBench({
  grant,
  mapId,
  tags,
}: {
  grant: "owner" | "moderator";
  mapId: string;
  tags: readonly MapTag[];
}) {
  const queryClient = useQueryClient();
  const [chosen, setChosen] = useState<MapTag[]>([...tags]);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: setMapTagsFn,
    onSuccess: async () => {
      setReason("");
      setError(null);
      await invalidateMap(queryClient);
    },
  });

  const isChanged = !sameTags(chosen, tags);
  const isReasonNeeded = grant === "moderator";
  const isReasonGiven = reason.trim().length >= MODERATION_REASON_MIN_LENGTH;

  async function handleSubmit(): Promise<void> {
    if (!isChanged) return;
    if (isReasonNeeded && !isReasonGiven) {
      setError("Say why the tags change. A moderator retagging somebody's map signs the change.");
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
      setError(writeFailure(tagError, "The tags could not be written."));
    }
  }

  return (
    <VStack gap={4}>
      <VStack gap={1.5}>
        <Text color="secondary" type="label">
          {grant === "moderator" ? "Tags, as moderator" : "Tags"}
        </Text>
        <ToggleButtonGroup
          label="Tags"
          onChange={(next) => {
            setError(null);
            setChosen(sortMapTags((next as MapTag[]) ?? []));
          }}
          size="sm"
          type="multiple"
          value={[...chosen]}
        >
          {MAP_TAGS.map((tag) => (
            <ToggleButton key={tag} label={MAP_TAG_LABELS[tag]} value={tag} />
          ))}
        </ToggleButtonGroup>
        <Text color="secondary">
          {chosen.length === 0
            ? "Untagged. A player filtering for a kind of game will not find this map."
            : "A map carries as many tags as fit it. Every tag pressed has to fit for a filter to find it."}
        </Text>
      </VStack>

      {isReasonNeeded ? (
        <TextArea
          description="Kept in the moderation log with your name against it."
          isRequired
          label="Reason"
          maxLength={MODERATION_REASON_MAX_LENGTH}
          onChange={(value) => {
            setError(null);
            setReason(value);
          }}
          placeholder="Why the tags change"
          rows={2}
          value={reason}
        />
      ) : null}

      {error ? <Banner description={error} status="error" title="Tags not written" /> : null}

      <HStack gap={3} wrap="wrap">
        <Button
          clickAction={handleSubmit}
          isDisabled={!isChanged || mutation.isPending}
          isLoading={mutation.isPending}
          label="Save tags"
          variant={grant === "moderator" ? "primary" : "secondary"}
        />
        {isChanged ? (
          <Button
            clickAction={() => {
              setChosen([...tags]);
              setError(null);
            }}
            label="Undo"
            variant="ghost"
          />
        ) : null}
      </HStack>
    </VStack>
  );
}

/**
 * The map, the boards it sits on, and the log, all read again.
 *
 * A rank changes which filtered boards hold this map, so the whole catalog is
 * dropped rather than one page of it.
 */
async function invalidateMap(queryClient: ReturnType<typeof useQueryClient>): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: mapKeys.all }),
    queryClient.invalidateQueries({ queryKey: moderationKeys.all }),
  ]);
}

function sameTags(left: readonly MapTag[], right: readonly MapTag[]): boolean {
  return left.length === right.length && left.every((tag, index) => tag === right[index]);
}

/** What a refused or failed write says, in this product's own words. */
function writeFailure(error: unknown, fallback: string): string {
  const message = error instanceof Error ? error.message : "";
  if (/forbidden|403/i.test(message)) return "That is not yours to change.";
  if (/reason/i.test(message)) return "The change needs a reason.";
  return `${fallback} Try again in a moment.`;
}
