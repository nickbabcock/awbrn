import { Text } from "@astryxdesign/core/Text";
import { HStack, Stack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import {
  borderVars,
  colorVars,
  radiusVars,
  spacingVars,
} from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { CoStamp } from "#/components/CoStamp.tsx";
import { FactionCrest } from "#/components/FactionCrest.tsx";
import { UiSprite } from "#/components/UiSprite.tsx";
import { getCoAbilities } from "#/co_abilities.ts";
import type { MatchArmy } from "#/matches/match_armies.ts";
import type { ArmyFall } from "#/matches/components/army_falls.ts";
import { matchLayout, matchType } from "#/matches/components/matchLayout.stylex.ts";
import { StatIcon } from "#/replay/RosterRow.tsx";
import {
  canActivatePower,
  readPowerMeter,
  type ActivatablePowerLevel,
} from "#/replay/power_meter.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { AirmailBand } from "#/ui/AirmailBand.tsx";
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { factionColorVar } from "#/ui/faction_colors.ts";
import { IconClose } from "#/ui/icons.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import type { PlayerRosterEntry } from "#/wasm/awbrn_wasm.js";

/** A seat's clock as one army shows it. */
export interface ArmyClock {
  /** `3m 42s`, `5d 11h`. */
  value: string;
  /** What a screen reader hears. */
  label: string;
  isRunning: boolean;
  pressure: "steady" | "low" | "critical";
}

/** One army, with what this screen knows about it beyond the engine's entry. */
export interface ArmyView {
  army: MatchArmy;
  isViewer: boolean;
  clock: ArmyClock | null;
  /** How the army left the match, as far as the page can say. */
  fall?: ArmyFall | null;
  /** Changes how this army is drawn (its depicted faction). */
  onFactionChange?: (factionId: number) => void;
  /**
   * The page will never read this army's state (a signed-out viewer): it
   * shows what the match record names, and holds no room for a meter or
   * figures that are not coming.
   */
  isRecordOnly?: boolean;
}

/** The power keys an army offers right now, and what pressing one does. */
export interface PowerControl {
  activating: ActivatablePowerLevel | undefined;
  /** Set only for the viewer's army on the viewer's turn. */
  onActivate?: (level: ActivatablePowerLevel) => void;
}

const formatFigure = (value: number | undefined) =>
  value === undefined ? "--" : value.toLocaleString("en-US");

/** `Out · Day 11`, or `Out` when the day is not known. */
function outMark(fall: ArmyFall | null | undefined): string {
  return fall?.day == null ? "Out" : `Out · Day ${fall.day}`;
}

/** The commander's name, for the army sheet and assistive tech. */
function coName(entry: PlayerRosterEntry): string {
  return entry.coName ?? "No commander";
}

/* ---------------------------------------------------------------- meter */

/**
 * The CO power meter in stars: small stars buy the power, large stars the
 * super power, parted by a rule. Gold fills what is charged.
 */
export function StarMeter({
  entry,
  hasReadout = false,
  size = "md",
  tone = "ink",
}: {
  entry: PlayerRosterEntry;
  hasReadout?: boolean;
  size?: "sm" | "md";
  tone?: "ink" | "paper";
}) {
  const meter = readPowerMeter(entry);
  if (!meter) {
    // The row is held at the meter's height until the engine reports it, so
    // a plate or a card does not grow under the board when it does.
    return (
      <HStack
        as="span"
        xstyle={[meterStyles.meter, size === "sm" ? meterStyles.reserveSmall : meterStyles.reserve]}
      >
        <VisuallyHidden>CO power not reported yet</VisuallyHidden>
      </HStack>
    );
  }
  const copStars = meter.cop?.stars ?? 0;
  const superStars = meter.scop === null ? 0 : meter.scop.stars - copStars;
  const readout = `${formatStars(meter.charged)}/${meter.totalStars}`;
  const stars: { big: boolean; fill: number; split: boolean }[] = [];
  for (let index = 0; index < copStars + superStars; index += 1) {
    stars.push({
      big: index >= copStars,
      fill: Math.min(Math.max(meter.charged - index, 0), 1),
      split: index === copStars && copStars > 0 && superStars > 0,
    });
  }

  return (
    <HStack as="span" xstyle={meterStyles.row}>
      <HStack
        as="span"
        aria-label="CO power"
        aria-valuemax={meter.totalStars}
        aria-valuemin={0}
        aria-valuenow={Math.round(meter.charged * 10) / 10}
        aria-valuetext={`${readout} stars${meter.level === "charging" ? "" : ", power ready"}`}
        role="meter"
        xstyle={[meterStyles.meter, size === "sm" && meterStyles.meterSmall]}
      >
        {stars.map((star, index) => (
          <HStack as="span" key={index} xstyle={meterStyles.starSlot}>
            {star.split ? (
              <i
                {...stylex.props(
                  meterStyles.split,
                  size === "sm" && meterStyles.splitSmall,
                  tone === "paper" && meterStyles.splitPaper,
                )}
              />
            ) : null}
            <i
              {...stylex.props(
                meterStyles.star,
                star.big
                  ? size === "sm"
                    ? meterStyles.starBigSmall
                    : meterStyles.starBig
                  : size === "sm"
                    ? meterStyles.starSmall
                    : null,
                tone === "paper"
                  ? meterStyles.fillPaper(star.fill * 100)
                  : meterStyles.fill(star.fill * 100),
              )}
            />
          </HStack>
        ))}
      </HStack>
      {hasReadout ? (
        <Text
          color="inherit"
          type="inherit"
          aria-hidden="true"
          xstyle={[
            figureStyles.figure,
            meterStyles.readout,
            tone === "paper" && meterStyles.readoutPaper,
            meter.level !== "charging" && tone === "ink" && meterStyles.readoutReady,
          ]}
        >
          {readout}
        </Text>
      ) : null}
    </HStack>
  );
}

function formatStars(charged: number): string {
  const truncated = Math.trunc(charged * 10) / 10;
  return truncated % 1 === 0 ? String(truncated) : truncated.toFixed(1);
}

/* ---------------------------------------------------------------- power key */

/**
 * The power key: the game's own POWER or SUPER sprite on ink 950 with a gold
 * ring. Never red. `size` is 34px in the rail and 52px in the phone dock.
 */
export function PowerKey({
  isArmed = false,
  isLoading = false,
  label,
  level,
  onPress,
  size = "rail",
}: {
  isArmed?: boolean;
  isLoading?: boolean;
  label: string;
  level: ActivatablePowerLevel;
  onPress: () => void;
  size?: "row" | "rail" | "dock";
}) {
  return (
    <button
      aria-busy={isLoading || undefined}
      aria-label={label}
      aria-pressed={size === "dock" ? isArmed : undefined}
      disabled={isLoading}
      onClick={onPress}
      title={label}
      type="button"
      {...stylex.props(
        powerStyles.key,
        size === "row" && powerStyles.row,
        size === "dock" && powerStyles.dock,
        isArmed && powerStyles.armed,
      )}
    >
      <UiSprite
        name={level === "scop" ? "SuperPower.png" : "NormalPower.png"}
        scale={size === "row" ? 1 : 1}
      />
    </button>
  );
}

/** Which power keys an army shows, strongest last. */
export function readyPowers(entry: PlayerRosterEntry): ActivatablePowerLevel[] {
  const levels: ActivatablePowerLevel[] = [];
  if (canActivatePower(entry, "cop")) levels.push("cop");
  if (canActivatePower(entry, "scop")) levels.push("scop");
  return levels;
}

export function powerName(entry: PlayerRosterEntry, level: ActivatablePowerLevel): string {
  const abilities = entry.coKey ? getCoAbilities(entry.coKey) : null;
  const power = level === "scop" ? abilities?.superPower : abilities?.power;
  return power?.name ?? (level === "scop" ? "Super power" : "Power");
}

export function powerEffect(entry: PlayerRosterEntry, level: ActivatablePowerLevel): string | null {
  const abilities = entry.coKey ? getCoAbilities(entry.coKey) : null;
  const power = level === "scop" ? abilities?.superPower : abilities?.power;
  return power?.effect ?? null;
}

function PowerKeys({
  entry,
  power,
  size,
}: {
  entry: PlayerRosterEntry;
  power?: PowerControl;
  size: "row" | "rail";
}) {
  if (!power?.onActivate) return null;
  const onActivate = power.onActivate;
  return (
    <>
      {readyPowers(entry).map((level) => (
        <PowerKey
          isLoading={power.activating === level}
          key={level}
          label={`Activate ${level === "scop" ? "super power" : "CO power"}: ${powerName(entry, level)}`}
          level={level}
          onPress={() => onActivate(level)}
          size={size}
        />
      ))}
    </>
  );
}

/* ---------------------------------------------------------------- small parts */

/** A seat's clock chip: not the mover's (the strip carries that). */
export function ClockChip({ clock }: { clock: ArmyClock }) {
  return (
    <HStack
      as="span"
      aria-label={clock.label}
      role="timer"
      xstyle={[
        figureStyles.figure,
        chipStyles.clock,
        clock.isRunning && chipStyles.running,
        clock.pressure === "low" && chipStyles.low,
        clock.pressure === "critical" && chipStyles.critical,
      ]}
    >
      <UiSprite name="Clock.png" />
      <Text color="inherit" type="inherit" aria-hidden="true">
        {clock.value}
      </Text>
    </HStack>
  );
}

function YouTag({ isSmall = false }: { isSmall?: boolean }) {
  return (
    <HStack as="span" xstyle={[chipStyles.you, isSmall && chipStyles.youSmall]}>
      YOU
    </HStack>
  );
}

function MovingDot({ label = "On the move" }: { label?: string }) {
  return <Stack as="span" aria-label={label} role="img" xstyle={chipStyles.dot} />;
}

function ArmyStamp({ entry, size }: { entry: PlayerRosterEntry; size: "sm" | "md" }) {
  return (
    <CoStamp
      coKey={entry.coKey}
      factionCode={entry.displayFactionCode}
      label={coName(entry)}
      size={size}
      state={entry.eliminated ? "canceled" : "default"}
    />
  );
}

/**
 * The army's crest. It is the faction picker only on paper (the army sheet);
 * on the ink rail it is the mark alone, and the stamp opens the sheet.
 */
function Crest({
  isPicker = false,
  size = 14,
  view,
}: {
  isPicker?: boolean;
  size?: number;
  view: ArmyView;
}) {
  return (
    <FactionCrest
      factionCode={view.army.entry.displayFactionCode}
      onChange={isPicker ? view.onFactionChange : undefined}
      size={size}
    />
  );
}

/** A stamp that opens the army sheet. */
function StampKey({
  entry,
  name,
  onOpen,
  size,
}: {
  entry: PlayerRosterEntry;
  name: string;
  onOpen?: () => void;
  size: "sm" | "md";
}) {
  if (!onOpen) return <ArmyStamp entry={entry} size={size} />;
  return (
    <button
      aria-haspopup="dialog"
      aria-label={`${name}, ${coName(entry)}: open army details`}
      onClick={onOpen}
      type="button"
      {...stylex.props(cardStyles.stampKey)}
    >
      <ArmyStamp entry={entry} size={size} />
    </button>
  );
}

function StatTile({
  icon,
  label,
  value,
  tone = "ink",
}: {
  icon: ReactNode;
  label: string;
  value: string;
  tone?: "ink" | "paper";
}) {
  // Raw span: Astryx components drop `title`, and this tooltip is its label.
  return (
    <span
      title={label}
      {...stylex.props(cardStyles.stat, tone === "paper" && cardStyles.statPaper)}
    >
      {icon}
      <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, cardStyles.statValue]}>
        <VisuallyHidden>{label}: </VisuallyHidden>
        {value}
      </Text>
    </span>
  );
}

