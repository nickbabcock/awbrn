import { Button } from "#/ui/Button.tsx";
import { Dialog } from "@astryxdesign/core/Dialog";
import { Grid } from "@astryxdesign/core/Grid";
import { NumberInput } from "@astryxdesign/core/NumberInput";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import {
  borderVars,
  colorVars,
  radiusVars,
  shadowVars,
  spacingVars,
  textSizeVars,
  typographyVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { Close as CloseIcon } from "pixelarticons/react/Close";
import { Plus as PlusIcon } from "pixelarticons/react/Plus";
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
import { getFactionByCode } from "#/factions.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { battleCalculatorLayout } from "./battleCalculatorLayout.stylex.ts";
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
  BattleFighter,
  BattleReportWire,
  BattleRow,
  BattleSide,
  CatalogDomain,
  PlayerRosterSnapshot,
  PowerLevel,
  Terrain,
  UnitKind,
  WeatherKind,
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
 * Its shape is one persistent strip of everything that moves a damage figure —
 * both commanders, both treasuries, both property counts, the towers — and a
 * list of targets underneath that re-scores the instant any of it changes. That
 * is the arrangement the question has: the context is asked once and the
 * targets are compared against it, rather than each row carrying its own copy
 * of an army.
 *
 * Weather is absent from the strip on purpose. It is sent with every request,
 * but no commander in the ruleset gates a firepower or defense rule on it, so
 * offering the player a control would only invite them to click all three and
 * conclude the panel is broken.
 *
 * Every figure is AWVM's. The panel holds no formula, no table of costs and no
 * opinion about whether a trade is good; the numbers are what a player came for
 * and judging them is the thing they came to do.
 */

/** How the panel is drawn, following the input that opened it. */
export type BattleCalculatorPresentation = "board" | "sheet";

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

/** A target list this long already needs scrolling; more is a different tool. */
const MAX_TARGETS = 8;

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
  // Which arrangement is being looked at. Scaffolding: it comes out with the
  // two that lose.
  const [variant, setVariant] = useState<VariantChoice>("duel");
  const [catalog, setCatalog] = useState<BattleCatalog | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [portraits] = useState<CoPortraitCatalog>(() => loadCoPortraitCatalog());

  const seats = useMemo(
    () => seatsFrom(roster, viewerSlotIndex ?? null),
    [roster, viewerSlotIndex],
  );

  // The match's own weather, with no control to change it. No commander in the
  // ruleset gates a firepower or defense rule on weather, so the three settings
  // score identically; `weather_never_moves_a_calculator_result` in
  // `crates/awvm/tests/weather_sweep.rs` holds that. It is still sent rather
  // than assumed, because a later ruleset revision is free to add such a rule.
  const weather: WeatherKind = roster?.weather ?? "clear";
  const [attackerSide, setAttackerSide] = useState<CalculatorSide>(() => sideFrom(seats.attacker));
  const [defenderSide, setDefenderSide] = useState<CalculatorSide>(() => sideFrom(seats.defender));
  const [attacker, setAttacker] = useState<BattleFighter>(() => blankFighter());
  const [targets, setTargets] = useState<BattleFighter[]>(() => [blankFighter()]);
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
  // scores a column of engagements in microseconds, and a figure that lagged a
  // keystroke behind would be a figure a player could read and act on while it
  // was still describing the previous board.
  useEffect(() => {
    const attackerContext = completeSide(attackerSide);
    const defenderContext = completeSide(defenderSide);
    if (!runner || targets.length === 0 || !attackerContext || !defenderContext) {
      setReport(null);
      setReportError(null);
      return;
    }
    let cancelled = false;

    runner
      .forecastBattle({
        weather,
        attacker: attackerContext,
        attackingUnit: attacker,
        defender: defenderContext,
        defendingUnits: targets,
      })
      .then((next) => {
        if (cancelled) return;
        setReport(next);
        setReportError(null);
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setReport(null);
        setReportError(errorMessage(error, "The engagement could not be scored."));
      });

    return () => {
      cancelled = true;
    };
  }, [attacker, attackerSide, defenderSide, runner, targets, weather]);

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
    setAttacker(targets[0] ?? blankFighter());
    setTargets((previous) => [attacker, ...previous.slice(1)]);
  }, [attacker, attackerSide, defenderSide, targets]);

  const addTarget = useCallback(() => {
    setTargets((previous) =>
      previous.length >= MAX_TARGETS
        ? previous
        : [...previous, previous[previous.length - 1] ?? blankFighter()],
    );
  }, []);

  // The catalogues are the ruleset's and do not change while the panel is
  // open; only the colours the sprites are drawn in do. Building them once per
  // army rather than once per row is what keeps a nine-row column from
  // rebuilding two hundred sprites on every keystroke in the strip.
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

  const strip = (
    <ContextStrip
      attacker={attackerSide}
      attackerFaction={attackerFaction}
      attackerName={seats.attacker?.displayFactionName ?? "Attacking army"}
      commanderOptions={commanders}
      defender={defenderSide}
      defenderFaction={defenderFaction}
      defenderName={seats.defender?.displayFactionName ?? "Defending army"}
      isTrailing={presentation === "sheet"}
      onAttackerChange={setAttackerSide}
      onDefenderChange={setDefenderSide}
      portraits={portraits}
    />
  );

  const engagement = (
    <>
      <AttackerRow
        catalog={catalog}
        factionCode={attackerFaction}
        fighter={attacker}
        onChange={setAttacker}
        terrainOptions={terrains}
        unitOptions={attackerUnits}
        value={report?.attackerValue ?? null}
      />

      <TargetTable
        attackerName={unitName(catalog, attacker.unit)}
        catalog={catalog}
        factionCode={defenderFaction}
        onChange={setTargets}
        rows={report?.rows ?? null}
        targets={targets}
        terrainOptions={terrains}
        unitOptions={defenderUnits}
      />
    </>
  );

  const body = (
    <>
      {/* On a board the premise reads first, above the answers it governs. On a
          sheet it reads last: a phone shows one screenful, the premise is
          already filled in from the match, and a player who has to scroll past
          six fields to reach the figure they opened the panel for has been
          handed a form instead of a calculator. It is the same strip either
          way, and it is still always on the panel. */}
      <VStack gap={0} xstyle={styles.body}>
        {presentation === "sheet" ? (
          <>
            {engagement}
            {strip}
          </>
        ) : (
          <>
            {strip}
            {engagement}
          </>
        )}
      </VStack>

      {/* The one command the panel offers, and whatever it has to say about
          itself. It is pinned outside the scrolling body: a list a player is
          adding to must not push the key that adds to it off the panel. */}
      <HStack
        align="center"
        gap={2}
        justify="between"
        paddingBlock={2}
        paddingInline={3}
        wrap="wrap"
        xstyle={styles.footer}
      >
        <Button
          clickAction={addTarget}
          icon={<PlusIcon aria-hidden height={16} width={16} />}
          isDisabled={targets.length >= MAX_TARGETS || catalog === null}
          label="Add target"
          size="sm"
          variant="secondary"
        />
        {/* A failure is a sentence, so it is set in the body voice like every
            other sentence, and in the one red this system has. */}
        {(catalogError ?? reportError) ? (
          <span role="alert" {...stylex.props(styles.failure)}>
            {catalogError ?? reportError}
          </span>
        ) : (
          <Text color="secondary" type="supporting">
            Read the top of Deal with the bottom of Take.
          </Text>
        )}
      </HStack>
    </>
  );

  const original =
    presentation === "sheet" ? (
      <Dialog
        aria-label="Battle calculator"
        isOpen
        maxHeight={battleCalculatorLayout.sheetMaxBlockSize}
        onOpenChange={(isOpen) => {
          if (!isOpen) onDismiss();
        }}
        padding={0}
        position={{ bottom: 0, left: 0, right: 0 }}
        purpose="form"
        width="100%"
        xstyle={styles.sheet}
      >
        <PanelFrame onDismiss={onDismiss} onRestoreFocus={onRestoreFocus}>
          {body}
        </PanelFrame>
      </Dialog>
    ) : (
      <VStack aria-label="Battle calculator" gap={0} role="dialog" xstyle={styles.boardPanel}>
        <PanelFrame onDismiss={onDismiss} onRestoreFocus={onRestoreFocus}>
          {body}
        </PanelFrame>
      </VStack>
    );

  const kit: VariantKit = {
    isSheet: presentation === "sheet",
    addTarget,
    attacker,
    attackerFaction,
    attackerName: seats.attacker?.displayFactionName ?? "Attacking army",
    attackerSide,
    attackerUnits,
    catalog,
    commanders,
    defenderFaction,
    defenderName: seats.defender?.displayFactionName ?? "Defending army",
    defenderSide,
    defenderUnits,
    failure: catalogError ?? reportError,
    onAttackerChange: setAttackerSide,
    onAttackerUnitChange: setAttacker,
    onDefenderChange: setDefenderSide,
    onDismiss,
    onRestoreFocus,
    onSwap: swapSeats,
    onTargetsChange: setTargets,
    portraits,
    report,
    targets,
    terrains,
  };

  return (
    <>
      <style>{VARIANT_CSS}</style>
      <VariantSwitch isSheet={presentation === "sheet"} onChange={setVariant} value={variant} />
      {variant === "original" ? original : <DuelPanel kit={kit} />}
    </>
  );
}

