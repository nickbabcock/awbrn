import { Popover } from "@base-ui/react/popover";
import { ScrollArea } from "@base-ui/react/scroll-area";
import * as stylex from "@stylexjs/stylex";
import { startTransition, useEffect, useMemo, useRef, useState } from "react";
import { Cancel as CancelIcon } from "pixelarticons/react/Cancel";
import { Check as CheckIcon } from "pixelarticons/react/Check";
import { Logout as LogoutIcon } from "pixelarticons/react/Logout";
import { Plus as PlusIcon } from "pixelarticons/react/Plus";
import { useAppSession } from "#/auth/useAppSession.ts";
import { awbwMapAssetPath } from "#/awbw/paths.ts";
import type { AwbwMapData } from "#/awbw/schemas.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import {
  DEFAULT_CO_PORTRAIT_KEY,
  getCoPortraitByAwbwId,
  listCoPortraits,
  loadCoPortraitCatalog,
  type CoPortraitCatalog,
} from "#/components/co_portraits.ts";
import {
  defaultFactionIdForSlot,
  factions,
  getFactionById,
  type FactionCatalogEntry,
} from "#/factions.ts";
import { getFactionVisual } from "#/faction_visuals.ts";
import { FactionBadge, PlayerHeader } from "#/components/PlayerHeader.tsx";
import { Button, Frame, Heading, Kicker, Notice, Page, Section, Text } from "#/ui/primitives.tsx";
import { tokens } from "#/ui/theme.stylex.ts";
import { MatchMapPreview } from "#/matches/components/MatchMapPreview.tsx";
import { getMatchFn, mutateMatchFn } from "#/matches/matches.functions.ts";
import type { MatchMutationRequest, MatchSnapshot } from "#/matches/schemas.ts";

const coOptions = listCoPortraits();
const selectableCoOptions = coOptions.filter((option) => option.key !== DEFAULT_CO_PORTRAIT_KEY);
const factionHelperCopy =
  "Faction sets your army's look only. Starting position still comes from the map.";
