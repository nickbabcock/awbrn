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
 * Tracks a minimap picture from the first paint.
 *
 * The server draws the picture as drawn, so the page shows its minimaps
 * before (and without) the script that hydrates it; the minimaps are small,
 * cached files that are usually there by first paint. Once React holds the
 * element, the ref reads what it already knows: a picture still on its way
 * steps back to the paper well (before the browser paints) until it loads,
 * and one that failed shows the map glyph.
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
 * A map as a row's thumbnail: the 1x minimap on ink, cropped to its middle the
 * way a stamp shows part of a view, never resampled. Until the picture is
 * drawn the well is paper, not a black square; a picture that cannot load
 * leaves the paper well with a map glyph.
 *
 * Pass `map` when the screen already holds the map's catalog entry (a list of
 * maps): the thumbnail draws from it with the page, with no request of its
 * own. Otherwise `mapId` and `revision` look the entry up.
 *
 * `size` is the edge on desktop; `phoneSize` takes over at phone width (a
 * match row's 80px thumbnail is 64px there). `isEager` loads a thumbnail that
 * sits above the fold with the page rather than when it scrolls into view.
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
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    flexShrink: 0,
    overflow: "hidden",
    borderRadius: "6px",
    backgroundColor: awbrnVars.paperWell,
  },
  // The drawn minimap sits on ink, framed by a hairline.
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
  // The picture keeps its own pixels; the well crops what does not fit.
  picture: {
    display: "block",
    flexShrink: 0,
    maxInlineSize: "none",
    imageRendering: "pixelated",
  },
  // A picture still arriving draws nothing over the paper well.
  pending: {
    visibility: "hidden",
  },
  failed: {
    color: awbrnVars.textTertiary,
  },
});
