import { useSuspenseQuery } from "@tanstack/react-query";
import { Heading } from "@astryxdesign/core/Heading";
import { VStack } from "@astryxdesign/core/Stack";
import { useId, useMemo } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { myMatchesQueryOptions } from "#/matches/matches.queries.ts";
import { MatchRow } from "#/play/MatchRow.tsx";
import { orderMatchRows, type MatchRowData } from "#/play/match_rows.ts";
import { ROW_PREFETCH } from "#/play/prefetch_rows.ts";
import { useListNow, useRankedMatches } from "#/play/useRankedMatches.ts";
import { RouterButton } from "#/ui/astryx-links.tsx";
import { CountBadge } from "#/ui/CountBadge.tsx";
import { EmptySheet } from "#/ui/EmptySheet.tsx";
import { Page, PageHeader } from "#/ui/Page.tsx";
import { Sheet, SheetHead, SheetRow, SheetRows } from "#/ui/Sheet.tsx";

/**
 * My games, ongoing: every match the player sits in, as triage. What waits on
 * them is the owed sheet, its first key the one red key; what waits on
 * somebody else sits in the well below it.
 */
export function MyMatchesPage() {
  const { data } = useSuspenseQuery(myMatchesQueryOptions());
  const session = useAppSession();
  const viewerId = session?.user.id ?? "";
  const { confirmDeadlines, rankedMaps } = useRankedMatches(session !== null);
  const rows = useMemo(
    () => orderMatchRows(data.matches, confirmDeadlines, rankedMaps),
    [confirmDeadlines, data.matches, rankedMaps],
  );
  // The turn deadlines are read against a clock that is running, not against
  // the moment the page loaded.
  const now = useListNow(rows.map((row) => row.deadlineAt));

  const owed = rows.filter((row) => row.isOwed);
  const waiting = rows.filter((row) => !row.isOwed);

  return (
    <Page width="narrow">
      <PageHeader title="My games" />
      {rows.length === 0 ? (
        <EmptySheet
          action={<RouterButton label="Open lobbies" to="/matches" variant="ink" />}
          title="No matches yet"
        />
      ) : (
        <VStack gap={6}>
          {owed.length > 0 ? (
            <MatchSheet firstIndex={0} now={now} rows={owed} viewerId={viewerId} />
          ) : null}
          {waiting.length > 0 ? (
            <MatchSheet firstIndex={owed.length} now={now} rows={waiting} viewerId={viewerId} />
          ) : null}
        </VStack>
      )}
    </Page>
  );
}

function MatchSheet({
  firstIndex,
  now,
  rows,
  viewerId,
}: {
  /** Where the sheet's first row stands in the whole list. */
  firstIndex: number;
  now: number;
  rows: MatchRowData[];
  viewerId: string;
}) {
  const headingId = useId();
  const isOwed = rows[0]?.isOwed ?? false;

  return (
    <Sheet aria-labelledby={headingId} isOwed={isOwed} tone={isOwed ? "sheet" : "well"}>
      <SheetHead>
        {isOwed ? (
          <Heading id={headingId} level={2}>
            Your move{" "}
            <CountBadge
              count={rows.length}
              label={rows.length === 1 ? "1 match" : `${rows.length} matches`}
              size="heading"
              tone="ink"
            />
          </Heading>
        ) : (
          <Heading id={headingId} level={3}>
            Waiting
          </Heading>
        )}
      </SheetHead>
      <SheetRows>
        {rows.map((row, index) => (
          <SheetRow key={row.match.matchId}>
            <MatchRow
              // The loader brings the leading rows' records, so their
              // minimaps load with the page rather than as they scroll in.
              isAboveFold={firstIndex + index < ROW_PREFETCH}
              isKeyPrimary={isOwed && index === 0}
              now={now}
              row={row}
              viewerId={viewerId}
            />
          </SheetRow>
        ))}
      </SheetRows>
    </Sheet>
  );
}
