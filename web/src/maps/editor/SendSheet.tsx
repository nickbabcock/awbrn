import { Icon } from "@astryxdesign/core/Icon";
import { Kbd } from "@astryxdesign/core/Kbd";
import { HStack, Stack } from "@astryxdesign/core/Stack";
import { colorVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import type { ReactNode } from "react";
import { CancelMark } from "#/components/CoStamp.tsx";
import { FactionLogo } from "#/components/FactionLogo.tsx";
import { getFactionByCode } from "#/factions.ts";
import { MapRankMedal } from "#/maps/components/MapRankMedal.tsx";
import {
  SAVE_FAILURE_TITLES,
  tileList,
  type EditorProblem,
  type SaveFailure,
} from "#/maps/map_editor.ts";
import type { MapRank } from "#/maps/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { AirmailBand } from "#/ui/AirmailBand.tsx";
import { Button } from "#/ui/Button.tsx";
import { ProblemBanner } from "#/ui/EmptySheet.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import {
  IconAlert,
  IconBack,
  IconExternal,
  IconNext,
  IconOk,
  IconOpenSeat,
  IconRedo,
  IconRefresh,
  IconUndo,
  IconWarn,
} from "#/ui/icons.tsx";

export function SendSheet({
  canRedo,
  canUndo,
  failure,
  focusedAt,
  isPristine,
  isSaving,
  isUnsaved,
  latestHref,
  notice,
  onRedo,
  onRetry,
  onSave,
  onStep,
  onUndo,
  problems,
  saveKey,
  unranks,
}: {
  canRedo: boolean;
  canUndo: boolean;
  failure: SaveFailure | null;
  /** The focused problem's place in `problems`. */
  focusedAt: number;
  isPristine: boolean;
  isSaving: boolean;
  isUnsaved: boolean;
  /** The map page, for a conflict's View latest. */
  latestHref: string | null;
  /** A failure the board reported, outside a save. */
  notice: string | null;
  onRedo: () => void;
  onRetry: () => void;
  onSave: () => void;
  onStep: (step: 1 | -1) => void;
  onUndo: () => void;
  problems: readonly EditorProblem[];
  saveKey: { label: string; isRed: boolean; isInert: boolean };
  /** The rank a save gives up, when it gives one up. */
  unranks: MapRank | null;
}) {
  const focused = problems[focusedAt] ?? null;

  return (
    <section aria-label="Save" {...stylex.props(styles.sheet, isUnsaved && styles.owed)}>
      {isUnsaved ? <AirmailBand isOverlay thickness={5} /> : null}
      {notice ? <ProblemBanner icon={<Icon icon={IconAlert} />} title={notice} /> : null}
      {failure ? (
        <ProblemBanner
          action={
            failure.kind === "changed" && latestHref ? (
              <Button
                as="a"
                href={latestHref}
                icon={<Icon icon={IconExternal} />}
                label="View latest"
                rel="noopener"
                size="sm"
                target="_blank"
                variant="secondary"
              />
            ) : failure.kind === "busy" || failure.kind === "failed" ? (
              <Button
                icon={<Icon icon={IconRefresh} />}
                label="Retry"
                onClick={onRetry}
                size="sm"
                variant="secondary"
              />
            ) : undefined
          }
          description={failure.kind === "refused" ? failure.reason : undefined}
          icon={<Icon icon={IconAlert} />}
          title={SAVE_FAILURE_TITLES[failure.kind]}
        />
      ) : null}
      {isPristine ? (
        <HStack align="center" gap={2} xstyle={[styles.verdict, styles.verdictNew]}>
          <Icon icon={IconOpenSeat} size="sm" />
          New board
        </HStack>
      ) : focused === null ? (
        <HStack align="center" gap={2} xstyle={[styles.verdict, styles.verdictOk]}>
          <Icon icon={IconOk} size="sm" />
          Playable
        </HStack>
      ) : (
        <Stack xstyle={styles.problems}>
          <HStack align="center" gap={2} xstyle={[styles.verdict, styles.verdictWarn]}>
            <Icon icon={IconWarn} size="sm" />
            {problems.length} to fix
          </HStack>
          <HStack align="center" gap={1.5}>
            <span {...stylex.props(figureStyles.figure, styles.count)}>
              {focusedAt + 1} / {problems.length}
            </span>
            <Button
              icon={<Icon icon={IconBack} />}
              isIconOnly
              label="Previous"
              onClick={() => onStep(-1)}
              size="sm"
              tooltip="Previous · Shift+N"
              variant="secondary"
              xstyle={styles.stepKey}
            />
            <Button
              label="Next"
              onClick={() => onStep(1)}
              size="sm"
              tooltip="Next · N"
              variant="secondary"
              xstyle={styles.stepKey}
            >
              <HStack align="center" as="span" gap={1.5}>
                <Icon icon={IconNext} size="sm" />
                <Kbd keys="n" />
              </HStack>
            </Button>
          </HStack>
          <HStack align="center" gap={2} xstyle={styles.focused}>
            <ProblemLine problem={focused} />
          </HStack>
        </Stack>
      )}
      {unranks !== null ? (
        <HStack align="center" gap={2} xstyle={styles.rankCue}>
          <Stack as="span" xstyle={styles.medalBox}>
            <MapRankMedal rank={unranks} size="xs" />
            <CancelMark xstyle={styles.cancel} />
          </Stack>
          Unranked
        </HStack>
      ) : null}
      <Stack xstyle={styles.keys}>
        <Button
          icon={<Icon icon={IconUndo} />}
          isDisabled={!canUndo}
          isIconOnly
          label="Undo"
          onClick={onUndo}
          tooltip="Undo · Ctrl+Z"
          variant="secondary"
          xstyle={styles.historyKey}
        />
        <Button
          icon={<Icon icon={IconRedo} />}
          isDisabled={!canRedo}
          isIconOnly
          label="Redo"
          onClick={onRedo}
          tooltip="Redo · Ctrl+Shift+Z or Ctrl+Y"
          variant="secondary"
          xstyle={styles.historyKey}
        />
        <Button
          isDisabled={saveKey.isInert}
          isLoading={isSaving}
          label={saveKey.label}
          onClick={onSave}
          variant={saveKey.isRed ? "primary" : "secondary"}
          xstyle={styles.saveKey}
        />
      </Stack>
    </section>
  );
}

