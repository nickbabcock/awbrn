//! CPU-side rendering of AWBW maps into static PNG-ready images.
//!
//! Full-size renders mirror what the Bevy client renders on the GPU. They blit
//! terrain, property, unit, and unit-status sprites from the same atlases that
//! the client uses. Smallmaps use a fixed 4-by-4 pixel terrain palette and do
//! not need those assets.

use std::path::Path;

use anyhow::Context;
use awbrn_content::{
    INACTIVE_UNIT_TINT_SRGB, PixelSize, TILE_SIZE, UNIT_SPRITE_HEIGHT, UNIT_SPRITE_WIDTH,
    UiAtlasManifest, UnitOverlay, plain_backdrop_rect, sprite_top_left, spritesheet_index,
    terrain_sprite_rect, unit_overlay_spec, unit_sprite_rect, unit_spritesheet_index,
};
use awbrn_map::{AwbrnMap, AwbwMap};
use awbrn_types::{
    ExactHp, Faction, GraphicalHp, GraphicalMovement, GraphicalTerrain, PlayerFaction, Unit,
    VisualHp, Weather,
};
use awvm::semantic::{
    CAPTURE_REQUIRED_POINTS, Concealment, Location, Pos, State, TileOwner, UnitAction, UnitId,
};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, GenericImageView, ImageEncoder, ImageFormat, RgbaImage, imageops};
use std::collections::HashSet;
use std::hash::BuildHasher;

mod smallmap;

pub use smallmap::render_small_map;

/// What can stop a render.
///
/// This is a library: a caller that hands over a mismatched roster or an
/// incomplete atlas gets told so, rather than taking the process down.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RenderError {
    #[error("the state has {seats} roster seats but {factions} display factions")]
    FactionCount { seats: usize, factions: usize },
    #[error("the UI atlas is missing {0}")]
    MissingUiSprite(String),
}

/// The sprite atlases needed to render a map.
#[derive(Debug)]
pub struct Tilesets {
    /// Terrain / property spritesheet (`tiles.png`).
    pub tiles: RgbaImage,
    /// Unit spritesheet (`units.png`).
    pub units: RgbaImage,
    /// Unit status overlays (`ui.png`), such as health and capture markers.
    pub ui: RgbaImage,
    /// Renderer-neutral metadata for the packed UI atlas.
    pub ui_atlas: UiAtlasManifest,
}

impl Tilesets {
    /// Load `tiles.png`, `units.png`, and `ui.png` from a directory (e.g.
    /// `assets/textures`).
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Self> {
        let tiles = image::open(dir.join("tiles.png"))?.to_rgba8();
        let units = image::open(dir.join("units.png"))?.to_rgba8();
        let ui = image::open(dir.join("ui.png"))?.to_rgba8();
        let manifest_path = dir
            .parent()
            .map(|parent| parent.join("data/ui_atlas.json"))
            .filter(|path| path.exists())
            .unwrap_or_else(|| dir.join("ui_atlas.json"));
        let ui_atlas = serde_json::from_slice(&std::fs::read(manifest_path)?)?;
        Ok(Self {
            tiles,
            units,
            ui,
            ui_atlas,
        })
    }

    /// Decode the same atlases from memory, for a caller with no filesystem.
    ///
    /// A WebAssembly host reads the atlases over the network and gives them
    /// here, so the appearance stays the one the client renders.
    pub fn from_bytes(
        tiles: &[u8],
        units: &[u8],
        ui: &[u8],
        ui_atlas: &[u8],
    ) -> anyhow::Result<Self> {
        Ok(Self {
            tiles: decode_png(tiles, "tiles.png")?,
            units: decode_png(units, "units.png")?,
            ui: decode_png(ui, "ui.png")?,
            ui_atlas: serde_json::from_slice(ui_atlas)
                .context("reading the ui_atlas.json manifest")?,
        })
    }
}

fn decode_png(bytes: &[u8], name: &str) -> anyhow::Result<RgbaImage> {
    Ok(image::load_from_memory_with_format(bytes, ImageFormat::Png)
        .with_context(|| format!("decoding {name}"))?
        .to_rgba8())
}

/// The most colours a PNG palette can hold.
const PALETTE_LIMIT: usize = 256;

