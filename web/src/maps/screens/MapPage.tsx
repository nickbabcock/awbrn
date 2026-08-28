/*
 * THE FIELD RECORD
 *
 * THESIS: a map is a thing the site holds an opinion about, not a row in a
 *   picker. Its page is the record: the whole battlefield at native pixels,
 *   what it seats, how it plays, the grade this site gave it, and the one
 *   command that turns it into a match. It is also where the grade and the
 *   tags are written, so the judgement and the evidence for it are on the
 *   same screen.
 * OWN-WORLD: cream panels on the open sky. The grade is the game's own
 *   end-of-battle plate, the letter in the signage voice under a bar of its
 *   color, and an ungraded map wears the dashed empty slot this system gives
 *   a place waiting to be filled.
 * STORY: a player opens a map from the board, reads it, and starts a match on
 *   it. A moderator opens the same page and grades it.
 * FIRST VIEWPORT: the map's name and its grade, then the battlefield itself.
 */

import { useQuery } from "@tanstack/react-query";
import { Banner } from "@astryxdesign/core/Banner";
import { Card } from "@astryxdesign/core/Card";
import { Divider } from "@astryxdesign/core/Divider";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { Section } from "@astryxdesign/core/Section";
import { Skeleton } from "@astryxdesign/core/Skeleton";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { Token } from "@astryxdesign/core/Token";
import { ArrowLeft as ArrowLeftIcon } from "pixelarticons/react/ArrowLeft";
import { useActor } from "#/auth/useActor.ts";
import { MapCurationPanel } from "#/maps/components/MapCurationPanel.tsx";
import { MapJudgementRecord } from "#/maps/components/MapJudgementRecord.tsx";
import { MapPicture } from "#/maps/components/MapPicture.tsx";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import { mapQueryOptions } from "#/maps/maps.queries.ts";
import { MAP_TAG_LABELS, type MapCatalogEntry } from "#/maps/schemas.ts";
import { RouterButton, RouterTextLink } from "#/ui/astryx-links.tsx";
import { TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";

export function MapPage({ mapId }: { mapId: string }) {
  const actor = useActor();
  const mapQuery = useQuery(mapQueryOptions(mapId));

  return (
    <Section padding={6} variant="transparent">
      <VStack gap={8}>
        <HStack align="center" gap={1}>
          <ArrowLeftIcon aria-hidden height={14} width={14} />
          <RouterTextLink to="/maps">All maps</RouterTextLink>
        </HStack>

        {mapQuery.isPending ? <RecordSkeleton /> : null}

        {mapQuery.isError ? (
          <Banner
            description="The catalog could not be read. Try again in a moment."
            status="error"
            title="Map unavailable"
          />
        ) : null}

        {!mapQuery.isPending && !mapQuery.isError && mapQuery.data === null ? (
          <EmptyState
            actions={<RouterButton label="Back to the board" to="/maps" variant="secondary" />}
            description="No map in the catalog holds that id. It may never have been imported, or the address may be wrong."
            headingLevel={1}
            title="No such map"
          />
        ) : null}

        {mapQuery.data ? (
          <>
            <MapRecord map={mapQuery.data} />
            <MapCurationPanel actor={actor} map={mapQuery.data} />
            <MapJudgementRecord actor={actor} map={mapQuery.data} />
          </>
        ) : null}
      </VStack>
    </Section>
  );
}

function MapRecord({ map }: { map: MapCatalogEntry }) {
  const picture = mapScreenshotSize("full", map.width, map.height);

  return (
    <VStack gap={6}>
      <HStack align="center" gap={6} justify="between" wrap="wrap">
        <VStack gap={1}>
          <Heading level={1} type="display-2">
            {map.name}
          </Heading>
          <Text type="label">By {map.author}</Text>
        </VStack>
        <VStack align="center" gap={1.5}>
          <MapRankMedal rank={map.rank} size="lg" />
          {/*
           * A grade names the revision it was given to and not the map, so
           * the plate says which one. An edited map comes back here with a
           * new number and no grade, and this line is where a reader learns
           * why the medal emptied.
           */}
          <Text type="label">
            {map.rank ? `Rank ${map.rank}` : "Unranked"} · revision {map.revision}
          </Text>
        </VStack>
      </HStack>

      <Grid
        align="start"
        columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
        gap={6}
      >
        <Card padding={4}>
          <MapPicture
            alt={`The battlefield of ${map.name}`}
            sourceHeight={picture.height}
            sourceWidth={picture.width}
            src={map.screenshot.full}
          />
        </Card>

        <Card padding={6}>
          <VStack gap={6}>
            <MetadataList columns={2} label={{ position: "top" }}>
              <MetadataListItem label="Armies">{map.playerCount}</MetadataListItem>
              <MetadataListItem label="Size">
                {map.width} × {map.height}
              </MetadataListItem>
              <MetadataListItem label="Revision">{map.revision}, the current one</MetadataListItem>
              <MetadataListItem label="Source">
                {map.origin ? `AWBW ${map.origin.sourceMapId}` : "AWBRN"}
              </MetadataListItem>
              <MetadataListItem label="Held since">{heldSince(map.addedAt)}</MetadataListItem>
            </MetadataList>

            <VStack gap={2}>
              <Text color="secondary" type="label">
                Plays as
              </Text>
              {map.tags.length > 0 ? (
                <HStack gap={1.5} wrap="wrap">
                  {map.tags.map((tag) => (
                    <Token key={tag} label={MAP_TAG_LABELS[tag]} size="sm" />
                  ))}
                </HStack>
              ) : (
                <Text color="secondary">
                  Untagged. Nobody has said yet what kind of game this map makes.
                </Text>
              )}
            </VStack>

            <Divider />

            <VStack gap={2}>
              <RouterButton
                label="Create match on this map"
                search={{ map: map.mapId }}
                to="/matches/new"
                variant="primary"
                width="100%"
              />
              <Text color="secondary" type="supporting">
                Opens the create screen with this battlefield already chosen.
              </Text>
            </VStack>
          </VStack>
        </Card>
      </Grid>
    </VStack>
  );
}

function RecordSkeleton() {
  return (
    <VStack gap={6}>
      <Skeleton height={44} radius={0} width="45%" />
      <Grid
        align="start"
        columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
        gap={6}
      >
        <Card padding={4}>
          <Skeleton height={320} radius="none" />
        </Card>
        <Card padding={6}>
          <VStack gap={4}>
            <Skeleton height={20} index={1} radius={0} />
            <Skeleton height={20} index={2} radius={0} />
            <Skeleton height={20} index={3} radius={0} width="70%" />
          </VStack>
        </Card>
      </Grid>
    </VStack>
  );
}

/** The day the catalog took the map, written the way a log line is. */
function heldSince(addedAt: string): string {
  const held = new Date(addedAt);
  if (Number.isNaN(held.getTime())) return "Unknown";
  return held.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}