/**
 * The four figures as sprite-and-figure tiles. A figure the engine has not
 * reported yet is left out rather than drawn as a placeholder, and the grid is
 * not drawn at all until one is known.
 */
function ArmyStats({ entry, tone = "ink" }: { entry: PlayerRosterEntry; tone?: "ink" | "paper" }) {
  const { funds, income, unitCount, unitValue } = entry.stats;
  if (
    funds === undefined &&
    income === undefined &&
    unitCount === undefined &&
    unitValue === undefined
  ) {
    return null;
  }
  return (
    <Stack as="span" xstyle={cardStyles.stats}>
      {funds === undefined ? null : (
        <StatTile
          icon={<StatIcon spriteName="Coin.png" />}
          label="Funds"
          tone={tone}
          value={formatFigure(funds)}
        />
      )}
      {income === undefined ? null : (
        <StatTile
          icon={<StatIcon spriteName="BuildingsCaptured.png" />}
          label="Income"
          tone={tone}
          value={`+${income.toLocaleString("en-US")}`}
        />
      )}
      {unitCount === undefined ? null : (
        <StatTile
          icon={<StatIcon factionCode={entry.displayFactionCode} />}
          label="Units"
          tone={tone}
          value={formatFigure(unitCount)}
        />
      )}
      {unitValue === undefined ? null : (
        <StatTile
          icon={<StatIcon coinOverlay factionCode={entry.displayFactionCode} />}
          label="Unit value"
          tone={tone}
          value={formatFigure(unitValue)}
        />
      )}
    </Stack>
  );
}

/* ---------------------------------------------------------------- rail: cards */

/**
 * Two armies in the rail: one card each. The wash is the army's color; the
 * band runs along your card on your turn.
 */
export function ArmyCard({
  isOwed,
  onOpen,
  power,
  view,
}: {
  isOwed: boolean;
  /** Opens the army sheet (the CO, the powers, the faction picker). */
  onOpen?: () => void;
  power?: PowerControl;
  view: ArmyView;
}) {
  const { army, clock, fall, isViewer } = view;
  const entry = army.entry;
  return (
    <section
      aria-label={army.name}
      {...stylex.props(
        cardStyles.card,
        washStyles.wash(
          factionColorVar(entry.displayFactionCode, "accent"),
          isOwed ? 18 : army.isActive ? 16 : 9,
          isOwed ? awbrnVars.ink850 : awbrnVars.ink900,
        ),
      )}
    >
      {isOwed ? <AirmailBand isOverlay thickness={4} /> : null}
      <Stack as="span" xstyle={cardStyles.top}>
        <StampKey entry={entry} name={army.name} onOpen={onOpen} size="md" />
        <Stack as="span" xstyle={cardStyles.identity}>
          <HStack as="span" xstyle={cardStyles.name}>
            {army.isActive ? <MovingDot /> : null}
            <Text
              color="inherit"
              type="inherit"
              xstyle={[cardStyles.nameText, entry.eliminated && cardStyles.struck]}
            >
              {army.name}
            </Text>
            {isViewer ? <YouTag /> : null}
          </HStack>
          <HStack as="span" xstyle={cardStyles.crestLine}>
            <Crest view={view} size={14} />
            {entry.eliminated && fall?.reason ? (
              <Text color="inherit" type="inherit" xstyle={cardStyles.why}>
                {fall.reason}
              </Text>
            ) : null}
          </HStack>
        </Stack>
        {entry.eliminated ? (
          <Text color="inherit" type="inherit" xstyle={cardStyles.out}>
            {outMark(fall)}
          </Text>
        ) : clock && !clock.isRunning ? (
          <ClockChip clock={clock} />
        ) : null}
      </Stack>
      {view.isRecordOnly ? null : (
        <>
          <HStack as="span" xstyle={cardStyles.powerLine}>
            <StarMeter entry={entry} hasReadout />
            <HStack as="span" xstyle={cardStyles.powerKeys}>
              <PowerKeys entry={entry} power={power} size="rail" />
            </HStack>
          </HStack>
          <ArmyStats entry={entry} />
        </>
      )}
    </section>
  );
}