/// Encode a rendered image as PNG bytes.
///
/// A render is written once and then read many times, so both encoders below
/// are set for size rather than for speed, and both write their rows
/// unfiltered: these images are pixel art with long flat runs, which deflate
/// reads better on its own than through any filter.
///
/// A render that uses few enough colours is written with a palette, which is
/// most of what makes these files small: a smallmap draws from about a dozen
/// colours, so its pixels pack four to the byte. A busy board runs past what a
/// palette can hold — a map with several armies on it reaches five or six
/// hundred colours — and is written as it was drawn. Whichever is smaller
/// wins, so the palette can never cost anything.
pub fn encode_png(image: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    let truecolor = encode_truecolor(image)?;
    match encode_indexed(image)? {
        Some(indexed) if indexed.len() < truecolor.len() => Ok(indexed),
        _ => Ok(truecolor),
    }
}

fn encode_truecolor(image: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Best, FilterType::NoFilter)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )
        .context("encoding the render as PNG")?;
    Ok(bytes)
}

/// Write the image with a palette, or `None` when it holds too many colours.
fn encode_indexed(image: &RgbaImage) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(palette) = Palette::of(image) else {
        return Ok(None);
    };

    let depth = palette.bit_depth();
    let bits = u32::from(depth as u8);
    let per_byte = 8 / bits as usize;

    let mut rows = Vec::with_capacity(image.height() as usize * image.width() as usize / per_byte);
    for row in image.rows() {
        let start = rows.len();
        rows.resize(start + (image.width() as usize).div_ceil(per_byte), 0);
        for (x, pixel) in row.enumerate() {
            let index = palette.index_of(pixel.0);
            // Indices pack left to right, most significant bits first.
            let shift = 8 - bits * (x as u32 % per_byte as u32 + 1);
            rows[start + x / per_byte] |= index << shift;
        }
    }

    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, image.width(), image.height());
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(depth);
    encoder.set_compression(png::Compression::High);
    encoder.set_filter(png::Filter::NoFilter);
    encoder.set_palette(palette.rgb());
    if let Some(alpha) = palette.alpha() {
        encoder.set_trns(alpha);
    }
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&rows))
        .context("encoding the render as an indexed PNG")?;

    Ok(Some(bytes))
}

/// The distinct colours of an image, in the order they were first seen.
struct Palette {
    colors: Vec<[u8; 4]>,
    /// Where each colour sits in `colors`, keyed by the colour itself.
    index: std::collections::HashMap<[u8; 4], u8>,
}

impl Palette {
    /// `None` when the image holds more colours than a palette can name.
    fn of(image: &RgbaImage) -> Option<Self> {
        let mut palette = Self {
            colors: Vec::new(),
            index: std::collections::HashMap::new(),
        };
        for pixel in image.pixels() {
            if let std::collections::hash_map::Entry::Vacant(slot) = palette.index.entry(pixel.0) {
                let next = u8::try_from(palette.colors.len()).ok()?;
                slot.insert(next);
                palette.colors.push(pixel.0);
            }
        }
        (palette.colors.len() <= PALETTE_LIMIT).then_some(palette)
    }

    /// The narrowest depth that can name every colour.
    fn bit_depth(&self) -> png::BitDepth {
        match self.colors.len() {
            0..=2 => png::BitDepth::One,
            3..=4 => png::BitDepth::Two,
            5..=16 => png::BitDepth::Four,
            _ => png::BitDepth::Eight,
        }
    }

    fn index_of(&self, color: [u8; 4]) -> u8 {
        self.index[&color]
    }

    fn rgb(&self) -> Vec<u8> {
        self.colors
            .iter()
            .flat_map(|color| color[..3].to_vec())
            .collect()
    }

    /// The alpha of each entry, or `None` when every colour is opaque.
    ///
    /// A render is transparent only where a terrain sprite overhangs the top
    /// of the board, so most of these come back `None`.
    fn alpha(&self) -> Option<Vec<u8>> {
        self.colors
            .iter()
            .any(|color| color[3] != u8::MAX)
            .then(|| self.colors.iter().map(|color| color[3]).collect())
    }
}

