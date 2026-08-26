//! Map screenshots, drawn with the atlases the client renders with.
//!
//! A map gets its pictures once, when it is imported, in two sizes: the full
//! board, which needs the sprite atlases, and the smallmap, which needs no
//! atlas at all. That split is why the atlases are a value the caller holds
//! rather than state this module keeps: only one of the two renders wants
//! them, and the caller decides how long they live.

use anyhow::{Context, Result};
use awbrn_image::{Tilesets, encode_png, render_map, render_small_map};
use awbrn_map::ValidatedMapDocument;

/// Decode the sprite atlases a full-size screenshot is drawn from.
///
/// These are the same four files the client loads, so a screenshot and the
/// live board show one appearance.
pub fn load_atlases(tiles: &[u8], units: &[u8], ui: &[u8], ui_atlas: &[u8]) -> Result<Tilesets> {
    Tilesets::from_bytes(tiles, units, ui, ui_atlas).context("decoding the map screenshot atlases")
}

/// Draw the map at its starting position, as PNG bytes.
///
/// This is the full picture: every tile at sprite size, with the units the map
/// deploys.
pub fn full_screenshot(tilesets: &Tilesets, document: &ValidatedMapDocument) -> Result<Vec<u8>> {
    let image = render_map(document.map(), tilesets).context("drawing the map")?;
    encode_png(&image)
}

/// Draw the map as a smallmap, as PNG bytes.
///
/// This is the picture a listing shows: four pixels for each tile, terrain
/// only, from a fixed palette rather than the atlases.
pub fn small_screenshot(document: &ValidatedMapDocument) -> Result<Vec<u8>> {
    encode_png(&render_small_map(document.map()))
}
