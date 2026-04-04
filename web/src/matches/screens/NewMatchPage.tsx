import { Link, useNavigate } from "@tanstack/react-router";
import * as stylex from "@stylexjs/stylex";
import { startTransition, useEffect, useMemo, useRef, useState } from "react";
import { useAppSession } from "../../auth/useAppSession";
import { awbwMapAssetPath } from "../../awbw/paths";
import type { AwbwMapData } from "../../awbw/schemas";
import {
  Button,
  CheckboxField,
  Frame,
  Heading,
  Inline,
  Kicker,
  Page,
  Section,
  Stack,
  Text,
  TextField,
} from "../../ui/primitives";
import { tokens } from "../../ui/theme.stylex";
import { MatchMapPreview } from "../components/MatchMapPreview";
import { createMatchFn } from "../matches.functions";

export function NewMatchPage() {
  const navigate = useNavigate();
  const session = useAppSession();
  const [matchName, setMatchName] = useState("");
  const [mapIdInput, setMapIdInput] = useState("162795");
  const [mapData, setMapData] = useState<AwbwMapData | null>(null);
  const [fogEnabled, setFogEnabled] = useState(false);
  const [startingFunds, setStartingFunds] = useState("1000");
  const [isPrivate, setIsPrivate] = useState(false);
  const [isLoadingMap, setIsLoadingMap] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [mapError, setMapError] = useState<string | null>(null);
  const [createError, setCreateError] = useState<string | null>(null);
  const mapRequestRef = useRef<AbortController | null>(null);
  const [lastAutoMatchName, setLastAutoMatchName] = useState<string | null>(null);
  const lastAutoMatchNameRef = useRef<string | null>(null);

  const parsedMapId = useMemo(() => {
    const value = Number(mapIdInput);
    return Number.isSafeInteger(value) && value > 0 ? value : null;
  }, [mapIdInput]);

  const parsedStartingFunds = useMemo(() => {
    const value = Number(startingFunds);
    return Number.isSafeInteger(value) && value >= 0 ? value : null;
  }, [startingFunds]);

  useEffect(() => {
    return () => {
      mapRequestRef.current?.abort();
    };
  }, []);

  useEffect(() => {
    lastAutoMatchNameRef.current = lastAutoMatchName;
  }, [lastAutoMatchName]);

  async function handleLoadMap(): Promise<void> {
    mapRequestRef.current?.abort();

    if (parsedMapId === null) {
      mapRequestRef.current = null;
      setMapData(null);
      setMapError("Enter a valid AWBW map id.");
      setIsLoadingMap(false);
      return;
    }

    const controller = new AbortController();
    mapRequestRef.current = controller;
    setIsLoadingMap(true);
    setMapError(null);

    try {
      const response = await fetch(awbwMapAssetPath(parsedMapId), { signal: controller.signal });
      if (!response.ok) {
        throw new Error(response.status === 404 ? "Map not found." : "Map preview failed to load.");
      }

      const nextMap = (await response.json()) as AwbwMapData;
      if (controller.signal.aborted || mapRequestRef.current !== controller) {
        return;
      }

      startTransition(() => {
        setMapData(nextMap);
        setMapError(null);
        setMatchName((previous) => {
          const previousAutoName = lastAutoMatchNameRef.current;
          const shouldAutoFill = !previous.trim() || previous === previousAutoName;

          if (!shouldAutoFill) {
            return previous;
          }

          setLastAutoMatchName(nextMap.Name);
          return nextMap.Name;
        });
      });
    } catch (error) {
      if (controller.signal.aborted || mapRequestRef.current !== controller) {
        return;
      }

      startTransition(() => {
        setMapData(null);
        setMapError(error instanceof Error ? error.message : "Map preview failed to load.");
      });
    } finally {
      if (mapRequestRef.current === controller) {
        mapRequestRef.current = null;
        setIsLoadingMap(false);
      }
    }
  }

  async function handleCreateLobby(): Promise<void> {
    if (!session) {
      setCreateError("Sign in to create a match.");
      return;
    }
    if (parsedMapId === null || mapData === null) {
      setCreateError("Load a map before creating the lobby.");
      return;
    }
    if (parsedStartingFunds === null) {
      setCreateError("Starting funds must be a non-negative whole number.");
      return;
    }
    if (!matchName.trim()) {
      setCreateError("Match name is required.");
      return;
    }

    setIsCreating(true);
    setCreateError(null);

    try {
      const match = await createMatchFn({
        data: {
          name: matchName.trim(),
          mapId: parsedMapId,
          isPrivate,
          settings: { fogEnabled, startingFunds: parsedStartingFunds },
        },
      });

      await navigate({ to: "/matches/$matchId", params: { matchId: match.matchId } });
    } catch (error) {
      setCreateError(error instanceof Error ? error.message : "Failed to create the lobby.");
    } finally {
      setIsCreating(false);
    }
  }

  return (
    <Page width="wide">
      <Section>
        <div {...stylex.props(styles.layout)}>
          <Frame xstyle={styles.setupFrame}>
            <Stack gap="lg">
              <Stack gap="sm" xstyle={styles.setupHeader}>
                <Kicker xstyle={styles.setupKicker}>Match Setup</Kicker>
                <Heading size="display">Create Match</Heading>
                <Text size="lg" tone="strong" xstyle={styles.lead}>
                  Load an AWBW map, inspect the battlefield, and dial in the starting rules before
                  the lobby goes live.
                </Text>
              </Stack>

              <Stack gap="md">
                <TextField
                  label="Match Name"
                  onChange={(event) => {
                    setMatchName(event.target.value);
                    setLastAutoMatchName(null);
                  }}
                  placeholder="Riverside Duel"
                  type="text"
                  value={matchName}
                />

                <div {...stylex.props(styles.dualRow)}>
                  <TextField
                    label="AWBW Map ID"
                    onChange={(event) => {
                      mapRequestRef.current?.abort();
                      mapRequestRef.current = null;
                      setIsLoadingMap(false);
                      setMapIdInput(event.target.value);
                      setMapData(null);
                      setMapError(null);
                    }}
                    type="text"
                    inputMode="numeric"
                    value={mapIdInput}
                  />
                  <Button
                    disabled={isLoadingMap}
                    tone="success"
                    xstyle={styles.actionButton}
                    onClick={() => {
                      void handleLoadMap();
                    }}
                    type="button"
                  >
                    {isLoadingMap ? "Loading..." : "Load Map"}
                  </Button>
                </div>

                <div {...stylex.props(styles.settingsRow)}>
                  <TextField
                    label="Starting Funds"
                    onChange={(event) => setStartingFunds(event.target.value)}
                    type="text"
                    inputMode="numeric"
                    value={startingFunds}
                  />
                  <Stack gap="sm" xstyle={styles.checkboxGroup}>
                    <CheckboxField
                      checked={fogEnabled}
                      label="Fog Enabled"
                      onChange={setFogEnabled}
                    />
                    <CheckboxField
                      checked={isPrivate}
                      label="Private Match"
                      onChange={setIsPrivate}
                    />
                  </Stack>
                </div>

                <div {...stylex.props(styles.setupFooter)}>
                  {!session ? (
                    <Text size="sm" tone="strong">
                      <Link to="/auth" search={{}}>
                        Sign in
                      </Link>{" "}
                      to create a lobby.
                    </Text>
                  ) : (
                    <Text size="sm" tone="strong">
                      Lobby creator: {session.user.name}
                    </Text>
                  )}

                  {mapError ? (
                    <Text aria-live="polite" role="status" size="sm" tone="danger">
                      {mapError}
                    </Text>
                  ) : null}
                  {createError ? (
                    <Text aria-live="polite" role="status" size="sm" tone="danger">
                      {createError}
                    </Text>
                  ) : null}

                  <Inline gap="sm">
                    <Button
                      disabled={isCreating || mapData === null || !session}
                      tone="brand"
                      xstyle={styles.primaryAction}
                      onClick={() => {
                        void handleCreateLobby();
                      }}
                      type="button"
                    >
                      {isCreating ? "Creating Lobby..." : "Create Lobby"}
                    </Button>
                  </Inline>
                </div>
              </Stack>
            </Stack>
          </Frame>

          <Frame xstyle={styles.previewFrame}>
            <Stack gap="md">
              <Stack gap="xs">
                <Kicker>Battlefield</Kicker>
                <Heading size="lg">Map Preview</Heading>
                <Text size="sm">
                  {mapData
                    ? `${mapData.Name} · ${mapData.Author}`
                    : "Load a map to inspect its terrain."}
                </Text>
              </Stack>
              {mapData && parsedMapId !== null ? (
                <Stack gap="md">
                  <MatchMapPreview mapId={parsedMapId} xstyle={styles.previewCanvas} />
                  <div {...stylex.props(styles.metaGrid)}>
                    <div {...stylex.props(styles.metaItem)}>
                      <Text size="sm" tone="muted">
                        Players
                      </Text>
                      <Text tone="strong">{mapData["Player Count"]}</Text>
                    </div>
                    <div {...stylex.props(styles.metaItem)}>
                      <Text size="sm" tone="muted">
                        Size
                      </Text>
                      <Text tone="strong">
                        {mapData["Size X"]} × {mapData["Size Y"]}
                      </Text>
                    </div>
                    <div {...stylex.props(styles.metaItem)}>
                      <Text size="sm" tone="muted">
                        Published
                      </Text>
                      <Text tone="strong">{mapData["Published Date"]}</Text>
                    </div>
                  </div>
                </Stack>
              ) : (
                <Text size="lg" tone="muted" xstyle={styles.emptyPreview}>
                  No map loaded.
                </Text>
              )}
            </Stack>
          </Frame>
        </div>
      </Section>
    </Page>
  );
}

