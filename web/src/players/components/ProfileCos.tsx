import { Heading } from "@astryxdesign/core/Heading";
import { Text } from "@astryxdesign/core/Text";
import { VisuallyHidden } from "@astryxdesign/core/VisuallyHidden";
import { spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useId } from "react";
import { getCoById } from "#/co_roster.ts";
import { CoStamp } from "#/components/CoStamp.tsx";
import { getFactionById } from "#/factions.ts";
import { headingLooks } from "#/play/headings.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { figureStyles } from "#/ui/Figure.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";
import { Sheet, SheetHead } from "#/ui/Sheet.tsx";
import type { ProfileCo } from "../schemas.ts";

/** `41–22`, with draws only when there are any (`28–19–1`). */
function formatCoRecord(co: Pick<ProfileCo, "wins" | "losses" | "draws">): string {
  return [co.wins, co.losses, ...(co.draws > 0 ? [co.draws] : [])].join("–");
}

function recordLabel(co: ProfileCo): string {
  const parts = [
    `${co.wins} ${co.wins === 1 ? "win" : "wins"}`,
    `${co.losses} ${co.losses === 1 ? "loss" : "losses"}`,
  ];
  if (co.draws > 0) parts.push(`${co.draws} ${co.draws === 1 ? "draw" : "draws"}`);
  return parts.join(", ");
}

/** The COs the player picks most, each on their most used army, with their record. */
export function ProfileCos({ cos }: { cos: readonly ProfileCo[] }) {
  const headingId = useId();
  return (
    <Sheet aria-labelledby={headingId}>
      <SheetHead>
        <Heading id={headingId} level={2} xstyle={headingLooks.title}>
          COs
        </Heading>
      </SheetHead>
      <ul {...stylex.props(styles.list)}>
        {cos.map((co) => {
          const coKey = getCoById(co.coId)?.key ?? null;
          const factionCode = getFactionById(co.factionId)?.code ?? null;
          return (
            <li key={co.coId} {...stylex.props(styles.co)}>
              <CoStamp coKey={coKey} factionCode={factionCode} size="md" xstyle={styles.notPhone} />
              <CoStamp
                coKey={coKey}
                factionCode={factionCode}
                size="sm"
                xstyle={styles.phoneOnly}
              />
              <Text
                aria-hidden="true"
                color="inherit"
                type="inherit"
                xstyle={[figureStyles.figure, styles.record]}
              >
                {formatCoRecord(co)}
              </Text>
              <VisuallyHidden>{recordLabel(co)}</VisuallyHidden>
            </li>
          );
        })}
      </ul>
    </Sheet>
  );
}

const styles = stylex.create({
  list: {
    display: "grid",
    gridTemplateColumns: {
      default: "repeat(3, minmax(0, 1fr))",
      [pageLayout.phoneMedia]: "repeat(3, auto)",
    },
    justifyContent: {
      default: null,
      [pageLayout.phoneMedia]: "start",
    },
    gap: {
      default: spacingVars["--spacing-3"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-5"],
    },
    margin: 0,
    paddingBlock: {
      default: `${spacingVars["--spacing-1"]} ${spacingVars["--spacing-5"]}`,
      [pageLayout.phoneMedia]: `${spacingVars["--spacing-0-5"]} ${awbrnVars.spacing3_5}`,
    },
    paddingInline: {
      default: spacingVars["--spacing-5"],
      [pageLayout.phoneMedia]: spacingVars["--spacing-4"],
    },
    listStyle: "none",
  },
  co: {
    display: {
      default: "grid",
      [pageLayout.phoneMedia]: "flex",
    },
    justifyItems: "center",
    alignItems: "center",
    gap: {
      default: awbrnVars.spacing2_5,
      [pageLayout.phoneMedia]: spacingVars["--spacing-1-5"],
    },
  },
  record: {
    fontSize: awbrnVars.text15,
    fontWeight: 600,
  },
  // A stamp is an inline grid (`StampFrame`).
  notPhone: {
    display: {
      default: "inline-grid",
      [pageLayout.phoneMedia]: "none",
    },
  },
  phoneOnly: {
    display: {
      default: "none",
      [pageLayout.phoneMedia]: "inline-grid",
    },
  },
});
