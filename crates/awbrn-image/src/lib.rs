//! CPU-side compositing of an AWBW map into a static PNG-ready image.
//!
//! This mirrors what the Bevy client renders on the GPU, but produces a single
//! [`RgbaImage`] by blitting terrain/property and unit sprites out of the same
//! `tiles.png` / `units.png` atlases the client uses. Sprite-cell lookups reuse
//! [`awbrn_content`] so the appearance stays in sync with the live renderer.

use std::path::Path;

use awbrn_content::{
    TILESHEET_COLUMNS, UNIT_SPRITE_HEIGHT, UNIT_SPRITE_WIDTH, UNIT_SPRITESHEET_COLUMNS,
    UNIT_SPRITESHEET_OFFSET_X, UNIT_SPRITESHEET_OFFSET_Y, UNIT_SPRITESHEET_PADDING_X,
    UNIT_SPRITESHEET_PADDING_Y, spritesheet_index, unit_spritesheet_index,
};
use awbrn_map::{AwbrnMap, AwbwMap, PredeployedUnit};
use awbrn_types::{GraphicalMovement, GraphicalTerrain, PlayerFaction, Unit, UnitExt, Weather};
use image::{GenericImageView, RgbaImage, imageops};

/// Logical tile size, in pixels.
pub const TILE_SIZE: u32 = 16;

/// Terrain atlas cells are 16 wide by 32 tall; the bottom 16px is the ground
/// tile and the top 16px is overhang that rises into the cell above.
const TERRAIN_CELL_W: u32 = 16;
const TERRAIN_CELL_H: u32 = 32;

/// The two sprite atlases needed to render a map.
pub struct Tilesets {
    /// Terrain / property spritesheet (`tiles.png`).
    pub tiles: RgbaImage,
    /// Unit spritesheet (`units.png`).
    pub units: RgbaImage,
}

impl Tilesets {
    /// Load `tiles.png` and `units.png` from a directory (e.g. `assets/textures`).
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Self> {
        let tiles = image::open(dir.join("tiles.png"))?.to_rgba8();
        let units = image::open(dir.join("units.png"))?.to_rgba8();
        Ok(Self { tiles, units })
    }
}

/// Render a map (clear weather) with its pre-deployed units to an image.
///
/// Unknown unit ids or country codes are skipped with a warning rather than
/// aborting the whole render.
pub fn render_map(map: &AwbwMap, units: &[PredeployedUnit], tilesets: &Tilesets) -> RgbaImage {
    render_map_with_weather(map, units, tilesets, Weather::Clear)
}

/// Like [`render_map`] but with an explicit [`Weather`] for the terrain tileset.
pub fn render_map_with_weather(
    map: &AwbwMap,
    units: &[PredeployedUnit],
    tilesets: &Tilesets,
    weather: Weather,
) -> RgbaImage {
    let graphical = AwbrnMap::from_map(map);

    let width_px = map.width() as u32 * TILE_SIZE;
    // One extra tile row at the top for the terrain sprite overhang.
    let height_px = (map.height() as u32 + 1) * TILE_SIZE;
    let mut canvas = RgbaImage::new(width_px.max(1), height_px.max(1));

    // Base plains layer: the client paints a repeating plain-grass backdrop
    // beneath the terrain, so terrain sprites with transparency (neutral cities,
    // pipes, etc.) show grass underneath rather than empty pixels. The backdrop
    // is the bottom 16px of the Plain cell, tiled over every ground cell (the
    // top overhang strip stays transparent — it is "above" the map).
    let backdrop = plain_backdrop_tile(&tilesets.tiles, weather);
    for y in 0..map.height() as u32 {
        for x in 0..map.width() as u32 {
            let dx = (x * TILE_SIZE) as i64;
            let dy = ((y + 1) * TILE_SIZE) as i64;
            imageops::replace(&mut canvas, &backdrop, dx, dy);
        }
    }

    // Terrain (and properties). `iter` yields row-major (y ascending), so a
    // lower tile's overhang correctly paints over the tile above it.
    for (pos, terrain) in graphical.iter() {
        let index = spritesheet_index(weather, terrain).index();
        let (sx, sy, sw, sh) = terrain_cell_rect(index);
        let sprite = tilesets.tiles.view(sx, sy, sw, sh).to_image();
        let dx = (pos.x as u32 * TILE_SIZE) as i64;
        let dy = (pos.y as u32 * TILE_SIZE) as i64;
        imageops::overlay(&mut canvas, &sprite, dx, dy);
    }

    // Pre-deployed units, centered on their tile's ground cell.
    for unit in units {
        let Some(kind) = Unit::from_awbw_id(unit.unit_id) else {
            eprintln!("warning: skipping unknown unit id {}", unit.unit_id);
            continue;
        };
        let Some(faction) = PlayerFaction::from_country_code(&unit.country_code) else {
            eprintln!(
                "warning: skipping unit with unknown country code {:?}",
                unit.country_code
            );
            continue;
        };

        let index = unit_spritesheet_index(GraphicalMovement::Idle, kind, faction).index();
        let (sx, sy, sw, sh) = unit_cell_rect(index);
        let sprite = tilesets.units.view(sx, sy, sw, sh).to_image();

        // Ground cell top-left lives below the top overhang strip, i.e. at
        // canvas y = (uy + 1) * TILE_SIZE. The unit sprite is centered within
        // the 16x16 ground cell: x offset (16-23)/2 ≈ -4, y offset (16-24)/2 = -4.
        let dx = unit.unit_x as i64 * TILE_SIZE as i64 - 4;
        let dy = unit.unit_y as i64 * TILE_SIZE as i64 + (TILE_SIZE as i64) - 4;
        imageops::overlay(&mut canvas, &sprite, dx, dy);
    }

    canvas
}

