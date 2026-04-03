import { useEffect, useMemo, useRef, useState } from "react";
import * as stylex from "@stylexjs/stylex";
import { useAppSession } from "../../auth/useAppSession";
import { awbwMapAssetPath } from "../../awbw/paths";
import type { AwbwMapData } from "../../awbw/schemas";
import { CoPortrait } from "../../components/CoPortrait";
import { listCoPortraits, loadCoPortraitCatalog } from "../../components/co_portraits";
import { defaultFactionIdForSlot, factions, getFactionById } from "../../factions";
import { getFactionVisual } from "../../faction_visuals";
import {
  Badge,
  Button,
  CoPickerField,
  Frame,
  Heading,
  Kicker,
  Page,
  Section,
  SelectField,
  Stack,
  Text,
} from "../../ui/primitives";
import { tokens } from "../../ui/theme.stylex";
import { MatchMapPreview } from "../components/MatchMapPreview";
import { getMatchFn, mutateMatchFn } from "../matches.functions";
import type { MatchMutationRequest, MatchSnapshot } from "../schemas";

const coOptions = listCoPortraits();

export function MatchLobbyPage({
  matchId,
  joinSlug,
}: {
  matchId: string;
  joinSlug: string | null;
}) {
  const session = useAppSession();
  const portraitCatalog = useMemo(() => loadCoPortraitCatalog(), []);
  const [match, setMatch] = useState<MatchSnapshot | null>(null);
  const [mapData, setMapData] = useState<AwbwMapData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const requestKeyRef = useRef("");
  const snapshotRequestRef = useRef(0);

  requestKeyRef.current = `${matchId}:${joinSlug ?? ""}`;

  useEffect(() => {
    void loadMatchSnapshot();
  }, [matchId, joinSlug]);

  useEffect(() => {
    if (!match) {
      setMapData(null);
      return;
    }

    let cancelled = false;
    setMapData(null);
    void (async () => {
      try {
        const response = await fetch(awbwMapAssetPath(match.mapId));
        if (!response.ok) {
          throw new Error("Map metadata could not be loaded.");
        }
        const payload = (await response.json()) as AwbwMapData;
        if (!cancelled) {
          setMapData(payload);
        }
      } catch (nextError) {
        if (!cancelled) {
          setMapData(null);
          setError(
            nextError instanceof Error ? nextError.message : "Map metadata could not be loaded.",
          );
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [match?.mapId]);

  const currentUserId = session?.user.id ?? null;
  const participantsBySlot = useMemo(
    () =>
      new Map(match?.participants.map((participant) => [participant.slotIndex, participant]) ?? []),
    [match],
  );
  const myParticipant =
    currentUserId === null
      ? null
      : (match?.participants.find((participant) => participant.userId === currentUserId) ?? null);

  async function loadMatchSnapshot(): Promise<void> {
    const requestId = ++snapshotRequestRef.current;
    const requestKey = requestKeyRef.current;
    setIsLoading(true);
    setError(null);
    setMatch(null);

    try {
      const snapshot = await getMatchFn({ data: { matchId, joinSlug } });
      if (snapshotRequestRef.current !== requestId || requestKeyRef.current !== requestKey) {
        return;
      }
      setMatch(snapshot);
    } catch (nextError) {
      if (snapshotRequestRef.current !== requestId || requestKeyRef.current !== requestKey) {
        return;
      }
      setMatch(null);
      setError(nextError instanceof Error ? nextError.message : "Failed to load the lobby.");
    } finally {
      if (snapshotRequestRef.current === requestId && requestKeyRef.current === requestKey) {
        setIsLoading(false);
      }
    }
  }

  async function submitAction(action: MatchMutationRequest, pendingLabel: string): Promise<void> {
    const requestKey = requestKeyRef.current;
    setPendingAction(pendingLabel);
    setError(null);

    try {
      const response = await mutateMatchFn({ data: { matchId, action } });
      if (requestKeyRef.current !== requestKey) {
        return;
      }
      setMatch(response.match);
    } catch (nextError) {
      if (requestKeyRef.current !== requestKey) {
        return;
      }
      setError(nextError instanceof Error ? nextError.message : "Lobby update failed.");
    } finally {
      if (requestKeyRef.current === requestKey) {
        setPendingAction(null);
      }
    }
  }

  const shareUrl =
    match?.isPrivate && match.joinSlug && typeof window !== "undefined"
      ? `${window.location.origin}/matches/${match.matchId}?join=${match.joinSlug}`
      : null;

  return (
    <Page width="wide">
      <Section>
        <div {...stylex.props(styles.page)}>
          {isLoading ? (
            <Text size="lg" tone="muted">
              Loading lobby...
            </Text>
          ) : !match && error ? (
            <Text size="lg" tone="danger">
              {error}
            </Text>
          ) : !match ? (
            <Text size="lg" tone="muted">
              Match not found.
            </Text>
          ) : (
            <div {...stylex.props(styles.layout)}>
              <Stack gap="lg">
                <Frame xstyle={styles.summaryFrame}>
                  <Stack gap="sm">
                    <Kicker xstyle={styles.summaryKicker}>
                      {match.phase === "active" ? "Match Active" : "Lobby"}
                    </Kicker>
                    <Heading size="display">{match.name}</Heading>
                    <div {...stylex.props(styles.badges)}>
                      <Badge tone="brand">{match.phase}</Badge>
                      {match.isPrivate ? <Badge tone="danger">Private</Badge> : null}
                    </div>
                    <Text size="lg" tone="strong">
                      Map {match.mapId} · {match.maxPlayers} players · creator {match.creatorName}
                    </Text>
                    <Text size="sm" tone="strong">
                      Settings: {match.settings.fogEnabled ? "Fog On" : "Fog Off"} ·{" "}
                      {match.settings.startingFunds.toLocaleString()} funds
                    </Text>
                  </Stack>
                </Frame>

                <Frame xstyle={styles.previewFrame}>
                  <Stack gap="md">
                    <MatchMapPreview mapId={match.mapId} xstyle={styles.previewCanvas} />
                    {mapData ? (
                      <div {...stylex.props(styles.metaGrid)}>
                        <div {...stylex.props(styles.metaItem)}>
                          <Text size="sm" tone="muted">
                            Map
                          </Text>
                          <Text tone="strong">{mapData.Name}</Text>
                        </div>
                        <div {...stylex.props(styles.metaItem)}>
                          <Text size="sm" tone="muted">
                            Author
                          </Text>
                          <Text tone="strong">{mapData.Author}</Text>
                        </div>
                        <div {...stylex.props(styles.metaItem)}>
                          <Text size="sm" tone="muted">
                            Size
                          </Text>
                          <Text tone="strong">
                            {mapData["Size X"]} × {mapData["Size Y"]}
                          </Text>
                        </div>
                      </div>
                    ) : null}
                    {shareUrl ? (
                      <Text size="sm" tone="muted">
                        Private Join Link: {shareUrl}
                      </Text>
                    ) : null}
                  </Stack>
                </Frame>
              </Stack>

              <Frame xstyle={styles.rosterFrame}>
                <Stack gap="md">
                  <Stack gap="xs">
                    <Kicker>Participants</Kicker>
                    <Heading size="lg">Seats</Heading>
                  </Stack>

                  {error ? (
                    <Text size="sm" tone="danger">
                      {error}
                    </Text>
                  ) : null}
                  {!session ? (
                    <Text size="sm" tone="muted">
                      Sign in to claim a seat in the lobby.
                    </Text>
                  ) : null}
                  {match.phase === "starting" ? (
                    <Text size="sm" tone="muted">
                      All players are ready. Starting the match...
                    </Text>
                  ) : null}
                  {match.phase === "active" ? (
                    <Text size="sm" tone="muted">
                      The match is active. Lobby controls are locked.
                    </Text>
                  ) : null}

                  <div {...stylex.props(styles.participantList)}>
                    {Array.from({ length: match.maxPlayers }, (_, slotIndex) => {
                      const participant = participantsBySlot.get(slotIndex) ?? null;
                      const isMine = participant?.userId === currentUserId;
                      const fallbackFactionId =
                        participant?.factionId ?? defaultFactionIdForSlot(slotIndex);
                      const faction = getFactionById(fallbackFactionId);
                      const factionVisual = getFactionVisual(faction?.code ?? "os");
                      return (
                        <div
                          key={slotIndex}
                          {...stylex.props(
                            styles.participantCard(factionVisual.wash, factionVisual.accent),
                          )}
                        >
                          <div {...stylex.props(styles.participantHeader)}>
                            <div>
                              <Text size="sm" tone="muted" xstyle={styles.slotLabel}>
                                Slot {slotIndex + 1}
                              </Text>
                              <Text
                                as="h3"
                                size="lg"
                                tone="strong"
                                xstyle={styles.participantName(factionVisual.text)}
                              >
                                {participant ? participant.userName : "Open Seat"}
                              </Text>
                            </div>
                            {participant ? (
                              <Badge tone={participant.ready ? "success" : "neutral"}>
                                {participant.ready ? "Ready" : "Waiting"}
                              </Badge>
                            ) : null}
                          </div>

                          <div {...stylex.props(styles.participantPortraitRow)}>
                            <CoPortrait
                              catalog={portraitCatalog}
                              coKey={
                                coOptions.find((portrait) => portrait.awbwId === participant?.coId)
                                  ?.key ?? null
                              }
                              fallbackLabel="?"
                            />
                            <div {...stylex.props(styles.participantDetails)}>
                              <Text size="sm">{faction?.displayName ?? "Unknown Faction"}</Text>
                              <Text size="sm" tone="muted">
                                {participant?.coId
                                  ? (coOptions.find(
                                      (portrait) => portrait.awbwId === participant.coId,
                                    )?.displayName ?? `CO ${participant.coId}`)
                                  : "No CO selected"}
                              </Text>
                            </div>
                          </div>

                          {participant === null ? (
                            <Button
                              disabled={
                                pendingAction !== null ||
                                !session ||
                                myParticipant !== null ||
                                match.phase !== "lobby"
                              }
                              onClick={() => {
                                void submitAction(
                                  {
                                    action: "join",
                                    slotIndex,
                                    factionId: defaultFactionIdForSlot(slotIndex),
                                    joinSlug,
                                  },
                                  `join-${slotIndex}`,
                                );
                              }}
                              tone="brand"
                              type="button"
                            >
                              Claim Slot
                            </Button>
                          ) : isMine ? (
                            <Stack gap="md">
                              <SelectField
                                disabled={pendingAction !== null || match.phase !== "lobby"}
                                label="Faction"
                                onChange={(event) => {
                                  void submitAction(
                                    {
                                      action: "updateParticipant",
                                      factionId: Number(event.target.value),
                                      joinSlug,
                                    },
                                    "faction",
                                  );
                                }}
                                options={factions.map((option) => ({
                                  value: option.id,
                                  label: option.displayName,
                                }))}
                                value={participant.factionId}
                              />

                              <CoPickerField
                                disabled={pendingAction !== null || match.phase !== "lobby"}
                                label="CO"
                                onChange={(nextValue) => {
                                  void submitAction(
                                    {
                                      action: "updateParticipant",
                                      coId: nextValue,
                                      joinSlug,
                                    },
                                    "co",
                                  );
                                }}
                                options={coOptions}
                                value={participant.coId ?? null}
                              />

                              <div {...stylex.props(styles.actions)}>
                                <Button
                                  disabled={pendingAction !== null || match.phase !== "lobby"}
                                  onClick={() => {
                                    void submitAction(
                                      {
                                        action: "updateParticipant",
                                        ready: !participant.ready,
                                        joinSlug,
                                      },
                                      "ready",
                                    );
                                  }}
                                  tone="success"
                                  type="button"
                                >
                                  {participant.ready ? "Unready" : "Ready Up"}
                                </Button>
                                <Button
                                  disabled={pendingAction !== null || match.phase !== "lobby"}
                                  onClick={() => {
                                    void submitAction({ action: "leave" }, "leave");
                                  }}
                                  tone="neutral"
                                  type="button"
                                  variant="outline"
                                >
                                  Leave
                                </Button>
                              </div>
                            </Stack>
                          ) : (
                            <Text size="sm" tone="muted">
                              Seat claimed.
                            </Text>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </Stack>
              </Frame>
            </div>
          )}
        </div>
      </Section>
    </Page>
  );
}

const styles = stylex.create({
  page: {
    width: "100%",
  },
  layout: {
    display: "grid",
    gap: tokens.space8,
    gridTemplateColumns: {
      default: "minmax(0, 1.1fr) minmax(360px, 0.9fr)",
      "@media (max-width: 860px)": "1fr",
    },
    alignItems: "start",
  },
  badges: {
    display: "flex",
    flexWrap: "wrap",
    gap: tokens.space2,
  },
  summaryFrame: {
    backgroundColor: tokens.panelRaised,
    backgroundImage:
      "linear-gradient(180deg, rgba(255,255,255,0.28), transparent 38%), linear-gradient(135deg, rgba(231, 100, 38, 0.12), transparent 55%)",
  },
  summaryKicker: {
    color: tokens.brandHover,
  },
  previewFrame: {
    backgroundImage:
      "linear-gradient(180deg, rgba(47, 109, 168, 0.16), transparent 42%), linear-gradient(135deg, rgba(29, 37, 50, 0.08), transparent 45%)",
  },
  previewCanvas: {
    width: "100%",
  },
  rosterFrame: {
    backgroundImage:
      "linear-gradient(180deg, rgba(47, 142, 69, 0.16), transparent 42%), linear-gradient(135deg, rgba(29, 37, 50, 0.08), transparent 45%)",
  },
  metaGrid: {
    display: "grid",
    gap: tokens.space3,
    gridTemplateColumns: {
      default: "repeat(3, minmax(0, 1fr))",
      "@media (max-width: 640px)": "1fr",
    },
  },
  metaItem: {
    display: "grid",
    gap: 4,
    padding: tokens.space3,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    backgroundColor: tokens.panelRaised,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
  },
  participantList: {
    display: "grid",
    gap: tokens.space3,
  },
  participantCard: (wash: string, accent: string) => ({
    display: "grid",
    gap: tokens.space3,
    padding: tokens.space4,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius3,
    backgroundImage: `linear-gradient(180deg, ${wash}, ${tokens.panelRaised})`,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}, 0 0 0 2px ${accent}22`,
  }),
  participantHeader: {
    display: "flex",
    gap: tokens.space3,
    justifyContent: "space-between",
    alignItems: "start",
  },
  slotLabel: {
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: "0.08em",
    textTransform: "uppercase",
  },
  participantName: (color: string) => ({
    fontWeight: 800,
    color,
  }),
  participantPortraitRow: {
    display: "flex",
    alignItems: "center",
    gap: tokens.space3,
  },
  participantDetails: {
    display: "grid",
    gap: 4,
  },
  actions: {
    display: "flex",
    gap: tokens.space2,
    flexWrap: "wrap",
  },
});