const styles = stylex.create({
  layout: {
    display: "grid",
    gap: tokens.space8,
    gridTemplateColumns: {
      default: "minmax(320px, 480px) minmax(0, 1fr)",
      "@media (max-width: 860px)": "1fr",
    },
    alignItems: "start",
  },
  lead: {
    maxWidth: 540,
  },
  setupFrame: {
    backgroundColor: tokens.panelRaised,
    backgroundImage:
      "linear-gradient(180deg, rgba(255, 255, 255, 0.3), rgba(255, 255, 255, 0) 34%), linear-gradient(135deg, rgba(231, 100, 38, 0.12), rgba(0, 0, 0, 0) 54%)",
  },
  setupHeader: {
    paddingBottom: tokens.space4,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "rgba(61, 45, 26, 0.16)",
  },
  setupKicker: {
    color: tokens.brandHover,
  },
  dualRow: {
    display: "grid",
    gap: tokens.space3,
    gridTemplateColumns: {
      default: "minmax(0, 1fr) auto",
      "@media (max-width: 640px)": "1fr",
    },
    alignItems: "end",
  },
  settingsRow: {
    display: "grid",
    gap: tokens.space4,
    gridTemplateColumns: {
      default: "minmax(0, 1fr) minmax(220px, 280px)",
      "@media (max-width: 860px)": "1fr",
    },
    alignItems: "start",
  },
  checkboxGroup: {
    paddingTop: tokens.space6,
  },
  setupFooter: {
    display: "grid",
    gap: tokens.space3,
    paddingTop: tokens.space4,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "rgba(61, 45, 26, 0.16)",
  },
  actionButton: {
    minWidth: 132,
  },
  primaryAction: {
    minWidth: 180,
  },
  previewFrame: {
    backgroundImage:
      "linear-gradient(180deg, rgba(47, 109, 168, 0.16), transparent 42%), linear-gradient(135deg, rgba(29, 37, 50, 0.08), transparent 45%)",
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
    padding: tokens.space3,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    backgroundColor: tokens.panelRaised,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
  },
  emptyPreview: {
    minHeight: 320,
    display: "flex",
    alignItems: "center",
  },
});
