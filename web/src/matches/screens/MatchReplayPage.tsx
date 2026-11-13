import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { useActiveMatchRunner } from "#/engine/runtime_context.tsx";
import { useGameStore } from "#/engine/store.ts";
import { getFactionByCode } from "#/factions.ts";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import type { ArmyView } from "#/matches/components/Armies.tsx";
import { InkIconKey } from "#/matches/components/MatchFrame.tsx";
import type { LogBoundary } from "#/matches/components/turn_log.ts";
import { ViewpointSelector } from "#/matches/components/ViewpointSelector.tsx";
import { buildArmies, recordedSeat } from "#/matches/match_armies.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { fetchMatchReplayBytes, matchReplayDownloadPath } from "#/matches/replay_archive.ts";
import { readArchiveOutline } from "#/replay/archive_outline.ts";
import { ReplayScreen } from "#/replay/ReplayScreen.tsx";
import { BackKey } from "#/ui/BackKey.tsx";
import { IconDownload } from "#/ui/icons.tsx";

/**
 * A finished match, read back at its own address.
 *
 * It plays the stored archive rather than asking the host, which answers a
 * live match one seat at a time. The archive carries every action, so the
 * engine can project any seat's view: that is what makes the viewpoint
 * control possible.
 */
export function MatchReplayPage({
  joinSlug = null,
  matchId,
}: {
  joinSlug?: string | null;
  matchId: string;
}) {
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, joinSlug));
  const { data: mapEntry } = useQuery(mapCatalogEntryQueryOptions(match.mapId, match.mapRevision));
  const runner = useActiveMatchRunner();
  const session = useAppSession();
  const playerRoster = useGameStore((state) => state.playerRoster);
  const replayViewpoint = useGameStore((state) => state.replayViewpoint);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [outline, setOutline] = useState<LogBoundary[] | null>(null);
  // Bumped to read the archive again after a failure.
  const [attempt, setAttempt] = useState(0);
  const armies = useMemo(
    () => buildArmies(match.participants, playerRoster),
    [match.participants, playerRoster],
  );
  // This account's army reads as YOU and keeps its pinned row in a long
  // roster, as it did while the match was played.
  const userId = session?.user.id ?? null;
  const viewerSeat = useMemo(
    () => recordedSeat(match.participants, userId),
    [match.participants, userId],
  );

  // The archive is immutable and served with a year's cache, so it is fetched
  // once for the life of the page and never revalidated.
  useEffect(() => {
    let canceled = false;

    setLoadError(null);
    setOutline(null);
    void fetchMatchReplayBytes(matchId)
      .then((bytes) => {
        if (canceled) return undefined;
        // The outline is read beside the engine, so the day ticks and the log
        // are there as soon as the board is.
        void readArchiveOutline(bytes).then((read) => {
          if (!canceled) setOutline(read);
        });
        return runner.loadReplay(bytes, () => !canceled);
      })
      .catch((error: unknown) => {
        if (canceled) return;
        setLoadError(
          error instanceof Error
            ? error.message
            : "No archive was stored for this match, so there is nothing to watch.",
        );
        console.error("Failed to open a finished match for review:", error);
      });

    return () => {
      canceled = true;
    };
  }, [attempt, matchId, runner]);

  const views: ArmyView[] = armies.map((army) => ({
    army,
    isViewer: viewerSeat === army.entry.playerId,
    clock: null,
    onFactionChange: (factionId: number) => {
      void runner
        .setPlayerDisplayFaction(
          army.entry.playerId,
          factionId === getFactionByCode(army.entry.actualFactionCode)?.id ? null : factionId,
        )
        .catch((error: unknown) => {
          console.error("Error updating replay faction depiction:", error);
        });
    },
  }));

  return (
    <ReplayScreen
      archiveKey={
        <InkIconKey
          href={matchReplayDownloadPath(matchId)}
          icon={IconDownload}
          label="Download the replay archive"
        />
      }
      back={<BackKey label="Back to History" to="/my/history" />}
      error={loadError}
      locatorSrc={mapEntry?.screenshot.small ?? null}
      map={mapEntry ? { width: mapEntry.width, height: mapEntry.height } : null}
      onRetry={() => setAttempt((count) => count + 1)}
      outline={outline}
      runner={runner}
      title={mapEntry?.name ?? match.name}
      viewpoint={
        match.settings.fogEnabled && playerRoster ? (
          <ViewpointSelector
            armies={armies}
            onChange={(playerId, followsActivePlayer) => {
              void runner
                .setReplayViewpoint(playerId, followsActivePlayer)
                .catch((error: unknown) => {
                  console.error("Error changing the replay viewpoint:", error);
                });
            }}
            viewpoint={replayViewpoint}
          />
        ) : undefined
      }
      views={views}
    />
  );
}
