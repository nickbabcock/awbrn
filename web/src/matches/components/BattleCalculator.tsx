import { Button } from "#/ui/Button.tsx";
import { BottomSheet } from "@astryxdesign/core/BottomSheet";
import { Grid } from "@astryxdesign/core/Grid";
import { NumberInput } from "@astryxdesign/core/NumberInput";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import {
  borderVars,
  colorVars,
  fontWeightVars,
  radiusVars,
  shadowVars,
  spacingVars,
  textSizeVars,
  typographyVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { Close as CloseIcon } from "pixelarticons/react/Close";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { loadCoPortraitCatalog, type CoPortraitCatalog } from "#/components/co_portraits.ts";
import {
  terrainSpriteStyle,
  terrainTileStyle,
  uiAtlasSpriteStyle,
  unitSpriteSize,
  unitSpriteStyle,
} from "#/components/game_sprites.ts";
import { SpritePicker, type SpritePickerOption } from "./SpritePicker.tsx";
import type { GameRunner } from "#/engine/game_runner.ts";
import {
  FULL_HEALTH_POINTS,
  HEALTH_BARS_MAX,
  barsToPoints,
  defaultTerrain,
  engagementLabel,
  formatDamage,
  formatFunds,
  formatFundsBracket,
  formatNet,
  impossibleLabel,
  pointsToBars,
  retypeFighter,
  seatsFrom,
  sideFrom,
  terrainEntry,
  unitEntry,
  type CalculatorSide,
} from "./battle_calculator.ts";
import type {
  BattleCatalog,
  BattleCalculatorError,
  BattleFighter,
  BattleReportWire,
  BattleSide,
  CatalogDomain,
  PlayerRosterSnapshot,
  PowerLevel,
  Terrain,
  UnitKind,
} from "#/wasm/awbrn_wasm.js";

/**
 * The engagement a player is imagining, priced.
 *
 * The board already forecasts the attacks a unit can make right now. This is
 * the other half of the same question and the reason a player still opens
 * AWBW's calculator in a second tab: what an attack would cost if the unit were
 * built, if it stood somewhere else, if the tower were taken, if the power were
 * up. None of that is on the board, so none of it can be asked of the board.
 *
 * Its shape is the reference tool's own: two armies fully specified side by
 * side, a swap on the centreline between them, and the exchange read across it.
 * Each column carries everything that moves a damage figure for the army
 * standing in it — the commander, the power, the treasury, the properties, the
 * towers, and the unit itself — so the two are compared by reading across
 * rather than by remembering what the other one said.
 *
 * Weather is absent on purpose, and is not sent either. No commander in the
 * ruleset gates a firepower or defense rule on it, so the three settings score
 * the same exchange; offering the player a control would only invite them to
 * click all three and conclude the panel is broken.
 *
 * Every figure is AWVM's. The panel holds no formula, no table of costs and no
 * opinion about whether a trade is good; the numbers are what a player came for
 * and judging them is the thing they came to do.
 */

/** How the panel is drawn, following the input that opened it. */
export type BattleCalculatorPresentation = "board" | "sheet";

/**
 * When the board frame is too small to hold the panel, and the panel becomes a
 * sheet instead.
 *
 * Height is in the rule because the board is 3:2 and its width is capped by the
 * window's height, so a short window makes a short board however wide the
 * screen is. A panel taller than the frame it sits in is a panel whose last
 * line is cut in half, and a bottom sheet is a better answer than a scroll the
 * player did not ask for.
 */
export const BATTLE_CALCULATOR_SHEET_MEDIA = "(max-width: 1279px), (max-height: 899px)";

interface BattleCalculatorProps {
  onDismiss: () => void;
  /** Hands the keyboard back to the board once the panel has closed. */
  onRestoreFocus: () => void;
  presentation: BattleCalculatorPresentation;
  /** The armies as the board currently has them. `null` seeds a blank sheet. */
  roster: PlayerRosterSnapshot | null;
  runner: GameRunner | null;
  /** Which seat the player occupies, when they occupy one. */
  viewerSlotIndex?: number | null;
}

/** What the panel opens on when no board has reported an army. */
const FALLBACK_UNIT: UnitKind = "infantry";
const FALLBACK_ATTACKER_FACTION = "os";
const FALLBACK_DEFENDER_FACTION = "bm";

/**
 * The picker value that means no commander at all.
 *
 * It is not a commander the ruleset has, so it cannot be one of its keys. The
 * combat algebra reads it as the neutral commander, which is a commander with
 * no rules rather than an army fighting without one.
 */
const NO_COMMANDER = "none";

/**
 * The two power stars, as the source tool draws them beside a commander.
 *
 * The atlas also holds `NormalPower` and `SuperPower`, but that art is
 * lettering: it draws the words POWER and SUPER at 66px wide, which is a
 * banner rather than a key on a menu strip. These are 14px marks that sit in
 * a control at their own size, and red for the power against blue for the
 * super is the source tool's own pairing rather than a choice made here.
 */
const POWER_MARK = uiAtlasSpriteStyle("CO/Power.png");
const SUPER_POWER_MARK = uiAtlasSpriteStyle("CO/SuperPower.png");

/** The marks a player already reads these five quantities by. */
const COIN = uiAtlasSpriteStyle("Coin.png");
const PROPERTY = uiAtlasSpriteStyle("BuildingsCaptured.png");
const COM_TOWER = uiAtlasSpriteStyle("commtowericon.png");
const HP = uiAtlasSpriteStyle("HP.png");
const AMMO = uiAtlasSpriteStyle("Ammo.png");

export function BattleCalculator({
  onDismiss,
  onRestoreFocus,
  presentation,
  roster,
  runner,
  viewerSlotIndex,
}: BattleCalculatorProps) {
  const [catalog, setCatalog] = useState<BattleCatalog | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [portraits] = useState<CoPortraitCatalog>(() => loadCoPortraitCatalog());

  const seats = useMemo(
    () => seatsFrom(roster, viewerSlotIndex ?? null),
    [roster, viewerSlotIndex],
  );

  const [attackerSide, setAttackerSide] = useState<CalculatorSide>(() => sideFrom(seats.attacker));
  const [defenderSide, setDefenderSide] = useState<CalculatorSide>(() => sideFrom(seats.defender));
  const [attacker, setAttacker] = useState<BattleFighter>(() => blankFighter());
  const [defender, setDefender] = useState<BattleFighter>(() => blankFighter());
  const [report, setReport] = useState<BattleReportWire | null>(null);
  const [reportError, setReportError] = useState<string | null>(null);

  // Which seat each army is standing in. The board seats them once; the swap
  // key is the player asking the other half of the question — what this trade
  // costs when it is made against them instead of by them.
  const [isSwapped, setIsSwapped] = useState(false);
  const seatedAttackerFaction =
    seats.attacker?.displayFactionCode ?? seats.attacker?.factionCode ?? FALLBACK_ATTACKER_FACTION;
  const seatedDefenderFaction =
    seats.defender?.displayFactionCode ?? seats.defender?.factionCode ?? FALLBACK_DEFENDER_FACTION;
  const attackerFaction = isSwapped ? seatedDefenderFaction : seatedAttackerFaction;
  const defenderFaction = isSwapped ? seatedAttackerFaction : seatedDefenderFaction;

  useEffect(() => {
    if (!runner) return;
    let cancelled = false;

    runner
      .loadBattleCatalog()
      .then((loaded) => {
        if (!cancelled) setCatalog(loaded);
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setCatalogError(error instanceof Error ? error.message : "The rules could not be read.");
        }
      });

    return () => {
      cancelled = true;
    };
  }, [runner]);

  // Every edit is a new question, and the answer is one message to the worker
  // that already holds the engine. Requests are not debounced: the reducer
  // scores an engagement in microseconds, and a figure that lagged a keystroke
  // behind would be a figure a player could read and act on while it was still
  // describing the previous board.
  //
  // The defender is sent as a column of one. The wire scores a list because the
  // reducer does, and the panel asks about a single pairing.
  useEffect(() => {
    const attackerContext = completeSide(attackerSide);
    const defenderContext = completeSide(defenderSide);
    if (!runner || !attackerContext || !defenderContext) {
      setReport(null);
      setReportError(null);
      return;
    }
    let cancelled = false;
    setReport(null);
    setReportError(null);

    runner
      .forecastBattle({
        attacker: attackerContext,
        attackingUnit: attacker,
        defender: defenderContext,
        defendingUnits: [defender],
      })
      .then((response) => {
        if (cancelled) return;
        if (response.status === "failure") {
          setReport(null);
          setReportError(errorMessage(response.error));
          return;
        }
        setReport(response.report);
        setReportError(null);
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setReport(null);
        setReportError(unexpectedErrorMessage(error, "The engagement could not be scored."));
      });

    return () => {
      cancelled = true;
    };
  }, [attacker, attackerSide, defender, defenderSide, runner]);

  /**
   * Turn the engagement around.
   *
   * Both armies, both premises and both units change seats at once, because a
   * trade read from one side only is half an answer: a player who has just
   * been told an attack costs them 2,100 wants to know what the reply costs
   * before they commit to it. It is the reference calculator's one command
   * this panel did not have.
   */
  const swapSeats = useCallback(() => {
    setIsSwapped((previous) => !previous);
    setAttackerSide(defenderSide);
    setDefenderSide(attackerSide);
    setAttacker(defender);
    setDefender(attacker);
  }, [attacker, attackerSide, defender, defenderSide]);

  // The catalogues are the ruleset's and do not change while the panel is
  // open; only the colours the sprites are drawn in do. Building them once per
  // army rather than once per control is what keeps a keystroke in a treasury
  // field from rebuilding two hundred sprites.
  const attackerUnits = useMemo(
    () => unitOptionsFor(catalog, attackerFaction),
    [attackerFaction, catalog],
  );
  const defenderUnits = useMemo(
    () => unitOptionsFor(catalog, defenderFaction),
    [catalog, defenderFaction],
  );
  const terrains = useMemo(() => terrainOptionsFor(catalog), [catalog]);
  const commanders = useMemo(() => commanderOptionsFor(catalog, portraits), [catalog, portraits]);

  const kit: PanelKit = {
    isSheet: presentation === "sheet",
    attacker,
    attackerFaction,
    attackerSide,
    attackerUnits,
    catalog,
    commanders,
    defender,
    defenderFaction,
    defenderSide,
    defenderUnits,
    failure: catalogError ?? reportError,
    isScoring:
      runner !== null &&
      completeSide(attackerSide) !== null &&
      completeSide(defenderSide) !== null &&
      report === null &&
      reportError === null,
    missingFigures: missingFigureInstruction(attackerSide, defenderSide),
    onAttackerChange: setAttackerSide,
    onAttackerUnitChange: setAttacker,
    onDefenderChange: setDefenderSide,
    onDefenderUnitChange: setDefender,
    onDismiss,
    onRestoreFocus,
    onSwap: swapSeats,
    portraits,
    report,
    terrains,
  };

  return <DuelPanel kit={kit} />;
}

