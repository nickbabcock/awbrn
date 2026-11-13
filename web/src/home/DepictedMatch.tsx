import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { CSSProperties } from "react";
import { UiSprite } from "#/components/UiSprite.tsx";
import { unitSpriteStyle } from "#/components/game_sprites.ts";
import { getFactionByCode } from "#/factions.ts";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import type { MapCatalogEntry } from "#/maps/schemas.ts";
import { ArmyPlate, type ArmyView } from "#/matches/components/Armies.tsx";
import { EndTurnKey } from "#/matches/components/EndTurn.tsx";
import {
  ConnectionState,
  DockKey,
  DockRow,
  InkIconKey,
  TurnLine,
} from "#/matches/components/MatchFrame.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconBack, IconCalculator, IconMore, IconReplay, IconWholeMap } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import type { PlayerRosterEntry } from "#/wasm/awbrn_wasm.js";
import { DepictedBoard } from "./DepictedBoard.tsx";
import type { DepictedSkirmish } from "./depicted.ts";

const noop = () => {};

/** A star of CO power, as the game counts funds into it. */
const STAR = 9000;

/**
 * The phone match screen as a picture: the turn line, two army plates, the
 * board at the touch floor, the tile row and the dock, drawn by the match's
 * own parts. It is inert: no hit targets, and a swipe over it scrolls the
 * page. Its accessible name says what it shows.
 */
export function DepictedMatch({
  factionCodes,
  map,
  skirmish,
}: {
  factionCodes: readonly [string, string];
  map: MapCatalogEntry;
  skirmish: DepictedSkirmish | null;
}) {
  const views: ArmyView[] = [
    {
      army: {
        entry: rosterEntry({
          coKey: "sami",
          coName: "Sami",
          factionCode: factionCodes[0],
          funds: 9200,
          playerId: 0,
          charge: 3.6 * STAR,
          copStars: 3,
          scopStars: 8,
        }),
        hasLiveStats: true,
        isActive: true,
        name: "Ash",
      },
      clock: { isRunning: true, label: "3m 42s left", pressure: "low", value: "3m 42s" },
      isViewer: true,
    },
    {
      army: {
        entry: rosterEntry({
          coKey: "max",
          coName: "Max",
          factionCode: factionCodes[1],
          funds: 2400,
          playerId: 1,
          charge: 2.2 * STAR,
          copStars: 3,
          scopStars: 6,
        }),
        hasLiveStats: true,
        isActive: false,
        name: "Hard CPU",
      },
      clock: { isRunning: false, label: "6m 58s left", pressure: "steady", value: "6m 58s" },
      isViewer: false,
    },
  ];

  return (
    <figure
      aria-label={`${map.name} on a phone, day 9`}
      role="img"
      {...stylex.props(styles.figure)}
    >
      <VStack inert xstyle={styles.screen}>
        <TurnLine
          back={
            <VStack as="span" xstyle={styles.back}>
              <Icon icon={IconBack} size="lg" />
            </VStack>
          }
          day={9}
          keys={<InkIconKey icon={IconReplay} label="Review earlier moments" onPress={noop} />}
          sub={
            <>
              {map.name} <ConnectionState status="connected" />
            </>
          }
          title="Your turn"
        />
        <VStack xstyle={styles.plates}>
          {views.map((view) => (
            <ArmyPlate
              hasYouTag
              isOwed={view.isViewer}
              key={view.army.entry.playerId}
              onOpen={noop}
              showUnits={false}
              view={view}
            />
          ))}
        </VStack>
        <VStack xstyle={styles.board}>
          <DepictedBoard alt="" factionCodes={factionCodes} map={map} skirmish={skirmish} />
        </VStack>
        <HStack xstyle={styles.tileRow}>
          <TileLinePicture factionCode={factionCodes[0]} map={map} skirmish={skirmish} />
          <Button label="Whole map" variant="on-ink" xstyle={styles.viewKey}>
            <VStack as="span" xstyle={styles.viewKeyBody}>
              <Icon icon={IconWholeMap} size="md" />
              <Text color="inherit" type="inherit" xstyle={styles.viewKeyLabel}>
                Whole map
              </Text>
            </VStack>
          </Button>
        </HStack>
        <DockRow>
          <VStack as="span" xstyle={styles.powerKey}>
            <UiSprite name="NormalPower.png" />
          </VStack>
          <DockKey icon={IconCalculator} label="Calculator" onPress={noop} />
          <DockKey icon={IconMore} label="Match" onPress={noop} />
          <EndTurnKey
            factionCode={factionCodes[0]}
            isDisabled={false}
            isLoading={false}
            isSpent={false}
            onPress={noop}
            readiness={{ freeSites: 1, idleUnits: 2 }}
            size="dock"
          />
        </DockRow>
      </VStack>
    </figure>
  );
}

/**
 * The tile line under the cursor: the terrain cut from the map's own render
 * and its defense, a rule, then the unit with HP, ammo and fuel.
 */