/* ---------------------------------------------------------------- rail: ledger */

/**
 * Three or more armies in the rail: one 64px row each, in turn order. A fallen
 * army keeps its place, canceled. From seven armies the ledger scrolls and
 * your row stays pinned.
 */
export function Ledger({
  isOwed,
  onOpen,
  power,
  views,
}: {
  isOwed: boolean;
  /** Opens an army's sheet. */
  onOpen?: (playerId: number) => void;
  power?: PowerControl;
  views: ArmyView[];
}) {
  const isLong = views.length >= 7;
  const listRef = useRef<HTMLOListElement>(null);
  const activeId = views.find((view) => view.army.isActive)?.army.entry.playerId;
  const more = useScrollEdges(listRef, "block");

  // The ledger scrolls inside the rail, so the view block under it stays on
  // screen; the mover is kept in view as the turn passes down a long one.
  // Only the ledger moves: `scrollIntoView` would scroll the rail and the page
  // along with it.
  useEffect(() => {
    const list = listRef.current;
    if (activeId === undefined || !list) return;
    const row = list.querySelector<HTMLElement>(`[data-player="${activeId}"]`);
    if (!row) return;
    const shown = list.getBoundingClientRect();
    const place = row.getBoundingClientRect();
    if (place.top < shown.top) list.scrollTop -= shown.top - place.top;
    else if (place.bottom > shown.bottom) list.scrollTop += place.bottom - shown.bottom;
  }, [activeId]);

  return (
    <ol
      aria-label="Armies in turn order"
      ref={listRef}
      {...stylex.props(
        ledgerStyles.list,
        ledgerStyles.floor(Math.min(views.length, 3)),
        more.before && ledgerStyles.fadeAbove,
        more.after && ledgerStyles.fadeBelow,
      )}
      onScroll={more.read}
    >
      {views.map((view, index) => {
        const { army, clock, fall, isViewer } = view;
        const entry = army.entry;
        const isRowOwed = isOwed && isViewer;
        const { funds, unitCount } = entry.stats;
        return (
          <li
            aria-label={`${index + 1}. ${army.name}${isViewer ? " (you)" : ""}, ${coName(entry)}, ${entry.displayFactionName}${entry.eliminated ? `, ${outMark(fall).toLowerCase()}${fall?.reason ? `, ${fall.reason}` : ""}` : ""}`}
            data-player={entry.playerId}
            key={entry.playerId}
            {...stylex.props(
              ledgerStyles.row,
              washStyles.wash(
                factionColorVar(entry.displayFactionCode, "accent"),
                isRowOwed ? 18 : army.isActive ? 16 : 8,
                isRowOwed ? awbrnVars.ink850 : awbrnVars.ink900,
              ),
              entry.eliminated && ledgerStyles.rowOut,
              isLong && isViewer && ledgerStyles.pinned,
            )}
          >
            {isRowOwed ? <AirmailBand isOverlay thickness={4} /> : null}
            <Text
              color="inherit"
              type="inherit"
              xstyle={[figureStyles.figure, ledgerStyles.number]}
            >
              {index + 1}
            </Text>
            <StampKey
              entry={entry}
              name={army.name}
              onOpen={onOpen ? () => onOpen(entry.playerId) : undefined}
              size="sm"
            />
            <Stack as="span" xstyle={ledgerStyles.main}>
              <HStack as="span" xstyle={ledgerStyles.name}>
                {army.isActive ? <MovingDot /> : null}
                <b
                  {...stylex.props(ledgerStyles.nameText, entry.eliminated && ledgerStyles.struck)}
                >
                  {army.name}
                </b>
                {isViewer ? <YouTag isSmall /> : null}
              </HStack>
              <HStack as="span" xstyle={ledgerStyles.line2}>
                <Crest view={view} size={14} />
                {entry.eliminated ? (
                  fall?.reason ? (
                    <Text color="inherit" type="inherit" xstyle={ledgerStyles.why}>
                      {fall.reason}
                    </Text>
                  ) : null
                ) : (
                  <StarMeter entry={entry} hasReadout size="sm" />
                )}
              </HStack>
            </Stack>
            <Stack as="span" xstyle={ledgerStyles.end}>
              {entry.eliminated ? (
                <Text color="inherit" type="inherit" xstyle={ledgerStyles.out}>
                  {outMark(fall)}
                </Text>
              ) : (
                <>
                  {isViewer && power?.onActivate && readyPowers(entry).length > 0 ? (
                    <HStack as="span" xstyle={ledgerStyles.endTop}>
                      <PowerKeys entry={entry} power={power} size="row" />
                    </HStack>
                  ) : clock && !clock.isRunning ? (
                    <HStack as="span" xstyle={ledgerStyles.endTop}>
                      <ClockChip clock={clock} />
                    </HStack>
                  ) : null}
                  {funds === undefined && unitCount === undefined ? null : (
                    <HStack as="span" xstyle={[figureStyles.figure, ledgerStyles.figures]}>
                      {funds === undefined ? null : (
                        <HStack as="span" xstyle={ledgerStyles.figure}>
                          <UiSprite name="Coin.png" />
                          <VisuallyHidden>Funds </VisuallyHidden>
                          {formatFigure(funds)}
                        </HStack>
                      )}
                      {unitCount === undefined ? null : (
                        <HStack as="span" xstyle={ledgerStyles.figure}>
                          <StatIcon factionCode={entry.displayFactionCode} />
                          <VisuallyHidden>Units </VisuallyHidden>
                          {formatFigure(unitCount)}
                        </HStack>
                      )}
                    </HStack>
                  )}
                </>
              )}
            </Stack>
          </li>
        );
      })}
    </ol>
  );
}

/**
 * Whether a scroller has content out of sight before or after along `axis`, so
 * it fades on that edge the way the board fades into the table. `read` goes on
 * the scroller's `onScroll`.
 */
function useScrollEdges(
  ref: RefObject<HTMLElement | null>,
  axis: "block" | "inline",
): { before: boolean; after: boolean; read: () => void } {
  const [edges, setEdges] = useState({ before: false, after: false });
  const read = useCallback(() => {
    const scroller = ref.current;
    if (!scroller) return;
    const [offset, shown, whole] =
      axis === "block"
        ? [scroller.scrollTop, scroller.clientHeight, scroller.scrollHeight]
        : [Math.abs(scroller.scrollLeft), scroller.clientWidth, scroller.scrollWidth];
    const before = offset > 0;
    const after = offset + shown < whole - 1;
    setEdges((previous) =>
      previous.before === before && previous.after === after ? previous : { before, after },
    );
  }, [axis, ref]);

  useLayoutEffect(() => {
    const scroller = ref.current;
    if (!scroller) return;
    // The scroller's own size moves with the screen (and the ledger's cap);
    // the armies in it can come and go.
    const sizes = new ResizeObserver(read);
    sizes.observe(scroller);
    const joins = new MutationObserver(read);
    joins.observe(scroller, { childList: true });
    read();
    return () => {
      joins.disconnect();
      sizes.disconnect();
    };
  }, [read, ref]);

  return { ...edges, read };
}

