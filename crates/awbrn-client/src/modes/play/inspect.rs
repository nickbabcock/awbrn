//! Reading a unit without commanding it.
//!
//! Commanding a unit answers "what shall this do"; inspection answers "what is
//! this". The board already knows the second answer for every unit on it, and
//! until now it only offered the answer for the units the viewer was allowed to
//! move. A player watching an opponent's turn, a spectator, and anyone stepping
//! through a replay all looked at a board that reported nothing.
//!
//! Inspection is not a phase of [`super::PlayUiPhase`]. It is a subject that
//! sits beside the selection, so it cannot reach a menu and cannot commit. The
//! selection implies it: a commanded unit is an inspected unit, and the two
//! draw the same fields, so a player learns one language rather than two.

use std::collections::HashSet;

use awbrn_bevy::MapPosition;
use awbrn_bevy::world::{GameMap, Unit};
use awbrn_map::Pos;
use awbrn_types::UnitExt;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::core::coords::{TILE_SIZE, position_to_world_translation};
use crate::core::{RenderLayer, SpriteSize};

use crate::features::event_bus::{EventSink, InspectedUnitReadout, UnitInspectionChanged};

use super::{PlayUnitSelectionParams, SelectedUnit};

/// Outlines share the movement field's layer and separate by their own depth,
/// so the three fields stack in one predictable order.
const OUTLINE_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::MOVE_RANGE_OVERLAY,
};

/// The unit the board is currently reporting on.
///
/// One entity, or none. This never holds a unit the viewer cannot see: the
/// fields are derived from the viewer's own projection, and a unit missing from
/// that projection produces no fields and is therefore never inspected.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InspectedUnit(pub Option<Entity>);

/// How far a unit reaches with each of its three senses.
///
/// The three sets are kept apart rather than merged because each is drawn in a
/// different form: movement is the only field a unit can stand in and is the
/// only one that gets a fill, while the other two are outlines that must lie
/// over it without burying it.
#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct InspectionFields {
    /// Where the unit stands, absent when nothing is inspected.
    pub origin: Option<Pos>,
    /// Everywhere the unit could move to.
    pub movement: HashSet<Pos>,
    /// Everywhere the unit could put a shot.
    pub fire: HashSet<Pos>,
    /// Everywhere the unit reveals.
    pub vision: HashSet<Pos>,
    /// Tiles inside the vision that still conceal a ground unit.
    pub blind: HashSet<Pos>,
    /// The same three answers as numbers, for the readout.
    pub readout: Option<InspectionReadout>,
    /// Whether the selection is already painting this unit's movement.
    ///
    /// A commanded unit and a read unit are the same subject, so the movement
    /// glass has to come from one renderer or the other and never from both.
    /// The selection owns it whenever it holds the same unit, because that
    /// field also decides where a tap may land.
    pub movement_drawn_by_selection: bool,
}

impl InspectionFields {
    fn clear(&mut self) {
        self.origin = None;
        self.movement.clear();
        self.fire.clear();
        self.vision.clear();
        self.blind.clear();
        self.readout = None;
        self.movement_drawn_by_selection = false;
    }
}

/// What the readout says about an inspected unit.
///
/// These are the same facts the board paints. A player who only wants the
/// number should not have to read a field of colour to get it, and a player
/// who cannot see colour at all gets the whole feature here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectionReadout {
    /// Movement points, after fuel and the commander.
    pub movement: u32,
    /// The firing band, absent for a unit with no weapon that reaches.
    pub range: Option<(u32, u32)>,
    /// Effective sight, after the commander, the terrain and the weather.
    pub sight: u32,
    /// The unit's sight before the weather and the terrain moved it.
    ///
    /// The readout marks a sight that has been moved off its base value, so a
    /// player watching rain arrive can see what it cost them.
    pub base_sight: u32,
}

#[derive(SystemParam)]
pub(crate) struct InspectionParams<'w, 's> {
    positions: Query<'w, 's, &'static MapPosition, With<Unit>>,
    kinds: Query<'w, 's, &'static Unit>,
}

