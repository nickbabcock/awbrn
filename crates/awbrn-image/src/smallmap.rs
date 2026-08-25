//! Compact, asset-free map rendering.

use awbrn_map::AwbwMap;
use awbrn_types::{AwbwTerrain, Faction, PlayerFaction, Property};
use image::{Rgba, RgbaImage};

const SMALLMAP_TILE_SIZE: u32 = 4;

type Color = Rgba<u8>;

const PLAIN: Color = Rgba([168, 240, 80, 255]);
const PLAIN_DARK: Color = Rgba([104, 232, 56, 255]);
const WOOD_DARK: Color = Rgba([88, 200, 16, 255]);
const MOUNTAIN_LIGHT: Color = Rgba([248, 232, 144, 255]);
const MOUNTAIN_MID: Color = Rgba([208, 128, 48, 255]);
const MOUNTAIN_DARK: Color = Rgba([152, 104, 48, 255]);
const WATER: Color = Rgba([88, 104, 248, 255]);
const RIVER_LIGHT: Color = Rgba([56, 120, 248, 255]);
const SHOAL: Color = Rgba([112, 176, 248, 255]);
const ROAD: Color = Rgba([184, 176, 168, 255]);
const BUILDING_DARK: Color = Rgba([104, 80, 56, 255]);
const NEUTRAL: Color = Rgba([248, 248, 248, 255]);
const PIPE: Color = Rgba([176, 144, 136, 255]);
const WHITE: Color = Rgba([255, 255, 255, 255]);

/// Render a terrain-only AWBW smallmap without loading sprite assets.
///
/// Each map tile becomes a 4-by-4 pixel glyph. The palette and glyphs follow
/// the smallmaps that the AWBW server generates.
pub fn render_small_map(map: &AwbwMap) -> RgbaImage {
    let mut image = RgbaImage::new(
        u32::from(map.width()) * SMALLMAP_TILE_SIZE,
        u32::from(map.height()) * SMALLMAP_TILE_SIZE,
    );

    for (position, terrain) in map.iter() {
        let glyph = terrain_glyph(terrain);
        let left = u32::from(position.x) * SMALLMAP_TILE_SIZE;
        let top = u32::from(position.y) * SMALLMAP_TILE_SIZE;
        for (offset, color) in glyph.into_iter().enumerate() {
            let x = left + offset as u32 % SMALLMAP_TILE_SIZE;
            let y = top + offset as u32 / SMALLMAP_TILE_SIZE;
            image.put_pixel(x, y, color);
        }
    }

    image
}

fn terrain_glyph(terrain: AwbwTerrain) -> [Color; 16] {
    match terrain {
        AwbwTerrain::Plain => [
            PLAIN, PLAIN, PLAIN, PLAIN, PLAIN, PLAIN, PLAIN_DARK, PLAIN, PLAIN, PLAIN, PLAIN,
            PLAIN, PLAIN_DARK, PLAIN, PLAIN, PLAIN,
        ],
        AwbwTerrain::Mountain => [
            PLAIN,
            PLAIN,
            PLAIN,
            PLAIN,
            PLAIN,
            MOUNTAIN_LIGHT,
            MOUNTAIN_DARK,
            PLAIN,
            MOUNTAIN_LIGHT,
            MOUNTAIN_LIGHT,
            MOUNTAIN_DARK,
            MOUNTAIN_DARK,
            MOUNTAIN_LIGHT,
            MOUNTAIN_MID,
            MOUNTAIN_DARK,
            MOUNTAIN_DARK,
        ],
        AwbwTerrain::Wood => [
            PLAIN, PLAIN_DARK, PLAIN_DARK, PLAIN, PLAIN_DARK, PLAIN_DARK, PLAIN_DARK, WOOD_DARK,
            PLAIN_DARK, PLAIN_DARK, WOOD_DARK, WOOD_DARK, PLAIN, WOOD_DARK, WOOD_DARK, PLAIN,
        ],
        AwbwTerrain::River(_) => [
            RIVER_LIGHT,
            RIVER_LIGHT,
            RIVER_LIGHT,
            RIVER_LIGHT,
            WATER,
            WATER,
            RIVER_LIGHT,
            RIVER_LIGHT,
            RIVER_LIGHT,
            RIVER_LIGHT,
            RIVER_LIGHT,
            RIVER_LIGHT,
            RIVER_LIGHT,
            WATER,
            WATER,
            WATER,
        ],
        AwbwTerrain::Road(_) | AwbwTerrain::Bridge(_) => [ROAD; 16],
        AwbwTerrain::Sea => [WATER; 16],
        AwbwTerrain::Shoal(_) => [SHOAL; 16],
        AwbwTerrain::Reef => [
            MOUNTAIN_LIGHT,
            WATER,
            WATER,
            WATER,
            MOUNTAIN_MID,
            SHOAL,
            MOUNTAIN_LIGHT,
            WATER,
            SHOAL,
            RIVER_LIGHT,
            MOUNTAIN_MID,
            SHOAL,
            WATER,
            WATER,
            SHOAL,
            RIVER_LIGHT,
        ],
        AwbwTerrain::Property(property) => property_glyph(property),
        AwbwTerrain::Pipe(_) | AwbwTerrain::PipeSeam(_) | AwbwTerrain::PipeRubble(_) => [PIPE; 16],
        AwbwTerrain::MissileSilo(_) => [WHITE; 16],
        AwbwTerrain::Teleporter => [Rgba([248, 72, 248, 255]); 16],
    }
}