/* ---------------------------------------------------------------- phone */

/**
 * One army as a phone plate: stamp, crest and name, YOU (two armies only),
 * meter, then funds and the clock (units in a replay or review). Opens the
 * army sheet.
 *
 * The plate lays itself out from its own width, not the screen's: narrow (two
 * plates side by side) sets the name beside the stamp, then the meter and the
 * figures each on a clear row under both; wide (one plate across, or a plate
 * in a wider surface) stacks the name over the meter beside the stamp, with
 * the figures at the end.
 */
export function ArmyPlate({
  hasYouTag,
  isOwed,
  isWide = false,
  onOpen,
  showUnits,
  view,
}: {
  hasYouTag: boolean;
  isOwed: boolean;
  /** The one plate under the turn-order row (three or more armies). */
  isWide?: boolean;
  onOpen: () => void;
  /** A replay or a review shows units where a live match shows the clock. */
  showUnits: boolean;
  view: ArmyView;
}) {
  const { army, clock, fall, isViewer } = view;
  const entry = army.entry;
  // The label names the army and the action; the readings (CO power, funds,
  // the clock or units) are its description, which the label would hide.
  const meterId = useId();
  const figuresId = useId();
  const hasMeter = !entry.eliminated && !view.isRecordOnly;
  const readings = [hasMeter && meterId, !view.isRecordOnly && figuresId].filter(Boolean);
  return (
    <button
      aria-describedby={readings.length > 0 ? readings.join(" ") : undefined}
      aria-haspopup="dialog"
      aria-label={`${army.name}${isViewer ? " (you)" : ""}, ${coName(entry)}${entry.eliminated ? `, ${outMark(fall).toLowerCase()}` : ""}: open army details`}
      onClick={onOpen}
      type="button"
      {...stylex.props(
        plateStyles.plate,
        isWide && plateStyles.plateWide,
        washStyles.wash(
          factionColorVar(entry.displayFactionCode, "accent"),
          isOwed ? 18 : army.isActive && !isWide ? 18 : 10,
          isOwed ? awbrnVars.ink850 : awbrnVars.ink900,
        ),
      )}
    >
      {isOwed ? <AirmailBand isOverlay thickness={4} /> : null}
      <Stack
        as="span"
        xstyle={[plateStyles.layout, view.isRecordOnly && plateStyles.layoutRecordOnly]}
      >
        <Stack as="span" xstyle={plateStyles.stamp}>
          <ArmyStamp entry={entry} size="sm" />
        </Stack>
        <HStack as="span" xstyle={[plateStyles.name, view.isRecordOnly && plateStyles.nameAlone]}>
          {army.isActive && showUnits ? <MovingDot /> : null}
          <FactionLogoInline code={entry.displayFactionCode} />
          <Text
            color="inherit"
            type="inherit"
            xstyle={[plateStyles.nameText, entry.eliminated && cardStyles.struck]}
          >
            {army.name}
          </Text>
          {hasYouTag && isViewer ? <YouTag isSmall /> : null}
        </HStack>
        {entry.eliminated ? (
          <Text color="inherit" type="inherit" xstyle={[plateStyles.meter, plateStyles.out]}>
            {outMark(fall)}
            {fall?.reason ? ` · ${fall.reason}` : ""}
          </Text>
        ) : view.isRecordOnly ? null : (
          <HStack as="span" id={meterId} xstyle={plateStyles.meter}>
            <VisuallyHidden>CO power </VisuallyHidden>
            <StarMeter entry={entry} size="sm" />
          </HStack>
        )}
        {view.isRecordOnly ? null : (
          <HStack as="span" id={figuresId} xstyle={plateStyles.figures}>
            <PlateFigures clock={clock} entry={entry} showUnits={showUnits} />
          </HStack>
        )}
      </Stack>
    </button>
  );
}

function PlateFigures({
  clock,
  entry,
  showUnits,
}: {
  clock: ArmyClock | null;
  entry: PlayerRosterEntry;
  showUnits: boolean;
}) {
  const { funds, unitCount } = entry.stats;
  return (
    <>
      {funds === undefined ? null : (
        <HStack as="span" xstyle={[figureStyles.figure, plateStyles.funds]}>
          <UiSprite name="Coin.png" />
          <VisuallyHidden>Funds </VisuallyHidden>
          {formatFigure(funds)}
        </HStack>
      )}
      {showUnits || clock === null ? (
        unitCount === undefined ? null : (
          <HStack as="span" xstyle={[figureStyles.figure, plateStyles.funds]}>
            <StatIcon factionCode={entry.displayFactionCode} />
            <VisuallyHidden>Units </VisuallyHidden>
            {formatFigure(unitCount)}
          </HStack>
        )
      ) : (
        <ClockChip clock={clock} />
      )}
    </>
  );
}

function FactionLogoInline({ code }: { code: string }) {
  return <FactionCrest factionCode={code} size={14} />;
}

/**
 * Three or more armies on a phone: the turn-order row (stamp and name per
 * army, YOU or OUT on the corner, the mover on a green foot). It scrolls
 * sideways past six, keeping the mover and you in view. Any cell opens that
 * army's sheet.
 */
export function TurnOrderRow({
  isOwed,
  onOpen,
  views,
}: {
  isOwed: boolean;
  onOpen: (playerId: number) => void;
  views: ArmyView[];
}) {
  const rowRef = useRef<HTMLElement>(null);
  const activeId = views.find((view) => view.army.isActive)?.army.entry.playerId;
  // Past six armies the row scrolls, and your own cell stays at an edge of it
  // while another army's turn carries the row away.
  const isLong = views.length >= 7;
  const more = useScrollEdges(rowRef, "inline");

  // The mover is kept in view as the turn passes along a long row, clear of
  // your own cell pinned at either edge. Only the row moves: `scrollIntoView`
  // would scroll the screen along with it.
  useEffect(() => {
    const row = rowRef.current;
    if (activeId === undefined || !row) return;
    const cell = row.querySelector<HTMLElement>(`[data-player="${activeId}"]`);
    if (!cell) return;
    const pinned = row.querySelector<HTMLElement>("[data-pinned]");
    const guard = pinned && pinned !== cell ? pinned.offsetWidth : 0;
    const shown = row.getBoundingClientRect();
    const place = cell.getBoundingClientRect();
    if (place.left < shown.left + guard) {
      row.scrollLeft -= shown.left + guard - place.left;
    } else if (place.right > shown.right - guard) {
      row.scrollLeft += place.right - (shown.right - guard);
    }
  }, [activeId]);

  return (
    <Stack as="section" aria-label="Turn order" xstyle={orderStyles.row}>
      <Stack
        ref={rowRef}
        xstyle={[
          orderStyles.track,
          orderStyles.columns(views.length),
          isLong && orderStyles.rowLong,
        ]}
        onScroll={more.read}
      >
        {views.map((view, index) => {
          const { army, isViewer } = view;
          const entry = army.entry;
          return (
            <button
              aria-haspopup="dialog"
              aria-label={`${index + 1}. ${army.name}${isViewer ? " (you)" : ""}, ${coName(entry)}${army.isActive ? ", on the move" : ""}${entry.eliminated ? ", out" : ""}: open army details`}
              data-pinned={isLong && isViewer ? "" : undefined}
              data-player={entry.playerId}
              key={entry.playerId}
              onClick={() => onOpen(entry.playerId)}
              type="button"
              {...stylex.props(
                orderStyles.cell,
                orderStyles.wash(
                  factionColorVar(entry.displayFactionCode, "accent"),
                  army.isActive ? 22 : 8,
                ),
                army.isActive && orderStyles.cellActive,
                isLong && isViewer && orderStyles.cellPinned,
              )}
            >
              <Stack as="span" xstyle={orderStyles.stampWrap}>
                <ArmyStamp entry={entry} size="sm" />
                {entry.eliminated ? (
                  <Text
                    color="inherit"
                    type="inherit"
                    xstyle={[orderStyles.tag, orderStyles.tagOut]}
                  >
                    OUT
                  </Text>
                ) : isViewer ? (
                  <Text color="inherit" type="inherit" xstyle={orderStyles.tag}>
                    YOU
                  </Text>
                ) : null}
              </Stack>
              <Text
                color="inherit"
                type="inherit"
                xstyle={[
                  orderStyles.name,
                  (army.isActive || isViewer) && orderStyles.nameBright,
                  entry.eliminated && orderStyles.nameOut,
                ]}
              >
                {army.name}
              </Text>
              {army.isActive ? (
                <Stack
                  as="span"
                  aria-hidden="true"
                  xstyle={[orderStyles.foot, isOwed && orderStyles.footOverBand]}
                />
              ) : null}
            </button>
          );
        })}
      </Stack>
      {more.before ? (
        <Stack as="span" aria-hidden="true" xstyle={[orderStyles.fade, orderStyles.fadeStart]} />
      ) : null}
      {more.after ? (
        <Stack as="span" aria-hidden="true" xstyle={[orderStyles.fade, orderStyles.fadeEnd]} />
      ) : null}
      {isOwed ? (
        <AirmailBand isOverlay={false} thickness={ORDER_BAND} xstyle={orderStyles.band} />
      ) : null}
    </Stack>
  );
}