/* ------------------------------- DUEL ----------------------------- *
 * The reference calculator's framing, taken seriously.
 *
 * AWBW's own tool is symmetric: two armies fully specified side by side, a
 * swap between them, and the exchange read across the middle.
 *
 * What is bold here is the scale. The board's own forecast sets its figures
 * at the HUD floor; this sets them well above it, because in the source game
 * a damage number is not a table cell. Silkscreen is a bitmap face and scales
 * up the way the game's own numerals do — the floor rule sets a minimum and
 * never a maximum.
 *
 * Each column carries what the unit standing in it does: the attacker's
 * damage under the attacker, the reply under the defender. That is the
 * reference tool's own arrangement and the one every player already reads,
 * so it is not re-derived here. "Deals" and "Takes" are still said from the
 * attacking seat, exactly as the order menu says them.
 * ------------------------------------------------------------------ */

function DuelPanel({ kit }: { kit: PanelKit }) {
  const row = kit.report?.rows?.[0];
  const result = row?.result;
  const attackerName = unitName(kit.catalog, kit.attacker.unit);
  const targetName = row?.name ?? unitName(kit.catalog, kit.defender.unit);

  return (
    <PanelFrame kit={kit}>
      <VStack gap={0} isScrollable xstyle={styles.body}>
        <Grid align="start" xstyle={styles.duelGrid}>
          <DuelColumn kit={kit} role="Attacking" />

          {/* The axis the exchange happens across, and the one command that
              turns it around. It sits on the centreline rather than in the
              head, because swapping is a thing done to the two columns. */}
          <VStack align="center" gap={0} justify="center" xstyle={styles.axis}>
            <span {...stylex.props(styles.axisRule)} />
            <button
              onClick={kit.onSwap}
              title="Swap the two armies"
              type="button"
              {...stylex.props(styles.swap)}
            >
              <span aria-hidden="true">⇄</span>
              <span {...stylex.props(styles.hiddenLabel)}>Swap the two armies</span>
            </button>
          </VStack>

          <DuelColumn kit={kit} role="Defending" />
        </Grid>

        <span {...stylex.props(styles.hiddenLabel)}>
          {engagementLabel(attackerName, targetName, kit.defender, result, row?.impossible)}
        </span>

        {result ? (
          <Grid aria-hidden="true" align="start" columns={2} gap={2} xstyle={styles.duelOutput}>
            <VStack align="center" gap={0.5} xstyle={styles.duelFigure}>
              <span {...stylex.props(styles.duelLabel)}>Deals</span>
              <span {...stylex.props(styles.duelPercent)}>{formatDamage(result.damage)}</span>
              <span {...stylex.props(styles.duelFunds)}>
                {formatFundsBracket(result.valueDealt)}
              </span>
            </VStack>

            {/* The reply, split the way the reference tool splits it. One
                range folds two different spreads together — how much of the
                target survives, and the luck it answers with — and a player
                reading 27 – 36% cannot tell which part is which. The rungs
                separate them: one per health it may be left standing in,
                each carrying the luck alone. Below two rungs there is no
                spread to show, so the single range says it better. */}
            <VStack align="center" gap={0.5} xstyle={styles.duelFigure}>
              <span {...stylex.props(styles.duelLabel)}>
                {result.counterFirst ? "Takes 1st" : "Takes"}
              </span>
              {result.counterSteps.length > 1 ? (
                <VStack as="ul" gap={0.5} xstyle={styles.rungs}>
                  {result.counterSteps.map((step) => (
                    <HStack align="center" as="li" gap={2} key={step.targetHealth}>
                      <span {...stylex.props(styles.rungAt)}>
                        @{pointsToBars(step.targetHealth)} HP
                      </span>
                      <span {...stylex.props(styles.rungPercent)}>
                        {formatDamage(step.counter)}
                      </span>
                    </HStack>
                  ))}
                </VStack>
              ) : (
                <span {...stylex.props(styles.duelPercent, !result.counter && styles.muted)}>
                  {result.counter ? formatDamage(result.counter) : "—"}
                </span>
              )}
              <span {...stylex.props(styles.duelFunds)}>
                {result.valueTaken ? formatFundsBracket(result.valueTaken) : "—"}
              </span>
            </VStack>
          </Grid>
        ) : (
          <Grid columns={1} xstyle={styles.duelOutput}>
            <span {...stylex.props(styles.panelLabel)}>
              {row
                ? impossibleLabel(row.impossible ?? "no-weapon")
                : (kit.missingFigures ?? (kit.isScoring ? "Scoring…" : "No score available."))}
            </span>
          </Grid>
        )}

        <HStack align="center" gap={2} justify="center" wrap="wrap" xstyle={styles.duelNet}>
          <span {...stylex.props(styles.duelLabel)}>Net</span>
          <span {...stylex.props(styles.netValue)}>{result ? formatNet(result.net) : "—"}</span>
          {result?.destroys ? (
            <span {...stylex.props(styles.panelLabel)}>Destroys</span>
          ) : result?.mayDestroy ? (
            <span {...stylex.props(styles.panelLabel)}>May destroy</span>
          ) : null}
        </HStack>
      </VStack>
    </PanelFrame>
  );
}