/// The 16x16 plain-grass backdrop tile: the bottom (ground) half of the Plain
/// terrain cell for the given weather, used to fill the base layer.
fn plain_backdrop_tile(tiles: &RgbaImage, weather: Weather) -> RgbaImage {
    let index = spritesheet_index(weather, GraphicalTerrain::Plain).index();
    let (sx, sy, _w, _h) = terrain_cell_rect(index);
    tiles
        .view(sx, sy + (TERRAIN_CELL_H - TILE_SIZE), TILE_SIZE, TILE_SIZE)
        .to_image()
}

/// Pixel rectangle `(x, y, w, h)` of a terrain atlas cell.
fn terrain_cell_rect(index: u16) -> (u32, u32, u32, u32) {
    let i = u32::from(index);
    let col = i % TILESHEET_COLUMNS;
    let row = i / TILESHEET_COLUMNS;
    (
        col * TERRAIN_CELL_W,
        row * TERRAIN_CELL_H,
        TERRAIN_CELL_W,
        TERRAIN_CELL_H,
    )
}

/// Pixel rectangle `(x, y, w, h)` of a unit atlas cell, accounting for the
/// atlas's per-cell padding and outer offset.
fn unit_cell_rect(index: u16) -> (u32, u32, u32, u32) {
    let i = u32::from(index);
    let col = i % UNIT_SPRITESHEET_COLUMNS;
    let row = i / UNIT_SPRITESHEET_COLUMNS;
    let x = UNIT_SPRITESHEET_OFFSET_X + col * (UNIT_SPRITE_WIDTH + UNIT_SPRITESHEET_PADDING_X);
    let y = UNIT_SPRITESHEET_OFFSET_Y + row * (UNIT_SPRITE_HEIGHT + UNIT_SPRITESHEET_PADDING_Y);
    (x, y, UNIT_SPRITE_WIDTH, UNIT_SPRITE_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_cell_rect_walks_the_grid() {
        assert_eq!(terrain_cell_rect(0), (0, 0, 16, 32));
        assert_eq!(terrain_cell_rect(1), (16, 0, 16, 32));
        // First cell of the second row.
        let cols = TILESHEET_COLUMNS as u16;
        assert_eq!(terrain_cell_rect(cols), (0, 32, 16, 32));
    }

    #[test]
    fn unit_cell_rect_applies_offset_and_padding() {
        // Offset (1,1) for cell 0.
        assert_eq!(
            unit_cell_rect(0),
            (1, 1, UNIT_SPRITE_WIDTH, UNIT_SPRITE_HEIGHT)
        );
        // Cell 1: x advances by width + padding.
        assert_eq!(
            unit_cell_rect(1),
            (
                1 + UNIT_SPRITE_WIDTH + UNIT_SPRITESHEET_PADDING_X,
                1,
                UNIT_SPRITE_WIDTH,
                UNIT_SPRITE_HEIGHT
            )
        );
    }
}