/* ---------------------------------------------------------------- army sheet */

/**
 * The army sheet a plate or a turn-order cell opens: the CO and army named,
 * the meter, the four figures, and POWER and SUPER with what is left to
 * charge. No clock.
 */
export function ArmySheet({
  isOpen,
  onOpenChange,
  power,
  view,
}: {
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  power?: PowerControl;
  view: ArmyView | null;
}) {
  // No army has been opened yet. Once one has, it stays while the sheet closes.
  if (view === null) return null;
  const entry = view.army.entry;
  const meter = readPowerMeter(entry);
  const levels: ActivatablePowerLevel[] = ["cop", "scop"];

  return (
    <ModalSheet
      headStart={<ArmyStamp entry={entry} size="sm" />}
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      title={view.isViewer ? `${view.army.name} (you)` : view.army.name}
    >
      <HStack as="span" xstyle={sheetStyles.who}>
        <Crest isPicker size={16} view={view} />
        {coName(entry)} · {entry.displayFactionName}
        {entry.eliminated ? (
          <Text color="inherit" type="inherit" xstyle={sheetStyles.out}>
            {outMark(view.fall)}
            {view.fall?.reason ? ` · ${view.fall.reason}` : ""}
          </Text>
        ) : null}
      </HStack>
      <HStack as="span" xstyle={sheetStyles.meterRow}>
        <StarMeter entry={entry} tone="paper" />
        {meter ? (
          <Text
            color="inherit"
            type="inherit"
            xstyle={[figureStyles.figure, sheetStyles.meterReadout]}
          >
            {formatStars(meter.charged)} / {meter.totalStars}
          </Text>
        ) : null}
      </HStack>
      <Stack as="span" xstyle={sheetStyles.stats}>
        <ArmyStats entry={entry} tone="paper" />
      </Stack>
      {meter ? (
        <Stack as="span" xstyle={sheetStyles.powers}>
          {levels.map((level) => {
            const cost = level === "scop" ? meter.scop : meter.cop;
            if (cost === null) return null;
            const isReady = canActivatePower(entry, level);
            const canPress = isReady && power?.onActivate !== undefined;
            const name = powerName(entry, level);
            const isOn = entry.activePower === level;
            return (
              <button
                aria-label={
                  canPress
                    ? `Activate ${level === "scop" ? "super power" : "CO power"}: ${name}`
                    : `${name}: ${isOn ? "on" : isReady ? "ready" : `${formatStars(cost.remaining / meter.starCharge)} stars to go`}`
                }
                disabled={!canPress || power?.activating !== undefined}
                key={level}
                onClick={() => {
                  if (!canPress) return;
                  power?.onActivate?.(level);
                  onOpenChange(false);
                }}
                type="button"
                {...stylex.props(sheetStyles.power, isReady && sheetStyles.powerReady)}
              >
                <UiSprite name={level === "scop" ? "SuperPower.png" : "NormalPower.png"} />
                <Text color="inherit" type="inherit" xstyle={sheetStyles.powerName}>
                  {name}
                  {isOn
                    ? " · on"
                    : isReady
                      ? ""
                      : ` · ${formatStars(cost.remaining / meter.starCharge)}`}
                </Text>
              </button>
            );
          })}
        </Stack>
      ) : null}
    </ModalSheet>
  );
}

/** The power, armed from the phone dock: its name, effect and a gold Use key. */
export function ArmedPower({
  entry,
  level,
  onCancel,
  onUse,
}: {
  entry: PlayerRosterEntry;
  level: ActivatablePowerLevel;
  onCancel: () => void;
  onUse: () => void;
}) {
  return (
    <HStack as="span" xstyle={armedStyles.row}>
      <Stack as="span" xstyle={armedStyles.text}>
        <b {...stylex.props(armedStyles.name)}>{powerName(entry, level)}</b>{" "}
        {powerEffect(entry, level)}
      </Stack>
      <Button
        icon={<Icon icon={IconClose} />}
        isIconOnly
        label="Not now"
        onClick={onCancel}
        tooltip="Not now"
        variant="ghost-on-ink"
        xstyle={armedStyles.cancel}
      />
      <button onClick={onUse} type="button" {...stylex.props(armedStyles.use)}>
        Use
      </button>
    </HStack>
  );
}

/* ---------------------------------------------------------------- styles */

const meterStyles = stylex.create({
  row: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
  },
  meter: {
    display: "inline-flex",
    alignItems: "flex-end",
    gap: spacingVars["--spacing-0-5"],
    minInlineSize: 0,
  },
  meterSmall: {
    gap: borderVars["--border-width"],
  },
  reserve: {
    minBlockSize: matchLayout.starBig,
  },
  reserveSmall: {
    minBlockSize: matchLayout.starBigSmall,
  },
  starSlot: {
    display: "inline-flex",
    alignItems: "flex-end",
  },
  star: {
    display: "block",
    flexShrink: 0,
    inlineSize: matchLayout.star,
    blockSize: matchLayout.star,
    maskImage:
      "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'><path d='M12 1.5l3.2 6.9 7.3.8-5.4 5 1.5 7.3L12 17.8l-6.6 3.7 1.5-7.3-5.4-5 7.3-.8z'/></svg>\")",
    maskPosition: "center",
    maskRepeat: "no-repeat",
    maskSize: "contain",
  },
  starSmall: {
    inlineSize: matchLayout.starSmall,
    blockSize: matchLayout.starSmall,
  },
  starBig: {
    inlineSize: matchLayout.starBig,
    blockSize: matchLayout.starBig,
  },
  starBigSmall: {
    inlineSize: matchLayout.starBigSmall,
    blockSize: matchLayout.starBigSmall,
  },
  fill: (percent: number) => ({
    backgroundImage: `linear-gradient(90deg, ${awbrnVars.gold} ${percent}%, ${awbrnVars.goldDim} ${percent}%)`,
  }),
  fillPaper: (percent: number) => ({
    backgroundImage: `linear-gradient(90deg, ${awbrnVars.goldOnPaper} ${percent}%, ${awbrnVars.goldTrackOnPaper} ${percent}%)`,
  }),
  split: {
    display: "block",
    inlineSize: borderVars["--border-width"],
    blockSize: matchLayout.starBig,
    marginInline: spacingVars["--spacing-1"],
    backgroundColor: awbrnVars.ink600,
  },
  splitSmall: {
    blockSize: matchLayout.starBigSmall,
    marginInline: spacingVars["--spacing-0-5"],
  },
  splitPaper: {
    backgroundColor: awbrnVars.ruleStrong,
  },
  readout: {
    fontSize: matchType.figure,
    fontWeight: 600,
    color: awbrnVars.textOnInkSecondary,
  },
  readoutPaper: {
    color: colorVars["--color-text-secondary"],
  },
  readoutReady: {
    color: awbrnVars.gold,
  },
});

