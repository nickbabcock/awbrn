/**
 * What has been decided about this map, and why.
 *
 * A rank is an opinion the site publishes, so a moderator about to change one
 * should be able to read the last one and the reason given for it. The log
 * already holds both; this is the part of it that names this map.
 *
 * Two subjects have to be read, because a rank names a revision and a tag
 * names the map. They are one history to the person reading, so the two are
 * put in one list, newest first.
 *
 * Only a viewer who may read the log sees it. A player reads the grade on the
 * plate above and does not need the argument behind it.
 */

import { useQueries } from "@tanstack/react-query";
import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { List, ListItem } from "@astryxdesign/core/List";
import { VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import type { Actor } from "#/auth/actor.ts";
import { moderationLogQueryOptions } from "#/moderation/moderation.queries.ts";
import type { ModerationLogEntry } from "#/moderation/schemas.ts";
import type { MapCatalogEntry } from "#/maps/schemas.ts";

/** How much of a map's history the page shows before it is a screen of its own. */
const RECORD_LIMIT = 10;

export function MapJudgementRecord({ actor, map }: { actor: Actor | null; map: MapCatalogEntry }) {
  const mayRead = actor?.can({ user: ["list"] }) ?? false;

  const queries = useQueries({
    queries: [
      {
        ...moderationLogQueryOptions({
          limit: RECORD_LIMIT,
          subjectType: "map",
          subjectId: map.mapId,
        }),
        enabled: mayRead,
      },
      {
        ...moderationLogQueryOptions({
          limit: RECORD_LIMIT,
          subjectType: "map_revision",
          subjectId: `${map.mapId}:${map.revision}`,
        }),
        enabled: mayRead,
      },
    ],
  });

  if (!mayRead) return null;

  const isPending = queries.some((query) => query.isPending);
  // A log that could not be read is not a log with nothing in it, so a failed
  // read says so rather than claiming the map has never been judged.
  const isFailed = queries.some((query) => query.isError);
  const entries = queries
    .flatMap((query) => query.data?.actions ?? [])
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt))
    .slice(0, RECORD_LIMIT);

  return (
    <Card padding={6}>
      <VStack gap={4}>
        <VStack gap={1}>
          <Heading level={2}>Record</Heading>
          <Text color="secondary">
            Every rank and every retag this map has been through, with the reason given at the time.
            A rank names one revision of the map; a tag names the map itself.
          </Text>
        </VStack>

        {isPending ? (
          <Text color="secondary" type="label">
            Reading the log
          </Text>
        ) : isFailed ? (
          <Banner
            description="The moderation log could not be read. Try again in a moment."
            status="error"
            title="Record unavailable"
          />
        ) : entries.length === 0 ? (
          <EmptyState
            description="The first grade or retag written here starts its record."
            headingLevel={3}
            isCompact
            title="Nothing decided yet"
          />
        ) : (
          <List>
            {entries.map((entry) => (
              <ListItem
                description={<Text color="secondary">{entry.reason}</Text>}
                endContent={
                  <Text color="secondary" type="label">
                    {entry.actorName} · {stamp(entry.createdAt)}
                  </Text>
                }
                key={entry.id}
                label={judgement(entry)}
              />
            ))}
          </List>
        )}
      </VStack>
    </Card>
  );
}

/**
 * One line of the log, said as the change it was.
 *
 * The details of an act are written by the act itself and are printed rather
 * than branched on, so an act whose details are shaped differently than
 * expected still reads as the act it was.
 */
function judgement(entry: ModerationLogEntry): string {
  const details = entry.details ?? {};
  if (entry.action === "map.rank") {
    const before = typeof details.before === "string" ? details.before : "no rank";
    const after = typeof details.after === "string" ? details.after : "no rank";
    // A rank is given to one revision, and the list holds acts against the
    // map next to it, so the line says which revision it graded.
    const revision = typeof details.revision === "number" ? ` of revision ${details.revision}` : "";
    return `Rank ${before} to ${after}${revision}`;
  }
  if (entry.action === "map.retag") {
    const after = Array.isArray(details.after) ? details.after : [];
    return after.length === 0 ? "Tags cleared" : `Tagged ${after.join(", ")}`;
  }
  return entry.action;
}

function stamp(createdAt: string): string {
  const at = new Date(createdAt);
  if (Number.isNaN(at.getTime())) return "Unknown";
  return at.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}