/// Render a map (clear weather) with the units it deploys to an image.
pub fn render_map(map: &AwbwMap, tilesets: &Tilesets) -> Result<RgbaImage, RenderError> {
    render_map_with_weather(map, tilesets, Weather::Clear)
}

/// Like [`render_map`] but with an explicit [`Weather`] for the terrain tileset.
pub fn render_map_with_weather(
    map: &AwbwMap,
    tilesets: &Tilesets,
    weather: Weather,
) -> Result<RgbaImage, RenderError> {
    let graphical = AwbrnMap::from_map(map);

    let width_px = map.width() as u32 * TILE_SIZE;
    // One extra tile row at the top for the terrain sprite overhang.
    let height_px = (map.height() as u32 + 1) * TILE_SIZE;
    let mut canvas = RgbaImage::new(width_px.max(1), height_px.max(1));

    render_terrain(&graphical, tilesets, weather, &mut canvas);

    let deployments = map.deployments();
    let mut overlays = Vec::with_capacity(deployments.len());
    for (position, deployment) in deployments.iter() {
        // A map's pre-deployed units belong to nobody's turn yet, so they are
        // all drawn ready.
        let origin = draw_unit(
            &mut canvas,
            deployment.unit,
            deployment.faction,
            u32::from(position.x),
            u32::from(position.y),
            true,
            tilesets,
        );
        overlays.push((origin, UnitStatus::from_visual_hp(deployment.hp.get())));
    }
    for (origin, status) in overlays {
        draw_unit_status(&mut canvas, origin, status, tilesets)?;
    }

    Ok(canvas)
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
        let rect = terrain_sprite_rect(index);
        let sprite = tilesets
            .tiles
            .view(rect.x, rect.y, rect.width, rect.height)
            .to_image();
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
    active: bool,
    tilesets: &Tilesets,
) -> (i64, i64) {
    let index = unit_spritesheet_index(GraphicalMovement::Idle, kind, faction).index();
    let rect = unit_sprite_rect(index);
    let mut sprite = tilesets
        .units
        .view(rect.x, rect.y, rect.width, rect.height)
        .to_image();
    if !active {
        tint_inactive(&mut sprite);
    }

    let (dx, dy) = unit_draw_origin(x, y);
    imageops::overlay(canvas, &sprite, dx, dy);
    (dx, dy)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct UnitStatus {
    /// Graphical HP, on the board's 1–10 scale. Full health has no overlay.
    health: Option<u8>,
    capturing: bool,
    /// The unit is a transport carrying at least one passenger.
    cargo: bool,
    /// A submarine that has dived.
    dive: bool,
}

impl UnitStatus {
    fn from_visual_hp(health: u8) -> Self {
        Self {
            health: (health > 0 && health < 10).then_some(health),
            capturing: false,
            cargo: false,
            dive: false,
        }
    }

    fn from_exact_hp(health: u8) -> Self {
        Self::from_visual_hp(ExactHp::new(health).visual().get())
    }

    /// The overlays an authoritative unit standing on the board shows.
    fn of_state_unit<S: BuildHasher>(
        state: &State,
        unit: &awvm::semantic::Unit,
        position: Pos,
        loaded_transports: &HashSet<UnitId, S>,
    ) -> Self {
        Self {
            health: Self::from_exact_hp(unit.hp).health,
            capturing: state
                .board
                .tile(position)
                .capture_points
                .is_some_and(|points| points < CAPTURE_REQUIRED_POINTS),
            cargo: loaded_transports.contains(&unit.id),
            dive: unit.concealment == Concealment::Hidden,
        }
    }
}

/// The UI atlas is packed rather than gridded. These rectangles mirror the
/// entries in `assets/data/ui_atlas.json`, which is also what the Bevy client
/// uses to name these sprites.
fn draw_unit_status(
    canvas: &mut RgbaImage,
    unit_origin: (i64, i64),
    status: UnitStatus,
    tilesets: &Tilesets,
) -> Result<(), RenderError> {
    if let Some(health) = status.health {
        draw_ui_sprite(
            canvas,
            unit_origin,
            UnitOverlay::Health(GraphicalHp::Visible(VisualHp::new(health))),
            tilesets,
        )?;
    }
    if status.capturing {
        draw_ui_sprite(canvas, unit_origin, UnitOverlay::Capturing, tilesets)?;
    }
    if status.cargo {
        draw_ui_sprite(canvas, unit_origin, UnitOverlay::Cargo, tilesets)?;
    }
    if status.dive {
        draw_ui_sprite(canvas, unit_origin, UnitOverlay::Dive, tilesets)?;
    }
    Ok(())
}

/// Draw a status sprite at the same local offset used by the Bevy unit
/// renderer. The raster compositor rounds the half-pixel positions that the
/// center-anchored GPU sprites use.
fn draw_ui_sprite(
    canvas: &mut RgbaImage,
    unit_origin: (i64, i64),
    overlay: UnitOverlay,
    tilesets: &Tilesets,
) -> Result<(), RenderError> {
    // An overlay with nothing to show (full health) has no sprite.
    let Some(spec) = unit_overlay_spec(overlay) else {
        return Ok(());
    };
    let sprite = tilesets
        .ui_atlas
        .sprite(&spec.sprite_name)
        .ok_or_else(|| RenderError::MissingUiSprite(spec.sprite_name.clone()))?;
    let rect = sprite.rect();
    let source = tilesets
        .ui
        .view(rect.x, rect.y, rect.width, rect.height)
        .to_image();
    let (x, y) = overlay_origin(unit_origin, rect.width, rect.height, spec.offset);
    imageops::overlay(canvas, &source, x, y);
    Ok(())
}

fn overlay_origin(
    origin: (i64, i64),
    width: u32,
    height: u32,
    offset: awbrn_content::LogicalOffset,
) -> (i64, i64) {
    (
        (origin.0 as f32 + UNIT_SPRITE_WIDTH as f32 / 2.0 + offset.x - width as f32 / 2.0).round()
            as i64,
        (origin.1 as f32 + UNIT_SPRITE_HEIGHT as f32 / 2.0 + offset.y - height as f32 / 2.0).round()
            as i64,
    )
}

/// Return the top-left draw position that matches the client's center anchor.
///
/// Unit atlas cells include transparent space above and to the left of the
/// visible unit. The client alignment offsets compensate for this space. A
/// top-left compositor must apply the equivalent offset directly.
fn unit_draw_origin(x: u32, y: u32) -> (i64, i64) {
    let point = sprite_top_left(x, y, PixelSize::new(UNIT_SPRITE_WIDTH, UNIT_SPRITE_HEIGHT));
    (i64::from(point.x), i64::from(point.y))
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
) -> Result<RgbaImage, RenderError> {
    if state.players.len() != factions.len() {
        return Err(RenderError::FactionCount {
            seats: state.players.len(),
            factions: factions.len(),
        });
    }

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
    render_terrain(&map, tilesets, state.weather.kind, &mut canvas);

    // Cargo is spelled on the carried unit, so gather the loaded transports
    // once rather than rescanning the roster for every unit drawn.
    let loaded_transports = state.units.loaded_transports();
    let mut overlays = Vec::with_capacity(state.units.len());
    for unit in state.units.iter() {
        let Location::Board { position } = unit.location else {
            continue;
        };
        // Only the player whose turn it is can spend units, so only their
        // spent units are greyed out — the same rule the client draws by.
        let spent = unit.action != UnitAction::Ready
            && state.player_id(unit.owner) == &state.turn.active_player;
        let origin = draw_unit(
            &mut canvas,
            unit.kind,
            factions[unit.owner.get()],
            u32::from(position.x),
            u32::from(position.y),
            !spent,
            tilesets,
        );
        overlays.push((
            origin,
            UnitStatus::of_state_unit(state, unit, position, &loaded_transports),
        ));
    }
    for (origin, status) in overlays {
        draw_unit_status(&mut canvas, origin, status, tilesets)?;
    }
    Ok(canvas)
}

/// Grey out a unit that cannot act.
///
/// The client tints on the GPU, which decodes the texel to linear light,
/// multiplies, and re-encodes. Scaling the encoded sRGB bytes instead is a
/// different curve, so the conversion happens here too.
fn tint_inactive(sprite: &mut RgbaImage) {
    let factor = srgb_to_linear(INACTIVE_UNIT_TINT_SRGB);
    for pixel in sprite.pixels_mut() {
        for channel in &mut pixel.0[..3] {
            let linear = srgb_to_linear(f32::from(*channel) / 255.0) * factor;
            *channel = (linear_to_srgb(linear).clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

/// Decode one sRGB-encoded component (0..=1) to linear light.
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Encode one linear-light component (0..=1) back to sRGB.
fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// The 16x16 plain-grass backdrop tile: the bottom (ground) half of the Plain
/// terrain cell for the given weather, used to fill the base layer.
fn plain_backdrop_tile(tiles: &RgbaImage, weather: Weather) -> RgbaImage {
    let rect = plain_backdrop_rect(weather);
    tiles
        .view(rect.x, rect.y, rect.width, rect.height)
        .to_image()
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_content::{
        TERRAIN_SPRITE_HEIGHT, TILESHEET_COLUMNS, TILESHEET_ROWS, UNIT_SPRITESHEET_COLUMNS,
        UNIT_SPRITESHEET_PADDING_X, UNIT_SPRITESHEET_PADDING_Y, UNIT_SPRITESHEET_ROWS, UiAtlasSize,
        UiAtlasSprite,
    };
    use awbrn_map::{Deployment, Dimensions};
    use awbrn_types::AwbwTerrain;

    /// Synthetic atlases: the tests here compare renders with each other, so
    /// the sprites only have to be distinguishable, not real.
    fn test_tilesets() -> Tilesets {
        fn pixel(x: u32, y: u32, salt: u32) -> image::Rgba<u8> {
            let cell_x = x / 8;
            let cell_y = y / 8;
            image::Rgba([
                cell_x.wrapping_add(salt) as u8,
                cell_y.wrapping_add(salt * 3) as u8,
                cell_x
                    .wrapping_mul(37)
                    .wrapping_add(cell_y.wrapping_mul(17))
                    .wrapping_add(salt * 7) as u8,
                255,
            ])
        }

        let names = (1..=9)
            .map(|health| format!("Healthv2/{health}.png"))
            .chain(
                ["Capturing.png", "HasCargo.png", "Dive.png"]
                    .into_iter()
                    .map(String::from),
            );
        let sprites = names
            .enumerate()
            .map(|(index, name)| UiAtlasSprite {
                name,
                x: index as u32 * 8,
                y: 0,
                width: 8,
                height: 8,
            })
            .collect();

        Tilesets {
            tiles: RgbaImage::from_fn(
                TILESHEET_COLUMNS * TILE_SIZE,
                TILESHEET_ROWS * TERRAIN_SPRITE_HEIGHT,
                |x, y| pixel(x, y, 1),
            ),
            units: RgbaImage::from_fn(
                UNIT_SPRITESHEET_COLUMNS * (UNIT_SPRITE_WIDTH + UNIT_SPRITESHEET_PADDING_X),
                UNIT_SPRITESHEET_ROWS * (UNIT_SPRITE_HEIGHT + UNIT_SPRITESHEET_PADDING_Y),
                |x, y| pixel(x, y, 2),
            ),
            ui: RgbaImage::from_fn(160, 160, |x, y| pixel(x, y, 3)),
            ui_atlas: UiAtlasManifest {
                size: UiAtlasSize {
                    width: 160,
                    height: 160,
                },
                sprites,
            },
        }
    }

    /// A two-player state whose second player owns a loaded APC.
    fn fixture() -> (State, AwbrnMap, [PlayerFaction; 2]) {
        let json: serde_json::Value = serde_json::from_slice(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../spec/fixtures/fog/vision-sources-and-terrain.json"
        )))
        .unwrap();
        let state: State = serde_json::from_value(json["initial_state"].clone()).unwrap();
        let map = AwbrnMap::new(state.board.dimensions(), GraphicalTerrain::Plain);
        (
            state,
            map,
            [PlayerFaction::OrangeStar, PlayerFaction::BlueMoon],
        )
    }

    fn board_unit(state: &State, id: u32) -> (&awvm::semantic::Unit, Pos) {
        let unit = state.units.get(UnitId::new(id)).unwrap();
        let Location::Board { position } = unit.location else {
            panic!("unit {id} is not on the board");
        };
        (unit, position)
    }

    #[test]
    fn terrain_cell_rect_walks_the_grid() {
        assert_eq!(
            terrain_sprite_rect(0),
            awbrn_content::PixelRect::new(0, 0, 16, 32)
        );
        assert_eq!(
            terrain_sprite_rect(1),
            awbrn_content::PixelRect::new(16, 0, 16, 32)
        );
        // First cell of the second row.
        let cols = awbrn_content::TILESHEET_COLUMNS as u16;
        assert_eq!(
            terrain_sprite_rect(cols),
            awbrn_content::PixelRect::new(0, 32, 16, 32)
        );
    }

    #[test]
    fn unit_cell_rect_applies_offset_and_padding() {
        // Offset (1,1) for cell 0.
        assert_eq!(
            unit_sprite_rect(0),
            awbrn_content::PixelRect::new(1, 1, UNIT_SPRITE_WIDTH, UNIT_SPRITE_HEIGHT)
        );
        // Cell 1: x advances by width + padding.
        assert_eq!(
            unit_sprite_rect(1),
            awbrn_content::PixelRect::new(
                1 + UNIT_SPRITE_WIDTH + awbrn_content::UNIT_SPRITESHEET_PADDING_X,
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
        let capture = unit_overlay_spec(UnitOverlay::Capturing).unwrap();
        let health =
            unit_overlay_spec(UnitOverlay::Health(GraphicalHp::Visible(VisualHp::new(9)))).unwrap();
        let (capture_x, capture_y) = overlay_origin(origin, 8, 8, capture.offset);
        let (health_x, health_y) = overlay_origin(origin, 8, 7, health.offset);

        assert_eq!((capture_x, capture_y), (1, 24));
        assert_eq!((health_x, health_y), (8, 25));
    }

    #[test]
    fn an_inactive_unit_tints_in_linear_light() {
        let mut sprite = RgbaImage::from_pixel(1, 1, image::Rgba([128, 128, 128, 255]));
        tint_inactive(&mut sprite);

        // Scaling the encoded bytes would give 86; the GPU's linear multiply
        // lands on 84 once re-encoded.
        assert_eq!(sprite.get_pixel(0, 0), &image::Rgba([84, 84, 84, 255]));
    }

    #[test]
    fn the_authoritative_render_follows_the_state_weather() {
        let (mut state, map, factions) = fixture();
        let tilesets = test_tilesets();

        let clear = render_state(&map, &state, &factions, &tilesets).unwrap();
        state.weather.kind = Weather::Snow;
        let snow = render_state(&map, &state, &factions, &tilesets).unwrap();

        assert_ne!(clear.as_raw(), snow.as_raw());
    }

    #[test]
    fn a_spent_unit_of_the_active_player_renders_tinted() {
        let (mut state, map, factions) = fixture();
        let tilesets = test_tilesets();
        let ready = render_state(&map, &state, &factions, &tilesets).unwrap();

        // The fixture's first unit belongs to the player whose turn it is.
        let owner = state.units.at(0).unwrap().owner;
        assert_eq!(state.player_id(owner), &state.turn.active_player);
        state.units.at_mut(0).unwrap().action = UnitAction::Spent;
        let spent = render_state(&map, &state, &factions, &tilesets).unwrap();
        assert_ne!(ready.as_raw(), spent.as_raw());

        // A waiting unit of any other player says nothing to the viewer, so it
        // keeps its colors.
        let (idle, _) = board_unit(&state, 1);
        let idle = state.units.index_of(idle.id).unwrap();
        state.units.at_mut(idle).unwrap().action = UnitAction::Spent;
        let other_player_spent = render_state(&map, &state, &factions, &tilesets).unwrap();
        assert_eq!(spent.as_raw(), other_player_spent.as_raw());
    }

    #[test]
    fn overlays_follow_cargo_and_concealment() {
        let (mut state, ..) = fixture();
        let loaded = state.units.loaded_transports();

        // Unit 1 is an APC carrying unit 2; unit 3 is a bomber carrying nobody.
        let (transport, position) = board_unit(&state, 1);
        let status = UnitStatus::of_state_unit(&state, transport, position, &loaded);
        assert!(status.cargo);
        assert!(!status.dive);

        let (empty, position) = board_unit(&state, 3);
        assert!(!UnitStatus::of_state_unit(&state, empty, position, &loaded).cargo);

        let index = state.units.index_of(empty.id).unwrap();
        state.units.at_mut(index).unwrap().concealment = Concealment::Hidden;
        let (hidden, position) = board_unit(&state, 3);
        assert!(UnitStatus::of_state_unit(&state, hidden, position, &loaded).dive);
    }

    #[test]
    fn a_roster_without_a_faction_for_every_seat_is_an_error() {
        let (state, map, _) = fixture();
        let tilesets = test_tilesets();

        assert!(matches!(
            render_state(&map, &state, &[PlayerFaction::OrangeStar], &tilesets),
            Err(RenderError::FactionCount {
                seats: 2,
                factions: 1
            })
        ));
    }

    /// A three-tile board of plains, holding one infantry at its middle.
    fn deployed_map(hp: u8) -> AwbwMap {
        let mut map = AwbwMap::new(Dimensions::new(3, 3), AwbwTerrain::Plain);
        map.deploy(
            Pos::new(1, 1),
            Deployment {
                unit: Unit::Infantry,
                hp: VisualHp::new(hp),
                faction: PlayerFaction::OrangeStar,
            },
        )
        .expect("the middle tile is on the board and empty");
        map
    }

    /// Decode a PNG back to its pixels, and say how it was written.
    fn decode(png: &[u8]) -> (RgbaImage, png::ColorType, png::BitDepth) {
        let reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let color = reader.info().color_type;
        let depth = reader.info().bit_depth;
        let image = image::load_from_memory_with_format(png, ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        (image, color, depth)
    }

    #[test]
    fn few_colors_are_written_with_a_palette_and_read_back_the_same() {
        // Sixteen colours: four bits for each pixel.
        let image = RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([(x as u8 % 4) * 64, (y as u8 % 4) * 64, 0, 255])
        });

        let (decoded, color, depth) = decode(&encode_png(&image).unwrap());

        assert_eq!(color, png::ColorType::Indexed);
        assert_eq!(depth, png::BitDepth::Four);
        assert_eq!(decoded.as_raw(), image.as_raw());
    }

    #[test]
    fn a_palette_keeps_the_transparent_pixels_transparent() {
        // Big enough that the palette pays for the chunks it adds; on a tiny
        // image the pixels as drawn can still be the smaller of the two.
        let image = RgbaImage::from_fn(64, 64, |x, _| {
            if x < 32 {
                image::Rgba([0, 0, 0, 0])
            } else {
                image::Rgba([0, 0, 0, 255])
            }
        });

        let (decoded, color, _) = decode(&encode_png(&image).unwrap());

        // Black is both the clear colour and an opaque one, which only a
        // palette entry can tell apart.
        assert_eq!(color, png::ColorType::Indexed);
        assert_eq!(decoded.as_raw(), image.as_raw());
    }

    #[test]
    fn too_many_colors_fall_back_to_the_pixels_as_drawn() {
        // 257 colours, one past what a palette can name.
        let image = RgbaImage::from_fn(257, 1, |x, _| {
            image::Rgba([(x >> 8) as u8, (x & 0xff) as u8, 0, 255])
        });

        let (decoded, color, _) = decode(&encode_png(&image).unwrap());

        assert_eq!(color, png::ColorType::Rgba);
        assert_eq!(decoded.as_raw(), image.as_raw());
    }

    #[test]
    fn a_map_renders_the_units_it_deploys() {
        let tilesets = test_tilesets();
        let bare = render_map(
            &AwbwMap::new(Dimensions::new(3, 3), AwbwTerrain::Plain),
            &tilesets,
        )
        .unwrap();
        let deployed = render_map(&deployed_map(10), &tilesets).unwrap();

        assert_eq!(deployed.dimensions(), bare.dimensions());
        assert_ne!(deployed.as_raw(), bare.as_raw());
    }

    #[test]
    fn a_damaged_unit_renders_its_health() {
        let tilesets = test_tilesets();

        assert_ne!(
            render_map(&deployed_map(4), &tilesets).unwrap().as_raw(),
            render_map(&deployed_map(10), &tilesets).unwrap().as_raw()
        );
    }
}
