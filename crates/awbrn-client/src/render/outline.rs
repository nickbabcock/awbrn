//! Cased outlines around a field of tiles.
//!
//! The inspection draws a unit's fire and vision fields with these helpers,
//! and the move planner draws each strike area with them. One implementation
//! keeps every field boundary on the board in the same form.

use std::collections::HashSet;

use awbrn_map::Pos;
use bevy::prelude::*;

use crate::core::coords::TILE_SIZE;
use crate::core::{RenderLayer, SpriteSize};

/// Outlines share the movement field's layer and separate by their own depth,
/// so the three fields stack in one predictable order.
pub(crate) const OUTLINE_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::MOVE_RANGE_OVERLAY,
};

pub(crate) const OUTLINE_WIDTH: f32 = 1.0;
/// The ink every field boundary is drawn on.
///
/// The board underneath is pixel art in every hue the tileset owns, and a
/// coloured hairline laid straight onto it disappears — amber over grass, red
/// over a road, and either one over the cyan glass the movement field already
/// put there. Sprite art has always solved this by outlining a shape in dark
/// ink before colouring it, so the fields do the same: a boundary reads against
/// the map rather than against luck.
pub(crate) const OUTLINE_CASING_COLOR: Color = Color::srgba(0.05, 0.06, 0.08, 0.6);
/// How far the casing stands out past its line, across both axes. Half a world
/// unit a side, which is the one pixel the art itself would have used.
pub(crate) const OUTLINE_CASING_BLEED: f32 = 1.0;

/// Where the outlines sit: above the movement glass and below the units, so a
/// field never covers the sprite it describes.
pub(crate) const OUTLINE_Z: f32 = 0.02;
/// How far under its own line a casing sits.
pub(crate) const CASING_Z: f32 = 0.005;

/// Draw one segment of a field boundary, on its casing.
///
/// Every field goes through here, so no field can be given the casing and
/// another left without it — which would read as one field being drawn more
/// carefully than the other rather than as two fields that differ in form.
pub(crate) fn spawn_segment<B: Bundle + Clone>(
    commands: &mut Commands,
    marker: B,
    color: Color,
    size: Vec2,
    translation: Vec3,
) {
    commands.spawn((
        marker.clone(),
        Sprite::from_color(
            OUTLINE_CASING_COLOR,
            size + Vec2::splat(OUTLINE_CASING_BLEED),
        ),
        OUTLINE_SPRITE_SIZE,
        Transform::from_translation(translation - Vec3::Z * CASING_Z),
    ));
    commands.spawn((
        marker,
        Sprite::from_color(color, size),
        OUTLINE_SPRITE_SIZE,
        Transform::from_translation(translation),
    ));
}

/// The four sides of `tile` that face out of `field`.
///
/// An outline is only drawn where the field stops. Drawing every side of every
/// tile would put a grid over the board instead of a boundary around a region.
pub(crate) fn boundary_edges(tile: Pos, field: &HashSet<Pos>) -> impl Iterator<Item = usize> + '_ {
    let neighbors = [
        (Pos::new(tile.x, tile.y.saturating_sub(1)), tile.y > 0),
        (Pos::new(tile.x.saturating_sub(1), tile.y), tile.x > 0),
        (Pos::new(tile.x, tile.y + 1), true),
        (Pos::new(tile.x + 1, tile.y), true),
    ];
    neighbors
        .into_iter()
        .enumerate()
        .filter_map(move |(index, (neighbor, in_bounds))| {
            (!(in_bounds && field.contains(&neighbor))).then_some(index)
        })
}

/// Whether the edge at `index` runs across the tile rather than down it.
pub(crate) const fn edge_is_horizontal(index: usize) -> bool {
    index == 0 || index == 2
}

