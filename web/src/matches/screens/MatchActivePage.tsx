import { useSuspenseQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useMemo } from "react";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import {
  DEFAULT_CO_PORTRAIT_KEY,
  getCoPortraitByAwbwId,
  loadCoPortraitCatalog,
} from "#/components/co_portraits.ts";
import { getFactionById } from "#/factions.ts";
import { getFactionVisual } from "#/faction_visuals.ts";
import { PlayerHeader } from "#/components/PlayerHeader.tsx";
import { Frame, Heading, Kicker, Page, Section, Text } from "#/ui/primitives.tsx";
import { tokens } from "#/ui/theme.stylex.ts";
import { matchDetailQueryOptions } from "#/matches/matches.queries.ts";
import { useMatchWebSocket } from "#/matches/match_websocket.ts";

export function MatchActivePage({ matchId }: { matchId: string }) {
  const { data: match } = useSuspenseQuery(matchDetailQueryOptions(matchId, null));
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const { status } = useMatchWebSocket(matchId, (_msg) => {
    // TODO: apply incoming game events to local state
  });

  return (
    <Page width="wide">
      <Section>
        <div {...stylex.props(styles.layout)}>
          <header {...stylex.props(styles.header)}>
            <div {...stylex.props(styles.headerCopy)}>
              <Kicker xstyle={styles.headerKicker}>Match Active</Kicker>
              <Heading size="display">{match.name}</Heading>
              <Text size="lg" tone="strong">
                Map {match.mapId} · {match.maxPlayers} players ·{" "}
                {match.settings.fogEnabled ? "Fog on" : "Fog off"}
              </Text>
            </div>
          </header>

          <div {...stylex.props(styles.mainGrid)}>
            <Frame as="section" surface="panel" padding="none" xstyle={styles.gameSection}>
              <div {...stylex.props(styles.gamePlaceholder)}>
                <Kicker>Game Board</Kicker>
                <Text tone="muted">
                  {status === "connected"
                    ? "Connected. Game canvas will render here."
                    : status === "connecting"
                      ? "Connecting to match..."
                      : status === "error"
                        ? "Connection error — retrying."
                        : "Disconnected — reconnecting."}
                </Text>
                <div {...stylex.props(styles.statusDot(status))} />
              </div>
            </Frame>

            <Frame as="section" surface="panel" padding="none" xstyle={styles.rosterSection}>
              <div {...stylex.props(styles.rosterInner)}>
                <div {...stylex.props(styles.sectionHeader)}>
                  <Kicker>Roster</Kicker>
                  <Heading size="lg">Players</Heading>
                </div>

                <div {...stylex.props(styles.participantList)}>
                  {match.participants.map((participant) => {
                    const faction = getFactionById(participant.factionId);
                    const factionVisual = getFactionVisual(faction?.code ?? "os");
                    const portrait = getCoPortraitByAwbwId(participant.coId);

                    return (
                      <div
                        key={participant.slotIndex}
                        {...stylex.props(styles.participantCard(factionVisual.accent))}
                      >
                        <PlayerHeader
                          factionCode={faction?.code ?? "os"}
                          name={participant.userName}
                        />
                        <div {...stylex.props(styles.participantBody)}>
                          <CoPortrait
                            catalog={portraitCatalog}
                            coKey={portrait?.key ?? DEFAULT_CO_PORTRAIT_KEY}
                            fallbackLabel={portrait?.displayName ?? "No CO"}
                          />
                          <Text size="sm" tone="muted">
                            {portrait?.displayName ?? "No CO"}
                          </Text>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </Frame>
          </div>
        </div>
      </Section>
    </Page>
  );
}

const STATUS_COLORS: Record<string, string> = {
  connected: "#4ade80",
  connecting: "#facc15",
  disconnected: "#94a3b8",
  error: "#f87171",
};

const styles = stylex.create({
  layout: {
    display: "grid",
    gap: tokens.space6,
  },
  header: {
    paddingBottom: tokens.space5,
    borderBottomWidth: 3,
    borderBottomStyle: "solid",
    borderBottomColor: tokens.chromeBorderSoft,
  },
  headerCopy: {
    display: "grid",
    gap: tokens.space2,
  },
  headerKicker: {
    color: tokens.brandHover,
  },
  mainGrid: {
    display: "grid",
    gap: tokens.space8,
    gridTemplateColumns: {
      default: "minmax(0, 1fr) minmax(360px, 460px)",
      "@media (max-width: 980px)": "1fr",
    },
    alignItems: "start",
  },
  gameSection: {
    overflow: "visible",
    minHeight: 360,
  },
  gamePlaceholder: {
    display: "grid",
    gap: tokens.space3,
    padding: tokens.space6,
    alignContent: "start",
  },
  statusDot: (status: string) => ({
    width: 10,
    height: 10,
    borderRadius: "50%",
    backgroundColor: STATUS_COLORS[status] ?? STATUS_COLORS.disconnected,
  }),
  rosterSection: {
    overflow: "visible",
  },
  rosterInner: {
    display: "grid",
    gap: tokens.space4,
    alignContent: "start",
    padding: tokens.space6,
  },
  sectionHeader: {
    display: "grid",
    gap: tokens.space1,
  },
  participantList: {
    display: "grid",
    gap: tokens.space3,
  },
  participantCard: (accent: string) => ({
    display: "grid",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: accent,
    borderRadius: tokens.radius2,
    overflow: "hidden",
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
  }),
  participantBody: {
    display: "flex",
    alignItems: "center",
    gap: tokens.space3,
    padding: tokens.space3,
  },
});