/**
 * One army, fully specified.
 *
 * The seat is named and the army is not. Which army is standing in a seat is
 * already told by the portrait, by the colours its unit sprite is drawn in,
 * and by the match the panel opened over; spelling out "Orange Star" beside
 * all three buys a line of height to repeat what is already on screen twice.
 *
 * The faction wash stays. It is not doing informational work the way a
 * faction chip on a roster row does — the seat is named in words above it and
 * the portrait and unit sprite are already the army's — so it echoes what is
 * on screen rather than being the only thing saying it.
 */
function DuelColumn({ kit, role }: { kit: PanelKit; role: "Attacking" | "Defending" }) {
  const isAttacker = role === "Attacking";
  const side = isAttacker ? kit.attackerSide : kit.defenderSide;
  const onChange = isAttacker ? kit.onAttackerChange : kit.onDefenderChange;
  const factionCode = isAttacker ? kit.attackerFaction : kit.defenderFaction;
  const fighter = isAttacker ? kit.attacker : kit.defender;

  return (
    <VStack
      as="section"
      gap={1}
      xstyle={[
        styles.duelColumn,
        styles.factionWash(
          `var(--color-faction-${factionCode}-wash, ${colorVars["--color-background-muted"]})`,
        ),
        !isAttacker && styles.defenderColumn,
      ]}
    >
      <VStack as="span" gap={0} xstyle={styles.role}>
        {isAttacker ? "Attacker" : "Defender"}
      </VStack>

      <SpritePicker
        label={`${role} commander`}
        onChange={(value) => onChange({ ...side, commander: value as CalculatorSide["commander"] })}
        options={kit.commanders}
        shape="commander"
        triggerArt={
          <CoPortrait
            catalog={kit.portraits}
            coKey={side.commander ?? "no-co"}
            fallbackLabel={`${role} commander`}
            hasFrame={false}
            size={48}
          />
        }
        triggerLabel={side.commander === undefined ? "No CO" : undefined}
        value={side.commander ?? NO_COMMANDER}
      />

      <PowerSelect
        isDisabled={side.commander === undefined}
        label={`${role} power`}
        onChange={(next) => onChange({ ...side, power: next })}
        value={side.power ?? "d2d"}
      />

      <TreasuryFields isDefender={!isAttacker} onChange={onChange} role={role} side={side} />

      <HStack gap={1} wrap="wrap" xstyle={styles.duelUnit}>
        <FighterFields
          catalog={kit.catalog}
          factionCode={factionCode}
          fighter={fighter}
          onChange={isAttacker ? kit.onAttackerUnitChange : kit.onDefenderUnitChange}
          role={isAttacker ? "Attacker" : "Target"}
          terrainOptions={kit.terrains}
          unitOptions={isAttacker ? kit.attackerUnits : kit.defenderUnits}
        />
      </HStack>
    </VStack>
  );
}

