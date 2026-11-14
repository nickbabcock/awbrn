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

export interface ArmyClock {
  /** `3m 42s`, `5d 11h`. */
  value: string;
  /** What a screen reader hears. */
  label: string;
  /** The tooltip: the exact span left, or when the running clock runs out. */
  detail?: string;
  isRunning: boolean;
  pressure: "steady" | "low" | "critical";
}

/** One army, with what this screen knows about it beyond the engine's entry. */
export interface ArmyView {
  army: MatchArmy;
  isViewer: boolean;
  clock: ArmyClock | null;
  fall?: ArmyFall | null;
  /** Changes how this army is drawn (its depicted faction). */
  onFactionChange?: (factionId: number) => void;
  /**
   * The page will never read this army's state (a signed-out viewer), so it
   * holds no room for a meter or figures that are not coming.
   */
  isRecordOnly?: boolean;
}

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

/** The CO, and the tag CO when the army has one. */
function coName(entry: PlayerRosterEntry): string {
  const co = entry.coName ?? "No commander";
  return entry.tagCoName ? `${co} and ${entry.tagCoName}` : co;
}

/** `Team A`, or null outside a team game. */
function teamName(entry: PlayerRosterEntry): string | null {
  return entry.team ? `Team ${entry.team}` : null;
}

function TeamMark({ entry }: { entry: PlayerRosterEntry }) {
  const team = teamName(entry);
  if (team === null) return null;
  return (
    <Text color="inherit" type="inherit" xstyle={textStyles.team}>
      {team}
    </Text>
  );
}

/** An army's faction color mixed into the ink under it. */
function armyWash(factionCode: string, percent: number, isOwed = false) {
  return washStyles.wash(
    factionColorVar(factionCode, "accent"),
    isOwed ? 18 : percent,
    isOwed ? awbrnVars.ink850 : awbrnVars.ink900,
  );
}

/* ---------------------------------------------------------------- meter */