const rowReveal = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(10px)" },
  to: { opacity: 1, transform: "translateY(0)" },
});
const popIn = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(8px) scale(0.98)" },
  to: { opacity: 1, transform: "translateY(0) scale(1)" },
});

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
  const [pageError, setPageError] = useState<string | null>(null);
  const [mapError, setMapError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
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
      setMapError(null);
      return;
    }

    let cancelled = false;
    setMapData(null);
    setMapError(null);
    void (async () => {
      try {
        const response = await fetch(awbwMapAssetPath(match.mapId));
        if (!response.ok) {
          throw new Error("Map metadata could not be loaded.");
        }
        const payload = (await response.json()) as AwbwMapData;
        if (!cancelled) {
          startTransition(() => {
            setMapData(payload);
          });
        }
      } catch (nextError) {
        if (!cancelled) {
          startTransition(() => {
            setMapData(null);
          });
          setMapError(
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
    setPageError(null);
    setActionError(null);
    startTransition(() => {
      setMatch(null);
    });

    try {
      const snapshot = await getMatchFn({ data: { matchId, joinSlug } });
      if (snapshotRequestRef.current !== requestId || requestKeyRef.current !== requestKey) {
        return;
      }
      startTransition(() => {
        setMatch(snapshot);
      });
    } catch (nextError) {
      if (snapshotRequestRef.current !== requestId || requestKeyRef.current !== requestKey) {
        return;
      }
      startTransition(() => {
        setMatch(null);
      });
      setPageError(nextError instanceof Error ? nextError.message : "Failed to load the lobby.");
    } finally {
      if (snapshotRequestRef.current === requestId && requestKeyRef.current === requestKey) {
        setIsLoading(false);
      }
    }
  }

  async function submitAction(action: MatchMutationRequest, pendingLabel: string): Promise<void> {
    const requestKey = requestKeyRef.current;
    setPendingAction(pendingLabel);
    setActionError(null);

    // Optimistically apply participant mutations so the UI updates instantly
    let prevMatch: MatchSnapshot | null = null;
    if (action.action === "updateParticipant" && match !== null && currentUserId !== null) {
      prevMatch = match;
      startTransition(() => {
        setMatch({
          ...match,
          participants: match.participants.map((p) =>
            p.userId !== currentUserId
              ? p
              : {
                  ...p,
                  ...(action.coId !== undefined ? { coId: action.coId } : {}),
                  ...(action.factionId !== undefined ? { factionId: action.factionId } : {}),
                  ...(action.ready !== undefined ? { ready: action.ready } : {}),
                },
          ),
        });
      });
    }

    try {
      const response = await mutateMatchFn({ data: { matchId, action } });
      if (requestKeyRef.current !== requestKey) {
        return;
      }
      startTransition(() => {
        setMatch(response.match);
      });
    } catch (nextError) {
      if (requestKeyRef.current !== requestKey) {
        return;
      }
      if (prevMatch !== null) {
        startTransition(() => {
          setMatch(prevMatch);
        });
      }
      setActionError(nextError instanceof Error ? nextError.message : "Lobby update failed.");
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
  const phaseLabel = formatPhaseLabel(match?.phase ?? null);

  return (
    <Page width="wide">
      <Section>
        <div {...stylex.props(styles.page)}>
          {isLoading ? (
            <Text size="lg" tone="muted">
              Loading lobby...
            </Text>
          ) : !match && pageError ? (
            <Text size="lg" tone="danger">
              {pageError}
            </Text>
          ) : !match ? (
            <Text size="lg" tone="muted">
              Match not found.
            </Text>
          ) : (
            <div {...stylex.props(styles.layout)}>
              <header {...stylex.props(styles.header)}>
                <div {...stylex.props(styles.headerCopy)}>
                  <Kicker xstyle={styles.headerKicker}>{phaseLabel}</Kicker>
                  <Heading size="display">{match.name}</Heading>
                  <Text size="lg" tone="strong">
                    Map {match.mapId} · {match.maxPlayers} players ·{" "}
                    {match.isPrivate ? "Private invite" : "Open lobby"}
                  </Text>
                </div>
                <div {...stylex.props(styles.headerFacts)}>
                  <div {...stylex.props(styles.headerFact)}>
                    <Text size="sm" tone="muted" xstyle={styles.factLabel}>
                      Creator
                    </Text>
                    <Text tone="strong">{match.creatorName}</Text>
                  </div>
                  <div {...stylex.props(styles.headerFact)}>
                    <Text size="sm" tone="muted" xstyle={styles.factLabel}>
                      Match Rules
                    </Text>
                    <Text tone="strong">
                      {match.settings.fogEnabled ? "Fog on" : "Fog off"} ·{" "}
                      {match.settings.startingFunds.toLocaleString()} funds
                    </Text>
                  </div>
                  {shareUrl ? (
                    <div {...stylex.props(styles.headerFact, styles.headerFactWide)}>
                      <Text size="sm" tone="muted" xstyle={styles.factLabel}>
                        Private Join Link
                      </Text>
                      <Text size="sm" tone="strong" xstyle={styles.shareLink}>
                        {shareUrl}
                      </Text>
                    </div>
                  ) : null}
                </div>
              </header>

              <div {...stylex.props(styles.mainGrid)}>
                <Frame as="section" surface="panel" padding="none" xstyle={styles.mapSection}>
                  <div {...stylex.props(styles.mapSectionInner)}>
                    <div {...stylex.props(styles.sectionHeader)}>
                      <Kicker>Map</Kicker>
                      <Heading size="lg">{mapData?.Name ?? `Map ${match.mapId}`}</Heading>
                      <Text size="sm" tone="muted">
                        {mapData
                          ? `${mapData.Author} · ${mapData["Size X"]} × ${mapData["Size Y"]}`
                          : "Preview the terrain before everyone locks in."}
                      </Text>
                    </div>

                    <div {...stylex.props(styles.mapPreviewWrap)}>
                      <MatchMapPreview mapId={match.mapId} xstyle={styles.previewCanvas} />
                    </div>

                    <div {...stylex.props(styles.metaGrid)}>
                      <div {...stylex.props(styles.metaItem)}>
                        <Text size="sm" tone="muted" xstyle={styles.factLabel}>
                          Layout
                        </Text>
                        <Text tone="strong">
                          {mapData
                            ? `${mapData["Size X"]} × ${mapData["Size Y"]}`
                            : `${match.maxPlayers} player map`}
                        </Text>
                      </div>
                      <div {...stylex.props(styles.metaItem)}>
                        <Text size="sm" tone="muted" xstyle={styles.factLabel}>
                          Visibility
                        </Text>
                        <Text tone="strong">
                          {match.settings.fogEnabled ? "Fog enabled" : "Clear vision"}
                        </Text>
                      </div>
                      <div {...stylex.props(styles.metaItem)}>
                        <Text size="sm" tone="muted" xstyle={styles.factLabel}>
                          Economy
                        </Text>
                        <Text tone="strong">
                          {match.settings.startingFunds.toLocaleString()} starting funds
                        </Text>
                      </div>
                    </div>

                    {mapError ? (
                      <Text size="sm" tone="danger">
                        {mapError}
                      </Text>
                    ) : null}
                  </div>
                </Frame>

                <Frame as="section" surface="panel" padding="none" xstyle={styles.rosterSection}>
                  <div {...stylex.props(styles.rosterSectionInner)}>
                    <div {...stylex.props(styles.sectionHeader)}>
                      <Kicker>Roster</Kicker>
                      <Heading size="lg">Choose CO and army look</Heading>
                    </div>

                    <div {...stylex.props(styles.statusStack)}>
                      {actionError ? <Notice tone="danger">{actionError}</Notice> : null}
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
                    </div>

                    <div {...stylex.props(styles.participantList)}>
                      {Array.from({ length: match.maxPlayers }, (_, slotIndex) => {
                        const participant = participantsBySlot.get(slotIndex) ?? null;
                        const isMine = participant?.userId === currentUserId;
                        const fallbackFactionId =
                          participant?.factionId ?? defaultFactionIdForSlot(slotIndex);
                        const faction = getFactionById(fallbackFactionId);
                        const factionVisual = getFactionVisual(faction?.code ?? "os");
                        const isInteractive = isMine && match.phase === "lobby";
                        const isLocked = pendingAction !== null || match.phase !== "lobby";

                        return (
                          <div
                            key={slotIndex}
                            style={{ animationDelay: `${slotIndex * 45}ms` }}
                            {...stylex.props(
                              styles.participantCard(factionVisual.accent),
                              participant === null && styles.participantCardOpen,
                            )}
                          >
                            <PlayerHeader
                              factionCode={faction?.code ?? "os"}
                              name={participant ? participant.userName : "Open Seat"}
                              trailing={
                                <>
                                  {participant !== null ? (
                                    <FactionSelectionControl
                                      disabled={isLocked}
                                      faction={faction}
                                      interactive={isInteractive}
                                      onDark
                                      onChange={(nextValue) => {
                                        void submitAction(
                                          {
                                            action: "updateParticipant",
                                            factionId: nextValue,
                                            joinSlug,
                                          },
                                          "faction",
                                        );
                                      }}
                                    />
                                  ) : null}
                                  {participant === null ? (
                                    <Button
                                      disabled={isLocked || !session || myParticipant !== null}
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
                                      size="sm"
                                      tone="brand"
                                      type="button"
                                    >
                                      <PlusIcon width={14} height={14} aria-hidden />
                                      Claim
                                    </Button>
                                  ) : isMine ? (
                                    <>
                                      <Button
                                        aria-label={participant.ready ? "Unready" : "Ready up"}
                                        disabled={isLocked}
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
                                        size="sm"
                                        tone="success"
                                        type="button"
                                        variant={participant.ready ? "outline" : "solid"}
                                        xstyle={styles.iconButton}
                                      >
                                        {participant.ready ? (
                                          <CancelIcon width={14} height={14} aria-hidden />
                                        ) : (
                                          <CheckIcon width={14} height={14} aria-hidden />
                                        )}
                                      </Button>
                                      <Button
                                        aria-label="Leave lobby"
                                        disabled={isLocked}
                                        onClick={() => {
                                          void submitAction({ action: "leave" }, "leave");
                                        }}
                                        size="sm"
                                        tone="neutral"
                                        type="button"
                                        variant="outline"
                                        xstyle={styles.iconButton}
                                      >
                                        <LogoutIcon width={14} height={14} aria-hidden />
                                      </Button>
                                    </>
                                  ) : null}
                                </>
                              }
                            />
                            {participant !== null ? (
                              <div {...stylex.props(styles.participantBody)}>
                                <CoSelectionControl
                                  catalog={portraitCatalog}
                                  coId={participant.coId}
                                  disabled={isLocked}
                                  interactive={isInteractive}
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
                                />
                                <Text
                                  size="sm"
                                  tone={participant.ready ? "success" : "muted"}
                                  xstyle={styles.participantState}
                                >
                                  {participant.ready
                                    ? "Ready"
                                    : match.phase === "active"
                                      ? "In match"
                                      : "Waiting"}
                                </Text>
                              </div>
                            ) : null}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                </Frame>
              </div>
            </div>
          )}
        </div>
      </Section>
    </Page>
  );
}

function CoSelectionControl({
  catalog,
  coId,
  disabled,
  interactive,
  onChange,
}: {
  catalog: CoPortraitCatalog;
  coId: number | null;
  disabled: boolean;
  interactive: boolean;
  onChange: (nextValue: number | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const selectedPortrait = getCoPortraitByAwbwId(coId);
  const title = selectedPortrait?.displayName ?? "No CO";

  const portrait = (
    <CoPortrait
      catalog={catalog}
      coKey={selectedPortrait?.key ?? DEFAULT_CO_PORTRAIT_KEY}
      fallbackLabel={title}
    />
  );

  if (!interactive) {
    return <div>{portrait}</div>;
  }

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger
        aria-label={`CO: ${title} — click to change`}
        disabled={disabled}
        {...stylex.props(styles.portraitButton, disabled && styles.portraitButtonDisabled)}
      >
        {portrait}
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner align="start" sideOffset={10}>
          <Popover.Popup initialFocus={false} {...stylex.props(styles.pickerPopup, styles.coPopup)}>
            <ScrollArea.Root>
              <ScrollArea.Viewport {...stylex.props(styles.pickerViewport)}>
                <ScrollArea.Content>
                  <div {...stylex.props(styles.coGrid)}>
                    <button
                      onClick={() => {
                        onChange(null);
                        setOpen(false);
                      }}
                      type="button"
                      {...stylex.props(styles.coTile, coId === null && styles.coTileSelected)}
                    >
                      <CoPortrait
                        catalog={catalog}
                        coKey={DEFAULT_CO_PORTRAIT_KEY}
                        fallbackLabel="No CO"
                      />
                      <span {...stylex.props(styles.tileTitle)}>No CO</span>
                      <span {...stylex.props(styles.tileMeta)}>Clear selection</span>
                    </button>
                    {selectableCoOptions.map((option) => (
                      <button
                        key={option.awbwId}
                        onClick={() => {
                          onChange(option.awbwId);
                          setOpen(false);
                        }}
                        type="button"
                        {...stylex.props(
                          styles.coTile,
                          option.awbwId === coId && styles.coTileSelected,
                        )}
                      >
                        <CoPortrait
                          catalog={catalog}
                          coKey={option.key}
                          fallbackLabel={option.displayName}
                        />
                        <span {...stylex.props(styles.tileTitle)}>{option.displayName}</span>
                      </button>
                    ))}
                  </div>
                </ScrollArea.Content>
              </ScrollArea.Viewport>
            </ScrollArea.Root>
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}

function FactionSelectionControl({
  faction,
  disabled,
  interactive,
  onDark = false,
  onChange,
}: {
  faction: FactionCatalogEntry | null;
  disabled: boolean;
  interactive: boolean;
  onDark?: boolean;
  onChange: (nextValue: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const activeFaction = faction ?? factions[0] ?? null;
  const activeVisual = getFactionVisual(activeFaction?.code ?? "os");
  const title = activeFaction?.displayName ?? "Unknown Faction";
  const factionCode = activeFaction?.code ?? "os";

  const badge = <FactionLogo factionCode={factionCode} />;

  if (!interactive) {
    return onDark ? (
      <FactionBadge factionCode={factionCode} title={title} />
    ) : (
      <span
        aria-label={`Faction: ${title}`}
        title={title}
        {...stylex.props(styles.factionBadge(activeVisual.accentSoft, activeVisual.accent))}
      >
        {badge}
      </span>
    );
  }

  if (onDark) {
    return (
      <Popover.Root open={open} onOpenChange={setOpen}>
        <Popover.Trigger
          aria-label={`Faction: ${title} — click to change`}
          disabled={disabled}
          {...stylex.props(
            styles.factionBadgeDarkButton,
            disabled && styles.portraitButtonDisabled,
          )}
        >
          {badge}
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Positioner align="start" sideOffset={10}>
            <Popover.Popup
              initialFocus={false}
              {...stylex.props(styles.pickerPopup, styles.factionPopup)}
            >
              <ScrollArea.Root>
                <ScrollArea.Viewport {...stylex.props(styles.pickerViewport)}>
                  <ScrollArea.Content>
                    <div {...stylex.props(styles.factionPickerIntro)}>
                      <span {...stylex.props(styles.dropdownSubtitle)}>{factionHelperCopy}</span>
                    </div>
                    <div {...stylex.props(styles.factionGrid)}>
                      {factions.map((option) => {
                        const optionVisual = getFactionVisual(option.code);
                        return (
                          <button
                            key={option.id}
                            onClick={() => {
                              onChange(option.id);
                              setOpen(false);
                            }}
                            type="button"
                            {...stylex.props(
                              styles.factionTile(optionVisual.wash),
                              option.id === activeFaction?.id && styles.coTileSelected,
                            )}
                          >
                            <FactionLogo factionCode={option.code} />
                            <span {...stylex.props(styles.tileTitle)}>{option.displayName}</span>
                          </button>
                        );
                      })}
                    </div>
                  </ScrollArea.Content>
                </ScrollArea.Viewport>
              </ScrollArea.Root>
            </Popover.Popup>
          </Popover.Positioner>
        </Popover.Portal>
      </Popover.Root>
    );
  }

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger
        aria-label={`Faction: ${title} — click to change`}
        disabled={disabled}
        {...stylex.props(
          styles.factionBadge(activeVisual.accentSoft, activeVisual.accent),
          styles.factionBadgeButton,
          disabled && styles.portraitButtonDisabled,
        )}
      >
        {badge}
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner align="start" sideOffset={10}>
          <Popover.Popup
            initialFocus={false}
            {...stylex.props(styles.pickerPopup, styles.factionPopup)}
          >
            <ScrollArea.Root>
              <ScrollArea.Viewport {...stylex.props(styles.pickerViewport)}>
                <ScrollArea.Content>
                  <div {...stylex.props(styles.factionPickerIntro)}>
                    <span {...stylex.props(styles.dropdownSubtitle)}>{factionHelperCopy}</span>
                  </div>
                  <div {...stylex.props(styles.factionGrid)}>
                    {factions.map((option) => {
                      const optionVisual = getFactionVisual(option.code);
                      return (
                        <button
                          key={option.id}
                          onClick={() => {
                            onChange(option.id);
                            setOpen(false);
                          }}
                          type="button"
                          {...stylex.props(
                            styles.factionTile(optionVisual.wash),
                            option.id === activeFaction?.id && styles.coTileSelected,
                          )}
                        >
                          <FactionLogo factionCode={option.code} />
                          <span {...stylex.props(styles.tileTitle)}>{option.displayName}</span>
                        </button>
                      );
                    })}
                  </div>
                </ScrollArea.Content>
              </ScrollArea.Viewport>
            </ScrollArea.Root>
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}

function FactionLogo({ factionCode }: { factionCode: string }) {
  const visual = getFactionVisual(factionCode);

  return (
    <span aria-hidden="true" {...stylex.props(styles.factionLogoWrap)}>
      <span
        style={{
          backgroundImage: `url(${visual.logoUrl})`,
          backgroundPosition: visual.logoPosition,
        }}
        {...stylex.props(styles.factionLogo)}
      />
    </span>
  );
}

function formatPhaseLabel(phase: MatchSnapshot["phase"] | null): string {
  switch (phase) {
    case "active":
      return "Match Active";
    case "starting":
      return "Match Starting";
    case "completed":
      return "Match Complete";
    case "cancelled":
      return "Match Cancelled";
    case "draft":
      return "Draft";
    case "lobby":
      return "Lobby Setup";
    default:
      return "Lobby";
  }
}

const styles = stylex.create({
  page: {
    width: "100%",
  },
  layout: {
    display: "grid",
    gap: tokens.space6,
  },
  header: {
    display: "grid",
    gap: tokens.space5,
    gridTemplateColumns: {
      default: "minmax(0, 1.2fr) minmax(280px, 0.8fr)",
      "@media (max-width: 860px)": "1fr",
    },
    alignItems: "end",
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
  headerFacts: {
    display: "grid",
    gap: tokens.space3,
    alignContent: "start",
  },
  headerFact: {
    display: "grid",
    gap: 4,
  },
  headerFactWide: {
    minWidth: 0,
  },
  factLabel: {
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  shareLink: {
    wordBreak: "break-all",
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
  mapSection: {
    overflow: "visible",
  },
  mapSectionInner: {
    display: "grid",
    gap: tokens.space4,
    padding: tokens.space6,
  },
  rosterSection: {
    overflow: "visible",
  },
  rosterSectionInner: {
    display: "grid",
    gap: tokens.space4,
    alignContent: "start",
    padding: tokens.space6,
  },
  sectionHeader: {
    display: "grid",
    gap: tokens.space1,
  },
  mapPreviewWrap: {
    animationDuration: "240ms",
    animationFillMode: "both",
    animationName: rowReveal,
  },
  previewCanvas: {
    width: "100%",
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
    paddingTop: tokens.space3,
    borderTopWidth: 3,
    borderTopStyle: "solid",
    borderTopColor: tokens.strokeLight,
  },
  statusStack: {
    display: "grid",
    gap: tokens.space2,
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
    animationDuration: "220ms",
    animationFillMode: "both",
    animationName: rowReveal,
  }),
  participantCardOpen: {
    borderStyle: "dashed",
    borderColor: tokens.strokeBase,
  },
  participantBody: {
    display: "flex",
    alignItems: "center",
    gap: tokens.space3,
    padding: tokens.space3,
  },
  portraitButton: {
    display: "block",
    padding: tokens.space1,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    backgroundColor: "rgba(255, 255, 255, 0.2)",
    cursor: "pointer",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, border-color, background-color, opacity",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
      ":disabled": "translateY(0)",
    },
    boxShadow: {
      default: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
      ":hover": `${tokens.highlightInset}, ${tokens.shadowHardMd}`,
      ":active": tokens.highlightInset,
      ":disabled": `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    },
  },
  portraitButtonDisabled: {
    opacity: 0.55,
    cursor: "not-allowed",
  },
  participantState: {
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  factionBadge: (accentSoft: string, accent: string) => ({
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 24,
    height: 24,
    borderRadius: tokens.radius1,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: accent,
    backgroundColor: accentSoft,
  }),
  factionBadgeButton: {
    cursor: "pointer",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, opacity",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
      ":disabled": "translateY(0)",
    },
    boxShadow: {
      default: tokens.shadowHardSm,
      ":hover": tokens.shadowHardMd,
      ":active": "none",
    },
    opacity: {
      default: 1,
      ":disabled": 0.55,
    },
  },
  factionBadgeDarkButton: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 24,
    height: 24,
    borderRadius: tokens.radius1,
    backgroundColor: "rgba(255, 255, 255, 0.16)",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: "rgba(255, 255, 255, 0.24)",
    cursor: "pointer",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, opacity",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
      ":disabled": "translateY(0)",
    },
    opacity: {
      default: 1,
      ":disabled": 0.55,
    },
  },
  actions: {
    display: "flex",
    gap: tokens.space2,
    flexWrap: "wrap",
    justifyContent: "flex-end",
  },
  iconButton: {
    paddingInline: tokens.space2,
  },
  pickerPopup: {
    borderWidth: 3,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius3,
    backgroundColor: tokens.panelRaised,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardLg}`,
    padding: tokens.space3,
    animationDuration: "140ms",
    animationFillMode: "both",
    animationName: popIn,
  },
  coPopup: {
    width: "min(700px, calc(100vw - 32px))",
  },
  factionPopup: {
    width: "min(420px, calc(100vw - 32px))",
  },
  pickerViewport: {
    maxHeight: "min(420px, 60vh)",
  },
  coGrid: {
    display: "grid",
    gap: tokens.space2,
    gridTemplateColumns: {
      default: "repeat(auto-fill, minmax(120px, 1fr))",
      "@media (max-width: 640px)": "repeat(2, minmax(0, 1fr))",
    },
  },
  factionGrid: {
    display: "grid",
    gap: tokens.space1,
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
  },
  factionPickerIntro: {
    paddingBottom: tokens.space3,
  },
  coTile: {
    display: "grid",
    gap: tokens.space2,
    justifyItems: "start",
    alignContent: "start",
    padding: tokens.space2,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    backgroundColor: tokens.panelBg,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    cursor: "pointer",
    textAlign: "left",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, border-color, background-color",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
    },
  },
  coTileSelected: {
    borderColor: tokens.strokeHeavy,
    backgroundColor: tokens.brandSoft,
  },
  factionTile: (wash: string) => ({
    display: "flex",
    alignItems: "center",
    gap: tokens.space2,
    width: "100%",
    padding: tokens.space1,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    backgroundColor: wash,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    cursor: "pointer",
    textAlign: "left",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, border-color, background-color",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
    },
  }),
  tileTitle: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontBody,
    fontSize: tokens.textSm,
    fontWeight: 800,
  },
  tileMeta: {
    color: tokens.inkMuted,
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  dropdownSubtitle: {
    color: tokens.inkMuted,
    fontFamily: tokens.fontBody,
    fontSize: tokens.textSm,
    lineHeight: tokens.leadingBody,
  },
  factionLogoWrap: {
    flex: "0 0 auto",
    width: 14,
    height: 14,
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
  },
  factionLogo: {
    width: 14,
    height: 14,
    backgroundRepeat: "no-repeat",
    imageRendering: "pixelated",
  },
});