const powerStyles = stylex.create({
  key: {
    display: "inline-grid",
    placeItems: "center",
    flexShrink: 0,
    blockSize: matchLayout.railPowerKey,
    paddingInline: spacingVars["--spacing-1-5"],
    borderStyle: "none",
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.ink950,
    boxShadow: {
      default: `inset 0 0 0 1.5px ${awbrnVars.gold}`,
      ":hover": { default: null, [pageLayout.hoverMedia]: `inset 0 0 0 2px ${awbrnVars.gold}` },
      ":active": `inset 0 0 0 2px ${awbrnVars.gold}`,
    },
    cursor: "pointer",
  },
  row: {
    blockSize: matchLayout.rowPowerKey,
    paddingInline: spacingVars["--spacing-1"],
  },
  dock: {
    blockSize: matchLayout.dockKeyHeight,
    inlineSize: matchLayout.rowHeight,
    paddingInline: 0,
    borderRadius: radiusVars["--radius-element"],
  },
  armed: {
    boxShadow: `inset 0 0 0 3px ${awbrnVars.gold}`,
  },
});

const chipStyles = stylex.create({
  clock: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1-5"],
    flexShrink: 0,
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: spacingVars["--spacing-2"],
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.ink950,
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.detail,
    fontWeight: 600,
  },
  running: {
    color: awbrnVars.textOnInkStrong,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink600}`,
  },
  low: {
    color: awbrnVars.warnFill,
    boxShadow: `inset 0 0 0 ${borderVars["--border-width"]} color-mix(in srgb, ${awbrnVars.warnFill} 60%, transparent)`,
  },
  critical: {
    backgroundColor: awbrnVars.warnFill,
    color: awbrnVars.ink950,
  },
  you: {
    display: "inline-flex",
    alignItems: "center",
    flexShrink: 0,
    blockSize: matchLayout.youHeight,
    paddingInline: spacingVars["--spacing-1-5"],
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.ink100,
    color: awbrnVars.ink900,
    fontSize: matchType.micro,
    fontWeight: 750,
    letterSpacing: "0.04em",
  },
  youSmall: {
    blockSize: matchLayout.youHeightSmall,
    paddingInline: spacingVars["--spacing-1"],
  },
  dot: {
    display: "inline-block",
    flexShrink: 0,
    inlineSize: matchLayout.movingDot,
    blockSize: matchLayout.movingDot,
    borderRadius: "50%",
    backgroundColor: awbrnVars.okOnInk,
    boxShadow: `0 0 0 ${spacingVars["--spacing-0-5"]} color-mix(in srgb, ${awbrnVars.okOnInk} 22%, transparent)`,
  },
});

const cardStyles = stylex.create({
  card: {
    position: "relative",
    display: "grid",
    gap: awbrnVars.spacing2_5,
    paddingBlock: awbrnVars.spacing3_5,
    paddingInline: spacingVars["--spacing-4"],
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
    color: awbrnVars.textOnInk,
    // The card lays out from its own width: the rail, the full screen
    // surface and anything wider than either.
    containerType: "inline-size",
  },
  stampKey: {
    display: "inline-grid",
    padding: 0,
    borderStyle: "none",
    borderRadius: 0,
    backgroundColor: "transparent",
    cursor: "pointer",
  },
  top: {
    display: "grid",
    gridTemplateColumns: "auto minmax(0, 1fr) auto",
    alignItems: "center",
    gap: spacingVars["--spacing-3"],
  },
  identity: {
    display: "grid",
    gap: spacingVars["--spacing-1"],
    minInlineSize: 0,
  },
  name: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
    color: awbrnVars.textOnInkStrong,
    fontSize: matchType.cardName,
    fontWeight: 750,
  },
  nameText: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  struck: {
    color: awbrnVars.textOnInkSecondary,
    textDecorationLine: "line-through",
    textDecorationColor: awbrnVars.violetOnInk,
  },
  crestLine: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1-5"],
  },
  out: {
    color: awbrnVars.violetOnInk,
    fontSize: matchType.label,
    fontWeight: 700,
    letterSpacing: "0.04em",
    whiteSpace: "nowrap",
  },
  why: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.figure,
  },
  powerLine: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: awbrnVars.spacing2_5,
    minInlineSize: 0,
  },
  powerKeys: {
    display: "inline-flex",
    gap: spacingVars["--spacing-1-5"],
  },
  stats: {
    display: "grid",
    gridTemplateColumns: {
      default: "repeat(2, minmax(0, 1fr))",
      "@container (min-width: 520px)": "repeat(4, minmax(0, 1fr))",
    },
    gap: spacingVars["--spacing-1-5"],
  },
  stat: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
    paddingBlock: spacingVars["--spacing-2"],
    paddingInline: spacingVars["--spacing-2"],
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: `color-mix(in srgb, ${awbrnVars.ink950} 45%, transparent)`,
    color: awbrnVars.textOnInkStrong,
  },
  statPaper: {
    paddingBlock: awbrnVars.spacing2_5,
    paddingInline: spacingVars["--spacing-3"],
    backgroundColor: awbrnVars.paper,
    color: colorVars["--color-text-primary"],
  },
  statValue: {
    marginInlineStart: "auto",
    fontSize: matchType.detail,
    fontWeight: 600,
  },
});

/** The ledger's edge fade: half a row. */
const LEDGER_FADE = `calc(${matchLayout.rowHeight} / 2)`;

const ledgerStyles = stylex.create({
  // Its own scroller in the rail's column: it gives up height before the view
  // block and the log under it do, down to three rows, and your row stays
  // pinned to whichever edge it would leave by. It shows six whole rows at
  // most and half of a seventh, so a row cut by the edge says more armies lie
  // below; the space the cap leaves goes to the growing block under it.
  list: {
    flexShrink: 1,
    maxBlockSize: `calc(6.5 * ${matchLayout.rowHeight} + ${borderVars["--border-width"]})`,
    margin: 0,
    padding: 0,
    overflowY: "auto",
    overscrollBehavior: "contain",
    listStyle: "none",
    scrollbarColor: `${awbrnVars.ink700} transparent`,
    scrollbarWidth: "thin",
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  floor: (rows: number) => ({
    minBlockSize: `calc(${rows} * ${matchLayout.rowHeight})`,
  }),
  // While rows lie past an edge, the ledger fades into the rail's ink there
  // over half a row, so the row the edge cuts dissolves rather than stopping
  // dead against the block below. The fade holds to the scrolled edge and
  // sits under your pinned row, so your own row is never dimmed.
  fadeAbove: {
    "::before": {
      content: "''",
      position: "sticky",
      insetBlockStart: 0,
      zIndex: 1,
      display: "block",
      blockSize: LEDGER_FADE,
      marginBlockEnd: `calc(-1 * ${LEDGER_FADE})`,
      backgroundImage: `linear-gradient(180deg, ${awbrnVars.ink900}, transparent)`,
      pointerEvents: "none",
    },
  },
  fadeBelow: {
    "::after": {
      content: "''",
      position: "sticky",
      insetBlockEnd: 0,
      zIndex: 1,
      display: "block",
      blockSize: LEDGER_FADE,
      marginBlockStart: `calc(-1 * ${LEDGER_FADE})`,
      backgroundImage: `linear-gradient(0deg, ${awbrnVars.ink900}, transparent)`,
      pointerEvents: "none",
    },
  },
  row: {
    position: "relative",
    display: "grid",
    gridTemplateColumns: `${matchLayout.ledgerNumber} auto minmax(0, 1fr) auto`,
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    minBlockSize: matchLayout.rowHeight,
    paddingBlock: spacingVars["--spacing-2"],
    paddingInlineStart: awbrnVars.spacing2_5,
    paddingInlineEnd: spacingVars["--spacing-4"],
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: {
      default: borderVars["--border-width"],
      ":first-child": 0,
    },
    color: awbrnVars.textOnInk,
  },
  rowOut: {
    backgroundColor: awbrnVars.ink900,
  },
  pinned: {
    position: "sticky",
    insetBlockStart: 0,
    insetBlockEnd: 0,
    zIndex: 2,
  },
  number: {
    fontSize: matchType.ordinal,
    color: awbrnVars.textOnInkSecondary,
    textAlign: "end",
  },
  main: {
    display: "grid",
    gap: spacingVars["--spacing-1"],
    minInlineSize: 0,
  },
  name: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1-5"],
    minInlineSize: 0,
    color: awbrnVars.textOnInkStrong,
    fontSize: matchType.name,
    lineHeight: 1.15,
  },
  nameText: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontWeight: 750,
  },
  struck: {
    color: awbrnVars.textOnInkSecondary,
    textDecorationLine: "line-through",
    textDecorationColor: awbrnVars.violetOnInk,
  },
  line2: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    minInlineSize: 0,
  },
  end: {
    display: "grid",
    justifyItems: "end",
    gap: spacingVars["--spacing-1-5"],
  },
  endTop: {
    display: "inline-flex",
    gap: spacingVars["--spacing-1"],
  },
  figures: {
    display: "inline-flex",
    alignItems: "center",
    gap: awbrnVars.spacing2_5,
    color: awbrnVars.textOnInkStrong,
    fontSize: matchType.figure,
    fontWeight: 600,
  },
  figure: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1"],
  },
  out: {
    color: awbrnVars.violetOnInk,
    fontSize: matchType.label,
    fontWeight: 700,
    letterSpacing: "0.04em",
    whiteSpace: "nowrap",
  },
  why: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.figure,
  },
});

/** An army's wash: its faction color mixed into the ink it sits on. */
const washStyles = stylex.create({
  wash: (accent: string, percent: number, base: string) => ({
    backgroundColor: `color-mix(in oklab, ${accent} ${percent}%, ${base})`,
  }),
});

/** Past this width a plate puts its figures at the end of its name line. */
const PLATE_WIDE = "@container (min-width: 300px)";

const plateStyles = stylex.create({
  plate: {
    position: "relative",
    display: "block",
    minInlineSize: 0,
    paddingBlock: awbrnVars.spacing2_5,
    paddingInline: spacingVars["--spacing-3"],
    borderStyle: "none",
    color: "inherit",
    fontFamily: "inherit",
    textAlign: "start",
    cursor: "pointer",
    containerType: "inline-size",
  },
  plateWide: {
    inlineSize: "100%",
    paddingBlock: spacingVars["--spacing-2"],
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  // Narrow: the name beside the stamp, the meter and the figures each on a
  // clear row under both. Wide: the name over the meter beside the stamp, the
  // figures at the end. A row with nothing in it (a fallen army's figures, a
  // record-only army's meter) takes no room.
  layout: {
    display: "grid",
    gridTemplateColumns: {
      default: "auto minmax(0, 1fr)",
      [PLATE_WIDE]: "auto minmax(0, 1fr) auto",
    },
    gridTemplateAreas: {
      default: '"stamp name" "meter meter" "figures figures"',
      [PLATE_WIDE]: '"stamp name figures" "stamp meter figures"',
    },
    alignItems: "center",
    columnGap: spacingVars["--spacing-2"],
    rowGap: {
      default: spacingVars["--spacing-2"],
      [PLATE_WIDE]: spacingVars["--spacing-1"],
    },
  },
  /** Only the stamp and the name: no empty rows, and no gaps around them. */
  layoutRecordOnly: {
    gridTemplateColumns: "auto minmax(0, 1fr)",
    gridTemplateAreas: '"stamp name"',
  },
  stamp: {
    gridArea: "stamp",
    display: "inline-grid",
  },
  nameAlone: {
    alignSelf: "center",
  },
  name: {
    gridArea: "name",
    alignSelf: {
      default: "center",
      [PLATE_WIDE]: "end",
    },
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1"],
    minInlineSize: 0,
    color: awbrnVars.textOnInkStrong,
    fontSize: matchType.name,
    fontWeight: 750,
    lineHeight: 1.15,
  },
  nameText: {
    minInlineSize: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  meter: {
    gridArea: "meter",
    alignSelf: {
      default: "center",
      [PLATE_WIDE]: "start",
    },
    minInlineSize: 0,
  },
  out: {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    color: awbrnVars.violetOnInk,
    fontSize: matchType.figure,
    fontWeight: 700,
  },
  figures: {
    gridArea: "figures",
    display: {
      default: "flex",
      [PLATE_WIDE]: "grid",
    },
    alignItems: "center",
    justifyContent: "space-between",
    justifyItems: "end",
    gap: spacingVars["--spacing-1-5"],
    fontSize: {
      default: matchType.figure,
      [PLATE_WIDE]: matchType.detail,
    },
  },
  funds: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1"],
    color: awbrnVars.textOnInkStrong,
    fontSize: "inherit",
    fontWeight: 600,
  },
});

/** The band's thickness along the turn-order row's foot edge, in px. */
const ORDER_BAND = 4;

const orderStyles = stylex.create({
  // The row holds the band, so it runs under your pinned cell too; the track
  // inside it is what scrolls.
  row: {
    position: "relative",
    display: "block",
    backgroundColor: awbrnVars.ink900,
    borderBlockEndColor: awbrnVars.ink800,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  track: {
    display: "grid",
    overflowX: "auto",
    overscrollBehaviorX: "contain",
    scrollbarWidth: "none",
  },
  columns: (count: number) => ({
    gridTemplateColumns: `repeat(${count}, minmax(56px, 1fr))`,
  }),
  cell: {
    position: "relative",
    display: "grid",
    justifyItems: "center",
    alignContent: "start",
    gap: spacingVars["--spacing-1"],
    minInlineSize: matchLayout.cellMinWidth,
    minBlockSize: matchLayout.rowHeight,
    paddingBlock: `${spacingVars["--spacing-2"]} ${spacingVars["--spacing-1-5"]}`,
    paddingInline: spacingVars["--spacing-0-5"],
    borderStyle: "none",
    boxShadow: {
      default: `inset 1px 0 0 ${awbrnVars.ink800}`,
      ":first-child": "none",
    },
    color: awbrnVars.textOnInkSecondary,
    fontFamily: "inherit",
    cursor: "pointer",
  },
  wash: (accent: string, percent: number) => ({
    backgroundColor: `color-mix(in oklab, ${accent} ${percent}%, ${awbrnVars.ink900})`,
  }),
  cellActive: {
    color: awbrnVars.textOnInkStrong,
  },
  /** The mover is scrolled clear of your pinned cell rather than under it. */
  rowLong: {
    scrollPaddingInline: matchLayout.rowHeight,
  },
  /** Your cell in a long row: held at whichever edge it would leave by. */
  cellPinned: {
    position: "sticky",
    insetInlineStart: 0,
    insetInlineEnd: 0,
    zIndex: 2,
    boxShadow: `inset ${borderVars["--border-width"]} 0 0 ${awbrnVars.ink800}, inset calc(-1 * ${borderVars["--border-width"]}) 0 0 ${awbrnVars.ink800}, 0 0 ${spacingVars["--spacing-3"]} ${spacingVars["--spacing-0-5"]} color-mix(in srgb, ${awbrnVars.ink950} 60%, transparent)`,
  },
  stampWrap: {
    position: "relative",
    display: "inline-grid",
  },
  tag: {
    position: "absolute",
    insetBlockStart: `calc(-1 * ${spacingVars["--spacing-1-5"]})`,
    insetInlineEnd: `calc(-1 * ${awbrnVars.spacing2_5})`,
    zIndex: 1,
    paddingBlock: borderVars["--border-width"],
    paddingInline: spacingVars["--spacing-1"],
    borderRadius: radiusVars["--radius-inner"],
    backgroundColor: awbrnVars.ink100,
    color: awbrnVars.ink900,
    fontSize: matchType.tag,
    fontWeight: 750,
    letterSpacing: "0.03em",
    lineHeight: 1.2,
  },
  tagOut: {
    backgroundColor: awbrnVars.violetOnInk,
    color: awbrnVars.ink950,
  },
  // One short line, condensed, so the row keeps its height; a long handle
  // ends in an ellipsis (the cell's label and the army sheet name it whole).
  name: {
    display: "block",
    maxInlineSize: "100%",
    overflow: "hidden",
    paddingInline: spacingVars["--spacing-1"],
    textAlign: "center",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontSize: matchType.micro,
    fontStretch: "85%",
    fontWeight: 650,
    lineHeight: 1.1,
  },
  nameBright: {
    color: awbrnVars.textOnInkStrong,
  },
  nameOut: {
    textDecorationLine: "line-through",
    textDecorationColor: awbrnVars.violetOnInk,
  },
  // A pinned cell (z 2) covers a foot that scrolls under it.
  foot: {
    position: "absolute",
    zIndex: 1,
    insetBlockEnd: 0,
    insetInlineStart: "50%",
    inlineSize: matchLayout.footWidth,
    blockSize: matchLayout.moverFootHeight,
    translate: "-50% 0",
    borderRadius: `${radiusVars["--radius-inner"]} ${radiusVars["--radius-inner"]} 0 0`,
    backgroundColor: awbrnVars.okOnInk,
  },
  // While cells lie past a side, the row fades into the table there as the
  // board does. Your pinned cell (z 2) stands over the fade.
  fade: {
    position: "absolute",
    zIndex: 1,
    insetBlock: 0,
    inlineSize: matchLayout.fadeDepth,
    pointerEvents: "none",
  },
  fadeStart: {
    insetInlineStart: 0,
    backgroundImage: `linear-gradient(90deg, ${matchLayout.boardFade}, transparent)`,
  },
  fadeEnd: {
    insetInlineEnd: 0,
    backgroundImage: `linear-gradient(-90deg, ${matchLayout.boardFade}, transparent)`,
  },
  // On your turn the band runs along the row's foot edge (over a pinned cell
  // too), and your green foot stands on it so the row still says who moves.
  footOverBand: {
    insetBlockEnd: `${ORDER_BAND}px`,
  },
  band: {
    position: "absolute",
    zIndex: 3,
    insetInline: 0,
    insetBlockEnd: 0,
    pointerEvents: "none",
  },
});

const sheetStyles = stylex.create({
  who: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    paddingBlock: spacingVars["--spacing-3"],
    paddingInline: spacingVars["--spacing-4"],
    borderBlockEndColor: awbrnVars.rule,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
    color: colorVars["--color-text-secondary"],
    fontSize: matchType.small,
  },
  out: {
    marginInlineStart: "auto",
    color: awbrnVars.violet,
    fontWeight: 700,
  },
  meterRow: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: spacingVars["--spacing-3"],
    paddingBlock: `${awbrnVars.spacing3_5} ${spacingVars["--spacing-3"]}`,
    paddingInline: spacingVars["--spacing-4"],
    borderBlockEndColor: awbrnVars.rule,
    borderBlockEndStyle: "solid",
    borderBlockEndWidth: borderVars["--border-width"],
  },
  meterReadout: {
    color: colorVars["--color-text-secondary"],
    fontSize: matchType.body,
    fontWeight: 600,
  },
  stats: {
    display: "block",
    paddingBlock: awbrnVars.spacing3_5,
    paddingInline: spacingVars["--spacing-4"],
  },
  powers: {
    display: "grid",
    gridTemplateColumns: "1fr 1fr",
    gap: spacingVars["--spacing-2"],
    paddingBlockEnd: spacingVars["--spacing-4"],
    paddingInline: spacingVars["--spacing-4"],
  },
  power: {
    display: "grid",
    justifyItems: "start",
    alignContent: "center",
    gap: spacingVars["--spacing-1"],
    minBlockSize: matchLayout.footHeight,
    paddingBlock: spacingVars["--spacing-2"],
    paddingInline: spacingVars["--spacing-3"],
    borderColor: awbrnVars.ruleStrong,
    borderRadius: radiusVars["--radius-element"],
    borderStyle: "solid",
    borderWidth: borderVars["--border-width"],
    backgroundColor: awbrnVars.paperWell,
    color: colorVars["--color-text-secondary"],
    fontFamily: "inherit",
    textAlign: "start",
    cursor: {
      default: "pointer",
      ":disabled": "default",
    },
  },
  powerReady: {
    borderColor: awbrnVars.ink950,
    backgroundColor: awbrnVars.ink950,
    boxShadow: `inset 0 0 0 1.5px ${awbrnVars.gold}`,
    color: awbrnVars.gold,
  },
  powerName: {
    fontSize: matchType.detail,
    fontWeight: 500,
  },
});

const armedStyles = stylex.create({
  row: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-2"],
    flexGrow: 1,
    minInlineSize: 0,
    minBlockSize: matchLayout.target,
    paddingInlineStart: spacingVars["--spacing-3"],
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.ink950,
    boxShadow: `inset 0 0 0 1.5px ${awbrnVars.gold}`,
  },
  text: {
    flexGrow: 1,
    minInlineSize: 0,
    overflow: "hidden",
    display: "-webkit-box",
    WebkitLineClamp: 2,
    WebkitBoxOrient: "vertical",
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.note,
    lineHeight: 1.3,
  },
  name: {
    color: awbrnVars.textOnInkStrong,
    fontWeight: 700,
  },
  cancel: {
    flexShrink: 0,
  },
  use: {
    flexShrink: 0,
    alignSelf: "stretch",
    minInlineSize: matchLayout.dockKeyWidth,
    borderStyle: "none",
    borderRadius: radiusVars["--radius-element"],
    backgroundColor: awbrnVars.gold,
    color: awbrnVars.ink950,
    fontFamily: "inherit",
    fontSize: matchType.body,
    fontWeight: 750,
    cursor: "pointer",
  },
});
