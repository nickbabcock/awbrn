//! CPU-side compositing of an AWBW map into a static PNG-ready image.
//!
//! This mirrors what the Bevy client renders on the GPU, but produces a single
//! [`RgbaImage`] by blitting terrain/property, unit, and unit-status sprites out
//! of the same atlases the client uses. Sprite-cell lookups reuse
//! [`awbrn_content`] so the appearance stays in sync with the live renderer.

use std::path::Path;

use awbrn_content::{
    TILESHEET_COLUMNS, UNIT_SPRITE_HEIGHT, UNIT_SPRITE_WIDTH, UNIT_SPRITESHEET_COLUMNS,
    UNIT_SPRITESHEET_OFFSET_X, UNIT_SPRITESHEET_OFFSET_Y, UNIT_SPRITESHEET_PADDING_X,
    UNIT_SPRITESHEET_PADDING_Y, spritesheet_index, unit_spritesheet_index,
};
use awbrn_map::{AwbrnMap, AwbwMap, PredeployedUnit};
use awbrn_types::{
    Faction, GraphicalMovement, GraphicalTerrain, PlayerFaction, Unit, UnitExt, Weather,
};
use awvm::semantic::{Location, State, TileOwner};
use image::{GenericImageView, RgbaImage, imageops};

/// Logical tile size, in pixels.
pub const TILE_SIZE: u32 = 16;

/// Terrain atlas cells are 16 wide by 32 tall; the bottom 16px is the ground
/// tile and the top 16px is overhang that rises into the cell above.
const TERRAIN_CELL_W: u32 = 16;
const TERRAIN_CELL_H: u32 = 32;
const CAPTURE_REQUIRED_POINTS: u8 = 20;

/// The sprite atlases needed to render a map.
pub struct Tilesets {
    /// Terrain / property spritesheet (`tiles.png`).
    pub tiles: RgbaImage,
    /// Unit spritesheet (`units.png`).
    pub units: RgbaImage,
    /// Unit status overlays (`ui.png`), such as health and capture markers.
    pub ui: RgbaImage,
}

impl Tilesets {
    /// Load `tiles.png`, `units.png`, and `ui.png` from a directory (e.g.
    /// `assets/textures`).
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Self> {
        let tiles = image::open(dir.join("tiles.png"))?.to_rgba8();
        let units = image::open(dir.join("units.png"))?.to_rgba8();
        let ui = image::open(dir.join("ui.png"))?.to_rgba8();
        Ok(Self { tiles, units, ui })
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

    render_terrain(&graphical, tilesets, weather, &mut canvas);

    let mut overlays = Vec::with_capacity(units.len());
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
        let origin = draw_unit(
            &mut canvas,
            kind,
            faction,
            unit.unit_x,
            unit.unit_y,
            tilesets,
        );
        overlays.push((
            origin,
            UnitStatus::from_visual_hp(unit.unit_hp.min(10) as u8),
        ));
    }
    for (origin, status) in overlays {
        draw_unit_status(&mut canvas, origin, status, tilesets);
    }

    canvas
}