/// Work out what the inspected unit reaches.
///
/// Everything here is asked of AWVM. Movement comes from the same search the
/// selection uses, sight from the same operators the fog projection uses, and
/// the firing band from the ruleset profile. The client owns none of these
/// rules and must not restate one, or the board and the outcome drift apart.
pub(crate) fn update_inspection_fields(
    inspected: Res<InspectedUnit>,
    selected: Res<SelectedUnit>,
    unit_selection: PlayUnitSelectionParams<'_, '_>,
    params: InspectionParams<'_, '_>,
    mut fields: ResMut<InspectionFields>,
) {
    if !inspected.is_changed() && !selected.is_changed() {
        return;
    }

    let Some(entity) = inspected.0 else {
        if fields.origin.is_some() {
            fields.clear();
        }
        return;
    };

    let mut next = InspectionFields::default();
    if let Some(origin) = params
        .positions
        .get(entity)
        .ok()
        .map(|position| position.position())
    {
        next.origin = Some(origin);
        next.movement_drawn_by_selection = selected
            .0
            .is_some_and(|selection| selection.entity == entity);
        collect_fields(entity, origin, &unit_selection, &params, &mut next);
    }

    if *fields != next {
        *fields = next;
    }
}

/// Fill `into` with everything the unit at `origin` reaches.
///
/// A unit the viewer's projection does not describe leaves the fields empty
/// rather than guessed at. That is the same limit the order list already
/// carries, and an empty field says less than a wrong one.
fn collect_fields(
    entity: Entity,
    origin: Pos,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
    params: &InspectionParams<'_, '_>,
    into: &mut InspectionFields,
) {
    // The session is opened on whatever unit the viewer can command, because
    // reifying the projection is what gives a state to ask about at all. Which
    // unit that was does not matter here: the questions below name the subject
    // by its own id.
    let Some(session) = viewer_session(unit_selection) else {
        return;
    };
    let state = session.state();
    // A projection gives every enemy a synthetic id, so the board's own id can
    // only ever name a friendly unit. The tile is what both sides agree on.
    let Some(unit_id) = unit_id_at(state, origin) else {
        return;
    };

    // Movement, from the search that answers for a unit of any seat. The
    // selection's own field answers only for the seat whose turn it is, which
    // is exactly the unit an inspection does not need help with.
    let mut movement_points = 0;
    if let Ok(field) = awvm::query::reachable(state, unit_id) {
        movement_points = u32::try_from(field.budget()).unwrap_or(u32::MAX);
        for (position, _) in field.destinations() {
            if position != origin {
                into.movement.insert(position);
            }
        }
    }

    // Sight, and the tiles inside it that still hide a ground unit.
    let mut sight = 0;
    if let Ok(vision) = awvm::query::vision(state, unit_id) {
        sight = u32::try_from(vision.sight).unwrap_or(u32::MAX);
        into.vision.extend(vision.seen);
        into.blind.extend(vision.blind);
    }

    let Ok(unit) = params.kinds.get(entity) else {
        return;
    };
    let kind = unit.0;
    let (minimum, maximum) = (kind.attack_range_min(), kind.attack_range_max());
    let range = (maximum > 0).then_some((minimum, maximum));

    if let Some((minimum, maximum)) = range {
        if minimum > 1 {
            // An indirect fires from where it stands, so its band is measured
            // from the origin and nowhere else. A player who walks it forward
            // watches the band go, which is the rule drawn rather than
            // explained.
            collect_band(origin, minimum, maximum, &state.board, into);
        } else {
            // A direct carries its reach with it, so the band is measured from
            // every tile it could stop on as well as the one it holds.
            let mut sources: Vec<Pos> = into.movement.iter().copied().collect();
            sources.push(origin);
            for source in sources {
                collect_band(source, minimum, maximum, &state.board, into);
            }
        }
    }

    into.readout = Some(InspectionReadout {
        movement: movement_points,
        range,
        sight,
        base_sight: kind.base_vision(),
    });
}

/// The unit standing at `position` in `state`, named by that state's own id.
///
/// An observation discloses the id of a friendly unit and withholds the id of
/// an enemy, replacing it with a synthetic one. A caller that needs to ask
/// about an enemy therefore cannot carry an id in from the board, and asks by
/// tile instead.
fn unit_id_at(state: &awvm::semantic::State, position: Pos) -> Option<awvm::semantic::UnitId> {
    state
        .units
        .iter()
        .find(|unit| {
            matches!(
                unit.location,
                awvm::semantic::Location::Board { position: at } if at == position
            )
        })
        .map(|unit| unit.id)
}