/** Everything the panel's parts need, gathered once. */
interface PanelKit {
  attacker: BattleFighter;
  attackerFaction: string;
  attackerSide: CalculatorSide;
  attackerUnits: SpritePickerOption[];
  catalog: BattleCatalog | null;
  commanders: SpritePickerOption[];
  defender: BattleFighter;
  defenderFaction: string;
  defenderSide: CalculatorSide;
  defenderUnits: SpritePickerOption[];
  failure: string | null;
  isScoring: boolean;
  /** Whether the panel is a sheet on the viewport rather than a window on the board. */
  isSheet: boolean;
  onAttackerChange: (side: CalculatorSide) => void;
  onAttackerUnitChange: (fighter: BattleFighter) => void;
  onDefenderChange: (side: CalculatorSide) => void;
  onDefenderUnitChange: (fighter: BattleFighter) => void;
  onDismiss: () => void;
  onRestoreFocus: () => void;
  /** Turn the engagement around: both armies change seats at once. */
  onSwap: () => void;
  missingFigures: string | null;
  portraits: CoPortraitCatalog;
  report: BattleReportWire | null;
  terrains: SpritePickerOption[];
}

/**
 * The shell the panel stands in, in whichever of the two shapes it was asked
 * for, and the keyboard contract the board menus already hold.
 *
 * Both shapes are the ones the board's own menus use, and for the same reasons.
 * A sheet is the system's dialog: it takes the top layer, dims the board behind
 * it, locks the page's scroll, traps the keyboard and hands a single Escape to
 * whatever is layered over it. A window on the board is deliberately none of
 * those things — a player reads the map while they use it — so it is the one
 * that has to keep the promises itself.
 *
 * Either way the board takes its keyboard back when the panel goes, so a player
 * who opened it mid-turn is exactly where they were.
 */