/// The size of one boundary segment, and where it sits relative to the middle
/// of its tile, when the line runs `inset` world units inside the field.
///
/// `inset` is the distance from the tile edge to the middle of the line. Two
/// fields that share an edge can use different insets, and then both lines
/// show side by side. The segment is cut short at a convex corner and made
/// longer at a concave corner, so the lines of a field always meet.
pub(crate) fn inset_segment(
    tile: Pos,
    index: usize,
    field: &HashSet<Pos>,
    inset: f32,
) -> (Vec2, Vec3) {
    // Grid steps, with y down: the side the edge faces, and the direction
    // along the edge.
    let (normal, along) = match index {
        0 => ((0, -1), (1, 0)),
        1 => ((-1, 0), (0, 1)),
        2 => ((0, 1), (1, 0)),
        _ => ((1, 0), (0, 1)),
    };
    let contains = |dx: i32, dy: i32| {
        let x = i32::from(tile.x) + dx;
        let y = i32::from(tile.y) + dy;
        match (u8::try_from(x), u8::try_from(y)) {
            (Ok(x), Ok(y)) => field.contains(&Pos::new(x, y)),
            _ => false,
        }
    };
    let half_width = OUTLINE_WIDTH * 0.5;
    let stretch = |sign: i32| {
        let (dx, dy) = (along.0 * sign, along.1 * sign);
        if !contains(dx, dy) {
            // Convex corner: stop where the next line starts.
            half_width - inset
        } else if contains(dx + normal.0, dy + normal.1) {
            // Concave corner: go on to meet the next line.
            inset + half_width
        } else {
            0.0
        }
    };
    let (before, after) = (stretch(-1), stretch(1));
    let length = TILE_SIZE + before + after;
    let shift = (after - before) * 0.5;
    let depth = TILE_SIZE * 0.5 - inset;

    // World space has y up.
    let normal = Vec2::new(normal.0 as f32, -(normal.1 as f32));
    let along = Vec2::new(along.0 as f32, -(along.1 as f32));
    let offset = normal * depth + along * shift;
    let size = if edge_is_horizontal(index) {
        Vec2::new(length, OUTLINE_WIDTH)
    } else {
        Vec2::new(OUTLINE_WIDTH, length)
    };
    (size, offset.extend(OUTLINE_Z))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diamond(center: Pos, radius: i32) -> HashSet<Pos> {
        let mut field = HashSet::new();
        for dy in -radius..=radius {
            for dx in -(radius - dy.abs())..=(radius - dy.abs()) {
                field.insert(Pos::new(
                    (i32::from(center.x) + dx) as u8,
                    (i32::from(center.y) + dy) as u8,
                ));
            }
        }
        field
    }

    #[test]
    fn inset_lines_meet_at_convex_and_concave_corners() {
        let field = diamond(Pos::new(5, 5), 2);
        let inset = 2.5;

        // The top tip has three sides out of the field. Its top line is cut
        // at both convex corners.
        let (size, offset) = inset_segment(Pos::new(5, 3), 0, &field, inset);
        assert_eq!(size, Vec2::new(TILE_SIZE - 2.0 * (inset - 0.5), 1.0));
        assert_eq!(offset.truncate(), Vec2::new(0.0, TILE_SIZE * 0.5 - inset));

        // The tile below and left of the tip has a concave corner on its
        // right, under the tip, and a convex corner on its left.
        let (size, offset) = inset_segment(Pos::new(4, 4), 0, &field, inset);
        assert_eq!(size.x, TILE_SIZE + 1.0);
        // The line runs 2 units short on the left and 3 units long on the
        // right, so its middle moves 2.5 units right.
        assert_eq!(offset.x, 2.5);
    }

    #[test]
    fn a_tile_at_the_map_edge_faces_out_of_the_field() {
        let field = HashSet::from([Pos::new(0, 0), Pos::new(1, 0)]);
        let edges = boundary_edges(Pos::new(0, 0), &field).collect::<Vec<_>>();
        assert_eq!(edges, [0, 1, 2]);
    }
}