/** The CO power meter in stars: small stars buy the power, large the super power. */
function StarMeter({
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
  const isSmall = size === "sm";
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
        xstyle={[meterStyles.meter, isSmall && meterStyles.meterSmall]}
      >
        {stars.map((star, index) => (
          <HStack as="span" key={index} xstyle={meterStyles.starSlot}>
            {star.split ? (
              <i
                {...stylex.props(
                  meterStyles.split,
                  isSmall && meterStyles.splitSmall,
                  tone === "paper" && meterStyles.splitPaper,
                )}
              />
            ) : null}
            <i
              {...stylex.props(
                meterStyles.star,
                star.big
                  ? isSmall
                    ? meterStyles.starBigSmall
                    : meterStyles.starBig
                  : isSmall && meterStyles.starSmall,
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

/** The power key: the game's own POWER or SUPER sprite on ink with a gold ring. */
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
      <UiSprite name={level === "scop" ? "SuperPower.png" : "NormalPower.png"} />
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

function coPower(entry: PlayerRosterEntry, level: ActivatablePowerLevel) {
  const abilities = entry.coKey ? getCoAbilities(entry.coKey) : null;
  return level === "scop" ? abilities?.superPower : abilities?.power;
}

export function powerName(entry: PlayerRosterEntry, level: ActivatablePowerLevel): string {
  return coPower(entry, level)?.name ?? (level === "scop" ? "Super power" : "Power");
}

function activateLabel(entry: PlayerRosterEntry, level: ActivatablePowerLevel): string {
  return `Activate ${level === "scop" ? "super power" : "CO power"}: ${powerName(entry, level)}`;
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
  return readyPowers(entry).map((level) => (
    <PowerKey
      isLoading={power.activating === level}
      key={level}
      label={activateLabel(entry, level)}
      level={level}
      onPress={() => onActivate(level)}
      size={size}
    />
  ));
}

/* ---------------------------------------------------------------- small parts */

function ClockChip({ clock }: { clock: ArmyClock }) {
  // Raw span: Astryx components drop `title`.
  return (
    <span
      aria-label={clock.label}
      role="timer"
      title={clock.detail}
      {...stylex.props(
        figureStyles.figure,
        chipStyles.clock,
        clock.isRunning && chipStyles.running,
        clock.pressure === "low" && chipStyles.low,
        clock.pressure === "critical" && chipStyles.critical,
      )}
    >
      <UiSprite name="Clock.png" />
      <Text color="inherit" type="inherit" aria-hidden="true">
        {clock.value}
      </Text>
    </span>
  );
}

function YouTag({ isSmall = false }: { isSmall?: boolean }) {
  return (
    <HStack as="span" xstyle={[chipStyles.you, isSmall && chipStyles.youSmall]}>
      YOU
    </HStack>
  );
}

function MovingDot() {
  return <Stack as="span" aria-label="On the move" role="img" xstyle={chipStyles.dot} />;
}

function ArmyStamp({
  entry,
  hasTag = false,
  size,
}: {
  entry: PlayerRosterEntry;
  /** Sets the tag CO's stamp beside the CO's, when the army has one. */
  hasTag?: boolean;
  size: "sm" | "md";
}) {
  const state = entry.eliminated ? "canceled" : "default";
  const stamp = (
    <CoStamp
      coKey={entry.coKey}
      factionCode={entry.displayFactionCode}
      label={entry.coName ?? "No commander"}
      size={size}
      state={state}
    />
  );
  if (!hasTag || !entry.tagCoKey) return stamp;
  return (
    <HStack as="span" xstyle={chipStyles.stamps}>
      {stamp}
      <CoStamp
        coKey={entry.tagCoKey}
        factionCode={entry.displayFactionCode}
        label={`Tag: ${entry.tagCoName ?? "unknown CO"}`}
        size="sm"
        state={state}
      />
    </HStack>
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
  if (!onOpen) return <ArmyStamp entry={entry} hasTag size={size} />;
  return (
    <button
      aria-haspopup="dialog"
      aria-label={`${name}, ${coName(entry)}: open army details`}
      onClick={onOpen}
      type="button"
      {...stylex.props(cardStyles.stampKey)}
    >
      <ArmyStamp entry={entry} hasTag size={size} />
    </button>
  );
}

/** Funds or units as a sprite and a figure, left out until the engine reports it. */
function Count({ entry, of }: { entry: PlayerRosterEntry; of: "funds" | "units" }) {
  const value = of === "funds" ? entry.stats.funds : entry.stats.unitCount;
  if (value === undefined) return null;
  return (
    <HStack as="span" xstyle={[figureStyles.figure, chipStyles.count]}>
      {of === "funds" ? (
        <UiSprite name="Coin.png" />
      ) : (
        <StatIcon factionCode={entry.displayFactionCode} />
      )}
      <VisuallyHidden>{of === "funds" ? "Funds " : "Units "}</VisuallyHidden>
      {formatFigure(value)}
    </HStack>
  );
}

/** The four figures as tiles. A figure the engine has not reported is left out. */
function ArmyStats({ entry, tone = "ink" }: { entry: PlayerRosterEntry; tone?: "ink" | "paper" }) {
  const { funds, income, unitCount, unitValue } = entry.stats;
  const code = entry.displayFactionCode;
  const tiles = [
    { label: "Funds", value: funds, icon: <StatIcon spriteName="Coin.png" /> },
    {
      label: "Income",
      value: income,
      sign: "+",
      icon: <StatIcon spriteName="BuildingsCaptured.png" />,
    },
    { label: "Units", value: unitCount, icon: <StatIcon factionCode={code} /> },
    { label: "Unit value", value: unitValue, icon: <StatIcon coinOverlay factionCode={code} /> },
  ].filter((tile) => tile.value !== undefined);
  if (tiles.length === 0) return null;
  return (
    <Stack as="span" xstyle={cardStyles.stats}>
      {tiles.map(({ icon, label, sign, value }) => (
        // Raw span: Astryx components drop `title`, and this tooltip is its label.
        <span
          key={label}
          title={label}
          {...stylex.props(cardStyles.stat, tone === "paper" && cardStyles.statPaper)}
        >
          {icon}
          <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, cardStyles.statValue]}>
            <VisuallyHidden>{label}: </VisuallyHidden>
            {`${sign ?? ""}${formatFigure(value)}`}
          </Text>
        </span>
      ))}
    </Stack>
  );
}

/* ---------------------------------------------------------------- rail */

/** Two armies in the rail: one card each. */
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
        armyWash(entry.displayFactionCode, army.isActive ? 16 : 9, isOwed),
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
              xstyle={[textStyles.ellipsis, entry.eliminated && textStyles.struck]}
            >
              {army.name}
            </Text>
            {isViewer ? <YouTag /> : null}
          </HStack>
          <HStack as="span" xstyle={cardStyles.crestLine}>
            <FactionCrest factionCode={entry.displayFactionCode} size={14} />
            <TeamMark entry={entry} />
            {entry.eliminated && fall?.reason ? (
              <Text color="inherit" type="inherit" xstyle={[textStyles.ellipsis, textStyles.why]}>
                {fall.reason}
              </Text>
            ) : null}
          </HStack>
        </Stack>
        {entry.eliminated ? (
          <Text color="inherit" type="inherit" xstyle={textStyles.out}>
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

/**
 * Three or more armies in the rail: one row each, in turn order. A fallen
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
  onOpen?: (playerId: number) => void;
  power?: PowerControl;
  views: ArmyView[];
}) {
  const isLong = views.length >= 7;
  const listRef = useRef<HTMLOListElement>(null);
  const more = useScrollEdges(listRef, "block");
  useKeepMoverInView(listRef, views, "block");

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
        const endTop =
          isViewer && power?.onActivate && readyPowers(entry).length > 0 ? (
            <PowerKeys entry={entry} power={power} size="row" />
          ) : clock && !clock.isRunning ? (
            <ClockChip clock={clock} />
          ) : null;
        return (
          <li
            aria-label={`${index + 1}. ${army.name}${isViewer ? " (you)" : ""}, ${coName(entry)}, ${entry.displayFactionName}${teamName(entry) ? `, ${teamName(entry)}` : ""}${entry.eliminated ? `, ${outMark(fall).toLowerCase()}${fall?.reason ? `, ${fall.reason}` : ""}` : ""}`}
            data-pinned={isLong && isViewer ? "" : undefined}
            data-player={entry.playerId}
            key={entry.playerId}
            {...stylex.props(
              ledgerStyles.row,
              armyWash(entry.displayFactionCode, army.isActive ? 16 : 8, isRowOwed),
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
            <Stack as="span" xstyle={cardStyles.identity}>
              <HStack as="span" xstyle={ledgerStyles.name}>
                {army.isActive ? <MovingDot /> : null}
                <b
                  {...stylex.props(
                    textStyles.ellipsis,
                    ledgerStyles.nameText,
                    entry.eliminated && textStyles.struck,
                  )}
                >
                  {army.name}
                </b>
                {isViewer ? <YouTag isSmall /> : null}
              </HStack>
              <HStack as="span" xstyle={ledgerStyles.line2}>
                <FactionCrest factionCode={entry.displayFactionCode} size={14} />
                <TeamMark entry={entry} />
                {entry.eliminated ? (
                  fall?.reason ? (
                    <Text
                      color="inherit"
                      type="inherit"
                      xstyle={[textStyles.ellipsis, textStyles.why]}
                    >
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
                <Text color="inherit" type="inherit" xstyle={textStyles.out}>
                  {outMark(fall)}
                </Text>
              ) : (
                <>
                  {endTop ? (
                    <HStack as="span" xstyle={ledgerStyles.endTop}>
                      {endTop}
                    </HStack>
                  ) : null}
                  {funds === undefined && unitCount === undefined ? null : (
                    <HStack as="span" xstyle={[figureStyles.figure, ledgerStyles.figures]}>
                      <Count entry={entry} of="funds" />
                      <Count entry={entry} of="units" />
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
 * it fades on that edge. `read` goes on the scroller's `onScroll`.
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

/**
 * Keeps the mover's `[data-player]` item in view as the turn passes along a
 * long scroller, clear of a `[data-pinned]` item at either edge. Only the
 * scroller moves: `scrollIntoView` would scroll the page along with it.
 */
function useKeepMoverInView(
  ref: RefObject<HTMLElement | null>,
  views: ArmyView[],
  axis: "block" | "inline",
) {
  const activeId = views.find((view) => view.army.isActive)?.army.entry.playerId;
  useEffect(() => {
    const scroller = ref.current;
    if (activeId === undefined || !scroller) return;
    const item = scroller.querySelector<HTMLElement>(`[data-player="${activeId}"]`);
    if (!item) return;
    const isBlock = axis === "block";
    const pinned = scroller.querySelector<HTMLElement>("[data-pinned]");
    const guard =
      pinned && pinned !== item ? (isBlock ? pinned.offsetHeight : pinned.offsetWidth) : 0;
    const shown = scroller.getBoundingClientRect();
    const place = item.getBoundingClientRect();
    const [start, end, itemStart, itemEnd] = isBlock
      ? [shown.top, shown.bottom, place.top, place.bottom]
      : [shown.left, shown.right, place.left, place.right];
    const delta =
      itemStart < start + guard
        ? itemStart - (start + guard)
        : itemEnd > end - guard
          ? itemEnd - (end - guard)
          : 0;
    if (delta === 0) return;
    if (isBlock) scroller.scrollTop += delta;
    else scroller.scrollLeft += delta;
  }, [activeId, axis, ref]);
}

/* ---------------------------------------------------------------- phone */

/**
 * One army as a phone plate; it opens the army sheet. The plate lays itself
 * out from its own width, not the screen's: narrow (two plates side by side)
 * or wide (one plate across).
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
      aria-label={`${army.name}${isViewer ? " (you)" : ""}, ${coName(entry)}${teamName(entry) ? `, ${teamName(entry)}` : ""}${entry.eliminated ? `, ${outMark(fall).toLowerCase()}` : ""}: open army details`}
      onClick={onOpen}
      type="button"
      {...stylex.props(
        plateStyles.plate,
        isWide && plateStyles.plateWide,
        armyWash(entry.displayFactionCode, army.isActive && !isWide ? 18 : 10, isOwed),
      )}
    >
      {isOwed ? <AirmailBand isOverlay thickness={4} /> : null}
      <Stack
        as="span"
        xstyle={[plateStyles.layout, view.isRecordOnly && plateStyles.layoutRecordOnly]}
      >
        <Stack as="span" xstyle={plateStyles.stamp}>
          <ArmyStamp entry={entry} hasTag size="sm" />
        </Stack>
        <HStack as="span" xstyle={[plateStyles.name, view.isRecordOnly && plateStyles.nameAlone]}>
          {army.isActive && showUnits ? <MovingDot /> : null}
          <FactionCrest factionCode={entry.displayFactionCode} size={14} />
          <Text
            color="inherit"
            type="inherit"
            xstyle={[textStyles.ellipsis, entry.eliminated && textStyles.struck]}
          >
            {army.name}
          </Text>
          {hasYouTag && isViewer ? <YouTag isSmall /> : null}
          <TeamMark entry={entry} />
        </HStack>
        {entry.eliminated ? (
          <Text
            color="inherit"
            type="inherit"
            xstyle={[plateStyles.meter, textStyles.ellipsis, plateStyles.out]}
          >
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
            <Count entry={entry} of="funds" />
            {showUnits || clock === null ? (
              <Count entry={entry} of="units" />
            ) : (
              <ClockChip clock={clock} />
            )}
          </HStack>
        )}
      </Stack>
    </button>
  );
}

/**
 * Three or more armies on a phone: the turn-order row. Past six armies it
 * scrolls sideways, keeping the mover and you in view. Any cell opens that
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
  const isLong = views.length >= 7;
  const more = useScrollEdges(rowRef, "inline");
  useKeepMoverInView(rowRef, views, "inline");

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
                armyWash(entry.displayFactionCode, army.isActive ? 22 : 8),
                army.isActive && orderStyles.cellActive,
                isLong && isViewer && orderStyles.cellPinned,
              )}
            >
              <Stack as="span" xstyle={orderStyles.stampWrap}>
                <ArmyStamp entry={entry} size="sm" />
                {entry.eliminated || isViewer ? (
                  <Text
                    color="inherit"
                    type="inherit"
                    xstyle={[orderStyles.tag, entry.eliminated && orderStyles.tagOut]}
                  >
                    {entry.eliminated ? "OUT" : "YOU"}
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

/** The army sheet a plate or a turn-order cell opens. */
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

  return (
    <ModalSheet
      headStart={<ArmyStamp entry={entry} hasTag size="sm" />}
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      title={view.isViewer ? `${view.army.name} (you)` : view.army.name}
    >
      <HStack as="span" xstyle={sheetStyles.who}>
        <FactionCrest
          factionCode={entry.displayFactionCode}
          onChange={view.onFactionChange}
          size={16}
        />
        {coName(entry)} · {entry.displayFactionName}
        {teamName(entry) ? ` · ${teamName(entry)}` : null}
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
          {(["cop", "scop"] as const).map((level) => {
            const cost = level === "scop" ? meter.scop : meter.cop;
            if (cost === null) return null;
            const isReady = canActivatePower(entry, level);
            const canPress = isReady && power?.onActivate !== undefined;
            const name = powerName(entry, level);
            const isOn = entry.activePower === level;
            const toGo = formatStars(cost.remaining / meter.starCharge);
            return (
              <button
                aria-label={
                  canPress
                    ? activateLabel(entry, level)
                    : `${name}: ${isOn ? "on" : isReady ? "ready" : `${toGo} stars to go`}`
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
                  {isOn ? " · on" : isReady ? "" : ` · ${toGo}`}
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
        {coPower(entry, level)?.effect}
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

const textStyles = stylex.create({
  ellipsis: {
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
  out: {
    color: awbrnVars.violetOnInk,
    fontSize: matchType.label,
    fontWeight: 700,
    letterSpacing: "0.04em",
    whiteSpace: "nowrap",
  },
  why: {
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.figure,
  },
  team: {
    flexShrink: 0,
    color: awbrnVars.textOnInkSecondary,
    fontSize: matchType.figure,
    whiteSpace: "nowrap",
  },
});

const washStyles = stylex.create({
  wash: (accent: string, percent: number, base: string) => ({
    backgroundColor: `color-mix(in oklab, ${accent} ${percent}%, ${base})`,
  }),
});

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
  stamps: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1"],
  },
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
  count: {
    display: "inline-flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1"],
    color: awbrnVars.textOnInkStrong,
    fontWeight: 600,
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
    // The stat grid widens from the card's own width, not the screen's.
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
  crestLine: {
    display: "flex",
    alignItems: "center",
    gap: spacingVars["--spacing-1-5"],
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
  // It gives up height before the view block and the log under it do, down to
  // three rows. It shows six whole rows at most and half of a seventh, so a row
  // cut by the edge says more armies lie below.
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
  // The fade holds to the scrolled edge and sits under your pinned row, so
  // your own row is never dimmed.
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
    fontWeight: 750,
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
    fontSize: matchType.figure,
  },
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
  meter: {
    gridArea: "meter",
    alignSelf: {
      default: "center",
      [PLATE_WIDE]: "start",
    },
    minInlineSize: 0,
  },
  out: {
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
  cellActive: {
    color: awbrnVars.textOnInkStrong,
  },
  /** The mover is scrolled clear of your pinned cell rather than under it. */
  rowLong: {
    scrollPaddingInline: matchLayout.rowHeight,
  },
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
  // Your pinned cell (z 2) stands over the fade.
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
  // The green foot stands on the band so the row still says who moves.
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