function PanelFrame({ children, kit }: { children: React.ReactNode; kit: PanelKit }) {
  useEffect(() => {
    document
      .querySelector<HTMLElement>(
        '[aria-label="Battle calculator"][data-autofocus], [aria-label="Battle calculator"] [data-autofocus]',
      )
      ?.focus();

    return () => {
      if (document.activeElement === null || document.activeElement === document.body) {
        kit.onRestoreFocus();
      }
    };
    // The restore runs once, when the panel goes away.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const panel = (
    <>
      <HStack align="center" gap={2} justify="between" wrap="wrap" xstyle={styles.header}>
        <span {...stylex.props(styles.panelLabel)}>Battle calculator</span>
        <Button
          clickAction={kit.onDismiss}
          icon={<CloseIcon aria-hidden height={16} width={16} />}
          label="Close"
          size="sm"
          variant="secondary"
        />
      </HStack>

      {children}

      <HStack align="center" gap={2} justify="between" wrap="wrap" xstyle={styles.footer}>
        {kit.failure ? (
          <span role="alert" {...stylex.props(styles.failure)}>
            {kit.failure}
          </span>
        ) : (
          <span {...stylex.props(styles.hint)}>
            Read the top of Deals with the bottom of Takes.
          </span>
        )}
      </HStack>
    </>
  );

  if (kit.isSheet) {
    return (
      <BottomSheet
        height="tall"
        isOpen
        label="Battle calculator"
        onOpenChange={(isOpen) => {
          if (!isOpen) kit.onDismiss();
        }}
        purpose="form"
        snapPoints={[0.5]}
        xstyle={styles.sheet}
      >
        {/* The sheet takes the focus a modal has to place somewhere. Without a
            target the browser falls to the first control in the panel, which
            then wears a focus ring nobody asked for. */}
        <VStack data-autofocus gap={0} tabIndex={-1} xstyle={styles.sheetBody}>
          {panel}
        </VStack>
      </BottomSheet>
    );
  }

  return (
    <VStack
      aria-label="Battle calculator"
      data-autofocus
      gap={0}
      onKeyDown={(event) => {
        // A picker, a list or a menu standing over the panel owns Escape while
        // it is open: a player dismissing the unit grid is asking to close the
        // grid, and taking the whole calculator away with it would throw out
        // the engagement they spent four controls describing. Everything that
        // opens over the panel opens in the top layer, and a control holding
        // one open says so on itself, so the two together are the whole test:
        // the key landed inside a layer, or on the trigger that opened it.
        // The sheet needs none of this. BottomSheet manages its own top layer.
        if (event.key !== "Escape") return;
        if (
          event.target instanceof Element &&
          event.target.closest('[popover], [aria-expanded="true"]')
        ) {
          return;
        }
        event.preventDefault();
        kit.onDismiss();
      }}
      role="dialog"
      tabIndex={-1}
      xstyle={styles.boardPanel}
    >
      {panel}
    </VStack>
  );
}

/**
 * One army's premise, on a single line.
 *
 * Each figure says what it is with the icon the roster already uses for it —
 * the coin, the building, the tower — standing inside the field rather than in
 * a row of words above it. A player has been reading these three marks on
 * every army panel since they arrived, and a written label bought two lines of
 * height to repeat what the coin says. The name is still on the field for
 * anyone hearing it read.
 */
function TreasuryFields({
  isDefender,
  onChange,
  role,
  side,
}: {
  isDefender: boolean;
  onChange: (side: CalculatorSide) => void;
  role: string;
  side: CalculatorSide;
}) {
  return (
    <Grid
      align="center"
      columnGap={2}
      rowGap={1}
      xstyle={[styles.treasury, isDefender && styles.defenderTreasury]}
    >
      <VStack gap={0} xstyle={styles.leftToRight}>
        <NumberInput
          isLabelHidden
          label={`${role} funds`}
          min={0}
          onChange={(value) => onChange({ ...side, funds: Math.max(0, value ?? 0) })}
          size="sm"
          startIcon={<StatIcon art={COIN} />}
          step={1000}
          value={side.funds}
        />
      </VStack>
      {/* A tower is a property, so the count includes it. Bounding each field
          by the other is what keeps the pair from describing an army holding
          three towers and one building. */}
      <VStack gap={0} xstyle={styles.leftToRight}>
        <NumberInput
          isLabelHidden
          label={`${role} properties, com towers included`}
          max={200}
          min={side.comTowers ?? 0}
          onChange={(value) =>
            onChange({ ...side, properties: clamp(value, side.comTowers ?? 0, 200) })
          }
          size="sm"
          startIcon={<StatIcon art={PROPERTY} />}
          value={side.properties}
        />
      </VStack>
      <VStack gap={0} xstyle={styles.leftToRight}>
        <NumberInput
          isLabelHidden
          label={`${role} com towers`}
          max={side.properties ?? 200}
          min={0}
          onChange={(value) =>
            onChange({ ...side, comTowers: clamp(value, 0, side.properties ?? 200) })
          }
          size="sm"
          startIcon={<StatIcon art={COM_TOWER} />}
          value={side.comTowers}
        />
      </VStack>
    </Grid>
  );
}

/** What the three power keys offer, in the order a charge fills. */
const POWER_KEYS = [
  { value: "d2d", label: "Day to day", art: null },
  { value: "cop", label: "CO power", art: POWER_MARK },
  { value: "scop", label: "Super CO power", art: SUPER_POWER_MARK },
] as const;

/**
 * Which power an army is running, said in the game's own lettering.
 *
 * A player has read POWER and SUPER across the screen every time either one
 * fired, and has never once read COP or SCOP anywhere but a forum post. The
 * banners are the source game's own art at its own size, so the control is
 * read rather than decoded. Day-to-day keeps a word, because it is the absence
 * of a banner and there is no art for a thing not happening.
 *
 * It is a radio group rather than three toggles: the three states are one
 * choice, and two of them showing off at once would describe a match no board
 * can be in.
 */
function PowerSelect({
  isDisabled,
  label,
  onChange,
  value,
}: {
  isDisabled: boolean;
  label: string;
  onChange: (power: PowerLevel | undefined) => void;
  value: PowerLevel | "d2d";
}) {
  const keys = useRef<(HTMLButtonElement | null)[]>([]);
  const at = POWER_KEYS.findIndex((key) => key.value === value);

  return (
    <HStack
      aria-disabled={isDisabled || undefined}
      aria-label={label}
      as="div"
      gap={0}
      onKeyDown={(event) => {
        const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
        if (step === 0 || isDisabled) return;
        event.preventDefault();
        const next = (at + step + POWER_KEYS.length) % POWER_KEYS.length;
        const key = POWER_KEYS[next];
        if (!key) return;
        onChange(key.value === "d2d" ? undefined : (key.value as PowerLevel));
        keys.current[next]?.focus();
      }}
      role="radiogroup"
      xstyle={[styles.powerGroup, isDisabled && styles.powerGroupDisabled]}
    >
      {POWER_KEYS.map((key, index) => (
        <button
          aria-checked={key.value === value}
          disabled={isDisabled}
          key={key.value}
          onClick={() => onChange(key.value === "d2d" ? undefined : (key.value as PowerLevel))}
          ref={(element) => {
            keys.current[index] = element;
          }}
          role="radio"
          tabIndex={key.value === value ? 0 : -1}
          type="button"
          {...stylex.props(styles.powerKey, key.value === value && styles.powerKeySelected)}
        >
          <span aria-hidden="true" style={key.art ?? undefined} {...stylex.props(styles.spriteArt)}>
            {key.art ? null : "D2D"}
          </span>
          <span {...stylex.props(styles.hiddenLabel)}>{key.label}</span>
        </button>
      ))}
    </HStack>
  );
}

/**
 * A unit, its condition, and the ground under it.
 *
 * The same four controls for the attacker and for the defender, because they
 * are the same four facts: which unit, how much of it is left, what it is
 * standing on, and what it has left to fire.
 */
function FighterFields({
  catalog,
  factionCode,
  fighter,
  onChange,
  role,
  terrainOptions,
  unitOptions,
}: {
  catalog: BattleCatalog | null;
  factionCode: string;
  fighter: BattleFighter;
  onChange: (fighter: BattleFighter) => void;
  role: string;
  terrainOptions: SpritePickerOption[];
  unitOptions: SpritePickerOption[];
}) {
  const entry = unitEntry(catalog?.units ?? [], fighter.unit);
  const bars = pointsToBars(fighter.health);
  const ground = terrainEntry(catalog?.terrains ?? [], fighter.terrain);

  return (
    <HStack gap={0} xstyle={styles.fighterFields}>
      {/* The picker carries the sprite that used to stand beside it, so the
          art is the control rather than a caption on one.

          Both triggers draw their art at one and both grids draw it at two.
          The row is where a name has to survive next to three other controls,
          and a sprite at two takes the width the name needs; the grid is where
          the choice is actually made, and there the art leads. */}
      <SpritePicker
        label={`${role} unit`}
        onChange={(value) => {
          const next = unitEntry(catalog?.units ?? [], value as UnitKind);
          if (next) onChange(retypeFighter(fighter, next));
        }}
        options={unitOptions}
        shape="unit"
        triggerArt={<UnitSprite factionCode={factionCode} scale={1} unit={fighter.unit} />}
        triggerXstyle={styles.unitField}
        value={fighter.unit}
      />

      {/* Health is a count from one to ten, so it is typed or stepped like
          one. A list of ten fixed choices made the shortest question on the
          row into the widest control on it, and made "nine" a thing to find
          rather than a thing to enter. */}
      <NumberInput
        isLabelHidden
        label={`${role} health`}
        max={HEALTH_BARS_MAX}
        min={1}
        onChange={(value) =>
          onChange({ ...fighter, health: barsToPoints(clamp(value ?? 1, 1, 10)) })
        }
        size="sm"
        startIcon={<StatIcon art={HP} />}
        value={bars}
        xstyle={styles.healthField}
      />

      {/* The trigger shows the tile square alone and the grid shows the whole
          cell. What rises above a tile is half its art and twice its height,
          and a row that made room for a mountain peak would carry that height
          across the whole column for the sake of eight tiles that have one. */}
      <SpritePicker
        label={`${role} terrain`}
        onChange={(value) => onChange({ ...fighter, terrain: value as Terrain })}
        options={terrainOptions}
        shape="terrain"
        triggerArt={
          ground ? (
            <span
              aria-hidden="true"
              style={terrainTileStyle(ground.spriteIndex)}
              {...stylex.props(styles.spriteArt)}
            />
          ) : null
        }
        triggerXstyle={styles.terrainField}
        value={fighter.terrain}
      />

      {/* Ammo is asked only of units that carry a magazine, and it is asked
          because it changes which weapon fires: a Tank out of shells answers
          another Tank with its machine gun, which is a different attack and not
          only a weaker one. */}
      {entry && entry.maxAmmo > 0 ? (
        <NumberInput
          isLabelHidden
          label={`${role} ammo`}
          max={entry.maxAmmo}
          min={0}
          onChange={(value) => onChange({ ...fighter, ammo: clamp(value ?? 0, 0, entry.maxAmmo) })}
          size="sm"
          startIcon={<StatIcon art={AMMO} />}
          value={fighter.ammo ?? entry.maxAmmo}
          xstyle={styles.ammoField}
        />
      ) : null}
    </HStack>
  );
}

/**
 * The mark standing inside a field, naming what the field counts.
 *
 * Decoration to a reader: the field it sits in carries the name in words, so
 * an icon that also announced itself would say the same thing twice.
 */
function StatIcon({ art }: { art: React.CSSProperties | null }) {
  if (!art) return null;
  return <span aria-hidden="true" style={art} {...stylex.props(styles.spriteArt)} />;
}

/** A unit in an army's colours, at a whole multiple of its own pixels. */
function UnitSprite({
  factionCode,
  scale,
  unit,
}: {
  factionCode: string;
  scale: 1 | 2;
  unit: UnitKind;
}) {
  const sprite = unitSpriteStyle(unit, factionCode, scale);
  if (!sprite) return null;

  return (
    <span aria-hidden="true" {...stylex.props(styles.sprite)} style={unitSpriteSize(scale)}>
      <span style={sprite} {...stylex.props(styles.spriteArt)} />
    </span>
  );
}

/**
 * What each picker may offer, built once per army rather than per control.
 *
 * The units are filed under where they travel, which is the division the
 * game's own build menus use and the one a player is already thinking in: a
 * question about a Battleship is never answered by scrolling past the Recon.
 */
function unitOptionsFor(catalog: BattleCatalog | null, factionCode: string): SpritePickerOption[] {
  return (
    [...(catalog?.units ?? [])]
      // The ruleset names its units in its own order, which is alphabetical and
      // puts an Anti-Air, a B-Copter and a Battleship in a row. Gathering each
      // domain is what makes the headings mean anything.
      .sort(
        (left, right) =>
          DOMAIN_ORDER.indexOf(left.domain) - DOMAIN_ORDER.indexOf(right.domain) ||
          left.name.localeCompare(right.name),
      )
      .map((unit) => ({
        value: unit.unit,
        label: unit.name,
        detail: formatFunds(unit.cost),
        art: <UnitSprite factionCode={factionCode} scale={2} unit={unit.unit} />,
        group: DOMAIN_NAMES[unit.domain],
      }))
  );
}

/** Where a unit travels, in the order the build menus ask for it. */
const DOMAIN_ORDER: CatalogDomain[] = ["ground", "air", "sea"];

const DOMAIN_NAMES: Record<CatalogDomain, string> = {
  ground: "Ground",
  air: "Air",
  sea: "Sea",
};

function terrainOptionsFor(catalog: BattleCatalog | null): SpritePickerOption[] {
  return (catalog?.terrains ?? []).map((terrain) => ({
    value: terrain.terrain,
    label: terrain.name,
    // The stars are the whole reason terrain is in this panel, so they are the
    // figure on the cell rather than a suffix on the name.
    detail: `${terrain.stars}★`,
    art: (
      <span
        aria-hidden="true"
        style={terrainSpriteStyle(terrain.spriteIndex, 2)}
        {...stylex.props(styles.spriteArt)}
      />
    ),
  }));
}

/**
 * The commanders a player may pick, which is all of them and only them.
 *
 * "No CO" is not among them. It is a state a match can arrive in and the panel
 * still shows it when a board reports it, but it is not an answer to "who is
 * commanding": nobody weighing an attack wants to know what it would cost with
 * the commanders taken away, and offering it puts a cell that empties the
 * question in the middle of the twenty-nine that answer it.
 */
function commanderOptionsFor(
  catalog: BattleCatalog | null,
  portraits: CoPortraitCatalog,
): SpritePickerOption[] {
  return (catalog?.commanders ?? []).map((entry) => ({
    value: entry.commander,
    label: entry.name,
    art: (
      <CoPortrait
        catalog={portraits}
        coKey={entry.commander}
        fallbackLabel={entry.name}
        hasFrame={false}
        size={64}
      />
    ),
  }));
}

function blankFighter(): BattleFighter {
  return {
    unit: FALLBACK_UNIT,
    health: FULL_HEALTH_POINTS,
    ammo: undefined,
    terrain: defaultTerrain("ground"),
  };
}

function completeSide(side: CalculatorSide): BattleSide | null {
  if (side.funds === undefined || side.properties === undefined || side.comTowers === undefined) {
    return null;
  }
  return { ...side, funds: side.funds, properties: side.properties, comTowers: side.comTowers };
}

function errorMessage(error: BattleCalculatorError): string {
  const instruction = {
    health: "Check unit health.",
    properties: "Check the property count.",
    "com-towers": "Check the com tower count.",
    layout: "Check the engagement.",
  }[error.kind];
  return `${instruction} ${error.message}`;
}

function unexpectedErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

function missingFigureInstruction(
  attacker: CalculatorSide,
  defender: CalculatorSide,
): string | null {
  const missing = [
    ...missingFiguresFor("attacker", attacker),
    ...missingFiguresFor("defender", defender),
  ];
  return missing.length === 0 ? null : `Enter ${missing.join(", ")}.`;
}

function missingFiguresFor(role: string, side: CalculatorSide): string[] {
  return [
    side.funds === undefined ? `${role} funds` : null,
    side.properties === undefined ? `${role} properties` : null,
    side.comTowers === undefined ? `${role} com towers` : null,
  ].filter((figure): figure is string => figure !== null);
}

function unitName(catalog: BattleCatalog | null, unit: UnitKind): string {
  return unitEntry(catalog?.units ?? [], unit)?.name ?? unit;
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

const styles = stylex.create({
  header: {
    flex: "0 0 auto",
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-2"],
    backgroundColor: colorVars["--color-background-muted"],
    borderBlockEndWidth: borderVars["--border-width"],
    borderBlockEndStyle: "solid",
    borderBlockEndColor: colorVars["--color-border-emphasized"],
  },
  footer: {
    flex: "0 0 auto",
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-2"],
    backgroundColor: colorVars["--color-background-muted"],
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: colorVars["--color-border-emphasized"],
  },
  panelLabel: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
  },
  role: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-primary"],
  },
  hint: {
    fontSize: textSizeVars["--font-size-sm"],
    color: colorVars["--color-text-secondary"],
  },
  failure: {
    fontSize: textSizeVars["--font-size-sm"],
    color: colorVars["--color-error"],
  },
  body: {
    flex: "1 1 auto",
    minBlockSize: 0,
    overscrollBehavior: "contain",
  },
  muted: { opacity: 0.5 },
  netValue: {
    fontFamily: typographyVars["--font-family-body"],
    fontWeight: fontWeightVars["--font-weight-bold"],
    fontSize: textSizeVars["--font-size-base"],
    letterSpacing: "0.04em",
    fontVariantNumeric: "tabular-nums",
    color: colorVars["--color-text-primary"],
    whiteSpace: "nowrap",
  },
  treasury: {
    inlineSize: "100%",
    gridTemplateColumns: "minmax(0, 1.4fr) minmax(0, 1fr) minmax(0, 1fr)",
    columnGap: spacingVars["--spacing-1"],
  },
  defenderTreasury: {
    direction: { default: "rtl", "@media (max-width: 559px)": "ltr" },
  },
  leftToRight: {
    direction: "ltr",
  },
  duelGrid: {
    flex: "0 0 auto",
    gridTemplateColumns: {
      default: "minmax(0, 1fr) auto minmax(0, 1fr)",
      "@media (max-width: 559px)": "minmax(0, 1fr)",
    },
  },
  duelColumn: {
    alignItems: "flex-start",
    minInlineSize: 0,
    padding: spacingVars["--spacing-2"],
  },
  defenderColumn: {
    alignItems: { default: "flex-end", "@media (max-width: 559px)": "flex-start" },
  },
  factionWash: (color: string) => ({ backgroundColor: color }),
  duelUnit: {
    maxInlineSize: "100%",
  },
  axis: {
    position: "relative",
    alignSelf: "stretch",
    inlineSize: { default: spacingVars["--spacing-7"], "@media (max-width: 559px)": "auto" },
    blockSize: { default: "auto", "@media (max-width: 559px)": spacingVars["--spacing-8"] },
  },
  axisRule: {
    position: "absolute",
    insetBlock: { default: 0, "@media (max-width: 559px)": "auto" },
    insetInline: { default: "auto", "@media (max-width: 559px)": 0 },
    insetBlockStart: { default: 0, "@media (max-width: 559px)": "50%" },
    insetInlineStart: { default: "50%", "@media (max-width: 559px)": 0 },
    inlineSize: {
      default: borderVars["--border-width"],
      "@media (max-width: 559px)": "auto",
    },
    blockSize: {
      default: "auto",
      "@media (max-width: 559px)": borderVars["--border-width"],
    },
    backgroundColor: colorVars["--color-border-emphasized"],
  },
  swap: {
    position: "relative",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    inlineSize: spacingVars["--spacing-6"],
    blockSize: spacingVars["--spacing-6"],
    padding: 0,
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: colorVars["--color-background-surface"],
    boxShadow: { default: shadowVars["--shadow-low"], ":active": "none" },
    color: colorVars["--color-text-primary"],
    fontSize: textSizeVars["--font-size-base"],
    cursor: "pointer",
    transform: {
      default: "none",
      ":active": `translate(${spacingVars["--spacing-0-5"]}, ${spacingVars["--spacing-0-5"]})`,
    },
    outline: {
      default: "none",
      ":focus-visible": `${borderVars["--border-width"]} solid ${colorVars["--color-accent"]}`,
    },
    outlineOffset: { default: 0, ":focus-visible": `calc(-1 * ${borderVars["--border-width"]})` },
  },
  duelOutput: {
    flex: "0 0 auto",
    padding: spacingVars["--spacing-2"],
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-muted"],
  },
  duelFigure: { minInlineSize: 0 },
  duelPercent: {
    fontFamily: typographyVars["--font-family-body"],
    fontWeight: fontWeightVars["--font-weight-bold"],
    fontSize: {
      default: textSizeVars["--font-size-xl"],
      "@media (max-width: 559px)": textSizeVars["--font-size-lg"],
    },
    lineHeight: 1.1,
    letterSpacing: 0,
    fontVariantNumeric: "tabular-nums",
    color: colorVars["--color-text-primary"],
    whiteSpace: "nowrap",
  },
  duelLabel: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
    fontVariantNumeric: "tabular-nums",
  },
  duelFunds: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    color: colorVars["--color-text-secondary"],
    fontVariantNumeric: "tabular-nums",
  },
  rungs: {
    listStyle: "none",
    margin: 0,
    padding: 0,
  },
  rungAt: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    fontVariantNumeric: "tabular-nums",
    color: colorVars["--color-text-secondary"],
    whiteSpace: "nowrap",
  },
  rungPercent: {
    fontFamily: typographyVars["--font-family-body"],
    fontWeight: fontWeightVars["--font-weight-bold"],
    fontSize: {
      default: textSizeVars["--font-size-lg"],
      "@media (max-width: 559px)": textSizeVars["--font-size-base"],
    },
    lineHeight: 1.15,
    fontVariantNumeric: "tabular-nums",
    color: colorVars["--color-text-primary"],
    whiteSpace: "nowrap",
  },
  duelNet: {
    flex: "0 0 auto",
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-2"],
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: colorVars["--color-border"],
    backgroundColor: colorVars["--color-background-muted"],
  },
  // The panel as a window the board opened: the standard panel, lifted to the
  // level this system reserves for content that overlays other content. It
  // takes the board's height rather than asking for its own, so the map is
  // never pushed off the frame.
  boardPanel: {
    position: "absolute",
    insetBlockStart: spacingVars["--spacing-2"],
    insetInlineEnd: spacingVars["--spacing-2"],
    inlineSize: `calc(100% - 2 * ${spacingVars["--spacing-2"]})`,
    maxInlineSize: "min(100%, 60rem)",
    maxBlockSize: `calc(100% - 2 * ${spacingVars["--spacing-2"]})`,
    minBlockSize: 0,
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-container"],
    backgroundColor: colorVars["--color-background-surface"],
    boxShadow: shadowVars["--shadow-high"],
    color: colorVars["--color-text-primary"],
    outline: "none",
    overflow: "hidden",
    zIndex: 3,
  },
  sheet: {
    // A sheet takes the whole bottom edge; the stock dialog width cap would
    // leave it floating in the middle of the screen instead.
    maxWidth: "100%",
    borderRadius: `${radiusVars["--radius-container"]} ${radiusVars["--radius-container"]} 0 0`,
    borderBlockEndWidth: 0,
  },
  sheetBody: {
    minBlockSize: 0,
    outline: "none",
    // The BottomSheet handle is out of flow. Reserve its space above the form.
    paddingBlockStart: spacingVars["--spacing-6"],
    // The sheet ends at the bottom edge of the device, not at the bottom edge
    // of the screen.
    paddingBlockEnd: "env(safe-area-inset-bottom)",
  },
  fighterFields: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
    flexWrap: "wrap",
  },
  // The name of a control whose art already says what it is, kept for anyone
  // hearing it read.
  hiddenLabel: {
    position: "absolute",
    inlineSize: 1,
    blockSize: 1,
    padding: 0,
    margin: -1,
    overflow: "hidden",
    clipPath: "inset(50%)",
    whiteSpace: "nowrap",
    borderWidth: 0,
  },
  // Three keys under one outline, parted by the same rule. It is the game's
  // own menu strip: a row of commands with the cursor sitting on one of them.
  powerGroup: {
    display: "flex",
    alignItems: "stretch",
    alignSelf: "flex-start",
    maxInlineSize: "100%",
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: colorVars["--color-background-surface"],
    overflow: "hidden",
  },
  // A disabled command in this game is still a key on the menu, so it keeps
  // its outline and lies flat rather than disappearing.
  powerGroupDisabled: {
    opacity: 0.5,
    cursor: "not-allowed",
  },
  powerKey: {
    display: "flex",
    flex: "0 0 auto",
    alignItems: "center",
    justifyContent: "center",
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-2"],
    borderWidth: 0,
    borderInlineStartWidth: { default: borderVars["--border-width"], ":first-child": 0 },
    borderInlineStartStyle: "solid",
    borderInlineStartColor: colorVars["--color-border-emphasized"],
    backgroundColor: {
      default: "transparent",
      ":hover": { "@media (hover: hover)": colorVars["--color-background-muted"] },
    },
    color: colorVars["--color-text-secondary"],
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    cursor: "pointer",
    outline: {
      default: null,
      ":focus-visible": `2px solid ${colorVars["--color-accent"]}`,
    },
    outlineOffset: { default: null, ":focus-visible": "-2px" },
    ":disabled": { cursor: "not-allowed" },
  },
  powerKeySelected: {
    backgroundColor: {
      default: colorVars["--color-accent"],
      ":hover": { "@media (hover: hover)": colorVars["--color-accent"] },
    },
    color: colorVars["--color-text-primary"],
  },
  sprite: {
    position: "relative",
    display: "block",
    flex: "0 0 auto",
  },
  spriteArt: {
    display: "block",
    flex: "0 0 auto",
  },
  // The four field widths are fixed rather than fluid because the two armies
  // are read across the centreline: a control sized to its own contents would
  // put the defender's health somewhere other than opposite the attacker's.
  //
  // Sized to hold the art, the name and the caret. "Battleship" and "Missile
  // Silo" are the two names that still run past this and lose their last
  // letters to an ellipsis; both stay unmistakable from the sprite beside them,
  // and both are read whole in the grid the trigger opens.
  unitField: {
    inlineSize: `calc(${spacingVars["--spacing-12"]} + ${spacingVars["--spacing-12"]} + ${spacingVars["--spacing-12"]} + ${spacingVars["--spacing-2"]})`,
  },
  // A mark, two digits and a stepper. Health runs one to ten and never wider.
  healthField: {
    inlineSize: `calc(${spacingVars["--spacing-12"]} + ${spacingVars["--spacing-8"]})`,
  },
  terrainField: {
    inlineSize: `calc(${spacingVars["--spacing-12"]} + ${spacingVars["--spacing-12"]} + ${spacingVars["--spacing-12"]} + ${spacingVars["--spacing-6"]})`,
  },
  ammoField: {
    inlineSize: `calc(${spacingVars["--spacing-12"]} + ${spacingVars["--spacing-8"]})`,
  },
});