fn render_terrain(
    graphical: &AwbrnMap,
    tilesets: &Tilesets,
    weather: Weather,
    canvas: &mut RgbaImage,
) {
    // Base plains layer: the client paints a repeating plain-grass backdrop
    // beneath the terrain, so terrain sprites with transparency (neutral cities,
    // pipes, etc.) show grass underneath rather than empty pixels. The backdrop
    // is the bottom 16px of the Plain cell, tiled over every ground cell (the
    // top overhang strip stays transparent — it is "above" the map).
    let backdrop = plain_backdrop_tile(&tilesets.tiles, weather);
    for y in 0..graphical.height() as u32 {
        for x in 0..graphical.width() as u32 {
            let dx = (x * TILE_SIZE) as i64;
            let dy = ((y + 1) * TILE_SIZE) as i64;
            imageops::replace(canvas, &backdrop, dx, dy);
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
        imageops::overlay(canvas, &sprite, dx, dy);
    }
}

fn draw_unit(
    canvas: &mut RgbaImage,
    kind: Unit,
    faction: PlayerFaction,
    x: u32,
    y: u32,
    tilesets: &Tilesets,
) -> (i64, i64) {
    let index = unit_spritesheet_index(GraphicalMovement::Idle, kind, faction).index();
    let (sx, sy, sw, sh) = unit_cell_rect(index);
    let sprite = tilesets.units.view(sx, sy, sw, sh).to_image();

    let (dx, dy) = unit_draw_origin(x, y);
    imageops::overlay(canvas, &sprite, dx, dy);
    (dx, dy)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct UnitStatus {
    /// Graphical HP, on the board's 1–10 scale. Full health has no overlay.
    health: Option<u8>,
    capturing: bool,
}

impl UnitStatus {
    fn from_visual_hp(health: u8) -> Self {
        Self {
            health: (health > 0 && health < 10).then_some(health),
            capturing: false,
        }
    }

    fn from_exact_hp(health: u8) -> Self {
        Self::from_visual_hp(health.div_ceil(10))
    }
}

/// The UI atlas is packed rather than gridded. These rectangles mirror the
/// entries in `assets/data/ui_atlas.json`, which is also what the Bevy client
/// uses to name these sprites.
#[derive(Debug, Clone, Copy)]
enum UiSprite {
    Capturing,
    Health(u8),
}

impl UiSprite {
    fn rect(self) -> (u32, u32, u32, u32) {
        match self {
            Self::Capturing => (151, 20, 8, 8),
            Self::Health(0) => (151, 30, 8, 7),
            Self::Health(1) => (46, 150, 8, 7),
            Self::Health(2) => (56, 150, 8, 7),
            Self::Health(3) => (1, 151, 8, 7),
            Self::Health(4) => (11, 151, 8, 7),
            Self::Health(5) => (21, 151, 8, 7),
            Self::Health(6) => (31, 151, 8, 7),
            Self::Health(7) => (135, 1, 8, 7),
            Self::Health(8) => (135, 10, 8, 7),
            Self::Health(9) => (145, 1, 8, 7),
            Self::Health(_) => (145, 10, 8, 7),
        }
    }

    fn local_translation(self) -> (f32, f32) {
        match self {
            Self::Capturing => (0.0, 8.0),
            Self::Health(_) => (7.5, 8.0),
        }
    }
}

fn draw_unit_status(
    canvas: &mut RgbaImage,
    unit_origin: (i64, i64),
    status: UnitStatus,
    tilesets: &Tilesets,
) {
    if let Some(health) = status.health {
        draw_ui_sprite(canvas, unit_origin, UiSprite::Health(health), tilesets);
    }
    if status.capturing {
        draw_ui_sprite(canvas, unit_origin, UiSprite::Capturing, tilesets);
    }
}

/// Draw a status sprite at the same local offset used by the Bevy unit
/// renderer. The raster compositor rounds the half-pixel positions that the
/// center-anchored GPU sprites use.
fn draw_ui_sprite(
    canvas: &mut RgbaImage,
    unit_origin: (i64, i64),
    sprite: UiSprite,
    tilesets: &Tilesets,
) {
    let (sx, sy, width, height) = sprite.rect();
    let source = tilesets.ui.view(sx, sy, width, height).to_image();
    let (x, y) = overlay_origin(unit_origin, sprite);
    imageops::overlay(canvas, &source, x, y);
}

fn overlay_origin(origin: (i64, i64), sprite: UiSprite) -> (i64, i64) {
    let (_, _, width, height) = sprite.rect();
    let (local_x, local_y) = sprite.local_translation();
    (
        (origin.0 as f32 + UNIT_SPRITE_WIDTH as f32 / 2.0 + local_x - width as f32 / 2.0).round()
            as i64,
        (origin.1 as f32 + UNIT_SPRITE_HEIGHT as f32 / 2.0 + local_y - height as f32 / 2.0).round()
            as i64,
    )
}

/// Return the top-left draw position that matches the client's center anchor.
///
/// Unit atlas cells include transparent space above and to the left of the
/// visible unit. The client alignment offsets compensate for this space. A
/// top-left compositor must apply the equivalent offset directly.
fn unit_draw_origin(x: u32, y: u32) -> (i64, i64) {
    let ground_x = i64::from(x * TILE_SIZE);
    let ground_y = i64::from((y + 1) * TILE_SIZE);
    (
        ground_x + i64::from(TILE_SIZE) - i64::from(UNIT_SPRITE_WIDTH),
        ground_y + i64::from(TILE_SIZE) - i64::from(UNIT_SPRITE_HEIGHT),
    )
}

/// Render an authoritative game state over the graphical map it came from.
///
/// `factions` is in roster order. The state stores ownership as roster seats,
/// while the sprite atlas identifies armies by faction.
pub fn render_state(
    source: &AwbrnMap,
    state: &State,
    factions: &[PlayerFaction],
    tilesets: &Tilesets,
) -> RgbaImage {
    assert_eq!(
        state.players.len(),
        factions.len(),
        "each roster seat needs a display faction"
    );

    let mut map = source.clone();
    for (position, tile) in state.board.iter() {
        let Some(GraphicalTerrain::Property(property)) = map.terrain_at(position) else {
            continue;
        };
        let owner = match tile.owner {
            TileOwner::Owned(seat) => Faction::Player(factions[seat.get()]),
            TileOwner::Neutral | TileOwner::NotOwnable => Faction::Neutral,
        };
        map.set_terrain(
            position,
            GraphicalTerrain::Property(property.with_owner(owner)),
        );
    }

    let width_px = map.width() as u32 * TILE_SIZE;
    let height_px = (map.height() as u32 + 1) * TILE_SIZE;
    let mut canvas = RgbaImage::new(width_px.max(1), height_px.max(1));
    render_terrain(&map, tilesets, Weather::Clear, &mut canvas);

    let mut overlays = Vec::with_capacity(state.units.len());
    for unit in state.units.iter() {
        let Location::Board { position } = unit.location else {
            continue;
        };
        let capturing = state
            .board
            .tile(position)
            .capture_points
            .is_some_and(|points| points < CAPTURE_REQUIRED_POINTS);
        let origin = draw_unit(
            &mut canvas,
            unit.kind,
            factions[unit.owner.get()],
            u32::from(position.x),
            u32::from(position.y),
            tilesets,
        );
        overlays.push((
            origin,
            UnitStatus {
                health: UnitStatus::from_exact_hp(unit.hp).health,
                capturing,
            },
        ));
    }
    for (origin, status) in overlays {
        draw_unit_status(&mut canvas, origin, status, tilesets);
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

    #[test]
    fn unit_draw_origin_matches_the_client_alignment() {
        assert_eq!(unit_draw_origin(0, 0), (-7, 8));
        assert_eq!(unit_draw_origin(2, 3), (25, 56));
    }

    #[test]
    fn exact_health_uses_the_client_graphical_health_rules() {
        assert_eq!(UnitStatus::from_exact_hp(100).health, None);
        assert_eq!(UnitStatus::from_exact_hp(91).health, None);
        assert_eq!(UnitStatus::from_exact_hp(90).health, Some(9));
        assert_eq!(UnitStatus::from_exact_hp(1).health, Some(1));
        assert_eq!(UnitStatus::from_exact_hp(0).health, None);
    }

    #[test]
    fn status_overlay_positions_match_the_client_offsets() {
        let origin = unit_draw_origin(0, 0);
        let (capture_x, capture_y) = overlay_origin(origin, UiSprite::Capturing);
        let (health_x, health_y) = overlay_origin(origin, UiSprite::Health(9));

        assert_eq!((capture_x, capture_y), (1, 24));
        assert_eq!((health_x, health_y), (8, 25));
    }
}