function ProblemLine({ problem }: { problem: EditorProblem }): ReactNode {
  const tiles = (text: string) => (
    <span {...stylex.props(figureStyles.figure, styles.tiles)}>{text}</span>
  );
  const army = (faction: string) => (
    <>
      <FactionLogo factionCode={faction} isLabelHidden size={14} />
      {getFactionByCode(faction)?.displayName ?? faction}
    </>
  );
  switch (problem.kind) {
    case "uneven":
      return (
        <>
          <span aria-hidden="true" {...stylex.props(styles.swatch)} />
          Uneven {tiles(tileList(problem.tiles))}
        </>
      );
    case "extra-hq":
      return (
        <>
          {army(problem.faction)} · {problem.tiles.length} HQs {tiles(tileList(problem.tiles))}
        </>
      );
    case "no-hq":
      return <>{army(problem.faction)} · no HQ</>;
    case "no-build":
      return <>{army(problem.faction)} · can't build</>;
    case "empty":
      return (
        <>
          {problem.factions.map((faction) => (
            <FactionLogo factionCode={faction} isLabelHidden key={faction} size={14} />
          ))}
          {problem.factions.length} {problem.factions.length === 1 ? "seat" : "seats"} empty
        </>
      );
  }
}

const styles = stylex.create({
  sheet: {
    position: "relative",
    display: "grid",
    flexShrink: 0,
    gap: { default: "8px", [pageLayout.shortMedia]: "6px" },
    paddingBlock: { default: "10px 12px", [pageLayout.shortMedia]: "8px 12px" },
    paddingInline: "12px",
    overflow: "hidden",
    borderRadius: "10px",
    backgroundColor: awbrnVars.sheet,
    boxShadow: `${awbrnVars.shadowSheet}, 0 0 0 1px ${awbrnVars.rule}`,
  },
  owed: {
    paddingBlockStart: { default: "17px", [pageLayout.shortMedia]: "14px" },
  },
  verdict: {
    minBlockSize: { default: "32px", [pageLayout.shortMedia]: "28px" },
    fontSize: awbrnVars.text14_5,
    fontWeight: 650,
  },
  verdictNew: {
    color: awbrnVars.textTertiary,
  },
  verdictOk: {
    color: awbrnVars.ok,
  },
  verdictWarn: {
    color: awbrnVars.warn,
  },
  problems: {
    display: "grid",
    gridTemplateColumns: "minmax(0, 1fr) auto",
    alignItems: "center",
    rowGap: "4px",
  },
  count: {
    color: colorVars["--color-text-secondary"],
  },
  stepKey: {
    blockSize: "32px",
    minInlineSize: "34px",
  },
  focused: {
    gridColumn: "1 / -1",
    paddingInlineStart: "28px",
    color: colorVars["--color-text-primary"],
    fontSize: awbrnVars.text14,
    fontWeight: 600,
    whiteSpace: "nowrap",
  },
  tiles: {
    color: colorVars["--color-text-secondary"],
  },
  swatch: {
    flexShrink: 0,
    inlineSize: "16px",
    blockSize: "16px",
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink950}, inset 0 0 0 4px ${awbrnVars.warnFill}, inset 0 0 0 5px ${awbrnVars.ink950}`,
  },
  rankCue: {
    paddingBlockStart: "10px",
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
    color: colorVars["--color-text-secondary"],
    fontFamily: awbrnVars.fontFigure,
    fontSize: awbrnVars.text12_5,
    fontWeight: 500,
  },
  medalBox: {
    position: "relative",
    display: "inline-grid",
    marginInline: "8px 6px",
  },
  cancel: {
    position: "absolute",
    insetBlockStart: "1px",
    insetInlineStart: "-8px",
    inlineSize: "38px",
    blockSize: "24px",
  },
  keys: {
    display: "grid",
    gridTemplateColumns: "44px 44px 1fr",
    gap: "8px",
  },
  historyKey: {
    inlineSize: "44px",
    minInlineSize: "44px",
    blockSize: { default: "48px", [pageLayout.shortMedia]: "44px" },
    paddingInline: 0,
  },
  saveKey: {
    blockSize: { default: "48px", [pageLayout.shortMedia]: "44px" },
    fontSize: awbrnVars.text16,
    fontWeight: 700,
  },
});
