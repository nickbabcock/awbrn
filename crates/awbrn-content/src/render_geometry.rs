use awbrn_types::{GraphicalHp, GraphicalTerrain, Weather};
use serde::Deserialize;

use crate::{
    TILESHEET_COLUMNS, UNIT_SPRITE_HEIGHT, UNIT_SPRITE_WIDTH, UNIT_SPRITESHEET_COLUMNS,
    UNIT_SPRITESHEET_OFFSET_X, UNIT_SPRITESHEET_OFFSET_Y, UNIT_SPRITESHEET_PADDING_X,
    UNIT_SPRITESHEET_PADDING_Y, spritesheet_index,
};

pub const TILE_SIZE: u32 = 16;
pub const TERRAIN_SPRITE_WIDTH: u32 = 16;
pub const TERRAIN_SPRITE_HEIGHT: u32 = 32;
/// Opacity of the black fog overlay.
///
/// Both renderers must agree on it, and both must composite it in linear
/// light: the GPU blends linear values, so blending the encoded sRGB bytes
/// instead darkens a tile far more than the client shows.
pub const FOG_OVERLAY_ALPHA: f32 = 0.75;
/// The sRGB grey a unit that cannot act is tinted with.
pub const INACTIVE_UNIT_TINT_SRGB: f32 = 0.67;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

impl PixelSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalOffset {
    pub x: f32,
    pub y: f32,
}

impl LogicalOffset {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl PixelPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

pub const fn terrain_sprite_rect(index: u16) -> PixelRect {
    let index = index as u32;
    PixelRect::new(
        index % TILESHEET_COLUMNS * TERRAIN_SPRITE_WIDTH,
        index / TILESHEET_COLUMNS * TERRAIN_SPRITE_HEIGHT,
        TERRAIN_SPRITE_WIDTH,
        TERRAIN_SPRITE_HEIGHT,
    )
}

pub const fn unit_sprite_rect(index: u16) -> PixelRect {
    let index = index as u32;
    let column = index % UNIT_SPRITESHEET_COLUMNS;
    let row = index / UNIT_SPRITESHEET_COLUMNS;
    PixelRect::new(
        UNIT_SPRITESHEET_OFFSET_X + column * (UNIT_SPRITE_WIDTH + UNIT_SPRITESHEET_PADDING_X),
        UNIT_SPRITESHEET_OFFSET_Y + row * (UNIT_SPRITE_HEIGHT + UNIT_SPRITESHEET_PADDING_Y),
        UNIT_SPRITE_WIDTH,
        UNIT_SPRITE_HEIGHT,
    )
}

pub const fn plain_backdrop_rect(weather: Weather) -> PixelRect {
    let cell = terrain_sprite_rect(spritesheet_index(weather, GraphicalTerrain::Plain).index());
    PixelRect::new(
        cell.x,
        cell.y + TERRAIN_SPRITE_HEIGHT - TILE_SIZE,
        TILE_SIZE,
        TILE_SIZE,
    )
}

/// Top-left, Y-down pixel position for a bottom-right-aligned sprite on a tile.
pub const fn sprite_top_left(tile_x: u32, tile_y: u32, sprite: PixelSize) -> PixelPoint {
    PixelPoint::new(
        (tile_x * TILE_SIZE + TILE_SIZE) as i32 - sprite.width as i32,
        ((tile_y + 1) * TILE_SIZE + TILE_SIZE) as i32 - sprite.height as i32,
    )
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiAtlasSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiAtlasSprite {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl UiAtlasSprite {
    pub const fn rect(&self) -> PixelRect {
        PixelRect::new(self.x, self.y, self.width, self.height)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiAtlasManifest {
    pub size: UiAtlasSize,
    pub sprites: Vec<UiAtlasSprite>,
}

impl UiAtlasManifest {
    pub fn sprite(&self, name: &str) -> Option<&UiAtlasSprite> {
        self.sprites.iter().find(|sprite| sprite.name == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitOverlay {
    Health(GraphicalHp),
    Capturing,
    Cargo,
    Dive,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnitOverlaySpec {
    pub sprite_name: String,
    /// Center-relative offset in logical pixels, with positive Y pointing down.
    pub offset: LogicalOffset,
}

pub fn unit_overlay_spec(overlay: UnitOverlay) -> Option<UnitOverlaySpec> {
    let (sprite_name, offset) = match overlay {
        UnitOverlay::Health(GraphicalHp::Visible(health))
            if health.get() >= 10 || health.get() == 0 =>
        {
            return None;
        }
        UnitOverlay::Health(GraphicalHp::Visible(health)) => (
            format!("Healthv2/{}.png", health.get()),
            LogicalOffset::new(7.5, 8.0),
        ),
        UnitOverlay::Health(GraphicalHp::Hidden) => (
            "Healthv2/Question.png".to_owned(),
            LogicalOffset::new(7.5, 8.0),
        ),
        UnitOverlay::Capturing => ("Capturing.png".to_owned(), LogicalOffset::new(0.0, 8.0)),
        UnitOverlay::Cargo => ("HasCargo.png".to_owned(), LogicalOffset::new(0.0, 8.0)),
        UnitOverlay::Dive => ("Dive.png".to_owned(), LogicalOffset::new(0.0, 8.0)),
    };
    Some(UnitOverlaySpec {
        sprite_name,
        offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_types::VisualHp;

    #[test]
    fn atlas_rectangles_include_grid_padding_and_offsets() {
        assert_eq!(terrain_sprite_rect(0), PixelRect::new(0, 0, 16, 32));
        assert_eq!(terrain_sprite_rect(1), PixelRect::new(16, 0, 16, 32));
        assert_eq!(unit_sprite_rect(0), PixelRect::new(1, 1, 23, 24));
        assert_eq!(unit_sprite_rect(1), PixelRect::new(26, 1, 23, 24));
    }

    #[test]
    fn backdrop_is_the_ground_half_of_plain() {
        let plain =
            terrain_sprite_rect(spritesheet_index(Weather::Clear, GraphicalTerrain::Plain).index());
        assert_eq!(
            plain_backdrop_rect(Weather::Clear),
            PixelRect::new(plain.x, plain.y + 16, 16, 16)
        );
    }

    #[test]
    fn overlays_share_names_and_logical_offsets() {
        let health =
            unit_overlay_spec(UnitOverlay::Health(GraphicalHp::Visible(VisualHp::new(9)))).unwrap();
        assert_eq!(health.sprite_name, "Healthv2/9.png");
        assert_eq!(health.offset, LogicalOffset::new(7.5, 8.0));
        assert!(
            unit_overlay_spec(UnitOverlay::Health(GraphicalHp::Visible(VisualHp::new(10))))
                .is_none()
        );
    }
}