fn property_glyph(property: Property) -> [Color; 16] {
    let (front, side) = match property.faction() {
        Faction::Neutral => (NEUTRAL, BUILDING_DARK),
        Faction::Player(faction) => faction_colors(faction),
    };
    [
        front, front, front, side, front, front, front, side, front, front, front, side, side,
        side, side, side,
    ]
}

fn faction_colors(faction: PlayerFaction) -> (Color, Color) {
    let primary = match faction {
        PlayerFaction::OrangeStar => [248, 72, 48],
        PlayerFaction::BlueMoon => [88, 104, 248],
        PlayerFaction::GreenEarth => [88, 200, 16],
        PlayerFaction::YellowComet => [240, 240, 8],
        PlayerFaction::BlackHole => return (Rgba([96, 72, 160, 255]), Rgba([79, 48, 112, 255])),
        PlayerFaction::RedFire => {
            return (Rgba([208, 70, 93, 255]), Rgba([119, 11, 35, 255]));
        }
        PlayerFaction::GreySky => {
            return (Rgba([129, 127, 128, 255]), Rgba([86, 92, 114, 255]));
        }
        PlayerFaction::BrownDesert => [152, 104, 48],
        PlayerFaction::AmberBlossom => [248, 168, 56],
        PlayerFaction::JadeSun => [160, 184, 152],
        PlayerFaction::CobaltIce => [48, 72, 176],
        PlayerFaction::PinkCosmos => [248, 104, 208],
        PlayerFaction::TealGalaxy => {
            return (Rgba([68, 172, 163, 255]), Rgba([10, 89, 82, 255]));
        }
        PlayerFaction::PurpleLightning => {
            return (Rgba([164, 70, 210, 255]), Rgba([110, 25, 153, 255]));
        }
        PlayerFaction::AcidRain => [104, 136, 24],
        PlayerFaction::WhiteNova => [216, 176, 176],
        PlayerFaction::AzureAsteroid => [72, 168, 232],
        PlayerFaction::NoirEclipse => [64, 56, 72],
        PlayerFaction::SilverClaw => [184, 192, 200],
        PlayerFaction::UmberWilds => [128, 88, 48],
    };
    (
        Rgba([primary[0], primary[1], primary[2], 255]),
        BUILDING_DARK,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use awvm::semantic::Dimensions;
    use highway::HighwayHash;

    #[test]
    fn one_map_tile_becomes_four_by_four_pixels() {
        let map = AwbwMap::new(Dimensions::new(2, 3), AwbwTerrain::Plain);
        let image = render_small_map(&map);

        assert_eq!(image.dimensions(), (8, 12));
    }

    #[test]
    fn terrain_families_use_distinct_glyphs() {
        let plain = terrain_glyph(AwbwTerrain::Plain);
        let mountain = terrain_glyph(AwbwTerrain::Mountain);
        let wood = terrain_glyph(AwbwTerrain::Wood);
        let sea = terrain_glyph(AwbwTerrain::Sea);

        assert_ne!(plain, mountain);
        assert_ne!(plain, wood);
        assert_ne!(plain, sea);
        assert_ne!(mountain, wood);
    }

    #[test]
    fn properties_show_their_owner_color() {
        let neutral = property_glyph(Property::City(Faction::Neutral));
        let orange = property_glyph(Property::Base(Faction::Player(PlayerFaction::OrangeStar)));
        let blue = property_glyph(Property::HQ(PlayerFaction::BlueMoon));

        assert_eq!(neutral[0], NEUTRAL);
        assert_eq!(orange[0], Rgba([248, 72, 48, 255]));
        assert_eq!(blue[0], WATER);
        assert_eq!(neutral[15], BUILDING_DARK);
    }

    #[test]
    fn extended_factions_use_awbw_smallmap_colors() {
        assert_eq!(
            faction_colors(PlayerFaction::TealGalaxy),
            (Rgba([68, 172, 163, 255]), Rgba([10, 89, 82, 255]))
        );
        assert_eq!(
            faction_colors(PlayerFaction::PurpleLightning),
            (Rgba([164, 70, 210, 255]), Rgba([110, 25, 153, 255]))
        );
    }

    #[test]
    fn known_awbw_map_matches_the_server_pixels() {
        let map = AwbwMap::parse_txt(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/maps/162795.txt"
        )))
        .unwrap();
        let image = render_small_map(&map);
        let digest = highway::HighwayHasher::new(highway::Key::default()).hash256(image.as_raw());
        let digest = format!(
            "0x{:016x}{:016x}{:016x}{:016x}",
            digest[0], digest[1], digest[2], digest[3]
        );

        assert_eq!(
            digest,
            "0x27fdb5b387ecbfaef8d480f0a97231ac09dd82080733600c61a29a560edce5d8"
        );
    }
}
