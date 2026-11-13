/**
 * Record: every rank and retag this map has been through, newest first.
 *
 * Each entry is a postmark (the day), the change as marks (`A → S`, the tags,
 * a medal), the reason in quotes and who gave it. A rank names a revision and
 * a tag names the map, so both log subjects are read and merged into one
 * history.
 *
 * Only a viewer who may read the moderation log sees it, and only once there
 * is something in it: the medal already says a map was never ranked.
 */

import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { Stack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { Token } from "@astryxdesign/core/Token";
import { colorVars, typeScaleVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useQueries } from "@tanstack/react-query";
import type { Actor } from "#/auth/actor.ts";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import {
  MAP_RANKS,
  MAP_TAG_LABELS,
  type MapCatalogEntry,
  type MapRank,
  type MapTag,
} from "#/maps/schemas.ts";
import { moderationLogQueryOptions } from "#/moderation/moderation.queries.ts";
import type { ModerationLogEntry } from "#/moderation/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { ProblemBanner } from "#/ui/EmptySheet.tsx";
import { IconRefresh } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Postmark } from "#/ui/Postmark.tsx";
import { Sheet, SheetBody, SheetHead } from "#/ui/Sheet.tsx";

/** How much of a map's history the page shows. */
const RECORD_LIMIT = 10;

export function MapJudgmentRecord({ actor, map }: { actor: Actor | null; map: MapCatalogEntry }) {
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
  // A log that could not be read is not a log with nothing in it.
  const isFailed = queries.some((query) => query.isError);
  const entries = queries
    .flatMap((query) => query.data?.actions ?? [])
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt))
    .slice(0, RECORD_LIMIT);

  if (!isPending && !isFailed && entries.length === 0) return null;

  return (
    <Sheet aria-labelledby="map-record">
      <SheetHead>
        <Heading accessibilityLevel={2} id="map-record" level={3}>
          Record
        </Heading>
      </SheetHead>
      {isFailed ? (
        <SheetBody>
          <ProblemBanner
            action={
              <Button
                clickAction={async () => {
                  await Promise.all(queries.map((query) => query.refetch()));
                }}
                icon={<Icon icon={IconRefresh} />}
                label="Retry"
                size="sm"
              />
            }
            title="Record failed to load"
          />
        </SheetBody>
      ) : isPending ? (
        <SheetBody>
          <Skeleton height={52} />
        </SheetBody>
      ) : (
        <Stack as="ol" xstyle={styles.list}>
          {entries.map((entry) => (
            <RecordEntry entry={entry} key={entry.id} />
          ))}
        </Stack>
      )}
    </Sheet>
  );
}

function RecordEntry({ entry }: { entry: ModerationLogEntry }) {
  const at = new Date(entry.createdAt);
  const isDated = !Number.isNaN(at.getTime());

  return (
    <Stack as="li" xstyle={styles.entry}>
      {isDated ? (
        <Postmark
          figure={at.getDate()}
          label={at.toLocaleDateString("en-US", {
            day: "numeric",
            month: "short",
            year: "numeric",
          })}
          ringText={String(at.getFullYear())}
          size={52}
          word={at.toLocaleDateString("en-US", { month: "short" }).toUpperCase()}
          xstyle={styles.postmark}
        />
      ) : (
        <Stack as="span" />
      )}
      <Stack xstyle={styles.body}>
        <Stack direction="horizontal" wrap="wrap" xstyle={styles.what}>
          <Change entry={entry} />
        </Stack>
        {entry.reason ? (
          <Text as="p" color="inherit" display="block" type="inherit" xstyle={styles.reason}>
            &ldquo;{entry.reason}&rdquo;
          </Text>
        ) : null}
        <Text color="inherit" maxLines={1} type="inherit" xstyle={styles.who}>
          {entry.actorName}
        </Text>
      </Stack>
    </Stack>
  );
}

/**
 * The change an entry made, as marks. The details are written by the act
 * itself and are read defensively, so an entry shaped differently than
 * expected still says what it was.
 */
function Change({ entry }: { entry: ModerationLogEntry }) {
  const details = entry.details ?? {};

  if (entry.action === "map.rank") {
    const before = asRank(details.before);
    const after = asRank(details.after);
    return (
      <>
        <MapRankMedal isUnrankedShown rank={after} size="xs" />
        {before !== null || after === null ? (
          <b>
            {before ?? "Unranked"} &rarr; {after ?? "Unranked"}
          </b>
        ) : null}
      </>
    );
  }

  if (entry.action === "map.retag") {
    const after = Array.isArray(details.after) ? details.after.map(String) : [];
    if (after.length === 0) return <b>Tags cleared</b>;
    return (
      <>
        {after.map((tag) => (
          <Token key={tag} label={MAP_TAG_LABELS[tag as MapTag] ?? tag} size="sm" />
        ))}
      </>
    );
  }

  return <b>{entry.action}</b>;
}

function asRank(value: unknown): MapRank | null {
  return typeof value === "string" && (MAP_RANKS as readonly string[]).includes(value)
    ? (value as MapRank)
    : null;
}

const styles = stylex.create({
  list: {
    margin: 0,
    paddingBlock: "0 8px",
    paddingInline: {
      default: "20px",
      [pageLayout.phoneMedia]: "16px",
    },
    listStyle: "none",
  },
  entry: {
    display: "grid",
    gridTemplateColumns: {
      default: "56px minmax(0, 1fr)",
      [pageLayout.phoneMedia]: "48px minmax(0, 1fr)",
    },
    alignItems: "start",
    gap: "12px",
    paddingBlock: "12px",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
  },
  postmark: {
    inlineSize: {
      default: "52px",
      [pageLayout.phoneMedia]: "46px",
    },
    blockSize: {
      default: "52px",
      [pageLayout.phoneMedia]: "46px",
    },
    marginBlockStart: "2px",
  },
  body: {
    display: "grid",
    gap: "4px",
    minInlineSize: 0,
  },
  what: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: "6px 8px",
    minBlockSize: "26px",
  },
  reason: {
    margin: 0,
    color: colorVars["--color-text-secondary"],
    fontSize: typeScaleVars["--text-supporting-size"],
    lineHeight: 1.4,
    textWrap: "pretty",
  },
  who: {
    fontSize: typeScaleVars["--text-supporting-size"],
    lineHeight: 1.4,
  },
});