function TileLinePicture({
  factionCode,
  map,
  skirmish,
}: {
  factionCode: string;
  map: MapCatalogEntry;
  skirmish: DepictedSkirmish | null;
}) {
  const focus = skirmish?.focus ?? null;
  const picture = mapScreenshotSize("full", map.width, map.height);
  const terrain: CSSProperties | null =
    focus === null
      ? null
      : {
          backgroundImage: `url(${map.screenshot.full})`,
          backgroundSize: `${picture.width * 2}px ${picture.height * 2}px`,
          backgroundPosition: `-${focus.x * 32}px -${(focus.y + 1) * 32}px`,
          imageRendering: "pixelated",
        };
  const tank = unitSpriteStyle("tank", factionCode, 2);
  return (
    <HStack as="span" xstyle={styles.tileLine}>
      <HStack as="span" xstyle={styles.tileGroup}>
        {/* Crops of the map render and the sprite atlas, placed by an inline
            style object that no Astryx component takes. */}
        {terrain ? <span style={terrain} {...stylex.props(styles.tileArt)} /> : null}
        <Stat sprite="TerrainStar.png" value={skirmish?.focusDefense ?? 1} />
      </HStack>
      <HStack as="span" xstyle={[styles.tileGroup, styles.tileUnit]}>
        {tank ? <span style={tank} {...stylex.props(styles.unitArt)} /> : null}
        <Stat sprite="HP.png" value={10} />
        <Stat sprite="Ammo.png" value={9} />
        <Stat sprite="Fuel.png" value={62} />
      </HStack>
    </HStack>
  );
}

function Stat({ sprite, value }: { sprite: string; value: number }) {
  return (
    <HStack as="span" xstyle={[figureStyles.figure, styles.stat]}>
      <UiSprite name={sprite} />
      {value}
    </HStack>
  );
}

/** One army as the engine would report it, for a depicted plate. */
function rosterEntry({
  charge,
  coKey,
  coName,
  copStars,
  factionCode,
  funds,
  playerId,
  scopStars,
}: {
  charge: number;
  coKey: string;
  coName: string;
  copStars: number;
  factionCode: string;
  funds: number;
  playerId: number;
  scopStars: number;
}): PlayerRosterEntry {
  const factionName = getFactionByCode(factionCode)?.displayName ?? factionCode;
  return {
    playerId,
    userId: playerId,
    turnOrder: playerId,
    team: undefined,
    eliminated: false,
    actualFactionCode: factionCode,
    actualFactionName: factionName,
    displayFactionCode: factionCode,
    displayFactionName: factionName,
    factionCode,
    factionName,
    coKey,
    coName,
    tagCoKey: undefined,
    tagCoName: undefined,
    powerCharge: charge,
    copCost: copStars * STAR,
    scopCost: scopStars * STAR,
    powerStarCharge: STAR,
    activePower: undefined,
    stats: {
      funds,
      unitCount: 12,
      unitValue: undefined,
      income: undefined,
      properties: undefined,
      comTowers: undefined,
    },
  };
}

const styles = stylex.create({
  figure: {
    margin: 0,
    inlineSize: {
      default: "390px",
      [pageLayout.phoneMedia]: "100%",
    },
    userSelect: "none",
    WebkitUserSelect: "none",
    cursor: "default",
  },
  screen: {
    display: "grid",
    gridTemplateRows: "auto auto minmax(0, 1fr) auto auto",
    blockSize: {
      default: "748px",
      [pageLayout.phoneMedia]: "560px",
    },
    overflow: "hidden",
    borderRadius: "16px",
    backgroundColor: awbrnVars.ink900,
    color: awbrnVars.textOnInk,
    boxShadow: awbrnVars.shadowBoard,
  },
  back: {
    display: "inline-grid",
    placeItems: "center",
    flexShrink: 0,
    inlineSize: "44px",
    blockSize: "44px",
    color: awbrnVars.textOnInk,
  },
  plates: {
    display: "grid",
    gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
    backgroundColor: awbrnVars.ink900,
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: "1px",
  },
  board: {
    minBlockSize: 0,
  },
  tileRow: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    minBlockSize: "56px",
    paddingBlock: "6px",
    paddingInline: "8px",
    backgroundColor: awbrnVars.ink900,
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
  },
  tileLine: {
    display: "flex",
    alignItems: "center",
    gap: "10px",
    flexGrow: 1,
    minInlineSize: 0,
    blockSize: "44px",
    paddingInlineStart: "6px",
    paddingInlineEnd: "12px",
    borderRadius: "6px",
    backgroundColor: awbrnVars.sheet,
    color: colorVars["--color-text-primary"],
    whiteSpace: "nowrap",
    overflow: "hidden",
  },
  tileGroup: {
    display: "inline-flex",
    alignItems: "center",
    gap: "8px",
  },
  tileUnit: {
    paddingInlineStart: "10px",
    borderInlineStartColor: awbrnVars.rule,
    borderInlineStartStyle: "solid",
    borderInlineStartWidth: "1px",
  },
  tileArt: {
    display: "block",
    flexShrink: 0,
    inlineSize: "32px",
    blockSize: "32px",
  },
  unitArt: {
    display: "block",
    flexShrink: 0,
    marginBlockStart: "-4px",
  },
  stat: {
    display: "inline-flex",
    alignItems: "center",
    gap: "4px",
    fontSize: awbrnVars.text12_5,
    fontWeight: 600,
  },
  viewKey: {
    flexShrink: 0,
    marginInlineStart: "auto",
    inlineSize: "64px",
    minInlineSize: "64px",
    blockSize: "44px",
    paddingInline: 0,
  },
  viewKeyBody: {
    display: "grid",
    justifyItems: "center",
    gap: "2px",
  },
  viewKeyLabel: {
    fontSize: awbrnVars.textMicro,
    fontWeight: 650,
    lineHeight: 1,
    whiteSpace: "nowrap",
  },
  // The depicted power key: 76px, so the 66px POWER sprite clears the gold
  // ring by 5px each side.
  powerKey: {
    display: "inline-grid",
    placeItems: "center",
    flexShrink: 0,
    inlineSize: "76px",
    blockSize: "52px",
    borderRadius: "6px",
    backgroundColor: awbrnVars.ink950,
    boxShadow: `inset 0 0 0 1.5px ${awbrnVars.gold}`,
  },
});