/* ---------------------------- Variant 4 --------------------------- *
 * DUEL. The reference calculator's framing, taken seriously.
 *
 * AWBW's own tool is symmetric: two armies fully specified side by side, a
 * swap between them, and the exchange read across the middle. That is a
 * different question from the one the target list answers — not "which of
 * these six targets is the best trade" but "what happens when these two
 * meet, and what happens when it goes the other way".
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

function DuelPanel({ kit }: { kit: VariantKit }) {
  const row = kit.report?.rows?.[0];
  const result = row?.result;
  const target = kit.targets[0];
  const attackerName = unitName(kit.catalog, kit.attacker.unit);
  const targetName = row?.name ?? (target ? unitName(kit.catalog, target.unit) : "");

  if (!target) return null;

  return (
    <VariantFrame className="bc-panel bc-duel" kit={kit}>
      <div className="bc-body bc-duel-body">
        <div className="bc-duel-grid">
          <DuelColumn kit={kit} role="Attacking" />

          {/* The axis the exchange happens across, and the one command that
              turns it around. It sits on the centreline rather than in the
              head, because swapping is a thing done to the two columns. */}
          <div className="bc-axis">
            <span className="bc-axis-rule" />
            <button
              className="bc-swap"
              onClick={kit.onSwap}
              title="Swap the two armies"
              type="button"
            >
              <span aria-hidden="true">⇄</span>
              <span className="bc-sr">Swap the two armies</span>
            </button>
          </div>

          <DuelColumn kit={kit} role="Defending" />
        </div>

        <span className="bc-sr">
          {engagementLabel(attackerName, targetName, target, result, row?.impossible)}
        </span>

        {result ? (
          <div aria-hidden="true" className="bc-duel-out">
            <div className="bc-duel-figure">
              <span className="bc-duel-word">Deals</span>
              <span className="bc-duel-pct">{formatDamage(result.damage)}</span>
              <span className="bc-duel-funds">{formatFundsBracket(result.valueDealt)}</span>
            </div>

            {/* The reply, split the way the reference tool splits it. One
                range folds two different spreads together — how much of the
                target survives, and the luck it answers with — and a player
                reading 27 – 36% cannot tell which part is which. The rungs
                separate them: one per health it may be left standing in,
                each carrying the luck alone. Below two rungs there is no
                spread to show, so the single range says it better. */}
            <div className="bc-duel-figure bc-duel-take">
              <span className="bc-duel-word">{result.counterFirst ? "Takes 1st" : "Takes"}</span>
              {result.counterSteps.length > 1 ? (
                <ul className="bc-rungs">
                  {result.counterSteps.map((step) => (
                    <li className="bc-rung" key={step.targetHealth}>
                      <span className="bc-rung-at">@{pointsToBars(step.targetHealth)} HP</span>
                      <span className="bc-rung-pct">{formatDamage(step.counter)}</span>
                    </li>
                  ))}
                </ul>
              ) : (
                <span className={result.counter ? "bc-duel-pct" : "bc-duel-pct bc-muted"}>
                  {result.counter ? formatDamage(result.counter) : "—"}
                </span>
              )}
              <span className="bc-duel-funds">
                {result.valueTaken ? formatFundsBracket(result.valueTaken) : "—"}
              </span>
            </div>
          </div>
        ) : (
          <div className="bc-duel-out">
            <span className="bc-impossible">
              {row ? impossibleLabel(row.impossible ?? "no-weapon") : "Scoring…"}
            </span>
          </div>
        )}

        <div className="bc-duel-net">
          <span className="bc-duel-net-label">Net</span>
          <span className="bc-net-value">{result ? formatNet(result.net) : "—"}</span>
          {result?.destroys ? (
            <span className="bc-outcome">Destroys</span>
          ) : result?.mayDestroy ? (
            <span className="bc-outcome">May destroy</span>
          ) : null}
        </div>
      </div>
    </VariantFrame>
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
function DuelColumn({ kit, role }: { kit: VariantKit; role: "Attacking" | "Defending" }) {
  const isAttacker = role === "Attacking";
  const side = isAttacker ? kit.attackerSide : kit.defenderSide;
  const onChange = isAttacker ? kit.onAttackerChange : kit.onDefenderChange;
  const factionCode = isAttacker ? kit.attackerFaction : kit.defenderFaction;
  const fighter = isAttacker ? kit.attacker : (kit.targets[0] ?? blankFighter());

  return (
    <section
      className="bc-duel-col"
      style={{ backgroundColor: `var(--color-faction-${factionCode}-wash, transparent)` }}
    >
      <span className="bc-role">{isAttacker ? "Attacker" : "Defender"}</span>

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

      <TreasuryFields onChange={onChange} role={role} side={side} />

      <div className="bc-duel-unit">
        <FighterFields
          catalog={kit.catalog}
          factionCode={factionCode}
          fighter={fighter}
          onChange={(next) =>
            isAttacker
              ? kit.onAttackerUnitChange(next)
              : kit.onTargetsChange((previous) =>
                  previous.map((item, at) => (at === 0 ? next : item)),
                )
          }
          role={isAttacker ? "Attacker" : "Target"}
          terrainOptions={kit.terrains}
          unitOptions={isAttacker ? kit.attackerUnits : kit.defenderUnits}
        />
      </div>
    </section>
  );
}

