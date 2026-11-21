import { HStack } from "@astryxdesign/core/Stack";
import { uiAtlasSpriteStyle } from "#/components/game_sprites.ts";

const STAR = uiAtlasSpriteStyle("TerrainStar.png");

/** The game's defense stars; none at zero. */
export function DefenseStars({ count }: { count: number }) {
  if (count <= 0) return null;
  return (
    <HStack as="span" aria-label={`${count} defense`} gap={0.5}>
      {Array.from({ length: count }, (_, index) => (
        <span aria-hidden="true" key={index} style={STAR ?? undefined} />
      ))}
    </HStack>
  );
}