/// A session over the viewer's own projection.
///
/// Any unit the viewer can reify opens the same state, so this takes the first
/// one that answers rather than asking for a particular unit. Reifying is the
/// cost here, and it is paid once per inspection.
fn viewer_session(
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> Option<awvm::session::Session> {
    let (Some(observations), Some(viewpoint)) = (
        unit_selection.observations.as_deref(),
        unit_selection.viewpoint.as_deref(),
    ) else {
        return None;
    };
    let awbrn_bevy::replay::ReplayViewpoint::Player(player) = viewpoint else {
        return None;
    };
    let observation = observations.for_player(*player)?;
    awvm::session::Session::from_observation(observation).ok()
}

/// Add every tile between `minimum` and `maximum` steps of `from`.
///
/// The ruleset measures range as Manhattan distance, so the band is a diamond
/// and the walk is bounded by the range rather than by the board.
fn collect_band(
    from: Pos,
    minimum: u32,
    maximum: u32,
    board: &awvm::semantic::Board,
    into: &mut InspectionFields,
) {
    let radius = i16::try_from(maximum).unwrap_or(i16::MAX);
    for dy in -radius..=radius {
        let span = radius - dy.abs();
        for dx in -span..=span {
            let distance = u32::from(dx.unsigned_abs()) + u32::from(dy.unsigned_abs());
            if distance < minimum {
                continue;
            }
            let Some(position) = offset(from, dx, dy) else {
                continue;
            };
            if board.contains(position) {
                into.fire.insert(position);
            }
        }
    }
}

/// The coordinate `dx` right and `dy` down of `from`, where one exists.
///
/// The board is anchored at the origin, so a tile off the top or left edge has
/// no coordinate at all and is dropped rather than clamped onto the far edge.
fn offset(from: Pos, dx: i16, dy: i16) -> Option<Pos> {
    let x = i16::from(from.x).checked_add(dx)?;
    let y = i16::from(from.y).checked_add(dy)?;
    Some(Pos::new(u8::try_from(x).ok()?, u8::try_from(y).ok()?))
}

/// Keep the inspection pointed at whatever the selection holds.
///
/// Commanding a unit is a way of reading it, so the selection drives the
/// subject rather than competing with it. This is what keeps one visual
/// language on the board: the fields under a unit the player is moving are the
/// same fields as under a unit they are only looking at.
pub(crate) fn follow_selection(selected: Res<SelectedUnit>, mut inspected: ResMut<InspectedUnit>) {
    if !selected.is_changed() {
        return;
    }
    match selected.0 {
        Some(selection) => {
            if inspected.0 != Some(selection.entity) {
                inspected.0 = Some(selection.entity);
            }
        }
        // Letting go of a unit stops reporting on it. A field left painted
        // under no unit is a field the player cannot dismiss.
        None => {
            if inspected.0.is_some() {
                inspected.0 = None;
            }
        }
    }
}

/// Stop reporting on a unit that has left the board.
pub(crate) fn clear_missing_inspection(
    mut inspected: ResMut<InspectedUnit>,
    units: Query<Entity, With<Unit>>,
) {
    if let Some(entity) = inspected.0
        && units.get(entity).is_err()
    {
        inspected.0 = None;
    }
}

pub(crate) fn cleanup_inspection(
    mut commands: Commands,
    mut inspected: ResMut<InspectedUnit>,
    mut fields: ResMut<InspectionFields>,
    mut emitted: ResMut<EmittedInspection>,
    move_glass: Query<Entity, With<InspectionMoveGlass>>,
    fire_outlines: Query<Entity, With<InspectionFireOutline>>,
    vision_outlines: Query<Entity, With<InspectionVisionOutline>>,
) {
    inspected.0 = None;
    fields.clear();
    emitted.0 = None;

    for entity in move_glass
        .iter()
        .chain(fire_outlines.iter())
        .chain(vision_outlines.iter())
    {
        commands.entity(entity).try_despawn();
    }
}

/// The solid outline that says what a unit can hit.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectionFireOutline;

/// The dashed outline that says what a unit can see.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectionVisionOutline;

/// The firing band, in the same red the board already uses for damage.
const FIRE_OUTLINE_COLOR: Color = Color::srgba(0.92, 0.08, 0.12, 0.85);
/// Sight, in the amber the interface already spends on a low supply.
const VISION_OUTLINE_COLOR: Color = Color::srgba(0.98, 0.74, 0.09, 0.88);
const OUTLINE_WIDTH: f32 = 1.0;
/// How long each dash of the vision outline is, in world units.
///
/// A tile divides into three dashes and two gaps, which keeps the rhythm the
/// same on every edge. An outline whose dashes do not divide the tile reads as
/// a rendering fault rather than as a line style.
const VISION_DASH: f32 = TILE_SIZE / 5.0;

/// Where the outlines sit: above the movement glass and below the units, so a
/// field never covers the sprite it describes.
const OUTLINE_Z: f32 = 0.02;

/// The four sides of `tile` that face out of `field`.
///
/// An outline is only drawn where the field stops. Drawing every side of every
/// tile would put a grid over the board instead of a boundary around a region.
fn boundary_edges(tile: Pos, field: &HashSet<Pos>) -> impl Iterator<Item = usize> + '_ {
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
const fn edge_is_horizontal(index: usize) -> bool {
    index == 0 || index == 2
}

/// Where the edge at `index` sits, relative to the middle of its tile.
fn edge_offset(index: usize) -> Vec3 {
    let half = (TILE_SIZE - OUTLINE_WIDTH) * 0.5;
    match index {
        0 => Vec3::new(0.0, half, OUTLINE_Z),
        1 => Vec3::new(-half, 0.0, OUTLINE_Z),
        2 => Vec3::new(0.0, -half, OUTLINE_Z),
        _ => Vec3::new(half, 0.0, OUTLINE_Z),
    }
}

/// Draw the band a unit can put a shot into.
///
/// The band is an outline and never a fill. It lies over the movement glass in
/// every case where the unit is a direct, and a second filled field there would
/// bury the first one and leave the board unreadable.
pub(crate) fn sync_fire_outline(
    mut commands: Commands,
    game_map: Res<GameMap>,
    fields: Res<InspectionFields>,
    outlines: Query<Entity, With<InspectionFireOutline>>,
) {
    if !fields.is_changed() {
        return;
    }
    for entity in &outlines {
        commands.entity(entity).try_despawn();
    }

    let mut tiles: Vec<Pos> = fields.fire.iter().copied().collect();
    tiles.sort();
    for tile in tiles {
        let center = position_to_world_translation(&OUTLINE_SPRITE_SIZE, tile, &game_map);
        for index in boundary_edges(tile, &fields.fire) {
            let size = if edge_is_horizontal(index) {
                Vec2::new(TILE_SIZE, OUTLINE_WIDTH)
            } else {
                Vec2::new(OUTLINE_WIDTH, TILE_SIZE)
            };
            commands.spawn((
                InspectionFireOutline,
                Sprite::from_color(FIRE_OUTLINE_COLOR, size),
                OUTLINE_SPRITE_SIZE,
                Transform::from_translation(center + edge_offset(index)),
            ));
        }
    }
}

/// Draw what a unit reveals.
///
/// The line is dashed, and that is the whole distinction from the firing band
/// beside it. Sight is a soft claim — terrain conceals, and a unit inside the
/// ring can still be missed — while a firing band is a hard rule. The line
/// style carries that difference, so the two fields stay apart for a player who
/// cannot separate them by colour.
pub(crate) fn sync_vision_outline(
    mut commands: Commands,
    game_map: Res<GameMap>,
    fields: Res<InspectionFields>,
    outlines: Query<Entity, With<InspectionVisionOutline>>,
) {
    if !fields.is_changed() {
        return;
    }
    for entity in &outlines {
        commands.entity(entity).try_despawn();
    }

    let mut tiles: Vec<Pos> = fields.vision.iter().copied().collect();
    tiles.sort();
    for tile in tiles {
        let center = position_to_world_translation(&OUTLINE_SPRITE_SIZE, tile, &game_map);
        for index in boundary_edges(tile, &fields.vision) {
            let offset = edge_offset(index);
            let horizontal = edge_is_horizontal(index);
            // Three dashes to a tile, with a gap between each pair.
            for step in 0..3 {
                let along = (step as f32 - 1.0) * (TILE_SIZE / 2.5);
                let size = if horizontal {
                    Vec2::new(VISION_DASH, OUTLINE_WIDTH)
                } else {
                    Vec2::new(OUTLINE_WIDTH, VISION_DASH)
                };
                let slide = if horizontal {
                    Vec3::new(along, 0.0, 0.0)
                } else {
                    Vec3::new(0.0, along, 0.0)
                };
                commands.spawn((
                    InspectionVisionOutline,
                    Sprite::from_color(VISION_OUTLINE_COLOR, size),
                    OUTLINE_SPRITE_SIZE,
                    Transform::from_translation(center + offset + slide),
                ));
            }
        }
    }
}

/// The cyan glass that says where a unit can go.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspectionMoveGlass;

/// Draw the movement field of a unit the player is reading rather than moving.
///
/// The glass is the selection's own, in the same colours and with the same
/// edges, because a player must not have to learn that cyan means one thing
/// under their unit and another under an enemy. Only the ownership differs: the
/// selection paints its own unit, and this paints every other one.
pub(crate) fn sync_inspection_move_glass(
    mut commands: Commands,
    game_map: Res<GameMap>,
    fields: Res<InspectionFields>,
    glass: Query<Entity, With<InspectionMoveGlass>>,
) {
    if !fields.is_changed() {
        return;
    }
    for entity in &glass {
        commands.entity(entity).try_despawn();
    }
    if fields.movement_drawn_by_selection {
        return;
    }

    let mut tiles: Vec<Pos> = fields.movement.iter().copied().collect();
    tiles.sort();
    for tile in tiles {
        let center = position_to_world_translation(&OUTLINE_SPRITE_SIZE, tile, &game_map);
        commands.spawn((
            InspectionMoveGlass,
            Sprite::from_color(super::MOVE_RANGE_GLASS_COLOR, Vec2::splat(TILE_SIZE)),
            OUTLINE_SPRITE_SIZE,
            Transform::from_translation(center),
        ));
        for index in boundary_edges(tile, &fields.movement) {
            let size = if edge_is_horizontal(index) {
                Vec2::new(TILE_SIZE, OUTLINE_WIDTH)
            } else {
                Vec2::new(OUTLINE_WIDTH, TILE_SIZE)
            };
            // The light edge falls on the sides that face the light, exactly as
            // the selection's own glass does.
            let color = if index < 2 {
                super::MOVE_RANGE_GLASS_LIGHT_EDGE
            } else {
                super::MOVE_RANGE_GLASS_DARK_EDGE
            };
            commands.spawn((
                InspectionMoveGlass,
                Sprite::from_color(color, size),
                OUTLINE_SPRITE_SIZE,
                Transform::from_translation(center + edge_offset(index) - Vec3::Z * 0.01),
            ));
        }
    }
}

/// What the readout was last told, so an unchanged inspection is not re-sent
/// every frame.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EmittedInspection(Option<InspectedUnitReadout>);

/// Send the three numbers to the readout.
///
/// The numbers travel with the paint they describe. A legend that named a value
/// the board was not drawing would be worse than no legend, because a player
/// would trust it.
pub(crate) fn emit_inspection_readout(
    fields: Res<InspectionFields>,
    inspected: Res<InspectedUnit>,
    unit_selection: PlayUnitSelectionParams<'_, '_>,
    sink: Option<Res<EventSink<UnitInspectionChanged>>>,
    mut emitted: ResMut<EmittedInspection>,
) {
    if !fields.is_changed() {
        return;
    }
    let Some(sink) = sink else {
        return;
    };

    let readout = inspected
        .0
        .zip(fields.readout)
        .and_then(|(entity, values)| {
            let (unit, faction, ..) = unit_selection.units.get(entity).ok()?;
            Some(InspectedUnitReadout {
                name: unit.0.name().to_string(),
                faction_code: faction.0.country_code().to_string(),
                movement: values.movement,
                range_minimum: values.range.map(|(minimum, _)| minimum),
                range_maximum: values.range.map(|(_, maximum)| maximum),
                sight: values.sight,
                sight_modified: values.sight != values.base_sight,
            })
        });

    if emitted.0 == readout {
        return;
    }
    emitted.0 = readout.clone();
    sink.emit(UnitInspectionChanged { unit: readout });
}
