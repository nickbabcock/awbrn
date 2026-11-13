import { Icon } from "@astryxdesign/core/Icon";
import { HStack, VStack } from "@astryxdesign/core/Stack";
import { Text } from "@astryxdesign/core/Text";
import * as stylex from "@stylexjs/stylex";
import type { MapCatalogEntry } from "#/maps/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { figureStyles } from "#/ui/Figure.tsx";
import { IconBack, IconNext, IconTurnBack, IconTurnNext } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { DepictedBoard } from "./DepictedBoard.tsx";
import type { DepictedSkirmish } from "./depicted.ts";

const noop = () => {};

/** The depicted replay's place: day 12 of 21, action 214 of 486. */
const DAYS = 21;
const POSITION = { index: 214, total: 486 };

/**
 * A replay as a picture: a crop of the board mid-game over the review bar
 * (step keys, the scrubber with a tick per day, the position). Inert.
 */
export function DepictedReplay({
  factionCodes,
  map,
  skirmish,
}: {
  factionCodes: readonly [string, string];
  map: MapCatalogEntry;
  skirmish: DepictedSkirmish | null;
}) {
  const played = (POSITION.index / POSITION.total) * 100;
  return (
    <figure
      aria-label={`${map.name}, day 12, action ${POSITION.index} of ${POSITION.total}`}
      role="img"
      {...stylex.props(styles.figure)}
    >
      <VStack inert xstyle={styles.frame}>
        <VStack xstyle={styles.board}>
          <DepictedBoard alt="" factionCodes={factionCodes} map={map} skirmish={skirmish} />
        </VStack>
        <HStack xstyle={styles.bar}>
          <HStack as="span" xstyle={styles.keys}>
            <StepKey icon={IconTurnBack} label="Previous turn" />
            <StepKey icon={IconBack} label="Previous action" />
            <StepKey icon={IconNext} label="Next action" />
            <StepKey icon={IconTurnNext} label="Next turn" />
          </HStack>
          <VStack as="span" xstyle={styles.track}>
            <i {...stylex.props(styles.played, styles.playedTo(played))} />
            {Array.from({ length: DAYS - 1 }, (_, day) => (
              <s
                key={day}
                {...stylex.props(styles.tick, styles.tickAt(((day + 1) / DAYS) * 100))}
              />
            ))}
          </VStack>
          <Text color="inherit" type="inherit" xstyle={[figureStyles.figure, styles.position]}>
            {POSITION.index} / {POSITION.total}
          </Text>
        </HStack>
      </VStack>
    </figure>
  );
}

function StepKey({ icon, label }: { icon: typeof IconBack; label: string }) {
  return (
    <Button
      icon={<Icon icon={icon} />}
      isIconOnly
      label={label}
      onClick={noop}
      size="sm"
      variant="on-ink"
      xstyle={styles.key}
    />
  );
}

const styles = stylex.create({
  figure: {
    margin: 0,
    justifySelf: {
      default: "end",
      [pageLayout.belowRailMedia]: "start",
    },
    inlineSize: {
      default: "560px",
      [pageLayout.phoneMedia]: "100%",
    },
    maxInlineSize: "100%",
    userSelect: "none",
    WebkitUserSelect: "none",
    cursor: "default",
  },
  frame: {
    overflow: "hidden",
    borderRadius: "10px",
    boxShadow: awbrnVars.shadowBoard,
  },
  board: {
    blockSize: {
      default: "336px",
      [pageLayout.phoneMedia]: "256px",
    },
  },
  bar: {
    display: "flex",
    flexWrap: {
      default: "nowrap",
      [pageLayout.phoneMedia]: "wrap",
    },
    alignItems: "center",
    columnGap: "12px",
    rowGap: "10px",
    paddingBlock: "8px",
    paddingInline: "16px",
    backgroundColor: awbrnVars.ink900,
    borderBlockStartColor: awbrnVars.ink800,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: "1px",
  },
  keys: {
    display: "flex",
    gap: "4px",
  },
  key: {
    inlineSize: "36px",
    minInlineSize: "36px",
    blockSize: "34px",
    paddingInline: 0,
  },
  track: {
    position: "relative",
    flexGrow: 1,
    flexBasis: {
      default: "auto",
      [pageLayout.phoneMedia]: "100%",
    },
    order: {
      default: 0,
      [pageLayout.phoneMedia]: 3,
    },
    minInlineSize: "80px",
    blockSize: "6px",
    marginBlock: "5px",
    borderRadius: "3px",
    backgroundColor: awbrnVars.ink800,
  },
  played: {
    position: "absolute",
    insetBlock: 0,
    insetInlineStart: 0,
    borderRadius: "3px",
    backgroundColor: awbrnVars.ink300,
  },
  playedTo: (percent: number) => ({
    inlineSize: `${percent}%`,
  }),
  tick: {
    position: "absolute",
    insetBlockStart: "-5px",
    inlineSize: "2px",
    blockSize: "16px",
    backgroundColor: awbrnVars.ink600,
  },
  tickAt: (percent: number) => ({
    insetInlineStart: `${percent}%`,
  }),
  position: {
    marginInlineStart: {
      default: 0,
      [pageLayout.phoneMedia]: "auto",
    },
    fontSize: awbrnVars.text12_5,
    fontWeight: 500,
    color: awbrnVars.textOnInkSecondary,
    whiteSpace: "nowrap",
  },
});
