import { Button } from "#/ui/Button.tsx";
import { Dialog } from "@astryxdesign/core/Dialog";
import { Grid } from "@astryxdesign/core/Grid";
import { NumberInput } from "@astryxdesign/core/NumberInput";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
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
import { uiAtlasSpriteStyle, unitSpriteSize, unitSpriteStyle } from "#/components/game_sprites.ts";
import type { GameRunner } from "#/engine/game_runner.ts";
import { getFactionByCode } from "#/factions.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { battleCalculatorLayout } from "./battleCalculatorLayout.stylex.ts";
import {
  FULL_HEALTH_POINTS,
  HEALTH_BARS,
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
  unitEntry,
  type CalculatorSide,
} from "./battle_calculator.ts";
import type {
  BattleCatalog,
  BattleFighter,
  BattleReportWire,
  BattleRow,
  BattleSide,
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
 * both commanders, both treasuries, both property counts, the towers, the
 * weather — and a list of targets underneath that re-scores the instant any of
 * it changes. That is the arrangement the question has: the context is asked
 * once and the targets are compared against it, rather than each row carrying
 * its own copy of an army.
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

  // Seeded once, on open. The panel is a scratchpad from that moment on: a
  // board that reports again mid-edit must not reach in and rewrite a field the
  // player is in the middle of reasoning about.
  const [weather, setWeather] = useState<WeatherKind>(() => roster?.weather ?? "clear");
  const [attackerSide, setAttackerSide] = useState<CalculatorSide>(() => sideFrom(seats.attacker));
  const [defenderSide, setDefenderSide] = useState<CalculatorSide>(() => sideFrom(seats.defender));
  const [attacker, setAttacker] = useState<BattleFighter>(() => blankFighter());
  const [targets, setTargets] = useState<BattleFighter[]>(() => [blankFighter()]);
  const [report, setReport] = useState<BattleReportWire | null>(null);
  const [reportError, setReportError] = useState<string | null>(null);

  const attackerFaction =
    seats.attacker?.displayFactionCode ?? seats.attacker?.factionCode ?? FALLBACK_ATTACKER_FACTION;
  const defenderFaction =
    seats.defender?.displayFactionCode ?? seats.defender?.factionCode ?? FALLBACK_DEFENDER_FACTION;

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

  const addTarget = useCallback(() => {
    setTargets((previous) =>
      previous.length >= MAX_TARGETS
        ? previous
        : [...previous, previous[previous.length - 1] ?? blankFighter()],
    );
  }, []);

  const strip = (
    <ContextStrip
      attacker={attackerSide}
      attackerFaction={attackerFaction}
      attackerName={seats.attacker?.displayFactionName ?? "Attacking army"}
      catalog={catalog}
      defender={defenderSide}
      defenderFaction={defenderFaction}
      defenderName={seats.defender?.displayFactionName ?? "Defending army"}
      isTrailing={presentation === "sheet"}
      onAttackerChange={setAttackerSide}
      onDefenderChange={setDefenderSide}
      onWeatherChange={setWeather}
      portraits={portraits}
      weather={weather}
    />
  );

  const engagement = (
    <>
      <AttackerRow
        catalog={catalog}
        factionCode={attackerFaction}
        fighter={attacker}
        onChange={setAttacker}
        value={report?.attackerValue ?? null}
      />

      <TargetTable
        attackerName={unitName(catalog, attacker.unit)}
        catalog={catalog}
        factionCode={defenderFaction}
        onChange={setTargets}
        rows={report?.rows ?? null}
        targets={targets}
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

  if (presentation === "sheet") {
    return (
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
    );
  }

  return (
    <VStack aria-label="Battle calculator" gap={0} role="dialog" xstyle={styles.boardPanel}>
      <PanelFrame onDismiss={onDismiss} onRestoreFocus={onRestoreFocus}>
        {body}
      </PanelFrame>
    </VStack>
  );
}

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
        if (event.key === "Escape") {
          event.preventDefault();
          onDismiss();
        }
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
  catalog,
  defender,
  defenderFaction,
  defenderName,
  isTrailing,
  onAttackerChange,
  onDefenderChange,
  onWeatherChange,
  portraits,
  weather,
}: {
  attacker: CalculatorSide;
  attackerFaction: string;
  attackerName: string;
  catalog: BattleCatalog | null;
  defender: CalculatorSide;
  defenderFaction: string;
  defenderName: string;
  /** Whether the strip reads after the engagement rather than before it. */
  isTrailing: boolean;
  onAttackerChange: (side: CalculatorSide) => void;
  onDefenderChange: (side: CalculatorSide) => void;
  onWeatherChange: (weather: WeatherKind) => void;
  portraits: CoPortraitCatalog;
  weather: WeatherKind;
}) {
  return (
    <VStack gap={0} xstyle={[styles.strip, isTrailing && styles.stripTrailing]}>
      <HStack align="center" gap={3} paddingBlock={2} paddingInline={3} wrap="wrap">
        <span {...stylex.props(styles.stripLabel)}>Weather</span>
        <SegmentedControl
          label="Weather"
          onChange={(value) => onWeatherChange(value as WeatherKind)}
          size="sm"
          value={weather}
        >
          <SegmentedControlItem label="Clear" value="clear" />
          <SegmentedControlItem label="Rain" value="rain" />
          <SegmentedControlItem label="Snow" value="snow" />
        </SegmentedControl>
      </HStack>

      <Grid gap={0} xstyle={styles.stripColumns}>
        <SideFields
          catalog={catalog}
          factionCode={attackerFaction}
          armyName={attackerName}
          onChange={onAttackerChange}
          portraits={portraits}
          role="Attacking"
          side={attacker}
        />
        <SideFields
          catalog={catalog}
          factionCode={defenderFaction}
          armyName={defenderName}
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
  catalog,
  factionCode,
  onChange,
  portraits,
  role,
  side,
}: {
  armyName: string;
  catalog: BattleCatalog | null;
  factionCode: string;
  onChange: (side: CalculatorSide) => void;
  portraits: CoPortraitCatalog;
  role: "Attacking" | "Defending";
  side: CalculatorSide;
}) {
  const commanders = catalog?.commanders ?? [];
  const accent = getFactionByCode(factionCode);
  const power: PowerLevel | "d2d" = side.power ?? "d2d";

  return (
    <VStack gap={2} paddingBlock={3} paddingInline={3} xstyle={styles.sideColumn}>
      <HStack align="center" gap={2} justify="between">
        <span {...stylex.props(styles.sideRole)}>{role}</span>
        <span {...stylex.props(styles.sideArmy)}>{accent?.displayName ?? armyName}</span>
      </HStack>

      <HStack align="center" gap={2} wrap="wrap">
        <CoPortrait
          catalog={portraits}
          coKey={side.commander ?? "no-co"}
          fallbackLabel={`${role} commander`}
          size={40}
        />
        <VStack gap={2} xstyle={styles.sideControls}>
          <Selector
            hasSearch
            isLabelHidden
            label={`${role} commander`}
            onChange={(value) =>
              onChange({
                ...side,
                commander: value === "none" ? undefined : (value as CalculatorSide["commander"]),
                // A power belongs to a commander. Dropping the commander and
                // keeping the power would leave the generic power bonus running
                // under nobody, which is a state no match can be in.
                power: value === "none" ? undefined : side.power,
              })
            }
            options={[
              { value: "none", label: "No CO" },
              ...commanders.map((entry) => ({ value: entry.commander, label: entry.name })),
            ]}
            size="sm"
            value={side.commander ?? "none"}
          />
          <SegmentedControl
            disabledMessage="Choose a commander before running a power."
            isDisabled={side.commander === undefined}
            label={`${role} power`}
            layout="fill"
            onChange={(value) =>
              onChange({ ...side, power: value === "d2d" ? undefined : (value as PowerLevel) })
            }
            size="sm"
            value={power}
          >
            <SegmentedControlItem label="D2D" value="d2d" />
            <SegmentedControlItem label="COP" value="cop" />
            <SegmentedControlItem label="SCOP" value="scop" />
          </SegmentedControl>
        </VStack>
      </HStack>

      {/* Three treasury figures in one fixed grid, so the two armies' numbers
          line up across the divider and can be compared without reading their
          labels twice. The labels are the panel's own, in the HUD face every
          other label on the board wears; the stock field label is body copy and
          would put two voices in one strip. */}
      <Grid gap={0} xstyle={styles.treasuryGrid}>
        {/* Decoration: each field already carries its own hidden label, which
            is the name assistive technology reads. */}
        <span aria-hidden="true" {...stylex.props(styles.fieldLabel)}>
          Funds
        </span>
        <span aria-hidden="true" {...stylex.props(styles.fieldLabel)}>
          Properties
        </span>
        <span aria-hidden="true" {...stylex.props(styles.fieldLabel)}>
          Com towers
        </span>

        <NumberInput
          isLabelHidden
          label={`${role} funds`}
          min={0}
          onChange={(value) => onChange({ ...side, funds: Math.max(0, value ?? 0) })}
          size="sm"
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
          value={side.comTowers}
        />
      </Grid>
    </VStack>
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
  value,
}: {
  catalog: BattleCatalog | null;
  factionCode: string;
  fighter: BattleFighter;
  onChange: (fighter: BattleFighter) => void;
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
}: {
  attackerName: string;
  catalog: BattleCatalog | null;
  factionCode: string;
  onChange: (update: (previous: BattleFighter[]) => BattleFighter[]) => void;
  rows: BattleRow[] | null;
  targets: BattleFighter[];
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
}: {
  attackerName: string;
  catalog: BattleCatalog | null;
  factionCode: string;
  onChange: (fighter: BattleFighter) => void;
  onRemove?: () => void;
  row: BattleRow | undefined;
  target: BattleFighter;
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
}: {
  catalog: BattleCatalog | null;
  factionCode: string;
  fighter: BattleFighter;
  onChange: (fighter: BattleFighter) => void;
  role: string;
}) {
  const entry = unitEntry(catalog?.units ?? [], fighter.unit);
  const sprite = unitSpriteStyle(fighter.unit, factionCode, 2);
  const bars = pointsToBars(fighter.health);
  const digit = bars < 10 ? uiAtlasSpriteStyle(`Healthv2/${bars}.png`, 2) : null;

  return (
    <HStack gap={0} xstyle={styles.fighterFields}>
      {sprite ? (
        <span {...stylex.props(styles.sprite)} style={unitSpriteSize(2)}>
          <span aria-hidden="true" style={sprite} {...stylex.props(styles.spriteArt)} />
          {digit ? (
            <span aria-hidden="true" style={digit} {...stylex.props(styles.healthDigit)} />
          ) : null}
        </span>
      ) : null}

      <Selector
        hasSearch
        isLabelHidden
        label={`${role} unit`}
        onChange={(value) => {
          const next = unitEntry(catalog?.units ?? [], value as UnitKind);
          if (next) onChange(retypeFighter(fighter, next));
        }}
        options={(catalog?.units ?? []).map((unit) => ({
          value: unit.unit,
          label: unit.name,
        }))}
        size="sm"
        value={fighter.unit}
        xstyle={styles.unitField}
      />

      <Selector
        isLabelHidden
        label={`${role} health`}
        onChange={(value) => onChange({ ...fighter, health: barsToPoints(Number(value)) })}
        options={HEALTH_BARS.map((value) => ({ value: String(value), label: `${value} HP` }))}
        size="sm"
        value={String(bars)}
        xstyle={styles.healthField}
      />

      <Selector
        hasSearch
        isLabelHidden
        label={`${role} terrain`}
        onChange={(value) => onChange({ ...fighter, terrain: value as Terrain })}
        options={(catalog?.terrains ?? []).map((terrain) => ({
          value: terrain.terrain,
          label: `${terrain.name} · ${terrain.stars}★`,
        }))}
        size="sm"
        value={fighter.terrain}
        xstyle={styles.terrainField}
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
          value={fighter.ammo ?? entry.maxAmmo}
          xstyle={styles.ammoField}
        />
      ) : null}
    </HStack>
  );
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
