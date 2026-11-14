import { Icon } from "@astryxdesign/core/Icon";
import { HStack } from "@astryxdesign/core/Stack";
import { useQuery } from "@tanstack/react-query";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useState } from "react";
import { mapCatalogEntryQueryOptions } from "#/maps/maps.queries.ts";
import { mapScreenshotSize } from "#/maps/map_screenshot.ts";
import type { MapCatalogEntry } from "#/maps/schemas.ts";
import { awbrnVars } from "#/themes/awbrnTokens.stylex.ts";
import { IconMap } from "#/ui/icons.tsx";
import { pageLayout } from "#/ui/pageLayout.stylex.ts";

export type MatchThumbSize = 48 | 56 | 64 | 80;

/** Where a minimap stands: on its way, drawn, or failed to arrive. */
export type MinimapState = "loading" | "ready" | "failed";

/**
 * Tracks a minimap picture from the first paint. The server renders it as
 * drawn, since minimaps are small cached files usually there by first paint;
 * once React holds the element, the ref reads what the image already knows.
 */
export function useMinimapState() {
  const [state, setState] = useState<MinimapState>("ready");
  const ref = useCallback((image: HTMLImageElement | null) => {
    if (!image) return;
    if (!image.complete) setState("loading");
    else setState(image.naturalWidth > 0 ? "ready" : "failed");
  }, []);
  return {
    state,
    imageProps: {
      ref,
      onError: () => setState("failed"),
      onLoad: () => setState("ready"),
    },
  };
}

/**
 * A map as a row's thumbnail: the 1x minimap on ink, cropped to its middle,
 * never resampled. Pass `map` when the screen already holds the catalog
 * entry, so the thumbnail needs no request of its own.
 */
export function MatchThumb({
  isEager = false,
  phoneSize,
  size = 80,
  ...source
}: {
  isEager?: boolean;
  phoneSize?: MatchThumbSize;
  size?: MatchThumbSize;
} & (
  | { map: MapCatalogEntry; mapId?: never; revision?: never }
  | { map?: never; mapId: string; revision: number }
)) {
  const lookup = useQuery({
    ...mapCatalogEntryQueryOptions(source.mapId ?? "", source.revision ?? 0),
    enabled: source.map === undefined,
  });
  const map = source.map ?? lookup.data;
  const isError = source.map === undefined && lookup.isError;
  const picture = map ? mapScreenshotSize("small", map.width, map.height) : null;
  const { imageProps, state } = useMinimapState();
  const isFailed = isError || state === "failed";

  return (
    <HStack
      as="span"
      aria-hidden="true"
      xstyle={[
        styles.well,
        state === "ready" && map && styles.inked,
        styles.edge(size, phoneSize ?? size),
      ]}
    >
      {isFailed ? (
        <Icon icon={IconMap} size="md" xstyle={styles.failed} />
      ) : map && picture ? (
        <img
          alt=""
          decoding="async"
          height={picture.height}
          loading={isEager ? "eager" : "lazy"}
          src={map.screenshot.small}
          width={picture.width}
          {...imageProps}
          {...stylex.props(styles.picture, state !== "ready" && styles.pending)}
        />
      ) : null}
    </HStack>
  );
}

const styles = stylex.create({
  well: {
    // Flex centering overflows both edges equally, so a map wider than the
    // well shows its middle.
    alignItems: "center",
    justifyContent: "center",
    flexShrink: 0,
    overflow: "hidden",
    borderRadius: "6px",
    backgroundColor: awbrnVars.paperWell,
  },
  inked: {
    backgroundColor: awbrnVars.ink900,
    boxShadow: `inset 0 0 0 1px ${awbrnVars.ink700}`,
  },
  edge: (desktop: number, phone: number) => ({
    inlineSize: {
      default: `${desktop}px`,
      [pageLayout.phoneMedia]: `${phone}px`,
    },
    blockSize: {
      default: `${desktop}px`,
      [pageLayout.phoneMedia]: `${phone}px`,
    },
  }),
  picture: {
    display: "block",
    flexShrink: 0,
    maxInlineSize: "none",
    imageRendering: "pixelated",
  },
  pending: {
    visibility: "hidden",
  },
  failed: {
    color: awbrnVars.textTertiary,
  },
});