/** Which arrangement of the calculator is on screen. */
type VariantChoice = "original" | "duel";

const VARIANT_KEYS: { label: string; value: VariantChoice }[] = [
  { value: "original", label: "Current" },
  { value: "duel", label: "Duel" },
];

/**
 * The comparison control, for as long as there is something to compare.
 *
 * Scaffolding rather than product: four keys in the game's own menu strip,
 * sitting beside the panel so the arrangement under it can be swapped without
 * losing the engagement the player configured. It comes out with the three
 * arrangements that lose.
 */
function VariantSwitch({
  isSheet,
  onChange,
  value,
}: {
  isSheet: boolean;
  onChange: (value: VariantChoice) => void;
  value: VariantChoice;
}) {
  const keys = useRef<(HTMLButtonElement | null)[]>([]);
  const at = VARIANT_KEYS.findIndex((key) => key.value === value);

  return (
    <div
      aria-label="Calculator arrangement"
      className={isSheet ? "bc-switch bc-switch-sheet" : "bc-switch"}
      onKeyDown={(event) => {
        const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
        if (step === 0) return;
        event.preventDefault();
        const next = (at + step + VARIANT_KEYS.length) % VARIANT_KEYS.length;
        const key = VARIANT_KEYS[next];
        if (!key) return;
        onChange(key.value);
        keys.current[next]?.focus();
      }}
      role="radiogroup"
    >
      {VARIANT_KEYS.map((key, index) => (
        <button
          aria-checked={key.value === value}
          className={key.value === value ? "bc-switch-key bc-switch-on" : "bc-switch-key"}
          key={key.value}
          onClick={() => onChange(key.value)}
          ref={(element) => {
            keys.current[index] = element;
          }}
          role="radio"
          tabIndex={key.value === value ? 0 : -1}
          type="button"
        >
          {key.label}
        </button>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------------ *
 * Live variants. Everything below this line is preview scaffolding:
 * three arrangements of the same state, the same controls and the same
 * AWVM figures. One survives; the rest come out with the wrapper.
 * ------------------------------------------------------------------ */

/** Everything a variant arrangement needs, gathered once. */
interface VariantKit {
  addTarget: () => void;
  attacker: BattleFighter;
  attackerFaction: string;
  attackerName: string;
  attackerSide: CalculatorSide;
  attackerUnits: SpritePickerOption[];
  catalog: BattleCatalog | null;
  commanders: SpritePickerOption[];
  defenderFaction: string;
  defenderName: string;
  defenderSide: CalculatorSide;
  defenderUnits: SpritePickerOption[];
  failure: string | null;
  /** Whether the panel is a sheet on the viewport rather than a window on the board. */
  isSheet: boolean;
  onAttackerChange: (side: CalculatorSide) => void;
  onAttackerUnitChange: (fighter: BattleFighter) => void;
  onDefenderChange: (side: CalculatorSide) => void;
  onDismiss: () => void;
  onRestoreFocus: () => void;
  /** Turn the engagement around: both armies change seats at once. */
  onSwap: () => void;
  onTargetsChange: (update: (previous: BattleFighter[]) => BattleFighter[]) => void;
  portraits: CoPortraitCatalog;
  report: BattleReportWire | null;
  targets: BattleFighter[];
  terrains: SpritePickerOption[];
}

/**
 * The frame every arrangement keeps: the head, the one way out, the key that
 * adds a target, and the keyboard contract the board menus already hold.
 */
function VariantFrame({
  children,
  className,
  kit,
  params,
}: {
  children: React.ReactNode;
  className: string;
  kit: VariantKit;
  /** The arrangement's tunable axes, read by its own scoped rules. */
  params?: Record<string, string>;
}) {
  useEffect(() => {
    return () => {
      if (document.activeElement === null || document.activeElement === document.body) {
        kit.onRestoreFocus();
      }
    };
    // The restore runs once, when the panel goes away.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      aria-label="Battle calculator"
      className={kit.isSheet ? `${className} bc-sheet` : className}
      {...params}
      data-autofocus
      onKeyDown={(event) => {
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
    >
      <div className="bc-head">
        <span className="bc-title">Battle calculator</span>
        <Button
          clickAction={kit.onDismiss}
          icon={<CloseIcon aria-hidden height={16} width={16} />}
          label="Close"
          size="sm"
          variant="secondary"
        />
      </div>

      {children}

      <div className="bc-foot">
        <Button
          clickAction={kit.addTarget}
          icon={<PlusIcon aria-hidden height={16} width={16} />}
          isDisabled={kit.targets.length >= MAX_TARGETS || kit.catalog === null}
          label="Add target"
          size="sm"
          variant="secondary"
        />
        {kit.failure ? (
          <span className="bc-failure" role="alert">
            {kit.failure}
          </span>
        ) : (
          <span className="bc-hint">Read the top of Deal with the bottom of Take.</span>
        )}
      </div>
    </div>
  );
}

/** One army's premise, on a single line, for the arrangements that want it there. */
function TreasuryFields({
  onChange,
  role,
  side,
}: {
  onChange: (side: CalculatorSide) => void;
  role: string;
  side: CalculatorSide;
}) {
  return (
    <div className="bc-treasury">
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
    </div>
  );
}

const VARIANT_CSS = `
/* The comparison control. Three keys under one outline with the cursor on
   one of them, the way the game's own menus do it — pinned to the board's
   inline start so it never sits under the panel it switches. */
.bc-switch {
  position: absolute;
  inset-block-start: var(--spacing-2);
  inset-inline-start: var(--spacing-2);
  display: flex;
  align-items: stretch;
  border: var(--border-width) solid var(--color-border-emphasized);
  border-radius: var(--radius-element);
  background-color: var(--color-background-surface);
  box-shadow: var(--shadow-low);
  overflow: hidden;
  z-index: 4;
}
.bc-switch-sheet {
  position: fixed;
  inset-block-start: auto;
  inset-block-end: calc(min(86svh, 46rem) + var(--spacing-2) + env(safe-area-inset-bottom));
  inset-inline-start: 50%;
  transform: translateX(-50%);
  z-index: 41;
}
.bc-switch-key {
  display: flex;
  align-items: center;
  padding: var(--spacing-1) var(--spacing-2);
  border: 0;
  border-inline-start: var(--border-width) solid var(--color-border-emphasized);
  background-color: transparent;
  color: var(--color-text-secondary);
  font-family: var(--font-family-code);
  font-size: var(--font-size-sm);
  letter-spacing: 0.06em;
  text-transform: uppercase;
  cursor: pointer;
}
.bc-switch-key:first-child { border-inline-start: 0; }
@media (hover: hover) {
  .bc-switch-key:hover { background-color: var(--color-background-muted); }
}
.bc-switch-key:focus-visible { outline: 2px solid var(--color-accent); outline-offset: -2px; }
.bc-switch-on,
.bc-switch-on:hover {
  background-color: var(--color-accent);
  color: var(--color-text-primary);
}

/* Shared: the panel shell every arrangement keeps, straight from the
   committed system — one ink outline, the hard step over its soft blur,
   4px corners, the HUD face for every readout. */
.bc-panel {
  position: absolute;
  inset-block-start: var(--spacing-2);
  inset-inline-end: var(--spacing-2);
  inline-size: calc(100% - 2 * var(--spacing-2));
  max-inline-size: min(100%, 60rem);
  max-block-size: calc(100% - 2 * var(--spacing-2));
  display: flex;
  flex-direction: column;
  min-block-size: 0;
  border: var(--border-width) solid var(--color-border-emphasized);
  border-radius: var(--radius-container);
  background-color: var(--color-background-surface);
  box-shadow: var(--shadow-high);
  color: var(--color-text-primary);
  overflow: hidden;
  z-index: 3;
}
/* The other presentation the brief calls for: when the board frame cannot
   hold the panel, it is a sheet on the viewport rather than a window on the
   map. Same panel, different anchor. */
.bc-panel.bc-sheet {
  position: fixed;
  inset-block-start: auto;
  inset-block-end: 0;
  inset-inline: 0;
  inline-size: 100%;
  max-inline-size: 100%;
  max-block-size: min(86svh, 46rem);
  border-radius: var(--radius-container) var(--radius-container) 0 0;
  border-block-end-width: 0;
  padding-block-end: env(safe-area-inset-bottom);
  z-index: 40;
}

.bc-head,
.bc-foot {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--spacing-2);
  flex-wrap: wrap;
  padding: var(--spacing-1) var(--spacing-2);
  background-color: var(--color-background-muted);
}
.bc-head {
  border-block-end: var(--border-width) solid var(--color-border-emphasized);
}
.bc-foot {
  border-block-start: var(--border-width) solid var(--color-border-emphasized);
}
.bc-title,
.bc-role,
.bc-outcome,
.bc-impossible,
.bc-attacker-value {
  font-family: var(--font-family-code);
  font-size: var(--font-size-sm);
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--color-text-secondary);
}
.bc-role { color: var(--color-text-primary); }
.bc-attacker-value {
  margin-inline-start: auto;
  font-variant-numeric: tabular-nums;
  text-transform: none;
}
.bc-hint {
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}
.bc-failure {
  font-size: var(--font-size-sm);
  color: var(--color-error);
}
.bc-body {
  flex: 1 1 auto;
  min-block-size: 0;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  overscroll-behavior: contain;
}
.bc-muted { opacity: 0.5; }
.bc-net-value {
  font-family: var(--font-family-body);
  font-weight: 700;
  font-size: var(--font-size-base);
  letter-spacing: 0.04em;
  font-variant-numeric: tabular-nums;
  color: var(--color-text-primary);
  white-space: nowrap;
}
.bc-sr {
  position: absolute;
  inline-size: 1px;
  block-size: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
  border-width: 0;
}
.bc-treasury {
  display: grid;
  grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr) minmax(0, 1fr);
  column-gap: var(--spacing-2);
  row-gap: var(--spacing-1);
  align-items: center;
}

/* ---- Variant 4: DUEL ---- */
.bc-duel-body { display: flex; flex-direction: column; }
.bc-duel-grid {
  flex: 0 0 auto;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: start;
}
.bc-duel-col {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--spacing-1);
  min-inline-size: 0;
  padding: var(--spacing-2);
}
/* The defending column reads toward the axis, so the two armies face each
   other the way the columns in the reference tool do. */
.bc-duel-grid .bc-duel-col:last-child { align-items: flex-end; }
.bc-duel-grid .bc-duel-col:last-child .bc-treasury { direction: rtl; }
.bc-duel-grid .bc-duel-col:last-child .bc-treasury > * { direction: ltr; }
.bc-duel-col .bc-treasury { inline-size: 100%; column-gap: var(--spacing-1); }
.bc-duel-unit { display: flex; flex-wrap: wrap; gap: var(--spacing-1); max-inline-size: 100%; }
/* The centreline. The exchange is read across it and the swap sits on it. */
.bc-axis {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  align-self: stretch;
  inline-size: var(--spacing-7);
}
.bc-axis-rule {
  position: absolute;
  inset-block: 0;
  inset-inline-start: 50%;
  inline-size: var(--border-width);
  background-color: var(--color-border-emphasized);
}
.bc-swap {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  inline-size: var(--spacing-6);
  block-size: var(--spacing-6);
  padding: 0;
  border: var(--border-width) solid var(--color-border-emphasized);
  border-radius: var(--radius-element);
  background-color: var(--color-background-surface);
  box-shadow: var(--shadow-low);
  color: var(--color-text-primary);
  font-size: var(--font-size-base);
  cursor: pointer;
}
.bc-swap:active {
  transform: translate(2px, 2px);
  box-shadow: none;
}
.bc-swap:focus-visible { outline: 2px solid var(--color-accent); outline-offset: -2px; }
/* The figures are the reason the panel is open, so they are the largest
   thing on it. A bitmap face has a floor, not a ceiling. */
.bc-duel-out {
  flex: 0 0 auto;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  align-items: start;
  gap: var(--spacing-2);
  padding: var(--spacing-2);
  border-block-start: var(--border-width) solid var(--color-border-emphasized);
  background-color: var(--color-background-muted);
}
.bc-duel-figure {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  min-inline-size: 0;
}
/* Silkscreen is the HUD face and it is a bitmap: it reads at 12px because
   that is the size it was drawn at, and enlarging it enlarges the jaggedness
   rather than the legibility. The system already makes this move for its
   signage face at title sizes — the body face carries them at full weight —
   and a figure this size needs the same. The HUD face keeps the labels, which
   are at the size it was drawn for. */
.bc-duel-pct {
  font-family: var(--font-family-body);
  font-weight: 700;
  font-size: var(--font-size-xl);
  line-height: 1.1;
  letter-spacing: 0;
  font-variant-numeric: tabular-nums;
  color: var(--color-text-primary);
  white-space: nowrap;
}
.bc-duel-word,
.bc-duel-funds,
.bc-duel-net-label {
  font-family: var(--font-family-code);
  font-size: var(--font-size-sm);
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--color-text-secondary);
  font-variant-numeric: tabular-nums;
}
.bc-duel-funds { text-transform: none; }
/* The rungs of the reply. Each names the health it is answered from, so the
   two spreads a single range folds together can be told apart. */
.bc-rungs {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.bc-rung {
  display: flex;
  align-items: baseline;
  justify-content: center;
  gap: var(--spacing-2);
}
.bc-rung-at {
  font-family: var(--font-family-code);
  font-size: var(--font-size-sm);
  letter-spacing: 0.06em;
  text-transform: uppercase;
  font-variant-numeric: tabular-nums;
  color: var(--color-text-secondary);
  white-space: nowrap;
}
.bc-rung-pct {
  font-family: var(--font-family-body);
  font-weight: 700;
  font-size: var(--font-size-lg);
  line-height: 1.15;
  letter-spacing: 0;
  font-variant-numeric: tabular-nums;
  color: var(--color-text-primary);
  white-space: nowrap;
}
.bc-duel-net {
  flex: 0 0 auto;
  display: flex;
  align-items: baseline;
  justify-content: center;
  gap: var(--spacing-2);
  flex-wrap: wrap;
  padding: var(--spacing-1) var(--spacing-2);
  border-block-start: var(--border-width) solid var(--color-border-soft);
  background-color: var(--color-background-muted);
}
@media (max-width: 559px) {
  .bc-duel-grid { grid-template-columns: minmax(0, 1fr); }
  .bc-duel-grid .bc-duel-col:last-child { align-items: flex-start; }
  .bc-duel-grid .bc-duel-col:last-child .bc-treasury { direction: ltr; }
  .bc-axis { inline-size: auto; block-size: var(--spacing-8); }
  .bc-axis-rule {
    inset-block: auto;
    inset-inline: 0;
    inset-block-start: 50%;
    block-size: var(--border-width);
    inline-size: auto;
  }
  .bc-duel-pct { font-size: var(--font-size-lg); }
  .bc-rung-pct { font-size: var(--font-size-base); }
}
`;

/**
 * The frame both presentations share: a titled head, one way out, and the
 * keyboard contract the board menus already keep.
 *
 * Escape closes and the board takes its keyboard back, so a player who opened
 * the panel mid-turn is exactly where they were when it goes away.
 */
function PanelFrame({
  children,
  onDismiss,
  onRestoreFocus,
}: {
  children: React.ReactNode;
  onDismiss: () => void;
  onRestoreFocus: () => void;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    return () => {
      if (document.activeElement === null || document.activeElement === document.body) {
        onRestoreFocus();
      }
    };
    // The restore runs once, when the panel goes away.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <VStack
      data-autofocus
      gap={0}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        // A picker, a list or a menu standing over the panel owns Escape while
        // it is open: a player dismissing the unit grid is asking to close the
        // grid, and taking the whole calculator away with it would throw out
        // the engagement they spent four controls describing. Everything that
        // opens over the panel opens in the top layer, and a control holding
        // one open says so on itself, so the two together are the whole test:
        // the key landed inside a layer, or on the trigger that opened it.
        if (
          event.target instanceof Element &&
          event.target.closest('[popover], [aria-expanded="true"]')
        ) {
          return;
        }
        event.preventDefault();
        onDismiss();
      }}
      tabIndex={-1}
      xstyle={styles.frame}
    >
      <HStack
        align="center"
        gap={3}
        justify="between"
        paddingBlock={2}
        paddingInline={3}
        xstyle={styles.head}
      >
        <span {...stylex.props(styles.title)}>Battle calculator</span>
        <Button
          clickAction={onDismiss}
          icon={<CloseIcon aria-hidden height={16} width={16} />}
          label="Close"
          ref={closeRef}
          size="sm"
          variant="secondary"
        />
      </HStack>
      {children}
    </VStack>
  );
}

/**
 * Everything that moves a damage figure and is not a unit.
 *
 * It stays on screen while the targets change, because it is the question's
 * premise rather than one of its answers: a player edits a tower count once and
 * watches the whole column move, which is the thing a per-row picker could
 * never show them.
 */
function ContextStrip({
  attacker,
  attackerFaction,
  attackerName,
  commanderOptions,
  defender,
  defenderFaction,
  defenderName,
  isTrailing,
  onAttackerChange,
  onDefenderChange,
  portraits,
}: {
  attacker: CalculatorSide;
  attackerFaction: string;
  attackerName: string;
  commanderOptions: SpritePickerOption[];
  defender: CalculatorSide;
  defenderFaction: string;
  defenderName: string;
  /** Whether the strip reads after the engagement rather than before it. */
  isTrailing: boolean;
  onAttackerChange: (side: CalculatorSide) => void;
  onDefenderChange: (side: CalculatorSide) => void;
  portraits: CoPortraitCatalog;
}) {
  return (
    <VStack gap={0} xstyle={[styles.strip, isTrailing && styles.stripTrailing]}>
      <Grid gap={0} xstyle={styles.stripColumns}>
        <SideFields
          armyName={attackerName}
          commanderOptions={commanderOptions}
          factionCode={attackerFaction}
          onChange={onAttackerChange}
          portraits={portraits}
          role="Attacking"
          side={attacker}
        />
        <SideFields
          armyName={defenderName}
          commanderOptions={commanderOptions}
          factionCode={defenderFaction}
          onChange={onDefenderChange}
          portraits={portraits}
          role="Defending"
          side={defender}
        />
      </Grid>
    </VStack>
  );
}

/**
 * One army's half of the strip.
 *
 * The army is named in words beside the colour its sprites wear, because a
 * faction that identified itself by hue alone would leave two seats
 * indistinguishable to a reader who cannot separate them.
 */
function SideFields({
  armyName,
  commanderOptions,
  factionCode,
  onChange,
  portraits,
  role,
  side,
}: {
  armyName: string;
  commanderOptions: SpritePickerOption[];
  factionCode: string;
  onChange: (side: CalculatorSide) => void;
  portraits: CoPortraitCatalog;
  role: "Attacking" | "Defending";
  side: CalculatorSide;
}) {
  const accent = getFactionByCode(factionCode);
  const power: PowerLevel | "d2d" = side.power ?? "d2d";

  return (
    <VStack gap={2} paddingBlock={3} paddingInline={3} xstyle={styles.sideColumn}>
      <HStack align="center" gap={2} justify="between">
        <span {...stylex.props(styles.sideRole)}>{role}</span>
        <span {...stylex.props(styles.sideArmy)}>{accent?.displayName ?? armyName}</span>
      </HStack>

      <VStack gap={2} xstyle={styles.sideControls}>
        {/* A board that reports a seat with no commander is shown as it is,
            and the picker names that state without offering it. Choosing from
            the grid always leaves a commander in the seat. */}
        <SpritePicker
          label={`${role} commander`}
          onChange={(value) =>
            onChange({ ...side, commander: value as CalculatorSide["commander"] })
          }
          options={commanderOptions}
          shape="commander"
          triggerArt={
            <CoPortrait
              catalog={portraits}
              coKey={side.commander ?? "no-co"}
              fallbackLabel={`${role} commander`}
              hasFrame={false}
              size={32}
            />
          }
          triggerLabel={side.commander === undefined ? "No CO" : undefined}
          value={side.commander ?? NO_COMMANDER}
        />

        {/* The two powers wear the game's own lettering rather than the
            abbreviations a spreadsheet uses. A player has read POWER and SUPER
            across the screen every time either one fired; COP and SCOP are
            what the forums call them afterwards. Day-to-day has no banner
            because it is the absence of one, so it stays a word. */}
        <PowerSelect
          isDisabled={side.commander === undefined}
          label={`${role} power`}
          onChange={(next) => onChange({ ...side, power: next })}
          value={power}
        />
      </VStack>

      {/* Three treasury figures in one fixed grid, so the two armies' numbers
          line up across the divider and can be compared without reading them.

          Each says what it is with the icon the roster already uses for it —
          the coin, the building, the tower — standing inside the field rather
          than in a row of words above it. A player has been reading these
          three marks on every army panel since they arrived, and a written
          label bought two lines of height to repeat what the coin says. The
          name is still on the field for anyone hearing it read. */}
      <Grid gap={0} xstyle={styles.treasuryGrid}>
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
        {/* A tower is a property, so the count includes it. Bounding each field
            by the other is what keeps the pair from describing an army holding
            three towers and one building. */}
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
      </Grid>
    </VStack>
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
    <div
      aria-disabled={isDisabled || undefined}
      aria-label={label}
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
      {...stylex.props(styles.powerGroup, isDisabled && styles.powerGroupDisabled)}
      title={isDisabled ? "Choose a commander before running a power." : undefined}
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
          <span {...stylex.props(styles.rowSummary)}>{key.label}</span>
        </button>
      ))}
    </div>
  );
}

/**
 * The unit doing the attacking, named once.
 *
 * It sits between the strip and the list because it belongs to both: it is the
 * last thing the player configures and the subject of every row beneath it.
 * Repeating it per row would spend the width twice on a fact that does not
 * change down the column.
 */
function AttackerRow({
  catalog,
  factionCode,
  fighter,
  onChange,
  terrainOptions,
  unitOptions,
  value,
}: {
  catalog: BattleCatalog | null;
  factionCode: string;
  fighter: BattleFighter;
  onChange: (fighter: BattleFighter) => void;
  terrainOptions: SpritePickerOption[];
  unitOptions: SpritePickerOption[];
  value: number | null;
}) {
  return (
    <HStack
      align="center"
      gap={3}
      paddingBlock={2}
      paddingInline={3}
      wrap="wrap"
      xstyle={styles.attackerRow}
    >
      <span {...stylex.props(styles.stripLabel)}>Attacker</span>
      <FighterFields
        catalog={catalog}
        factionCode={factionCode}
        fighter={fighter}
        onChange={onChange}
        role="Attacker"
        terrainOptions={terrainOptions}
        unitOptions={unitOptions}
      />
      {value === null ? null : (
        <span {...stylex.props(styles.attackerValue)}>{formatFunds(value)} funds</span>
      )}
    </HStack>
  );
}

/**
 * The targets, and what the exchange with each costs both sides.
 *
 * Four columns, and the two lines of a row are read across rather than down:
 * the first is damage in the percentage points AWVM works in, the second the
 * same two facts priced in funds. Net spans them both because it is what the
 * pair adds up to, and it is the one figure the board's own forecast cannot
 * give — a percentage cannot say whether 60% of a Mega Tank is worth 90% of a
 * Recon.
 */
function TargetTable({
  attackerName,
  catalog,
  factionCode,
  onChange,
  rows,
  targets,
  terrainOptions,
  unitOptions,
}: {
  attackerName: string;
  catalog: BattleCatalog | null;
  factionCode: string;
  onChange: (update: (previous: BattleFighter[]) => BattleFighter[]) => void;
  rows: BattleRow[] | null;
  targets: BattleFighter[];
  terrainOptions: SpritePickerOption[];
  unitOptions: SpritePickerOption[];
}) {
  return (
    <VStack gap={0} xstyle={styles.table}>
      {/* The header is built out of the same two parts a row is, so the two
          wrap together: where a narrow panel drops the figures below the
          target, the column names go with them and stay over their own
          numbers. */}
      <HStack gap={0} xstyle={styles.headerRow} aria-hidden="true">
        <span {...stylex.props(styles.headerCell)}>Target</span>
        <HStack gap={0} xstyle={styles.figures}>
          <span {...stylex.props(styles.headerFigure, styles.headerDamage)}>Damage</span>
          <span {...stylex.props(styles.headerFigure, styles.headerFunds)}>Funds</span>
          <span {...stylex.props(styles.headerFigure, styles.headerNet)}>Net</span>
        </HStack>
      </HStack>

      <VStack gap={0} xstyle={styles.rows}>
        {targets.map((target, index) => (
          <TargetRow
            attackerName={attackerName}
            catalog={catalog}
            factionCode={factionCode}
            // Targets are ordered and interchangeable, and two identical ones
            // are a real thing to ask about, so position is the only identity
            // a row has.
            key={index}
            onChange={(next) =>
              onChange((previous) => previous.map((item, at) => (at === index ? next : item)))
            }
            onRemove={
              targets.length > 1
                ? () => onChange((previous) => previous.filter((_, at) => at !== index))
                : undefined
            }
            row={rows?.[index]}
            target={target}
            terrainOptions={terrainOptions}
            unitOptions={unitOptions}
          />
        ))}
      </VStack>
    </VStack>
  );
}

function TargetRow({
  attackerName,
  catalog,
  factionCode,
  onChange,
  onRemove,
  row,
  target,
  terrainOptions,
  unitOptions,
}: {
  attackerName: string;
  catalog: BattleCatalog | null;
  factionCode: string;
  onChange: (fighter: BattleFighter) => void;
  onRemove?: () => void;
  row: BattleRow | undefined;
  target: BattleFighter;
  terrainOptions: SpritePickerOption[];
  unitOptions: SpritePickerOption[];
}) {
  const result = row?.result;
  const targetName = row?.name ?? unitName(catalog, target.unit);

  return (
    <HStack gap={0} xstyle={styles.row}>
      <HStack gap={0} xstyle={styles.targetCell}>
        <FighterFields
          catalog={catalog}
          factionCode={factionCode}
          fighter={target}
          onChange={onChange}
          role="Target"
          terrainOptions={terrainOptions}
          unitOptions={unitOptions}
        />
        {onRemove ? (
          <Button
            clickAction={onRemove}
            icon={<CloseIcon aria-hidden height={14} width={14} />}
            label={`Remove ${targetName}`}
            size="sm"
            variant="ghost"
          />
        ) : null}
      </HStack>

      {/* The whole prediction as one sentence, for a reader who gets the row
          rather than its columns. It carries the pairing the layout cannot. */}
      <span {...stylex.props(styles.rowSummary)}>
        {engagementLabel(attackerName, targetName, target, result, row?.impossible)}
      </span>

      <HStack gap={0} xstyle={styles.figures} aria-hidden="true">
        {result ? (
          <>
            <VStack gap={0} xstyle={styles.figureCell}>
              <Figure label="Deal" value={formatDamage(result.damage)} />
              <Figure
                label={result.counterFirst ? "Take 1st" : "Take"}
                muted={!result.counter}
                value={result.counter ? formatDamage(result.counter) : "—"}
              />
            </VStack>
            {/* The same two facts, priced. They carry no labels of their own:
                they sit on the same two lines as the damage beside them, in the
                same order, so the labels there name both columns. Repeating
                Deal and Take here would name each fact twice on one row. */}
            <VStack gap={0} xstyle={styles.fundsCell}>
              <span {...stylex.props(styles.figureValue)}>
                {formatFundsBracket(result.valueDealt)}
              </span>
              <span
                {...stylex.props(styles.figureValue, !result.valueTaken && styles.figureValueMuted)}
              >
                {result.valueTaken ? formatFundsBracket(result.valueTaken) : "—"}
              </span>
            </VStack>
            <VStack gap={0} xstyle={styles.netCell}>
              <span {...stylex.props(styles.netValue)}>{formatNet(result.net)}</span>
              {result.destroys ? (
                <span {...stylex.props(styles.outcome)}>Destroys</span>
              ) : result.mayDestroy ? (
                <span {...stylex.props(styles.outcome)}>May destroy</span>
              ) : null}
            </VStack>
          </>
        ) : (
          <span {...stylex.props(styles.impossibleCell)}>
            {row ? impossibleLabel(row.impossible ?? "no-weapon") : "Scoring…"}
          </span>
        )}
      </HStack>
    </HStack>
  );
}

/** One labelled figure, hung in the column its label reserves. */
function Figure({ label, muted, value }: { label: string; muted?: boolean; value: string }) {
  return (
    <span {...stylex.props(styles.figureLine)}>
      <span {...stylex.props(styles.figureLabel)}>{label}</span>
      <span {...stylex.props(styles.figureValue, muted && styles.figureValueMuted)}>{value}</span>
    </span>
  );
}

/**
 * A unit, its condition, and the ground under it.
 *
 * The same four controls for the attacker and for every target, because they
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
          down the entire column for the sake of eight tiles that have one. */}
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
 * What each picker may offer, built once per army rather than per row.
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

function errorMessage(error: unknown, fallback: string): string {
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    return error.message;
  }
  return error instanceof Error ? error.message : fallback;
}

function unitName(catalog: BattleCatalog | null, unit: UnitKind): string {
  return unitEntry(catalog?.units ?? [], unit)?.name ?? unit;
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

const styles = stylex.create({
  // The panel is a window the board opened, at the level this system reserves
  // for content that overlays other content. It takes the board's height rather
  // than asking for its own, so the map is never pushed off the frame.
  boardPanel: {
    position: "absolute",
    insetBlockStart: battleCalculatorLayout.panelInset,
    maxBlockSize: `calc(100% - 2 * ${battleCalculatorLayout.panelInset})`,
    insetInlineEnd: battleCalculatorLayout.panelInset,
    inlineSize: `calc(100% - 2 * ${battleCalculatorLayout.panelInset})`,
    maxInlineSize: battleCalculatorLayout.panelMaxInlineSize,
    borderWidth: borderVars["--border-width"],
    borderStyle: "solid",
    borderColor: colorVars["--color-border-emphasized"],
    borderRadius: radiusVars["--radius-container"],
    backgroundColor: colorVars["--color-background-surface"],
    boxShadow: shadowVars["--shadow-high"],
    color: colorVars["--color-text-primary"],
    overflow: "hidden",
    zIndex: 3,
  },
  sheet: {
    maxWidth: "100%",
    borderRadius: battleCalculatorLayout.sheetBorderRadius,
    borderBlockEndWidth: 0,
    // The board behind the sheet is dimmed, not defocused. A blurred backdrop
    // is the one soft edge this system does not have anywhere else.
    "::backdrop": { backdropFilter: "none" },
  },
  frame: {
    minBlockSize: 0,
    blockSize: "100%",
    outline: "none",
    paddingBlockEnd: "env(safe-area-inset-bottom)",
  },
  head: {
    flex: "0 0 auto",
    borderBlockEndWidth: borderVars["--border-width"],
    borderBlockEndStyle: "solid",
    borderBlockEndColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-muted"],
  },
  title: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
  },
  body: {
    minBlockSize: 0,
    flex: "1 1 auto",
    overflowY: "auto",
    overscrollBehavior: "contain",
  },
  // The premise, held above the answers. It divides from the list with the
  // frame's own rule rather than the soft one rows use, because the boundary
  // between what is being asked and what is being answered is the strongest
  // division on the panel.
  strip: {
    flex: "0 0 auto",
    borderBlockEndWidth: borderVars["--border-width"],
    borderBlockEndStyle: "solid",
    borderBlockEndColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-muted"],
  },
  // Reading last, the strip divides from what is above it rather than below.
  stripTrailing: {
    borderBlockEndWidth: 0,
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: colorVars["--color-border-emphasized"],
  },
  stripColumns: {
    display: "grid",
    gridTemplateColumns: {
      default: "minmax(0, 1fr)",
      [battleCalculatorLayout.stripMedia]: "minmax(0, 1fr) minmax(0, 1fr)",
    },
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: awbrnVars.colorBorderSoft,
  },
  // The two armies divide from each other, and the divider moves from the
  // inline edge to the block edge when they stack.
  sideColumn: {
    minInlineSize: 0,
    borderBlockStartWidth: {
      default: borderVars["--border-width"],
      [battleCalculatorLayout.stripMedia]: 0,
    },
    borderBlockStartStyle: "solid",
    borderBlockStartColor: awbrnVars.colorBorderSoft,
    borderInlineStartWidth: {
      default: 0,
      [battleCalculatorLayout.stripMedia]: borderVars["--border-width"],
    },
    borderInlineStartStyle: "solid",
    borderInlineStartColor: awbrnVars.colorBorderSoft,
    ":first-child": {
      borderBlockStartWidth: 0,
      borderInlineStartWidth: 0,
    },
  },
  sideControls: {
    flex: "1 1 auto",
    minInlineSize: 0,
  },
  sideRole: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-primary"],
  },
  // The army's own name, so a seat is never told apart by its colour alone.
  sideArmy: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  stripLabel: {
    flex: "0 0 auto",
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
  },
  // Funds are the widest of the three and the only one that runs past four
  // digits, so it takes the room and the two counts share what is left.
  treasuryGrid: {
    display: "grid",
    gridTemplateColumns: "minmax(0, 1.4fr) minmax(0, 1fr) minmax(0, 1fr)",
    columnGap: spacingVars["--spacing-2"],
    rowGap: spacingVars["--spacing-1"],
    alignItems: "center",
  },
  failure: {
    fontSize: textSizeVars["--font-size-sm"],
    color: colorVars["--color-error"],
  },
  fieldLabel: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  attackerRow: {
    flex: "0 0 auto",
    borderBlockEndWidth: borderVars["--border-width"],
    borderBlockEndStyle: "solid",
    borderBlockEndColor: colorVars["--color-border-emphasized"],
  },
  attackerValue: {
    marginInlineStart: "auto",
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    fontVariantNumeric: "tabular-nums",
    color: colorVars["--color-text-secondary"],
  },
  // Nothing inside the scrolling body may shrink below its own content: these
  // are stacked sections in a column that scrolls, and a section allowed to
  // compress paints over the one beneath it rather than making the body taller.
  table: {
    flex: "0 0 auto",
  },
  rows: {
    flex: "0 0 auto",
  },
  // Four columns, and the figures hang in the same three of them on every row.
  // That is what makes two engagements comparable without reading either one.
  headerRow: {
    display: "flex",
    flex: "0 0 auto",
    alignItems: "center",
    flexWrap: "wrap",
    gap: spacingVars["--spacing-2"],
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-3"],
    borderBlockEndWidth: borderVars["--border-width"],
    borderBlockEndStyle: "solid",
    borderBlockEndColor: awbrnVars.colorBorderSoft,
    backgroundColor: colorVars["--color-background-muted"],
  },
  headerCell: {
    flex: `1 1 ${battleCalculatorLayout.targetColumnInlineSize}`,
    minInlineSize: 0,
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
  },
  headerFigure: {
    // Set flush end, over the column its values fill.
    flex: "0 0 auto",
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    textAlign: "end",
    color: colorVars["--color-text-secondary"],
  },
  headerDamage: { inlineSize: battleCalculatorLayout.damageColumnInlineSize },
  headerFunds: { inlineSize: battleCalculatorLayout.fundsColumnInlineSize },
  headerNet: { inlineSize: battleCalculatorLayout.netColumnInlineSize },
  // The row is two parts, not four columns: what is being asked about, and
  // what the answer is. They sit side by side while there is room and stack
  // when there is not, and because the header is built the same way the figures
  // never lose the names above them.
  row: {
    display: "flex",
    flex: "0 0 auto",
    alignItems: "center",
    flexWrap: "wrap",
    gap: spacingVars["--spacing-2"],
    paddingBlock: spacingVars["--spacing-2"],
    paddingInline: spacingVars["--spacing-3"],
    borderBlockEndWidth: { default: borderVars["--border-width"], ":last-child": 0 },
    borderBlockEndStyle: "solid",
    borderBlockEndColor: awbrnVars.colorBorderSoft,
  },
  targetCell: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    flex: `1 1 ${battleCalculatorLayout.targetColumnInlineSize}`,
    minInlineSize: 0,
    flexWrap: "wrap",
  },
  // The answer. Its three parts keep fixed widths so the figures stack exactly
  // down the list, which is what lets two engagements be compared without
  // reading either one. On a panel too narrow to hold them beside the target
  // they wrap as a block rather than being crushed.
  figures: {
    display: "flex",
    alignItems: "center",
    justifyContent: "flex-end",
    flexWrap: "wrap",
    // It never shrinks: a squeezed figure column would stack the target's
    // pickers three deep to buy width the figures then waste. It sits beside
    // the target or it takes its own line, and nothing in between.
    flex: "0 0 auto",
    maxInlineSize: "100%",
    marginInlineStart: "auto",
    gap: spacingVars["--spacing-2"],
  },
  fighterFields: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
    flexWrap: "wrap",
  },
  figureCell: {
    display: "flex",
    flexDirection: "column",
    flex: "0 0 auto",
    gap: spacingVars["--spacing-1"],
    inlineSize: battleCalculatorLayout.damageColumnInlineSize,
  },
  // Values only, set flush end under the header that names them.
  fundsCell: {
    display: "flex",
    flexDirection: "column",
    alignItems: "flex-end",
    flex: "0 0 auto",
    gap: spacingVars["--spacing-1"],
    inlineSize: battleCalculatorLayout.fundsColumnInlineSize,
  },
  figureLine: {
    display: "flex",
    alignItems: "baseline",
    gap: spacingVars["--spacing-1"],
    justifyContent: "space-between",
    minInlineSize: 0,
  },
  // The label recedes by opacity rather than by a second colour, which is this
  // system's rule for receding.
  figureLabel: {
    flex: "0 0 auto",
    inlineSize: battleCalculatorLayout.figureLabelInlineSize,
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    opacity: 0.7,
  },
  figureValue: {
    flex: "0 0 auto",
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    fontVariantNumeric: "tabular-nums",
  },
  figureValueMuted: {
    opacity: 0.5,
  },
  // What the pair adds up to, and the only figure on the row set larger than
  // the HUD floor. It is the one number that is not on the board anywhere else.
  netCell: {
    display: "flex",
    flexDirection: "column",
    alignItems: "flex-end",
    flex: "0 0 auto",
    gap: spacingVars["--spacing-1"],
    inlineSize: battleCalculatorLayout.netColumnInlineSize,
  },
  netValue: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-base"],
    letterSpacing: "0.04em",
    fontVariantNumeric: "tabular-nums",
    color: colorVars["--color-text-primary"],
    // A figure broken across two lines stops being one figure, and the break
    // lands between a sign and its number.
    whiteSpace: "nowrap",
  },
  // Whether the strike finishes the target, which the damage against the health
  // implies and which is worth saying once rather than being worked out.
  outcome: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
    textAlign: "end",
  },
  impossibleCell: {
    fontFamily: typographyVars["--font-family-code"],
    fontSize: textSizeVars["--font-size-sm"],
    letterSpacing: "0.06em",
    textTransform: "uppercase",
    color: colorVars["--color-text-secondary"],
    textAlign: "end",
  },
  // The row's own sentence. It is the accessible reading of every figure beside
  // it, so the columns can stay wordless.
  rowSummary: {
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
  healthDigit: {
    position: "absolute",
    insetInlineEnd: 0,
    insetBlockEnd: 0,
    display: "block",
  },
  unitField: {
    inlineSize: battleCalculatorLayout.unitInlineSize,
  },
  healthField: {
    inlineSize: battleCalculatorLayout.healthInlineSize,
  },
  terrainField: {
    inlineSize: battleCalculatorLayout.terrainInlineSize,
  },
  ammoField: {
    inlineSize: battleCalculatorLayout.ammoInlineSize,
  },
  footer: {
    flex: "0 0 auto",
    borderBlockStartWidth: borderVars["--border-width"],
    borderBlockStartStyle: "solid",
    borderBlockStartColor: colorVars["--color-border-emphasized"],
    backgroundColor: colorVars["--color-background-muted"],
  },
});
