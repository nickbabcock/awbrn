import { Text } from "@astryxdesign/core/Text";
import { HStack, Stack } from "@astryxdesign/core/Stack";
import { Icon } from "@astryxdesign/core/Icon";
import { Link } from "@tanstack/react-router";
import { borderVars, colorVars, spacingVars } from "@astryxdesign/core/theme/tokens.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState } from "react";
import { PushToggle } from "#/players/components/PushToggle.tsx";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { Button } from "#/ui/Button.tsx";
import { IconFlag } from "#/ui/icons.tsx";
import { MenuRow, ModalSheet } from "#/ui/ModalSheet.tsx";
import { Terms } from "#/ui/Terms.tsx";

/**
 * What a player can do to the match itself rather than to a unit on it: the
 * terms said once (`Live 5m +2m`, the funds, the map as a link), Turn alerts,
 * and Resign, asked twice.
 */
export function MatchMenu({
  canResign,
  clockTerms,
  disabledReason,
  funds,
  hasFog,
  isOpen,
  isResigning,
  mapId,
  mapName,
  onOpenChange,
  onResign,
}: {
  /** A seat still in the match can resign; a spectator or a fallen seat cannot. */
  canResign: boolean;
  /** `Live 5m +2m`. */
  clockTerms: string;
  /** Why Resign cannot be sent right now, when it cannot. */
  disabledReason?: string;
  funds: number;
  hasFog: boolean;
  isOpen: boolean;
  isResigning: boolean;
  mapId: string;
  mapName: string | null;
  onOpenChange: (isOpen: boolean) => void;
  onResign: () => void;
}) {
  const [isConfirming, setIsConfirming] = useState(false);
  const keepPlayingRef = useRef<HTMLButtonElement>(null);
  const resignRowRef = useRef<HTMLElement>(null);
  // Where the keyboard goes once the row it was on is swapped out: the
  // question opens on its safe key, and backing out returns to Resign.
  const focusAfterSwapRef = useRef<"keep" | "resign" | null>(null);

  // A closed menu forgets the question it was asking.
  useEffect(() => {
    if (!isOpen) setIsConfirming(false);
  }, [isOpen]);

  useEffect(() => {
    const target = focusAfterSwapRef.current;
    focusAfterSwapRef.current = null;
    if (target === "keep") keepPlayingRef.current?.focus();
    if (target === "resign") resignRowRef.current?.querySelector("button")?.focus();
  }, [isConfirming]);

  return (
    <ModalSheet isOpen={isOpen} onOpenChange={onOpenChange} title="Match" width={360}>
      <HStack as="span" xstyle={styles.terms}>
        <Terms clock={clockTerms} funds={funds} hasFog={hasFog} />
        <Link params={{ mapId }} to="/maps/$mapId" {...stylex.props(styles.mapLink)}>
          {mapName ?? "The map"}
        </Link>
      </HStack>
      {/* The switch names itself, and draws nothing where alerts cannot
          work, so its row goes with it. */}
      <Stack as="span" xstyle={styles.alerts}>
        <PushToggle isSignedIn />
      </Stack>
      {!canResign ? null : isConfirming ? (
        <Stack as="span" xstyle={styles.confirm}>
          <Text color="inherit" type="inherit" xstyle={styles.confirmText}>
            <b>Resign?</b> Your army leaves the board. This cannot be undone.
          </Text>
          {disabledReason ? (
            <Text color="inherit" type="inherit" xstyle={styles.reason}>
              {disabledReason}
            </Text>
          ) : null}
          <HStack as="span" xstyle={styles.confirmKeys}>
            <Button
              label="Keep playing"
              onClick={() => {
                focusAfterSwapRef.current = "resign";
                setIsConfirming(false);
              }}
              ref={keepPlayingRef}
              variant="secondary"
            />
            <Button
              isDisabled={disabledReason !== undefined}
              isLoading={isResigning}
              label="Resign"
              onClick={() => {
                onResign();
                onOpenChange(false);
              }}
              variant="destructive"
            />
          </HStack>
        </Stack>
      ) : (
        <Stack as="span" ref={resignRowRef} xstyle={styles.resign}>
          <MenuRow
            icon={<Icon icon={IconFlag} size="sm" />}
            isDisabled={isResigning}
            label="Resign"
            onClick={() => {
              focusAfterSwapRef.current = "keep";
              setIsConfirming(true);
            }}
          />
        </Stack>
      )}
    </ModalSheet>
  );
}

const styles = stylex.create({
  terms: {
    display: "flex",
    alignItems: "center",
    flexWrap: "wrap",
    gap: `${spacingVars["--spacing-1-5"]} ${spacingVars["--spacing-3"]}`,
    paddingBlock: spacingVars["--spacing-3"],
    paddingInline: awbrnVars.spacing3_5,
    backgroundColor: awbrnVars.paperWell,
  },
  mapLink: {
    color: awbrnVars.blue,
    fontSize: awbrnVars.text13,
    fontWeight: 650,
    textUnderlineOffset: "3px",
  },
  alerts: {
    display: {
      default: "block",
      ":empty": "none",
    },
    paddingBlock: spacingVars["--spacing-1"],
    paddingInline: awbrnVars.spacing3_5,
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
    fontSize: awbrnVars.text15_5,
  },
  resign: {
    display: "block",
    color: awbrnVars.danger,
  },
  confirm: {
    display: "grid",
    gap: awbrnVars.spacing2_5,
    paddingBlock: awbrnVars.spacing3_5,
    paddingInline: awbrnVars.spacing3_5,
    borderBlockStartColor: awbrnVars.rule,
    borderBlockStartStyle: "solid",
    borderBlockStartWidth: borderVars["--border-width"],
    backgroundColor: awbrnVars.paper,
  },
  confirmText: {
    fontSize: awbrnVars.text14,
    color: colorVars["--color-text-secondary"],
  },
  reason: {
    fontSize: awbrnVars.text13,
    color: awbrnVars.danger,
  },
  confirmKeys: {
    display: "flex",
    justifyContent: "flex-end",
    gap: spacingVars["--spacing-2"],
  },
});
