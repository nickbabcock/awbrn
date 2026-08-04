import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Card } from "@astryxdesign/core/Card";
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Grid } from "@astryxdesign/core/Grid";
import { Heading } from "@astryxdesign/core/Heading";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { NumberInput } from "@astryxdesign/core/NumberInput";
import { Section } from "@astryxdesign/core/Section";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { startTransition, useEffect, useMemo, useRef, useState } from "react";
import { useAppSession } from "#/auth/useAppSession.ts";
import { AwbwMapDataQueryError, awbwMapDataQueryOptions } from "#/awbw/awbw.queries.ts";
import { usePreviewRunner } from "#/engine/runtime_context.tsx";
import { RouterTextLink } from "#/ui/astryx-links.tsx";
import { MatchMapPreview } from "#/matches/components/MatchMapPreview.tsx";
import { createMatchFn } from "#/matches/matches.functions.ts";
import { matchKeys } from "#/matches/matches.keys.ts";
import { TWO_COLUMN_GRID_MIN_WIDTH } from "#/ui/layout.ts";

export function NewMatchPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const session = useAppSession();
  const previewRunner = usePreviewRunner("matches-new");
  const [matchName, setMatchName] = useState("");
  const [mapIdInput, setMapIdInput] = useState("162795");
  const [loadedMapId, setLoadedMapId] = useState<number | null>(null);
  const [fogEnabled, setFogEnabled] = useState(false);
  const [startingFunds, setStartingFunds] = useState("1000");
  const [isPrivate, setIsPrivate] = useState(false);
  const [hotseatEnabled, setHotseatEnabled] = useState(false);
  const [loadingMapId, setLoadingMapId] = useState<number | null>(null);
  const [mapError, setMapError] = useState<string | null>(null);
  const [createError, setCreateError] = useState<string | null>(null);
  const mapLoadRequestRef = useRef(0);
  const matchNameRef = useRef("");
  const [lastAutoMatchName, setLastAutoMatchName] = useState<string | null>(null);
  const lastAutoMatchNameRef = useRef<string | null>(null);

  useEffect(() => {
    lastAutoMatchNameRef.current = lastAutoMatchName;
  }, [lastAutoMatchName]);

  const parsedMapId = useMemo(() => {
    const value = Number(mapIdInput);
    return Number.isSafeInteger(value) && value > 0 ? value : null;
  }, [mapIdInput]);

  const parsedStartingFunds = useMemo(() => {
    const value = Number(startingFunds);
    return Number.isSafeInteger(value) && value >= 0 ? value : null;
  }, [startingFunds]);

  const mapQuery = useQuery({
    ...awbwMapDataQueryOptions(loadedMapId ?? 0),
    enabled: loadedMapId !== null,
  });
  const mapData = loadedMapId === null ? null : (mapQuery.data ?? null);
  const isLoadingMap = loadingMapId !== null;

  const createMatchMutation = useMutation({
    mutationFn: createMatchFn,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: matchKeys.browse() }),
        queryClient.invalidateQueries({ queryKey: matchKeys.mine() }),
      ]);
    },
  });

  function handleMapIdChange(value: number): void {
    mapLoadRequestRef.current += 1;
    setLoadingMapId(null);
    setMapIdInput(String(value));
    setLoadedMapId(null);
    setMapError(null);
  }

  async function handleLoadMap(): Promise<void> {
    const requestId = mapLoadRequestRef.current + 1;
    mapLoadRequestRef.current = requestId;

    if (parsedMapId === null) {
      setLoadedMapId(null);
      setMapError("Enter a valid AWBW map id.");
      setLoadingMapId(null);
      return;
    }

    setLoadingMapId(parsedMapId);
    setMapError(null);

    try {
      const nextMap = await queryClient.fetchQuery(awbwMapDataQueryOptions(parsedMapId));
      if (mapLoadRequestRef.current !== requestId) return;
      const currentMatchName = matchNameRef.current;
      const shouldAssignAutoName =
        !currentMatchName.trim() || currentMatchName === lastAutoMatchNameRef.current;

      startTransition(() => {
        setLoadedMapId(parsedMapId);
        setMapError(null);
        if (shouldAssignAutoName) {
          matchNameRef.current = nextMap.Name;
          setMatchName(nextMap.Name);
          lastAutoMatchNameRef.current = nextMap.Name;
          setLastAutoMatchName(nextMap.Name);
        }
      });
    } catch (error) {
      if (mapLoadRequestRef.current !== requestId) return;
      startTransition(() => {
        setLoadedMapId(null);
        setMapError(formatMapPreviewError(error));
      });
    } finally {
      if (mapLoadRequestRef.current === requestId) setLoadingMapId(null);
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

    setCreateError(null);
    try {
      const match = await createMatchMutation.mutateAsync({
        data: {
          name: matchName.trim(),
          mapId: parsedMapId,
          isPrivate,
          settings: { fogEnabled, startingFunds: parsedStartingFunds, hotseatEnabled },
        },
      });
      await navigate({ to: "/matches/$matchId", params: { matchId: match.matchId } });
    } catch (error) {
      setCreateError(error instanceof Error ? error.message : "Failed to create the lobby.");
    }
  }

  return (
    <Section padding={6} variant="transparent">
      <Grid
        align="start"
        columns={{ minWidth: TWO_COLUMN_GRID_MIN_WIDTH, max: 2, repeat: "fit" }}
        gap={8}
      >
        <Card padding={6} width="100%">
          <VStack gap={6}>
            <VStack gap={2}>
              <Heading level={1} type="display-2">
                Create match
              </Heading>
              <Text color="secondary" type="large">
                Load an AWBW map, inspect the battlefield, and dial in the starting rules before the
                lobby goes live.
              </Text>
            </VStack>

            <VStack gap={4}>
              <TextInput
                isRequired
                label="Match name"
                onChange={(value) => {
                  matchNameRef.current = value;
                  setMatchName(value);
                  lastAutoMatchNameRef.current = null;
                  setLastAutoMatchName(null);
                }}
                placeholder="Riverside Duel"
                value={matchName}
              />

              <HStack align="end" gap={2} wrap="wrap">
                <NumberInput
                  isIntegerOnly
                  isRequired
                  label="AWBW map ID"
                  min={1}
                  onChange={handleMapIdChange}
                  value={parsedMapId}
                  width="100%"
                />
                <Button
                  clickAction={handleLoadMap}
                  isLoading={isLoadingMap}
                  label="Load map"
                  variant="secondary"
                />
              </HStack>

              <Grid columns={{ minWidth: 220, max: 2, repeat: "fit" }} gap={4}>
                <NumberInput
                  isIntegerOnly
                  isRequired
                  label="Starting funds"
                  min={0}
                  onChange={(value) => setStartingFunds(String(value))}
                  value={parsedStartingFunds}
                />
                <VStack gap={2}>
                  <CheckboxInput label="Fog enabled" onChange={setFogEnabled} value={fogEnabled} />
                  <CheckboxInput label="Private match" onChange={setIsPrivate} value={isPrivate} />
                  <CheckboxInput
                    description="Allow each signed-in user to claim more than one army."
                    label="Hotseat"
                    onChange={setHotseatEnabled}
                    value={hotseatEnabled}
                  />
                </VStack>
              </Grid>

              {!session ? (
                <Text weight="medium">
                  <RouterTextLink to="/auth" search={{ mode: undefined }}>
                    Sign in
                  </RouterTextLink>{" "}
                  to create a lobby.
                </Text>
              ) : (
                <Text type="supporting" weight="medium">
                  Lobby creator: {session.user.name}
                </Text>
              )}

              {mapError ? (
                <Banner description={mapError} status="error" title="Map preview failed" />
              ) : null}
              {createError ? (
                <Banner description={createError} status="error" title="Lobby creation failed" />
              ) : null}

              <Button
                clickAction={handleCreateLobby}
                isDisabled={createMatchMutation.isPending || mapData === null || !session}
                isLoading={createMatchMutation.isPending}
                label="Create lobby"
                variant="primary"
                width="100%"
              />
            </VStack>
          </VStack>
        </Card>

        <Section padding={6} variant="muted">
          <VStack gap={4}>
            <VStack gap={1}>
              <Heading level={2}>Map preview</Heading>
              <Text color="secondary" type="supporting">
                {mapData
                  ? `${mapData.Name} · ${mapData.Author}`
                  : "Load a map to inspect its terrain."}
              </Text>
            </VStack>
            {mapData && parsedMapId !== null ? (
              <VStack gap={4}>
                <MatchMapPreview mapId={parsedMapId} runner={previewRunner} />
                <MetadataList columns={3} label={{ position: "top" }}>
                  <MetadataListItem label="Players">{mapData["Player Count"]}</MetadataListItem>
                  <MetadataListItem label="Size">
                    {mapData["Size X"]} × {mapData["Size Y"]}
                  </MetadataListItem>
                  <MetadataListItem label="Published">{mapData["Published Date"]}</MetadataListItem>
                </MetadataList>
              </VStack>
            ) : (
              <EmptyState
                description="Enter an AWBW map ID and load it to inspect the battlefield."
                headingLevel={3}
                isCompact
                title="No map loaded"
              />
            )}
          </VStack>
        </Section>
      </Grid>
    </Section>
  );
}

function formatMapPreviewError(error: unknown): string {
  if (error instanceof AwbwMapDataQueryError && error.kind === "notFound") {
    return "Map not found.";
  }

  return "Map preview failed to load.";
}
