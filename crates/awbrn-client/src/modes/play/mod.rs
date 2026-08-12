use std::collections::HashMap;

use crate::core::coords::{TILE_SIZE, position_to_world_translation};
use crate::core::{AppState, GameMode, RenderLayer, SpriteSize};
use crate::features::camera::{CameraScale, FocusBoardOn};
use crate::features::event_bus::{
    AttackForecast, DamageBracket, DeleteUnitCommandRequested, EventSink, ForecastTarget,
    MoveCommandRequested, PostMoveAction, ProductionOption, ProductionOptionsChanged,
    ProductionSite, UnitActionOption, UnitActionsChanged, UnitBadge, UnitOrder,
    UnloadCommandRequested,
};
use crate::features::input::{
    BoardProjection, DragOwner, PointerGesture, PointerGestureKind, PointerSet, ReturnToTouchFloor,
};
use crate::render::UiAtlas;
use crate::render::course_arrow::{COURSE_ARROW_SPRITE_SIZE, build_course_arrow_spawns};
use awbrn_game::MapPosition;
use awbrn_game::replay::AwbwUnitId;
use awbrn_game::world::{
    BoardIndex, CarriedBy, Faction, FriendlyFactions, Fuel, GameMap, GraphicalHp, HasCargo, Unit,
    UnitActive,
};
use awbrn_map::Position;
use awbrn_types::UnitExt;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::loading::{LiveMatchBootstrap, PendingLiveTransitions};
use crate::modes::replay::presentation::{
    LiveTransitionCommand, ReplayAdvanceLock, position_from_pos,
};

const MOVE_RANGE_GLASS_COLOR: Color = Color::srgba(0.18, 0.82, 0.9, 0.28);
const MOVE_RANGE_GLASS_LIGHT_EDGE: Color = Color::srgba(0.82, 1.0, 1.0, 0.82);
const MOVE_RANGE_GLASS_DARK_EDGE: Color = Color::srgba(0.02, 0.34, 0.42, 0.72);
const MOVE_RANGE_EDGE_WIDTH: f32 = 1.0;
const PROPOSED_PATH_COLOR: Color = Color::srgba(0.88, 1.0, 0.96, 0.96);
const ATTACK_TARGET_GLASS_COLOR: Color = Color::srgba(0.92, 0.08, 0.12, 0.38);
const ATTACK_TARGET_GLASS_LIGHT_EDGE: Color = Color::srgba(1.0, 0.64, 0.66, 0.88);
const ATTACK_TARGET_GLASS_DARK_EDGE: Color = Color::srgba(0.42, 0.01, 0.03, 0.82);
const TARGET_RETICLE_ROTATIONS_PER_SECOND: f32 = 0.5;
const TARGET_RETICLE_PULSES_PER_SECOND: f32 = 0.8;
const TARGET_RETICLE_PULSE_SCALE: f32 = 0.1;

const MOVE_RANGE_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::MOVE_RANGE_OVERLAY,
};

const ATTACK_TARGET_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::UNIT + 1,
};

const TARGET_RETICLE_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::COURSE_ARROW + 1,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedUnitSelection {
    pub entity: Entity,
    pub origin: Position,
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SelectedUnit(pub Option<SelectedUnitSelection>);

#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct MoveRange {
    pub tiles: HashMap<Position, u8>,
}

#[derive(Resource, Debug, Clone, Default)]
struct SelectedMoveField(Option<awvm::query::MoveField>);

/// Legal firing tiles, grouped by the unit or destructible tile they target.
#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct AttackTargets {
    pub approaches: HashMap<Position, Vec<Position>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMoveDestinationSelection {
    pub unit: Entity,
    pub origin: Position,
    pub destination: Position,
    pub path: Vec<Position>,
    /// The enemy a drag was aimed at, when the destination was reached by
    /// releasing on one. It decides which order the menu opens on.
    pub attack_intent: Option<Position>,
}

#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct PendingMoveDestination(pub Option<PendingMoveDestinationSelection>);

/// The route currently shown beneath the pointer. `drawn_path` is the exact
/// prefix retained while the pointer moves, allowing a player to deliberately
/// route around a tile instead of being forced onto the cheapest path.
#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct ProposedMovePath {
    pub path: Vec<Position>,
    pub drawn_path: Vec<Position>,
    hovered: Option<Position>,
    was_shift_down: bool,
    /// The enemy the route is currently aimed at, when a drag is being held on
    /// one. Carried into the proposal so the menu can open on Fire.
    attack_intent: Option<Position>,
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayUiPhase {
    #[default]
    Idle,
    UnitSelected,
    DestinationSelected,
    /// A command has been sent and the server has not yet answered.
    AwaitingServer,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveRangeHighlight;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackTargetHighlight;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackTargetReticle;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProposedPathArrow;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DestinationGhost;

/// How solid the unit looks standing on the tile it has not reached yet.
const GHOST_ALPHA: f32 = 0.45;

/// The layer the ghost draws on: above the range wash and above the units, so
/// it reads as the unit having already arrived.
const DESTINATION_GHOST_Z: i8 = RenderLayer::UNIT + 1;

#[derive(SystemParam)]
pub(crate) struct PlaySelectionState<'w> {
    selected: ResMut<'w, SelectedUnit>,
    move_range: ResMut<'w, MoveRange>,
    move_field: ResMut<'w, SelectedMoveField>,
    attack_targets: ResMut<'w, AttackTargets>,
    pending_destination: ResMut<'w, PendingMoveDestination>,
    proposed_path: ResMut<'w, ProposedMovePath>,
    phase: ResMut<'w, PlayUiPhase>,
}

type UnitSelectionQueryItem<'a> = (
    &'a Unit,
    &'a Faction,
    &'a MapPosition,
    Option<&'a Fuel>,
    Has<UnitActive>,
    Has<CarriedBy>,
    Has<HasCargo>,
);

type SelectionValidityQueryItem<'a> = (
    &'a Faction,
    &'a MapPosition,
    Has<UnitActive>,
    Has<CarriedBy>,
    Has<HasCargo>,
);

#[derive(SystemParam)]
pub(crate) struct PlayUnitSelectionParams<'w, 's> {
    board_index: Res<'w, BoardIndex>,
    game_map: Res<'w, GameMap>,
    friendly_factions: Res<'w, FriendlyFactions>,
    units: Query<'w, 's, UnitSelectionQueryItem<'static>, With<Unit>>,
    unit_ids: Query<'w, 's, &'static AwbwUnitId, With<Unit>>,
    graphical_hp: Query<'w, 's, &'static GraphicalHp, With<Unit>>,
    observations: Option<Res<'w, awbrn_game::replay::RecipientObservations>>,
    viewpoint: Option<Res<'w, awbrn_game::replay::ReplayViewpoint>>,
}

#[derive(SystemParam)]
pub(crate) struct ProductionOptionsParams<'w> {
    observations: Option<Res<'w, awbrn_game::replay::RecipientObservations>>,
    viewpoint: Option<Res<'w, awbrn_game::replay::ReplayViewpoint>>,
    sink: Option<Res<'w, EventSink<ProductionOptionsChanged>>>,
}

/// What a gesture is allowed to do, and how to ask the board to move.
#[derive(SystemParam)]
pub(crate) struct PointerPolicy<'w> {
    camera_scale: Res<'w, CameraScale>,
    owner: ResMut<'w, DragOwner>,
    floor: MessageWriter<'w, ReturnToTouchFloor>,
    focus: MessageWriter<'w, FocusBoardOn>,
}

/// Bring the selected unit and everywhere it can go into view.
///
/// A selection whose consequences are off screen is a selection the player
/// cannot act on, and on a phone at a workable zoom that is most of them. The
/// middle of the range, rather than the unit, is what needs to be centred: the
/// unit is at the edge of its own reach as often as not.
fn frame_selection(
    origin: Position,
    range: &HashMap<Position, u8>,
    game_map: &GameMap,
    focus: &mut MessageWriter<FocusBoardOn>,
) {
    let (mut min, mut max) = (origin, origin);
    for tile in range.keys() {
        min = Position::new(min.x.min(tile.x), min.y.min(tile.y));
        max = Position::new(max.x.max(tile.x), max.y.max(tile.y));
    }

    let low = position_to_world_translation(&MOVE_RANGE_SPRITE_SIZE, min, game_map).truncate();
    let high = position_to_world_translation(&MOVE_RANGE_SPRITE_SIZE, max, game_map).truncate();
    focus.write(FocusBoardOn {
        world: (low + high) * 0.5,
    });
}

/// The read-only view of a selection that the hover preview needs.
#[derive(SystemParam)]
pub(crate) struct HoverState<'w> {
    selected: Res<'w, SelectedUnit>,
    move_range: Res<'w, MoveRange>,
    move_field: Res<'w, SelectedMoveField>,
    attack_targets: Res<'w, AttackTargets>,
    phase: Res<'w, PlayUiPhase>,
    owner: Res<'w, DragOwner>,
}

#[derive(SystemParam)]
pub(crate) struct UnitActionParams<'w> {
    observations: Option<Res<'w, awbrn_game::replay::RecipientObservations>>,
    viewpoint: Option<Res<'w, awbrn_game::replay::ReplayViewpoint>>,
    sink: Option<Res<'w, EventSink<UnitActionsChanged>>>,
}

#[derive(SystemParam)]
pub(crate) struct UnitCommandSinks<'w> {
    delete_unit_command: Option<Res<'w, EventSink<DeleteUnitCommandRequested>>>,
    move_command: Option<Res<'w, EventSink<MoveCommandRequested>>>,
    unload_command: Option<Res<'w, EventSink<UnloadCommandRequested>>>,
}

fn unit_is_selectable(
    faction: Faction,
    is_active: bool,
    is_carried: bool,
    has_cargo: bool,
    friendly_factions: &FriendlyFactions,
) -> bool {
    (is_active || has_cargo) && !is_carried && friendly_factions.0.contains(&faction.0)
}

/// The tiles sharing an edge with this one, in map order.
///
/// Positions are unsigned, so the two neighbours off the top and left edges are
/// dropped rather than clamped. The right and bottom edges need no guard: the
/// callers all test membership of a set that only holds on-board tiles.
fn orthogonal_neighbors(position: Position) -> impl Iterator<Item = Position> {
    [
        (position.y > 0).then(|| Position::new(position.x, position.y - 1)),
        (position.x > 0).then(|| Position::new(position.x - 1, position.y)),
        Some(Position::new(position.x, position.y + 1)),
        Some(Position::new(position.x + 1, position.y)),
    ]
    .into_iter()
    .flatten()
}

fn semantic_position(position: Position) -> Option<awvm::semantic::Pos> {
    Some(awvm::semantic::Pos::new(
        u8::try_from(position.x).ok()?,
        u8::try_from(position.y).ok()?,
    ))
}

fn position_path(path: Vec<awvm::semantic::Pos>) -> Vec<Position> {
    path.into_iter().map(position_from_pos).collect()
}

fn field_path(field: &awvm::query::MoveField, destination: Position) -> Option<Vec<Position>> {
    field
        .path_to(semantic_position(destination)?)
        .map(position_path)
}

fn field_route_cost(field: &awvm::query::MoveField, path: &[Position]) -> Option<u64> {
    let semantic: Option<Vec<_>> = path.iter().copied().map(semantic_position).collect();
    field.route_cost(&semantic?)
}

fn friendly_unit_at(position: Position, unit_selection: &PlayUnitSelectionParams<'_, '_>) -> bool {
    let Ok(Some(entity)) = unit_selection.board_index.unit_entity(position) else {
        return false;
    };
    unit_selection
        .units
        .get(entity)
        .is_ok_and(|(_, faction, _, _, _, is_carried, _)| {
            !is_carried && unit_selection.friendly_factions.0.contains(&faction.0)
        })
}

fn move_range(
    field: &awvm::query::MoveField,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> HashMap<Position, u8> {
    field
        .reach()
        .filter(|(position, _)| *position != field.origin())
        .filter(|(position, _)| {
            field.can_stop_at(*position)
                || friendly_unit_at(position_from_pos(*position), unit_selection)
        })
        .filter_map(|(position, cost)| {
            Some((position_from_pos(position), u8::try_from(cost).ok()?))
        })
        .collect()
}

fn observed_move_field(
    entity: Entity,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> Option<awvm::query::MoveField> {
    let (Some(observations), Some(viewpoint), Ok(id)) = (
        unit_selection.observations.as_deref(),
        unit_selection.viewpoint.as_deref(),
        unit_selection.unit_ids.get(entity),
    ) else {
        return None;
    };
    let awbrn_game::replay::ReplayViewpoint::Player(player) = viewpoint else {
        return None;
    };
    let observation = observations.for_player(*player)?;
    awvm::query::observed_reachable(observation, awvm::semantic::UnitId::new(id.0.as_u32())).ok()
}

fn observed_attack_targets(
    entity: Entity,
    field: Option<&awvm::query::MoveField>,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> HashMap<Position, Vec<Position>> {
    let (Some(field), Some(observations), Some(viewpoint), Ok(id)) = (
        field,
        unit_selection.observations.as_deref(),
        unit_selection.viewpoint.as_deref(),
        unit_selection.unit_ids.get(entity),
    ) else {
        return HashMap::new();
    };
    let awbrn_game::replay::ReplayViewpoint::Player(player) = viewpoint else {
        return HashMap::new();
    };
    let Some(observation) = observations.for_player(*player) else {
        return HashMap::new();
    };
    let destinations: Vec<_> = field.destinations().map(|(position, _)| position).collect();
    let Ok(attacks) = awvm::query::observed_attacks_from(
        observation,
        awvm::semantic::UnitId::new(id.0.as_u32()),
        &destinations,
    ) else {
        return HashMap::new();
    };

    let mut targets = HashMap::<Position, Vec<Position>>::new();
    for attack in attacks {
        for target in attack.targets {
            targets
                .entry(position_from_pos(target))
                .or_default()
                .push(position_from_pos(attack.from));
        }
    }
    for approaches in targets.values_mut() {
        approaches.sort_by_key(|approach| {
            (
                field
                    .step(semantic_position(*approach).expect("an AWVM position fits AWVM"))
                    .map_or(u64::MAX, |step| step.cost),
                *approach,
            )
        });
        approaches.dedup();
    }
    targets
}

fn observed_unloads_for(
    entity: Entity,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> Vec<awvm::query::ObservedUnload> {
    let (Some(observations), Some(viewpoint), Ok(id)) = (
        unit_selection.observations.as_deref(),
        unit_selection.viewpoint.as_deref(),
        unit_selection.unit_ids.get(entity),
    ) else {
        return Vec::new();
    };
    let awbrn_game::replay::ReplayViewpoint::Player(player) = viewpoint else {
        return Vec::new();
    };
    let Some(observation) = observations.for_player(*player) else {
        return Vec::new();
    };
    awvm::query::observed_unloads(observation, awvm::semantic::UnitId::new(id.0.as_u32()))
        .unwrap_or_default()
}

/// Whether the unit can act on the tile it already holds.
///
/// Most units can stay where they stand, but a teleporter moves whatever enters
/// it, so a unit that starts on one has no order that keeps it there. Such a
/// unit can still unload or delete, because these actions do not move the unit.
fn can_act_in_place(
    entity: Entity,
    origin: Position,
    field: Option<&awvm::query::MoveField>,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> bool {
    let can_stop = field.is_some_and(|field| {
        semantic_position(origin).is_some_and(|position| field.can_stop_at(position))
    });
    can_stop
        || !observed_unloads_for(entity, unit_selection).is_empty()
        || observed_can_delete(entity, unit_selection)
}

fn observed_can_delete(entity: Entity, unit_selection: &PlayUnitSelectionParams<'_, '_>) -> bool {
    let (Some(observations), Some(viewpoint), Ok(id)) = (
        unit_selection.observations.as_deref(),
        unit_selection.viewpoint.as_deref(),
        unit_selection.unit_ids.get(entity),
    ) else {
        return false;
    };
    let awbrn_game::replay::ReplayViewpoint::Player(player) = viewpoint else {
        return false;
    };
    let Some(observation) = observations.for_player(*player) else {
        return false;
    };
    awvm::query::observed_can_delete(observation, awvm::semantic::UnitId::new(id.0.as_u32()))
        .unwrap_or(false)
}

fn clear_selection_state(selection: &mut PlaySelectionState<'_>) {
    selection.selected.0 = None;
    selection.move_range.tiles.clear();
    selection.move_field.0 = None;
    selection.attack_targets.approaches.clear();
    selection.pending_destination.0 = None;
    *selection.proposed_path = ProposedMovePath::default();
    *selection.phase = PlayUiPhase::Idle;
}

fn select_unit(
    entity: Entity,
    origin: Position,
    field: Option<awvm::query::MoveField>,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
    selection: &mut PlaySelectionState<'_>,
) {
    let range = field
        .as_ref()
        .map(|field| move_range(field, unit_selection))
        .unwrap_or_default();
    let attack_targets = observed_attack_targets(entity, field.as_ref(), unit_selection);
    selection.selected.0 = Some(SelectedUnitSelection { entity, origin });
    selection.move_range.tiles = range;
    selection.move_field.0 = field;
    selection.attack_targets.approaches = attack_targets;
    selection.pending_destination.0 = None;
    *selection.proposed_path = ProposedMovePath::default();
    *selection.phase = PlayUiPhase::UnitSelected;
}

fn confirm_selected_destination(
    destination: Position,
    proposed_path: &[Position],
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
    selection: &mut PlaySelectionState<'_>,
) {
    let Some(selected_unit) = selection.selected.0 else {
        return;
    };
    let Ok((_, faction, map_position, _, is_active, is_carried, has_cargo)) =
        unit_selection.units.get(selected_unit.entity)
    else {
        clear_selection_state(selection);
        return;
    };

    if !unit_is_selectable(
        *faction,
        is_active,
        is_carried,
        has_cargo,
        &unit_selection.friendly_factions,
    ) || map_position.position() != selected_unit.origin
    {
        clear_selection_state(selection);
        return;
    }

    if destination == selected_unit.origin {
        // A unit that cannot stay here has no orders to offer. Keep it in hand
        // instead of moving to a menu that would come up empty.
        if !can_act_in_place(
            selected_unit.entity,
            selected_unit.origin,
            selection.move_field.0.as_ref(),
            unit_selection,
        ) {
            return_to_unit_selected(selection);
            return;
        }

        let path = vec![selected_unit.origin];
        selection.proposed_path.path = path.clone();
        selection.proposed_path.hovered = Some(destination);
        selection.pending_destination.0 = Some(PendingMoveDestinationSelection {
            unit: selected_unit.entity,
            origin: selected_unit.origin,
            destination,
            path,
            attack_intent: selection.proposed_path.attack_intent,
        });
        *selection.phase = PlayUiPhase::DestinationSelected;
        return;
    }

    let Some(field) = selection.move_field.0.as_ref() else {
        clear_selection_state(selection);
        return;
    };
    let destination_pos = semantic_position(destination);
    if destination_pos
        .and_then(|position| field.step(position))
        .is_none()
    {
        clear_selection_state(selection);
        return;
    }
    let path = if proposed_path.first() == Some(&selected_unit.origin)
        && proposed_path.last() == Some(&destination)
    {
        let semantic: Option<Vec<_>> = proposed_path
            .iter()
            .copied()
            .map(semantic_position)
            .collect();
        semantic
            .filter(|path| field.route_cost(path).is_some())
            .map(|_| proposed_path.to_vec())
    } else {
        None
    }
    .or_else(|| {
        destination_pos
            .and_then(|position| field.path_to(position))
            .map(position_path)
    });
    let Some(path) = path else {
        clear_selection_state(selection);
        return;
    };

    selection.proposed_path.path = path.clone();
    selection.proposed_path.hovered = Some(destination);
    selection.pending_destination.0 = Some(PendingMoveDestinationSelection {
        unit: selected_unit.entity,
        origin: selected_unit.origin,
        destination,
        path,
        attack_intent: selection.proposed_path.attack_intent,
    });
    *selection.phase = PlayUiPhase::DestinationSelected;
}

/// How far from a reachable tile a tap may land and still be pulled onto it.
///
/// The reachable set is already computed, so this costs almost nothing, and it
/// is what lets the touch floor sit below the platform minimums. It resolves
/// toward destinations only: a tap is never pulled onto an attack.
const TAP_SLOP_WORLD: f32 = TILE_SIZE * 0.4;

/// The tile a tap should be read as, given where it actually landed.
///
/// A tap on the selected unit or inside the move range is taken at face value.
/// One just outside the range is pulled to the nearest reachable neighbour,
/// but only when the pointer is genuinely near that tile.
fn resolve_tap_target(
    tapped: Position,
    world: Option<Vec2>,
    move_range: &MoveRange,
    game_map: &GameMap,
    selected_origin: Option<Position>,
) -> Position {
    if selected_origin == Some(tapped) || move_range.tiles.contains_key(&tapped) {
        return tapped;
    }
    let Some(world) = world else {
        return tapped;
    };

    orthogonal_neighbors(tapped)
        .filter(|candidate| move_range.tiles.contains_key(candidate))
        .filter_map(|candidate| {
            let center =
                position_to_world_translation(&MOVE_RANGE_SPRITE_SIZE, candidate, game_map);
            let distance = world.distance(center.truncate());
            (distance <= TILE_SIZE * 0.5 + TAP_SLOP_WORLD).then_some((candidate, distance))
        })
        .min_by(|(left_pos, left), (right_pos, right)| {
            left.partial_cmp(right)
                .unwrap_or(std::cmp::Ordering::Equal)
                // Ties resolve in map order so the same near-miss always lands
                // on the same tile.
                .then_with(|| left_pos.cmp(right_pos))
        })
        .map_or(tapped, |(candidate, _)| candidate)
}

/// Decide what the drag beginning under this pointer is for.
///
/// A press on a friendly active unit moves that unit; a press anywhere else
/// moves the camera. The question is settled at press time from the tile the
/// press landed on, so there is no timer and no ambiguity to resolve later.
pub(crate) fn claim_unit_drag(
    mut gestures: MessageReader<PointerGesture>,
    unit_selection: PlayUnitSelectionParams<'_, '_>,
    mut owner: ResMut<DragOwner>,
    mut selection: PlaySelectionState<'_>,
) {
    for gesture in gestures.read() {
        // Only the beginning of a drag decides anything. Releasing the claim
        // belongs to the system that handles the release, or the route would be
        // handed back to the hover preview before the drag had been read.
        if gesture.kind == PointerGestureKind::DragStart {
            let claimed = *selection.phase != PlayUiPhase::AwaitingServer
                && gesture
                    .tile
                    .and_then(|tile| selectable_unit_at(tile, &unit_selection))
                    .is_some_and(|(entity, origin, field)| {
                        let Some(field) = field else {
                            return false;
                        };
                        // The drag begins by selecting what it grabbed, so the
                        // range is on screen from the first pixel of travel
                        // rather than only after the release.
                        select_unit(entity, origin, Some(field), &unit_selection, &mut selection);
                        true
                    });

            *owner = if claimed {
                DragOwner::Unit
            } else {
                DragOwner::Camera
            };
        }
    }
}

/// The unit a press on this tile would pick up, with the range it would show.
fn selectable_unit_at(
    position: Position,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> Option<(Entity, Position, Option<awvm::query::MoveField>)> {
    let Ok(Some(entity)) = unit_selection.board_index.unit_entity(position) else {
        return None;
    };
    let Ok((_, faction, map_position, _, is_active, is_carried, has_cargo)) =
        unit_selection.units.get(entity)
    else {
        return None;
    };
    if !unit_is_selectable(
        *faction,
        is_active,
        is_carried,
        has_cargo,
        &unit_selection.friendly_factions,
    ) {
        return None;
    }

    let origin = map_position.position();
    let field = is_active
        .then(|| observed_move_field(entity, unit_selection))
        .flatten();
    let can_unload = !observed_unloads_for(entity, unit_selection).is_empty();
    let can_delete = observed_can_delete(entity, unit_selection);
    (field.is_some() || can_unload || can_delete).then_some((entity, origin, field))
}

pub(crate) fn handle_play_pointer_gestures(
    mut gestures: MessageReader<PointerGesture>,
    projection: BoardProjection<'_, '_>,
    mut policy: PointerPolicy<'_>,
    unit_selection: PlayUnitSelectionParams<'_, '_>,
    production_options: ProductionOptionsParams<'_>,
    mut selection: PlaySelectionState<'_>,
) {
    for gesture in gestures.read() {
        match gesture.kind {
            PointerGestureKind::Tap => {
                handle_tap(
                    gesture,
                    projection.world_at(gesture.viewport),
                    &mut policy,
                    &unit_selection,
                    &production_options,
                    &mut selection,
                );
            }
            PointerGestureKind::DragMove if *policy.owner == DragOwner::Unit => {
                extend_drag_route(gesture.tile, &mut selection);
            }
            PointerGestureKind::DragEnd => {
                if *policy.owner == DragOwner::Unit {
                    finish_drag(gesture.tile, &unit_selection, &mut selection);
                }
                *policy.owner = DragOwner::Camera;
            }
            PointerGestureKind::DragCancel => {
                if *policy.owner == DragOwner::Unit {
                    // The route goes, the unit stays picked up. A cancelled
                    // drag is a change of mind about where, not about which.
                    return_to_unit_selected(&mut selection);
                }
                *policy.owner = DragOwner::Camera;
            }
            _ => {}
        }
    }
}

fn handle_tap(
    gesture: &PointerGesture,
    world: Option<Vec2>,
    policy: &mut PointerPolicy<'_>,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
    production_options: &ProductionOptionsParams<'_>,
    selection: &mut PlaySelectionState<'_>,
) {
    let Some(tapped) = gesture.tile else {
        return;
    };

    if *selection.phase == PlayUiPhase::AwaitingServer {
        return;
    }

    if selection.selected.0.is_some() {
        close_production_options(production_options.sink.as_deref());
        let selected = selection.selected.0.expect("selection was checked above");

        if let Some(approach) = attack_approach(
            tapped,
            &selection.proposed_path.path,
            &selection.attack_targets,
            selection.move_field.0.as_ref(),
        ) {
            if gesture.coarse && policy.camera_scale.is_below_touch_floor() {
                policy.floor.write(ReturnToTouchFloor);
                return;
            }
            selection.proposed_path.attack_intent = Some(tapped);
            if selection.proposed_path.path.last() != Some(&approach) {
                selection.proposed_path.path = selection
                    .move_field
                    .0
                    .as_ref()
                    .and_then(|field| field_path(field, approach))
                    .unwrap_or_default();
            }
            let proposed_path = selection.proposed_path.path.clone();
            confirm_selected_destination(approach, &proposed_path, unit_selection, selection);
            return;
        }

        let destination = resolve_tap_target(
            tapped,
            world,
            &selection.move_range,
            &unit_selection.game_map,
            Some(selected.origin),
        );

        let destination_is_reachable = if destination == selected.origin {
            can_act_in_place(
                selected.entity,
                selected.origin,
                selection.move_field.0.as_ref(),
                unit_selection,
            )
        } else {
            semantic_position(destination).is_some_and(|position| {
                selection
                    .move_field
                    .0
                    .as_ref()
                    .and_then(|field| field.step(position))
                    .is_some()
            })
        };
        if destination_is_reachable {
            // A tile too small to hit is a tile too small to commit to. The
            // selection survives and the board comes back up to a size the
            // finger can work at.
            if gesture.coarse && policy.camera_scale.is_below_touch_floor() {
                policy.floor.write(ReturnToTouchFloor);
                return;
            }

            let proposed_path = selection.proposed_path.path.clone();
            confirm_selected_destination(destination, &proposed_path, unit_selection, selection);
            return;
        }

        // Picking up a different unit is a change of subject, not a mistake.
        if let Some((entity, origin, field)) = selectable_unit_at(tapped, unit_selection) {
            if gesture.coarse {
                frame_selection(
                    origin,
                    &field
                        .as_ref()
                        .map(|field| move_range(field, unit_selection))
                        .unwrap_or_default(),
                    &unit_selection.game_map,
                    &mut policy.focus,
                );
            }
            select_unit(entity, origin, field, unit_selection, selection);
            return;
        }

        // A tap on the open board with a destination already proposed steps
        // back to the unit rather than dropping it. Only a tap made with
        // nothing proposed lets go.
        if *selection.phase == PlayUiPhase::DestinationSelected {
            return_to_unit_selected(selection);
        } else {
            clear_selection_state(selection);
        }
        return;
    }

    if let Some((entity, origin, field)) = selectable_unit_at(tapped, unit_selection) {
        close_production_options(production_options.sink.as_deref());
        if gesture.coarse {
            frame_selection(
                origin,
                &field
                    .as_ref()
                    .map(|field| move_range(field, unit_selection))
                    .unwrap_or_default(),
                &unit_selection.game_map,
                &mut policy.focus,
            );
        }
        select_unit(entity, origin, field, unit_selection, selection);
        return;
    }

    let Ok(unit_entity) = unit_selection.board_index.unit_entity(tapped) else {
        close_production_options(production_options.sink.as_deref());
        return;
    };
    if unit_entity.is_some() {
        close_production_options(production_options.sink.as_deref());
        return;
    }

    emit_production_options(
        tapped,
        production_options.observations.as_deref(),
        production_options.viewpoint.as_deref(),
        production_options.sink.as_deref(),
    );
}

/// Follow the pointer while a unit is being dragged, holding the route at the
/// last tile it may legally end on.
///
/// The route stops following rather than disappearing, so the preview never
/// shows an illegal state. Overshoot is the most common drag error and
/// cancelling on it punishes the error hardest.
fn extend_drag_route(hovered: Option<Position>, selection: &mut PlaySelectionState<'_>) {
    if selection.selected.0.is_none() {
        return;
    }
    let Some(hovered) = hovered else {
        return;
    };
    if selection.proposed_path.hovered == Some(hovered) {
        return;
    }

    // A finger over an enemy is aiming at it. The route is held at the best
    // tile to fire from, so the preview already shows the attack it is going to
    // offer on release.
    let approach = attack_approach(
        hovered,
        &selection.proposed_path.path,
        &selection.attack_targets,
        selection.move_field.0.as_ref(),
    );
    selection.proposed_path.attack_intent = approach.map(|_| hovered);
    let target = approach.unwrap_or(hovered);

    if semantic_position(target)
        .and_then(|position| selection.move_field.0.as_ref()?.step(position))
        .is_none()
    {
        return;
    }

    selection.proposed_path.hovered = Some(hovered);
    let Some(field) = selection.move_field.0.as_ref() else {
        return;
    };
    if let Some(path) = field_path(field, target) {
        selection.proposed_path.path = path;
    }
}

/// The tile to fire on this enemy from, if it is an enemy at all.
///
/// A release on an enemy unit is explicit attack intent rather than a near
/// miss, which is why it may resolve onto a tile the pointer never touched.
/// Slop correction never does this.
fn attack_approach(
    target: Position,
    preferred_path: &[Position],
    attack_targets: &AttackTargets,
    field: Option<&awvm::query::MoveField>,
) -> Option<Position> {
    let approaches = attack_targets.approaches.get(&target)?;
    let field = field?;
    preferred_path
        .last()
        .copied()
        .filter(|approach| approaches.contains(approach))
        .filter(|_| field_route_cost(field, preferred_path).is_some())
        .or_else(|| approaches.first().copied())
}

fn finish_drag(
    released: Option<Position>,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
    selection: &mut PlaySelectionState<'_>,
) {
    let Some(selected_unit) = selection.selected.0 else {
        return;
    };

    // A release off the board keeps whatever the clamped route last showed.
    if let Some(released) = released {
        extend_drag_route(Some(released), selection);
    }

    let destination = selection
        .proposed_path
        .path
        .last()
        .copied()
        .unwrap_or(selected_unit.origin);
    if destination == selected_unit.origin {
        // Dropped where it was picked up. Nothing was decided, so the unit
        // stays in hand.
        return_to_unit_selected(selection);
        return;
    }

    let proposed_path = selection.proposed_path.path.clone();
    confirm_selected_destination(destination, &proposed_path, unit_selection, selection);
}

/// Step back to the unit with its range still shown.
///
/// This is what cancelling means everywhere: the menu's Cancel, a press outside
/// it, Escape, an abandoned drag. Dropping to `Idle` here is what used to make
/// a single mis-tap cost the player the whole selection.
fn return_to_unit_selected(selection: &mut PlaySelectionState<'_>) {
    if selection.selected.0.is_none() {
        return;
    }
    selection.pending_destination.0 = None;
    *selection.proposed_path = ProposedMovePath::default();
    *selection.phase = PlayUiPhase::UnitSelected;
}

fn close_production_options(sink: Option<&EventSink<ProductionOptionsChanged>>) {
    if let Some(sink) = sink {
        sink.emit(ProductionOptionsChanged {
            site: None,
            options: Vec::new(),
        });
    }
}

fn emit_production_options(
    position: Position,
    observations: Option<&awbrn_game::replay::RecipientObservations>,
    viewpoint: Option<&awbrn_game::replay::ReplayViewpoint>,
    sink: Option<&EventSink<ProductionOptionsChanged>>,
) {
    let Some(sink) = sink else {
        return;
    };
    let (Some(observations), Some(viewpoint)) = (observations, viewpoint) else {
        close_production_options(Some(sink));
        return;
    };
    let awbrn_game::replay::ReplayViewpoint::Player(player) = viewpoint else {
        close_production_options(Some(sink));
        return;
    };
    let Some(observation) = observations.for_player(*player) else {
        close_production_options(Some(sink));
        return;
    };
    let (Ok(x), Ok(y)) = (u8::try_from(position.x), u8::try_from(position.y)) else {
        close_production_options(Some(sink));
        return;
    };
    let semantic_position = awvm::semantic::Pos::new(x, y);
    let Some(tile) = observation.board.get(semantic_position) else {
        close_production_options(Some(sink));
        return;
    };
    let is_selectable_site = observation.turn.active_player == observation.recipient
        && observation.turn.phase == awvm::semantic::Phase::UnitAction
        && tile.owner.is_owned_by(&observation.recipient);
    if !is_selectable_site {
        close_production_options(Some(sink));
        return;
    }

    let options: Vec<_> = awvm::query::observed_production_options(observation, semantic_position)
        .into_iter()
        .map(|option| ProductionOption {
            unit: option.kind,
            name: option.kind.name().to_string(),
            cost: option.cost as u32,
            affordable: option.affordable,
        })
        .collect();
    if options.is_empty() {
        close_production_options(Some(sink));
        return;
    }
    sink.emit(ProductionOptionsChanged {
        site: Some(ProductionSite {
            x: position.x,
            y: position.y,
            facility: tile.terrain,
        }),
        options,
    });
}

/// Escape steps back one stage rather than dropping everything.
///
/// With a destination proposed it returns to the unit, which is what Cancel on
/// the menu and a press outside it also do. Only pressing it again lets the
/// unit go, so the keyboard and the two pointers all cancel the same way.
pub(crate) fn clear_selection_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    production_sink: Option<Res<EventSink<ProductionOptionsChanged>>>,
    mut selection: PlaySelectionState<'_>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if *selection.phase == PlayUiPhase::AwaitingServer {
        return;
    }

    if *selection.phase == PlayUiPhase::DestinationSelected {
        return_to_unit_selected(&mut selection);
    } else {
        clear_selection_state(&mut selection);
    }
    close_production_options(production_sink.as_deref());
}

pub(crate) fn update_proposed_move_path(
    projection: BoardProjection<'_, '_>,
    keys: Res<ButtonInput<KeyCode>>,
    hover: HoverState<'_>,
    mut proposed: ResMut<ProposedMovePath>,
) {
    // A drag draws its own route. Hover would otherwise overwrite it every
    // frame, and on a touch device there is no cursor to overwrite it with.
    if *hover.phase != PlayUiPhase::UnitSelected || *hover.owner == DragOwner::Unit {
        return;
    }
    let Some(selected_unit) = hover.selected.0 else {
        return;
    };
    let Some(field) = hover.move_field.0.as_ref() else {
        return;
    };

    let shift_down = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let shift_started = shift_down && !proposed.was_shift_down;
    // Written through the change-detection bypass: this is bookkeeping the
    // preview does not draw, and marking the resource dirty every frame would
    // make the arrow pass respawn the whole route at the frame rate.
    proposed.bypass_change_detection().was_shift_down = shift_down;
    let undo = keys.just_pressed(KeyCode::Backspace);
    if undo {
        proposed.drawn_path.pop();
        proposed.hovered = None;
    } else if shift_started && proposed.drawn_path.is_empty() && proposed.path.len() > 1 {
        // Starting to draw after seeing a shortest-path preview makes that
        // visible route the prefix the player is now editing.
        proposed.drawn_path = proposed.path.iter().copied().skip(1).collect();
    }

    let hovered = projection.cursor_tile();

    if hovered == proposed.hovered && !shift_started && !undo {
        return;
    }
    proposed.hovered = hovered;
    let Some(hovered) = hovered else {
        proposed.attack_intent = None;
        return;
    };
    let approach = attack_approach(hovered, &proposed.path, &hover.attack_targets, Some(field));
    proposed.attack_intent = approach.map(|_| hovered);
    let destination = approach.unwrap_or(hovered);

    let mut prefix = Vec::with_capacity(proposed.drawn_path.len() + 1);
    prefix.push(selected_unit.origin);
    prefix.extend(proposed.drawn_path.iter().copied());

    if undo {
        proposed.path = prefix;
        return;
    }

    if shift_down {
        if destination == selected_unit.origin {
            proposed.drawn_path.clear();
            proposed.path = vec![selected_unit.origin];
            return;
        }

        // Moving back across the last edge erases it, which makes tracing feel
        // like drawing rather than placing a series of abstract waypoints.
        if prefix.len() >= 2 && prefix[prefix.len() - 2] == destination {
            proposed.drawn_path.pop();
            prefix.pop();
            proposed.path = prefix;
            return;
        }

        let endpoint = *prefix.last().unwrap_or(&selected_unit.origin);
        let mut candidate = prefix.clone();
        if endpoint.manhattan(&destination) == 1 {
            candidate.push(destination);
        } else if let Some(route) = field_path(field, destination)
            && let Some(endpoint_index) = route.iter().position(|position| *position == endpoint)
        {
            let suffix = &route[endpoint_index + 1..];
            candidate.extend_from_slice(suffix);
            if field_route_cost(field, &candidate).is_none() {
                candidate = route;
            }
        }

        if candidate.last() == Some(&destination) && field_route_cost(field, &candidate).is_some() {
            proposed.drawn_path = candidate.iter().copied().skip(1).collect();
            proposed.path = candidate;
            return;
        }

        // The hand-drawn edge cannot fit (typically because of terrain cost),
        // so fall back to a valid cheapest route to the tile instead.
        if let Some(recalculated) = field_path(field, destination) {
            proposed.drawn_path = recalculated.iter().copied().skip(1).collect();
            proposed.path = recalculated;
        }
        return;
    }

    update_automatic_move_path(
        selected_unit.origin,
        destination,
        approach.is_some(),
        hover.move_range.tiles.contains_key(&destination),
        field,
        &mut proposed,
    );
}

fn update_automatic_move_path(
    origin: Position,
    destination: Position,
    is_attack: bool,
    is_in_move_range: bool,
    field: &awvm::query::MoveField,
    proposed: &mut ProposedMovePath,
) {
    if is_attack
        && proposed.path.last() == Some(&destination)
        && field_route_cost(field, &proposed.path).is_some()
    {
        // The current endpoint can fire on the new target. Keep the complete
        // route, including a shortest path that was not copied to drawn_path.
        return;
    }
    if destination != origin && !is_in_move_range {
        // An invalid tile between two targets is only a gap in hover input.
        // Keep the last legal route so the next target can reuse its firing
        // position when possible.
        return;
    }

    let mut prefix = Vec::with_capacity(proposed.drawn_path.len() + 1);
    prefix.push(origin);
    prefix.extend(proposed.drawn_path.iter().copied());
    proposed.path = field_path(field, destination)
        .and_then(|route| {
            let endpoint = *prefix.last().unwrap_or(&origin);
            let endpoint_index = route.iter().position(|position| *position == endpoint)?;
            let candidate: Vec<_> = prefix
                .iter()
                .copied()
                .chain(route.into_iter().skip(endpoint_index + 1))
                .collect();
            field_route_cost(field, &candidate).map(|_| candidate)
        })
        .or_else(|| field_path(field, destination))
        .unwrap_or_default();
}

pub(crate) fn clear_invalid_selection(
    friendly_factions: Res<FriendlyFactions>,
    units: Query<SelectionValidityQueryItem<'_>, With<Unit>>,
    mut committed: ResMut<CommittedCommand>,
    mut selection: PlaySelectionState<'_>,
) {
    let Some(selected_unit) = selection.selected.0 else {
        return;
    };
    let Ok((faction, map_position, is_active, is_carried, has_cargo)) =
        units.get(selected_unit.entity)
    else {
        if *selection.phase == PlayUiPhase::AwaitingServer {
            committed.0 = None;
        }
        clear_selection_state(&mut selection);
        return;
    };

    let accepted_move_finished = *selection.phase == PlayUiPhase::AwaitingServer
        && !is_active
        && committed
            .0
            .as_ref()
            .is_some_and(|snapshot| snapshot.kind == CommittedKind::Move);
    if accepted_move_finished
        || !unit_is_selectable(
            *faction,
            is_active,
            is_carried,
            has_cargo,
            &friendly_factions,
        )
        || map_position.position() != selected_unit.origin
    {
        if *selection.phase == PlayUiPhase::AwaitingServer {
            committed.0 = None;
        }
        clear_selection_state(&mut selection);
    }
}

pub(crate) fn sync_move_range_highlights(
    mut commands: Commands,
    game_map: Res<GameMap>,
    move_range: Res<MoveRange>,
    highlights: Query<Entity, With<MoveRangeHighlight>>,
) {
    if !move_range.is_changed() {
        return;
    }

    for entity in &highlights {
        commands.entity(entity).try_despawn();
    }

    let mut positions: Vec<_> = move_range.tiles.keys().copied().collect();
    positions.sort();

    for position in positions {
        let center = position_to_world_translation(&MOVE_RANGE_SPRITE_SIZE, position, &game_map);
        commands.spawn((
            MoveRangeHighlight,
            Sprite::from_color(MOVE_RANGE_GLASS_COLOR, Vec2::splat(TILE_SIZE)),
            MOVE_RANGE_SPRITE_SIZE,
            Transform::from_translation(center),
        ));

        let neighbors = [
            (
                Position::new(position.x, position.y.saturating_sub(1)),
                position.y > 0,
                true,
            ),
            (
                Position::new(position.x.saturating_sub(1), position.y),
                position.x > 0,
                true,
            ),
            (Position::new(position.x, position.y + 1), true, false),
            (Position::new(position.x + 1, position.y), true, false),
        ];
        for (edge_index, (neighbor, in_bounds, is_light)) in neighbors.into_iter().enumerate() {
            if in_bounds && move_range.tiles.contains_key(&neighbor) {
                continue;
            }
            let horizontal = edge_index == 0 || edge_index == 2;
            let edge_size = if horizontal {
                Vec2::new(TILE_SIZE, MOVE_RANGE_EDGE_WIDTH)
            } else {
                Vec2::new(MOVE_RANGE_EDGE_WIDTH, TILE_SIZE)
            };
            let half = (TILE_SIZE - MOVE_RANGE_EDGE_WIDTH) * 0.5;
            let offset = match edge_index {
                0 => Vec3::new(0.0, half, 0.01),
                1 => Vec3::new(-half, 0.0, 0.01),
                2 => Vec3::new(0.0, -half, 0.01),
                _ => Vec3::new(half, 0.0, 0.01),
            };
            commands.spawn((
                MoveRangeHighlight,
                Sprite::from_color(
                    if is_light {
                        MOVE_RANGE_GLASS_LIGHT_EDGE
                    } else {
                        MOVE_RANGE_GLASS_DARK_EDGE
                    },
                    edge_size,
                ),
                Transform::from_translation(center + offset),
            ));
        }
    }
}

pub(crate) fn sync_attack_target_highlights(
    mut commands: Commands,
    game_map: Res<GameMap>,
    targets: Res<AttackTargets>,
    highlights: Query<Entity, With<AttackTargetHighlight>>,
) {
    if !targets.is_changed() {
        return;
    }
    for entity in &highlights {
        commands.entity(entity).try_despawn();
    }

    let mut positions: Vec<_> = targets.approaches.keys().copied().collect();
    positions.sort();
    for position in positions {
        let center = position_to_world_translation(&ATTACK_TARGET_SPRITE_SIZE, position, &game_map);
        commands.spawn((
            AttackTargetHighlight,
            Sprite::from_color(ATTACK_TARGET_GLASS_COLOR, Vec2::splat(TILE_SIZE)),
            ATTACK_TARGET_SPRITE_SIZE,
            Transform::from_translation(center),
        ));

        let half = (TILE_SIZE - MOVE_RANGE_EDGE_WIDTH) * 0.5;
        let edges = [
            (
                Vec2::new(TILE_SIZE, MOVE_RANGE_EDGE_WIDTH),
                Vec3::new(0.0, half, 0.01),
                ATTACK_TARGET_GLASS_LIGHT_EDGE,
            ),
            (
                Vec2::new(MOVE_RANGE_EDGE_WIDTH, TILE_SIZE),
                Vec3::new(-half, 0.0, 0.01),
                ATTACK_TARGET_GLASS_LIGHT_EDGE,
            ),
            (
                Vec2::new(TILE_SIZE, MOVE_RANGE_EDGE_WIDTH),
                Vec3::new(0.0, -half, 0.01),
                ATTACK_TARGET_GLASS_DARK_EDGE,
            ),
            (
                Vec2::new(MOVE_RANGE_EDGE_WIDTH, TILE_SIZE),
                Vec3::new(half, 0.0, 0.01),
                ATTACK_TARGET_GLASS_DARK_EDGE,
            ),
        ];
        for (size, offset, color) in edges {
            commands.spawn((
                AttackTargetHighlight,
                Sprite::from_color(color, size),
                Transform::from_translation(center + offset),
            ));
        }
    }
}

pub(crate) fn sync_proposed_path_arrows(
    mut commands: Commands,
    game_map: Res<GameMap>,
    proposed: Res<ProposedMovePath>,
    ui_atlas: UiAtlas,
    arrows: Query<Entity, With<ProposedPathArrow>>,
) {
    if !proposed.is_changed() {
        return;
    }
    for entity in &arrows {
        commands.entity(entity).try_despawn();
    }
    for spawn in build_course_arrow_spawns(&proposed.path) {
        let mut sprite = ui_atlas.sprite_for(spawn.kind.sprite_name());
        sprite.color = PROPOSED_PATH_COLOR;
        let mut transform = Transform::from_translation(position_to_world_translation(
            &COURSE_ARROW_SPRITE_SIZE,
            spawn.position,
            &game_map,
        ));
        transform.rotation = Quat::from_rotation_z(spawn.rotation_degrees.to_radians());
        commands.spawn((ProposedPathArrow, sprite, transform));
    }
}

pub(crate) fn sync_attack_target_reticle(
    mut commands: Commands,
    game_map: Res<GameMap>,
    proposed: Res<ProposedMovePath>,
    ui_atlas: UiAtlas,
    reticles: Query<Entity, With<AttackTargetReticle>>,
) {
    if !proposed.is_changed() {
        return;
    }
    for entity in &reticles {
        commands.entity(entity).try_despawn();
    }
    let Some(target) = proposed.attack_intent else {
        return;
    };

    commands.spawn((
        AttackTargetReticle,
        ui_atlas.sprite_for("Effects/Target.png"),
        Transform::from_translation(position_to_world_translation(
            &TARGET_RETICLE_SPRITE_SIZE,
            target,
            &game_map,
        )),
    ));
}

fn apply_attack_target_reticle_pose(elapsed_seconds: f32, transform: &mut Transform) {
    let rotation = elapsed_seconds * std::f32::consts::TAU * TARGET_RETICLE_ROTATIONS_PER_SECOND;
    let pulse = (elapsed_seconds * std::f32::consts::TAU * TARGET_RETICLE_PULSES_PER_SECOND).sin();
    transform.rotation = Quat::from_rotation_z(rotation);
    transform.scale = Vec3::splat(1.0 + pulse * TARGET_RETICLE_PULSE_SCALE);
}

pub(crate) fn animate_attack_target_reticle(
    time: Res<Time>,
    mut reticles: Query<&mut Transform, With<AttackTargetReticle>>,
) {
    let elapsed = time.elapsed_secs();
    for mut transform in &mut reticles {
        apply_attack_target_reticle_pose(elapsed, &mut transform);
    }
}

pub(crate) fn cleanup_play_selection(
    mut commands: Commands,
    mut selection: PlaySelectionState<'_>,
    highlights: Query<Entity, With<MoveRangeHighlight>>,
    attack_highlights: Query<Entity, With<AttackTargetHighlight>>,
    reticles: Query<Entity, With<AttackTargetReticle>>,
    arrows: Query<Entity, With<ProposedPathArrow>>,
    ghosts: Query<Entity, With<DestinationGhost>>,
) {
    clear_selection_state(&mut selection);

    for entity in &highlights {
        commands.entity(entity).try_despawn();
    }
    for entity in &attack_highlights {
        commands.entity(entity).try_despawn();
    }
    for entity in &reticles {
        commands.entity(entity).try_despawn();
    }
    for entity in &arrows {
        commands.entity(entity).try_despawn();
    }
    for entity in &ghosts {
        commands.entity(entity).try_despawn();
    }
}

pub(crate) fn initialize_live_semantic_world(world: &mut World) {
    awbrn_game::world::initialize_terrain_semantic_world(world);
    let Some(bootstrap) = world.remove_resource::<LiveMatchBootstrap>() else {
        return;
    };

    let (config, funds, unit_costs, power_meters) =
        crate::features::player_roster::player_roster_seed_from_live_match(
            &bootstrap.players,
            &bootstrap.observation,
        );
    world.insert_resource(config);
    world.insert_resource(funds);
    world.insert_resource(unit_costs);
    world.insert_resource(power_meters);

    let mut registry = awbrn_game::replay::ReplayPlayerRegistry::default();
    for player in &bootstrap.players {
        let Some(faction) = awbrn_types::PlayerFaction::from_id(player.faction_id) else {
            warn!(
                "Ignoring invalid faction {} for live player {}",
                player.faction_id, player.player_id
            );
            continue;
        };
        registry.add_player(
            awbrn_types::AwbwGamePlayerId::new(player.player_id),
            faction,
            0,
        );
    }
    let recipient = bootstrap
        .observation
        .recipient
        .as_str()
        .parse::<u32>()
        .ok()
        .map(awbrn_types::AwbwGamePlayerId::new);
    let knowledge = awbrn_game::replay::ReplayTerrainKnowledge::from_map_and_registry(
        world.resource::<GameMap>(),
        &registry,
    );
    world.insert_resource(registry);
    world.insert_resource(knowledge);
    world.insert_resource(awbrn_game::replay::ReplayState::default());
    world.insert_resource(
        recipient
            .map(awbrn_game::replay::ReplayViewpoint::Player)
            .unwrap_or(awbrn_game::replay::ReplayViewpoint::Spectator),
    );
    let transition = awvm::semantic::ObservedTransition {
        post: bootstrap.observation,
        events: Vec::new(),
    };
    if let Err(error) = awbrn_game::replay::apply_observed_transition(world, &transition) {
        error!("Could not initialize live typed presentation state: {error}");
    }
    crate::features::player_roster::emit_player_roster_updated(world);
}

pub(crate) fn apply_pending_live_transition(
    mut commands: Commands,
    mut pending: Option<ResMut<PendingLiveTransitions>>,
    lock: Res<ReplayAdvanceLock>,
    mut committed: ResMut<CommittedCommand>,
    mut selection: PlaySelectionState<'_>,
) {
    if lock.is_active() {
        return;
    }
    let Some(transition) = pending
        .as_deref_mut()
        .and_then(|pending| pending.0.pop_front())
    else {
        return;
    };
    if committed
        .0
        .as_ref()
        .is_some_and(|snapshot| snapshot.kind == CommittedKind::Unload)
    {
        committed.0.take();
        // The unloaded unit is a new blocker beside the transport, so its
        // cached movement field is stale. Drop the selection; a transport with
        // more cargo remains selectable even when it is spent.
        clear_selection_state(&mut selection);
    }
    commands.queue(LiveTransitionCommand { transition });
}

/// The player picked an order off the destination menu.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitActionChosen {
    pub index: usize,
}

/// The destination menu was dismissed: Cancel, a press outside it, or Escape.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitActionDismissed;

/// The server refused the command that was sent.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingCommandRejected;

/// The orders last put on offer, so a chosen index means something.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct OfferedActions(pub Vec<UnitActionOption>);

/// Everything needed to put the board back the way it was before a command was
/// sent.
///
/// The interface commits optimistically, which is what makes it feel immediate,
/// but a refusal used to leave the player with a generic banner and no unit,
/// range, or route — every part of the decision they had just made, gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedSnapshot {
    pub unit: Entity,
    pub origin: Position,
    pub range: HashMap<Position, u8>,
    pub attack_targets: HashMap<Position, Vec<Position>>,
    pub path: Vec<Position>,
    pub destination: Position,
    pub attack_intent: Option<Position>,
    pub kind: CommittedKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommittedKind {
    Delete,
    Move,
    Unload,
}

#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct CommittedCommand(pub Option<CommittedSnapshot>);

fn close_unit_actions(sink: Option<&EventSink<UnitActionsChanged>>) {
    if let Some(sink) = sink {
        sink.emit(UnitActionsChanged {
            destination: None,
            options: Vec::new(),
            preselected: None,
            attacker: None,
        });
    }
}

/// Ask AWVM what this unit may do where it is going, and offer exactly that.
///
/// Nothing here decides whether an order is legal. The reducer answers against
/// the recipient's own observation, and this only names and orders what came
/// back.
pub(crate) fn emit_unit_actions(
    pending: Res<PendingMoveDestination>,
    unit_selection: PlayUnitSelectionParams<'_, '_>,
    actions: UnitActionParams<'_>,
    mut offered: ResMut<OfferedActions>,
) {
    if !pending.is_changed() {
        return;
    }

    let Some(pending) = pending.0.as_ref() else {
        offered.0.clear();
        close_unit_actions(actions.sink.as_deref());
        return;
    };
    let (Some(observations), Some(viewpoint), Some(sink)) = (
        actions.observations.as_deref(),
        actions.viewpoint.as_deref(),
        actions.sink.as_deref(),
    ) else {
        return;
    };
    let awbrn_game::replay::ReplayViewpoint::Player(player) = viewpoint else {
        close_unit_actions(Some(sink));
        return;
    };
    let (Some(observation), Ok(unit_id)) = (
        observations.for_player(*player),
        unit_selection.unit_ids.get(pending.unit),
    ) else {
        close_unit_actions(Some(sink));
        return;
    };
    let (Ok(x), Ok(y)) = (
        u8::try_from(pending.destination.x),
        u8::try_from(pending.destination.y),
    ) else {
        close_unit_actions(Some(sink));
        return;
    };

    // The AWBW unit id and the AWVM unit id are the same number by
    // construction; see `awvm_awbw::command::unit_id`.
    let semantic_unit_id = awvm::semantic::UnitId::new(unit_id.0.as_u32());
    let is_in_place = pending.destination == pending.origin;
    let unloads = if is_in_place {
        awvm::query::observed_unloads(observation, semantic_unit_id).unwrap_or_default()
    } else {
        Vec::new()
    };
    let can_delete = is_in_place && observed_can_delete(pending.unit, &unit_selection);
    let available = match awvm::query::observed_actions_at(
        observation,
        semantic_unit_id,
        awvm::semantic::Pos::new(x, y),
    ) {
        Ok(available) => available,
        Err(_) if !unloads.is_empty() || can_delete => awvm::query::ObservedActionSet::default(),
        Err(error) => {
            warn!(
                "Could not read the orders for unit {}: {error}",
                unit_id.0.as_u32()
            );
            close_unit_actions(Some(sink));
            return;
        }
    };
    // One call for every target on the menu. A forecast that could not be
    // computed is left empty rather than guessed at, and the row falls back to
    // naming its target alone.
    let forecasts = awvm::query::observed_forecasts(
        observation,
        semantic_unit_id,
        awvm::semantic::Pos::new(x, y),
        &available.attack,
    )
    .unwrap_or_else(|error| {
        warn!(
            "Could not forecast the attacks for unit {}: {error}",
            unit_id.0.as_u32()
        );
        vec![None; available.attack.len()]
    });
    let mut options = build_options(
        &available,
        &unloads,
        can_delete,
        pending,
        &unit_selection,
        &forecasts,
    );
    let attack_intent = pending.attack_intent.filter(|target| {
        options.iter().any(|option| {
            matches!(
                option.action,
                UnitOrder::Move {
                    action: PostMoveAction::Attack { target: at }
                } if at == *target
            )
        })
    });
    if let Some(target) = attack_intent {
        // Selecting a unit on the board is a complete target choice. Other
        // attacks from the same firing tile must not ask the player to choose
        // the target a second time.
        options.retain(|option| {
            matches!(
                option.action,
                UnitOrder::Move {
                    action: PostMoveAction::Attack { target: at }
                } if at == target
            )
        });
    }
    if options.is_empty() {
        offered.0.clear();
        close_unit_actions(Some(sink));
        return;
    }

    let preselected = attack_intent.and_then(|target| {
        options.iter().position(|option| {
            matches!(
                option.action,
                UnitOrder::Move {
                    action: PostMoveAction::Attack { target: at }
                } if at == target
            )
        })
    });

    offered.0 = options.clone();
    sink.emit(UnitActionsChanged {
        destination: Some(pending.destination),
        options,
        preselected,
        attacker: unit_badge(pending.unit, &unit_selection),
    });
}

/// Name the available orders, in the order the source game lists them.
///
/// An attack carries what it would cost, so a player choosing between two of
/// them is choosing between two brackets rather than two coordinates. The
/// numbers come from AWVM in one call for the whole menu: reifying the
/// observation is the expensive half of the question and asking per row would
/// pay it once a row.
fn build_options(
    available: &awvm::query::ObservedActionSet,
    unloads: &[awvm::query::ObservedUnload],
    can_delete: bool,
    pending: &PendingMoveDestinationSelection,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
    forecasts: &[Option<awvm::combat::Forecast>],
) -> Vec<UnitActionOption> {
    let mut options = Vec::new();

    for (index, target) in available.attack.iter().enumerate() {
        let position = position_from_pos(*target);
        options.push(UnitActionOption {
            name: "Fire".to_string(),
            action: UnitOrder::Move {
                action: PostMoveAction::Attack { target: position },
            },
            forecast: forecasts
                .get(index)
                .copied()
                .flatten()
                .and_then(|forecast| describe_forecast(forecast, position, unit_selection)),
        });
    }
    if available.capture {
        options.push(UnitActionOption::plain(
            "Capture",
            UnitOrder::Move {
                action: PostMoveAction::Capture,
            },
        ));
    }
    if available.supply {
        options.push(UnitActionOption::plain(
            "Supply",
            UnitOrder::Move {
                action: PostMoveAction::Supply,
            },
        ));
    }
    for target in &available.repair {
        let target = position_from_pos(*target);
        if let Some(target_id) = unit_id_at_position(target, unit_selection) {
            options.push(UnitActionOption::plain(
                "Repair",
                UnitOrder::Move {
                    action: PostMoveAction::Repair {
                        target_id: u64::from(target_id),
                    },
                },
            ));
        }
    }
    // Join and Load name the unit already standing there, which is friendly by
    // definition and therefore has a real id in the projection.
    if let Some(occupant) = unit_id_at_position(pending.destination, unit_selection) {
        if available.join {
            options.push(UnitActionOption::plain(
                "Join",
                UnitOrder::Move {
                    action: PostMoveAction::Join {
                        target_id: u64::from(occupant),
                    },
                },
            ));
        }
        if available.load {
            options.push(UnitActionOption::plain(
                "Load",
                UnitOrder::Move {
                    action: PostMoveAction::Load {
                        transport_id: u64::from(occupant),
                    },
                },
            ));
        }
    }
    if available.hide {
        options.push(UnitActionOption::plain(
            "Dive",
            UnitOrder::Move {
                action: PostMoveAction::Hide,
            },
        ));
    }
    if available.reveal {
        options.push(UnitActionOption::plain(
            "Surface",
            UnitOrder::Move {
                action: PostMoveAction::Unhide,
            },
        ));
    }
    for unload in unloads {
        let position = position_from_pos(unload.destination);
        options.push(UnitActionOption::plain(
            format!("Unload {}", unload.cargo_kind.name()),
            UnitOrder::Unload {
                cargo_id: unload.cargo.get(),
                position,
            },
        ));
    }
    for target in &available.launch {
        options.push(UnitActionOption::plain(
            "Launch",
            UnitOrder::Move {
                action: PostMoveAction::Launch {
                    target: position_from_pos(*target),
                },
            },
        ));
    }
    if available.explode {
        options.push(UnitActionOption::plain(
            "Explode",
            UnitOrder::Move {
                action: PostMoveAction::Explode,
            },
        ));
    }
    if available.wait {
        options.push(UnitActionOption::plain(
            "Wait",
            UnitOrder::Move {
                action: PostMoveAction::Wait,
            },
        ));
    }
    if can_delete {
        options.push(UnitActionOption::plain("Delete", UnitOrder::Delete));
    }

    options
}

/// Put a name and a sprite on what the numbers are about.
///
/// The bracket itself is AWVM's; everything added here is identity, read off
/// the board the player is looking at. A target with neither a unit nor a
/// destructible tile standing on it is a target this client cannot name, and an
/// unnamed forecast is worse than none.
fn describe_forecast(
    forecast: awvm::combat::Forecast,
    position: Position,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> Option<AttackForecast> {
    let target = forecast_target(position, unit_selection)?;
    Some(AttackForecast {
        target,
        damage: DamageBracket {
            low: forecast.attack.low,
            high: forecast.attack.high,
        },
        counter: forecast.counter.map(|range| DamageBracket {
            low: range.low,
            high: range.high,
        }),
        counter_first: forecast.counter_first,
        // Raw damage is compared against the health it would land on, which is
        // the one place the uncapped figure has to be read as a capped one.
        destroys: forecast.attack.low >= u16::from(forecast.target_hp),
        may_destroy: forecast.attack.low < u16::from(forecast.target_hp)
            && forecast.attack.high >= u16::from(forecast.target_hp),
    })
}

/// How one unit is drawn and named beside a number.
fn unit_badge(
    entity: Entity,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> Option<UnitBadge> {
    let (unit, faction, _, _, _, _, _) = unit_selection.units.get(entity).ok()?;
    Some(UnitBadge {
        unit: unit.0,
        name: unit.0.name().to_string(),
        faction_code: faction.0.country_code().to_string(),
        health: unit_selection
            .graphical_hp
            .get(entity)
            .ok()
            .and_then(|hp| hp.visible())
            .map(awbrn_types::VisualHp::get),
    })
}

/// Who or what is standing on the targeted tile.
fn forecast_target(
    position: Position,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> Option<ForecastTarget> {
    if let Ok(Some(entity)) = unit_selection.board_index.unit_entity(position)
        && let Some(badge) = unit_badge(entity, unit_selection)
    {
        return Some(ForecastTarget::Unit {
            unit: badge.unit,
            name: badge.name,
            faction_code: badge.faction_code,
            health: badge.health,
        });
    }
    let terrain = unit_selection.game_map.terrain_at(position)?;
    Some(ForecastTarget::Tile {
        name: terrain.as_terrain().type_name().to_string(),
    })
}

fn unit_id_at_position(
    position: Position,
    unit_selection: &PlayUnitSelectionParams<'_, '_>,
) -> Option<u32> {
    let Ok(Some(entity)) = unit_selection.board_index.unit_entity(position) else {
        return None;
    };
    unit_selection
        .unit_ids
        .get(entity)
        .ok()
        .map(|id| id.0.as_u32())
}

/// Send the chosen order, and remember enough to undo the sending.
pub(crate) fn handle_unit_action_chosen(
    mut chosen: MessageReader<UnitActionChosen>,
    offered: Res<OfferedActions>,
    unit_selection: PlayUnitSelectionParams<'_, '_>,
    actions: UnitActionParams<'_>,
    command_sinks: UnitCommandSinks<'_>,
    mut committed: ResMut<CommittedCommand>,
    mut selection: PlaySelectionState<'_>,
) {
    let Some(UnitActionChosen { index }) = chosen.read().last().copied() else {
        return;
    };
    let (Some(option), Some(pending)) = (
        offered.0.get(index),
        selection.pending_destination.0.clone(),
    ) else {
        return;
    };
    let Ok(unit_id) = unit_selection.unit_ids.get(pending.unit) else {
        return;
    };

    committed.0 = Some(CommittedSnapshot {
        unit: pending.unit,
        origin: pending.origin,
        range: selection.move_range.tiles.clone(),
        attack_targets: selection.attack_targets.approaches.clone(),
        path: pending.path.clone(),
        destination: pending.destination,
        attack_intent: pending.attack_intent,
        kind: match &option.action {
            UnitOrder::Delete => CommittedKind::Delete,
            UnitOrder::Move { .. } => CommittedKind::Move,
            UnitOrder::Unload { .. } => CommittedKind::Unload,
        },
    });

    match &option.action {
        UnitOrder::Delete => {
            let Some(sink) = command_sinks.delete_unit_command.as_deref() else {
                committed.0 = None;
                return;
            };
            sink.emit(DeleteUnitCommandRequested {
                unit_id: unit_id.0.as_u32(),
            });
        }
        UnitOrder::Move { action } => {
            let Some(sink) = command_sinks.move_command.as_deref() else {
                committed.0 = None;
                return;
            };
            sink.emit(MoveCommandRequested {
                unit_id: unit_id.0.as_u32(),
                path: pending.path.clone(),
                action: action.clone(),
            });
        }
        UnitOrder::Unload { cargo_id, position } => {
            let Some(sink) = command_sinks.unload_command.as_deref() else {
                committed.0 = None;
                return;
            };
            sink.emit(UnloadCommandRequested {
                transport_id: unit_id.0.as_u32(),
                cargo_id: *cargo_id,
                position: *position,
            });
        }
    }

    // The order is on its way. The board stops offering anything until the
    // server has spoken, but the unit stays selected so the ordinary
    // invalidation pass can retire the selection once it actually moves.
    close_unit_actions(actions.sink.as_deref());
    selection.pending_destination.0 = None;
    selection.move_range.tiles.clear();
    selection.attack_targets.approaches.clear();
    selection.proposed_path.path.clear();
    selection.proposed_path.drawn_path.clear();
    selection.proposed_path.hovered = None;
    selection.proposed_path.attack_intent = None;
    *selection.phase = PlayUiPhase::AwaitingServer;
}

/// Put the board back after a refusal, at the origin with the route intact.
pub(crate) fn handle_pending_command_rejected(
    mut rejected: MessageReader<PendingCommandRejected>,
    mut committed: ResMut<CommittedCommand>,
    mut selection: PlaySelectionState<'_>,
) {
    if rejected.read().last().is_none() {
        return;
    }
    let Some(snapshot) = committed.0.take() else {
        return;
    };

    selection.selected.0 = Some(SelectedUnitSelection {
        entity: snapshot.unit,
        origin: snapshot.origin,
    });
    selection.move_range.tiles = snapshot.range;
    selection.attack_targets.approaches = snapshot.attack_targets;
    selection.proposed_path.path = snapshot.path;
    selection.proposed_path.drawn_path.clear();
    selection.proposed_path.hovered = None;
    selection.proposed_path.attack_intent = snapshot.attack_intent;
    selection.pending_destination.0 = None;
    *selection.phase = PlayUiPhase::UnitSelected;
}

/// A dismissal steps back to the unit; it never drops it.
pub(crate) fn handle_unit_action_dismissed(
    mut dismissed: MessageReader<UnitActionDismissed>,
    mut selection: PlaySelectionState<'_>,
) {
    if dismissed.read().last().is_none() {
        return;
    }
    return_to_unit_selected(&mut selection);
}

/// Draw the unit where it would end up, faintly, while the menu is open.
///
/// `DestinationSelected` used to look almost exactly like `UnitSelected`, which
/// is what made an accidental commit so cheap to make and so expensive to
/// discover.
pub(crate) fn sync_destination_ghost(
    mut commands: Commands,
    game_map: Res<GameMap>,
    pending: Res<PendingMoveDestination>,
    sprites: Query<(&Sprite, &SpriteSize)>,
    ghosts: Query<Entity, With<DestinationGhost>>,
) {
    if !pending.is_changed() {
        return;
    }
    for entity in &ghosts {
        commands.entity(entity).try_despawn();
    }
    let Some(pending) = pending.0.as_ref() else {
        return;
    };
    if pending.destination == pending.origin {
        return;
    }
    let Ok((sprite, sprite_size)) = sprites.get(pending.unit) else {
        return;
    };

    // The alignment offset that centres a sprite in its cell depends on the
    // sprite's own dimensions, and a unit is 23x24 rather than a 16x16 tile.
    // Taking the size from the unit being depicted is what keeps the ghost on
    // the grid; a constant here drifts the moment any sprite is not tile-sized.
    let ghost_size = SpriteSize {
        z_index: DESTINATION_GHOST_Z,
        ..*sprite_size
    };
    let mut ghost = sprite.clone();
    ghost.color = ghost.color.with_alpha(GHOST_ALPHA);
    commands.spawn((
        DestinationGhost,
        ghost,
        Transform::from_translation(position_to_world_translation(
            &ghost_size,
            pending.destination,
            &game_map,
        )),
    ));
}

pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedUnit>()
            .init_resource::<MoveRange>()
            .init_resource::<SelectedMoveField>()
            .init_resource::<AttackTargets>()
            .init_resource::<PendingMoveDestination>()
            .init_resource::<ProposedMovePath>()
            .init_resource::<PlayUiPhase>()
            .init_resource::<OfferedActions>()
            .init_resource::<CommittedCommand>()
            .init_resource::<ReplayAdvanceLock>()
            .add_message::<UnitActionChosen>()
            .add_message::<UnitActionDismissed>()
            .add_message::<PendingCommandRejected>()
            // A drag is claimed before anything can act on it, so the camera
            // never pans a unit and a unit never rides a pan.
            .add_systems(
                Update,
                claim_unit_drag
                    .in_set(PointerSet::Claim)
                    .run_if(in_state(GameMode::Game).and_then(in_state(AppState::InGame))),
            )
            .add_systems(
                Update,
                (
                    handle_unit_action_dismissed,
                    update_proposed_move_path,
                    handle_play_pointer_gestures.after(update_proposed_move_path),
                    handle_unit_action_chosen,
                    handle_pending_command_rejected,
                    clear_selection_on_escape,
                    clear_invalid_selection,
                    emit_unit_actions,
                    sync_move_range_highlights,
                    sync_attack_target_highlights,
                    sync_destination_ghost,
                    sync_proposed_path_arrows
                        .run_if(resource_exists::<crate::render::UiAtlasResource>),
                    sync_attack_target_reticle
                        .run_if(resource_exists::<crate::render::UiAtlasResource>),
                    animate_attack_target_reticle.run_if(resource_exists::<Time>),
                    apply_pending_live_transition,
                )
                    .chain()
                    .in_set(PointerSet::Consume)
                    .run_if(in_state(GameMode::Game).and_then(in_state(AppState::InGame))),
            )
            .add_systems(OnExit(GameMode::Game), cleanup_play_selection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loading::{LiveMatchBootstrap, LiveMatchPlayer};
    use crate::modes::replay::presentation::{
        DeferredTransitions, LiveTransitionCommand, ReplayFollowupCommand,
    };
    use crate::render::animation::UnitPathAnimation;
    use awbrn_game::GameWorldPlugin;
    use awbrn_game::world::StrongIdMap;
    use awbrn_game::world::initialize_terrain_semantic_world;
    use awbrn_map::{AwbwMap, AwbwMapData};
    use awbrn_types::{GraphicalTerrain, PlayerFaction};
    use awbw_replay::ReplayParser;
    use awvm::semantic::{AwbwVisibility, State, observe};
    use awvm_awbw::RecordedAdapter;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    #[derive(Resource, Default)]
    struct TestNextUnitId(u32);

    #[derive(Resource)]
    struct TestObservationSync;

    fn play_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.insert_state(AppState::InGame);
        app.insert_state(GameMode::Game);
        app.add_message::<PointerGesture>();
        app.add_message::<ReturnToTouchFloor>();
        app.init_resource::<BoardIndex>();
        app.init_resource::<GameMap>();
        app.init_resource::<FriendlyFactions>();
        app.init_resource::<StrongIdMap<AwbwUnitId>>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<DragOwner>();
        app.init_resource::<TestNextUnitId>();
        app.insert_resource(TestObservationSync);
        app.init_resource::<CameraScale>();
        app.add_message::<FocusBoardOn>();
        app.add_plugins(PlayPlugin);
        app
    }

    fn gesture(kind: PointerGestureKind, tile: Option<Position>) -> PointerGesture {
        PointerGesture {
            kind,
            viewport: Vec2::ZERO,
            delta: Vec2::ZERO,
            tile,
            coarse: false,
        }
    }

    fn send(app: &mut App, kind: PointerGestureKind, tile: Option<Position>) {
        sync_test_observation(app);
        app.world_mut()
            .resource_mut::<Messages<PointerGesture>>()
            .write(gesture(kind, tile));
        app.update();
    }

    fn sync_test_observation(app: &mut App) {
        if !app.world().contains_resource::<TestObservationSync>() {
            return;
        }
        let game_map = app.world().resource::<GameMap>();
        let (width, height) = (game_map.width(), game_map.height());
        let tiles: Vec<Vec<_>> = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| {
                        let graphical = game_map
                            .terrain_at(Position::new(x, y))
                            .unwrap_or(GraphicalTerrain::Plain);
                        let terrain = awvm_awbw::semantic_terrain(graphical.as_terrain());
                        serde_json::json!({ "terrain": terrain })
                    })
                    .collect()
            })
            .collect();
        let mut query = app.world_mut().query::<(
            &MapPosition,
            &Unit,
            &Faction,
            Option<&Fuel>,
            Has<UnitActive>,
            &AwbwUnitId,
        )>();
        let units: Vec<_> = query
            .iter(app.world())
            .map(|(position, unit, faction, fuel, active, id)| {
                let owner = if faction.0 == PlayerFaction::OrangeStar { "0" } else { "1" };
                serde_json::json!({
                    "id": id.0.as_u32(),
                    "kind": unit.0,
                    "owner": owner,
                    "hp": 100,
                    "fuel": fuel.map_or_else(|| awvm::ruleset::profile(unit.0).max_fuel, |fuel| u64::from(fuel.value())),
                    "ammo": awvm::ruleset::profile(unit.0).max_ammo,
                    "action": if active && owner == "0" { "ready" } else { "spent" },
                    "concealment": "exposed",
                    "location": { "type": "board", "position": [position.position().x, position.position().y] }
                })
            })
            .collect();
        let value = serde_json::json!({
            "ruleset": { "id": "awbw", "revision": "2026-07-10" },
            "settings": {
                "fog": false, "income_per_property": 1000, "starting_funds": 0,
                "powers": "disabled", "tags": false, "weather": "clear",
                "lab_units": [], "unit_bans": [], "commander_bans": { "lead": [], "backup": [] },
                "capture_limit": null, "day_limit": null, "unit_limit": null
            },
            "board": { "width": width, "height": height, "tiles": tiles },
            "teams": [
                { "id": "red-team", "status": "active" },
                { "id": "blue-team", "status": "active" }
            ],
            "players": [
                { "id": "0", "team": "red-team", "funds": 0, "status": "active", "commanders": [{ "id": "nell", "active": true, "power_charge": 0, "power_uses": 0 }], "power_state": { "type": "none" } },
                { "id": "1", "team": "blue-team", "funds": 0, "status": "active", "commanders": [{ "id": "nell", "active": true, "power_charge": 0, "power_uses": 0 }], "power_state": { "type": "none" } }
            ],
            "turn": { "day": 1, "active_player": "0", "phase": "unit-action", "order": ["0", "1"], "position": 0 },
            "weather": { "kind": "clear", "remaining_turns": 0 },
            "units": units,
            "next_unit_id": null,
            "match": { "status": "active", "draw_offers": [] }
        });
        let state: State =
            serde_json::from_value(value).expect("test ECS should form an AWVM state");
        let observation = observe(&AwbwVisibility, &state, &state.players[0].id).unwrap();
        let mut observations = awbrn_game::replay::RecipientObservations::default();
        observations.set(vec![observation]);
        app.world_mut().insert_resource(observations);
        app.world_mut()
            .insert_resource(awbrn_game::replay::ReplayViewpoint::Player(
                awbrn_types::AwbwGamePlayerId::new(0),
            ));
    }

    /// Drag a unit from where it stands to a tile, the way a finger would.
    fn drag_unit(app: &mut App, from: Position, over: &[Position], release: Position) {
        send(app, PointerGestureKind::DragStart, Some(from));
        for step in over {
            send(app, PointerGestureKind::DragMove, Some(*step));
        }
        send(app, PointerGestureKind::DragMove, Some(release));
        send(app, PointerGestureKind::DragEnd, Some(release));
    }

    fn set_plain_map(app: &mut App, width: usize, height: usize) {
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(awbrn_map::AwbrnMap::new(
                width,
                height,
                GraphicalTerrain::Plain,
            ));
        initialize_terrain_semantic_world(app.world_mut());
    }

    fn spawn_unit(
        app: &mut App,
        position: Position,
        unit: awbrn_types::Unit,
        faction: PlayerFaction,
        active: bool,
        fuel: Option<u32>,
    ) -> Entity {
        let id = {
            let mut next = app.world_mut().resource_mut::<TestNextUnitId>();
            let id = next.0;
            next.0 += 1;
            id
        };
        let mut entity = app.world_mut().spawn((
            MapPosition::from(position),
            Unit(unit),
            Faction(faction),
            AwbwUnitId(awbrn_types::AwbwUnitId::new(id)),
        ));
        if active {
            entity.insert(UnitActive);
        }
        if let Some(fuel) = fuel {
            entity.insert(Fuel(fuel));
        }
        entity.id()
    }

    fn click_tile(app: &mut App, position: Position) {
        send(app, PointerGestureKind::Tap, Some(position));
    }

    #[test]
    fn clicking_hachi_scop_city_emits_observed_options() {
        let fixture = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../spec/fixtures/commander/hachi-scop.json"),
        )
        .unwrap();
        let mut fixture: serde_json::Value = serde_json::from_str(&fixture).unwrap();
        let state = &mut fixture["initial_state"];
        state["board"]["tiles"][0][0]["owner"] = serde_json::json!("0");
        state["players"][0]["id"] = serde_json::json!("0");
        state["turn"]["active_player"] = serde_json::json!("0");
        state["turn"]["order"][0] = serde_json::json!("0");
        let state: State = serde_json::from_value(state.clone()).unwrap();
        let recipient = state.players[0].id.clone();
        let observation = observe(&AwbwVisibility, &state, &recipient).unwrap();

        let mut app = play_test_app();
        app.world_mut().remove_resource::<TestObservationSync>();
        set_plain_map(&mut app, 1, 1);
        let mut observations = awbrn_game::replay::RecipientObservations::default();
        observations.set(vec![observation]);
        app.world_mut().insert_resource(observations);
        app.world_mut()
            .insert_resource(awbrn_game::replay::ReplayViewpoint::Player(
                awbrn_types::AwbwGamePlayerId::new(0),
            ));
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_by_sink = received.clone();
        app.world_mut()
            .insert_resource(EventSink::<ProductionOptionsChanged>::new(move |event| {
                received_by_sink.lock().unwrap().push(event);
            }));

        click_tile(&mut app, Position::new(0, 0));

        let received = received.lock().unwrap();
        let event = received.last().expect("production event");
        assert_eq!(
            event.site.as_ref().unwrap().facility,
            awvm::ruleset::Terrain::City
        );
        assert!(
            event.options.iter().any(
                |option| option.unit == awvm::ruleset::UnitKind::Infantry && option.cost == 500
            )
        );
    }

    #[test]
    fn live_recipient_movement_uses_typed_animation_and_followup() {
        let replay = ReplayParser::new()
            .parse(
                &std::fs::read(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/replays/1362397.zip"),
                )
                .unwrap(),
            )
            .unwrap();
        let map_data: AwbwMapData = serde_json::from_slice(
            &std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/maps/162795.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut adapter = RecordedAdapter::new(&replay, &map_data).unwrap();
        let recipient = adapter.state().players[0].id.clone();
        let observation = observe(&AwbwVisibility, adapter.state(), &recipient).unwrap();

        let mut app = App::new();
        app.add_plugins(GameWorldPlugin);
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(awbrn_map::AwbrnMap::from_map(
                &AwbwMap::try_from(&map_data).unwrap(),
            ));
        app.world_mut().insert_resource(LiveMatchBootstrap {
            players: replay.games[0]
                .players
                .iter()
                .map(|player| LiveMatchPlayer {
                    player_id: player.id.as_u32(),
                    faction_id: player.faction.id(),
                })
                .collect(),
            observation,
        });
        app.world_mut()
            .insert_resource(ReplayAdvanceLock::default());
        initialize_live_semantic_world(app.world_mut());

        for action in &replay.turns {
            let transition = adapter.advance(action).unwrap();
            let observed = transition.observe(&recipient).unwrap();
            LiveTransitionCommand {
                transition: observed,
            }
            .apply(app.world_mut());
            let Some(entity) = app.world().resource::<ReplayAdvanceLock>().active_entity() else {
                continue;
            };
            let destination = *app
                .world()
                .entity(entity)
                .get::<UnitPathAnimation>()
                .unwrap()
                .path
                .last()
                .unwrap();
            let followup = app
                .world_mut()
                .resource_mut::<ReplayAdvanceLock>()
                .release_for(entity)
                .unwrap();
            assert!(matches!(
                followup.transitions,
                Some(DeferredTransitions::Recipient(_))
            ));
            ReplayFollowupCommand {
                transitions: followup.transitions,
            }
            .apply(app.world_mut());
            assert_eq!(
                app.world()
                    .entity(entity)
                    .get::<MapPosition>()
                    .unwrap()
                    .position(),
                destination
            );
            return;
        }

        panic!("fixture did not produce movement visible to the live recipient");
    }

    #[test]
    fn live_bootstrap_seeds_the_player_roster_and_power_meter() {
        use crate::features::player_roster::{PlayerFunds, PlayerPowerMeters, PlayerRosterConfig};

        let replay = ReplayParser::new()
            .parse(
                &std::fs::read(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/replays/1362397.zip"),
                )
                .unwrap(),
            )
            .unwrap();
        let map_data: AwbwMapData = serde_json::from_slice(
            &std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/maps/162795.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let adapter = RecordedAdapter::new(&replay, &map_data).unwrap();
        let recipient = adapter.state().players[0].id.clone();
        let observation = observe(&AwbwVisibility, adapter.state(), &recipient).unwrap();

        let mut app = App::new();
        app.add_plugins(GameWorldPlugin);
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(awbrn_map::AwbrnMap::from_map(
                &AwbwMap::try_from(&map_data).unwrap(),
            ));
        app.world_mut().insert_resource(LiveMatchBootstrap {
            players: replay.games[0]
                .players
                .iter()
                .map(|player| LiveMatchPlayer {
                    player_id: player.id.as_u32(),
                    faction_id: player.faction.id(),
                })
                .collect(),
            observation,
        });
        app.world_mut()
            .insert_resource(ReplayAdvanceLock::default());
        initialize_live_semantic_world(app.world_mut());

        let config = app.world().resource::<PlayerRosterConfig>();
        assert_eq!(config.players.len(), replay.games[0].players.len());
        assert!(
            config.players.iter().all(|player| player.co_key.is_some()),
            "every live player should resolve a CO portrait from its commander"
        );

        // The recipient's own funds are private and only they observe them.
        let recipient_id =
            awbrn_types::AwbwGamePlayerId::new(recipient.as_str().parse::<u32>().unwrap());
        assert_eq!(
            app.world().resource::<PlayerFunds>().get(recipient_id),
            replay.games[0]
                .players
                .iter()
                .find(|player| player.id.as_u32() == recipient_id.as_u32())
                .unwrap()
                .funds
        );

        // Power charge is public, so it is seeded for every player.
        let meters = app.world().resource::<PlayerPowerMeters>();
        for player in &config.players {
            assert!(
                meters.get(player.player_id).is_some(),
                "power meter is public and should be seeded for player {}",
                player.player_id.as_u32()
            );
        }
    }

    #[test]
    fn clicking_owned_active_unit_selects_and_spawns_highlights() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let unit = spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );

        click_tile(&mut app, Position::new(2, 2));

        assert_eq!(
            app.world().resource::<SelectedUnit>().0,
            Some(SelectedUnitSelection {
                entity: unit,
                origin: Position::new(2, 2),
            })
        );
        assert!(
            app.world()
                .resource::<MoveRange>()
                .tiles
                .contains_key(&Position::new(2, 1))
        );
        assert!(
            !app.world()
                .resource::<MoveRange>()
                .tiles
                .contains_key(&Position::new(2, 2))
        );

        let highlight_count = app
            .world_mut()
            .query_filtered::<Entity, With<MoveRangeHighlight>>()
            .iter(app.world())
            .count();
        assert!(highlight_count > 0);
    }

    #[test]
    fn spent_and_enemy_units_are_not_selectable() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        spawn_unit(
            &mut app,
            Position::new(1, 1),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            false,
            Some(99),
        );
        spawn_unit(
            &mut app,
            Position::new(3, 3),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
            true,
            Some(99),
        );

        click_tile(&mut app, Position::new(1, 1));
        assert_eq!(app.world().resource::<SelectedUnit>().0, None);

        click_tile(&mut app, Position::new(3, 3));
        assert_eq!(app.world().resource::<SelectedUnit>().0, None);
    }

    #[test]
    fn fuel_limits_move_range() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 7, 7);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        spawn_unit(
            &mut app,
            Position::new(3, 3),
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
            true,
            Some(2),
        );

        click_tile(&mut app, Position::new(3, 3));

        let range = &app.world().resource::<MoveRange>().tiles;
        assert!(range.contains_key(&Position::new(5, 3)));
        assert!(!range.contains_key(&Position::new(6, 3)));
    }

    #[test]
    fn targetable_units_get_red_glass_and_keep_a_traced_attack_path() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 3);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let attacker = spawn_unit(
            &mut app,
            Position::new(0, 1),
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        let target = Position::new(3, 1);
        spawn_unit(
            &mut app,
            target,
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
            true,
            Some(99),
        );

        click_tile(&mut app, Position::new(0, 1));

        assert!(
            app.world()
                .resource::<AttackTargets>()
                .approaches
                .contains_key(&target)
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<AttackTargetHighlight>>()
                .iter(app.world())
                .count(),
            5,
            "one target has a red fill and four glass edges"
        );

        let unit_targets = app.world().resource::<AttackTargets>().clone();
        let tile_target = Position::new(4, 2);
        app.world_mut().resource_mut::<AttackTargets>().approaches =
            HashMap::from([(tile_target, vec![Position::new(3, 2)])]);
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<AttackTargetHighlight>>()
                .iter(app.world())
                .count(),
            5,
            "a legal tile target has the same red glass as a unit target"
        );
        *app.world_mut().resource_mut::<AttackTargets>() = unit_targets;

        let traced = vec![
            Position::new(0, 1),
            Position::new(0, 0),
            Position::new(1, 0),
            Position::new(2, 0),
            Position::new(2, 1),
        ];
        {
            let mut proposed = app.world_mut().resource_mut::<ProposedMovePath>();
            proposed.path = traced.clone();
            proposed.drawn_path = traced.iter().copied().skip(1).collect();
        }

        click_tile(&mut app, target);

        assert_eq!(
            app.world().resource::<PendingMoveDestination>().0,
            Some(PendingMoveDestinationSelection {
                unit: attacker,
                origin: Position::new(0, 1),
                destination: Position::new(2, 1),
                path: traced.clone(),
                attack_intent: Some(target),
            })
        );
        assert_eq!(app.world().resource::<ProposedMovePath>().path, traced);
    }

    #[test]
    fn shared_attack_position_survives_invalid_hover_between_targets() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 3);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let origin = Position::new(0, 1);
        let first_target = Position::new(3, 1);
        let second_target = Position::new(2, 0);
        let firing_position = Position::new(2, 1);
        spawn_unit(
            &mut app,
            origin,
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        for target in [first_target, second_target] {
            spawn_unit(
                &mut app,
                target,
                awbrn_types::Unit::Infantry,
                PlayerFaction::BlueMoon,
                true,
                Some(99),
            );
        }
        click_tile(&mut app, origin);

        let path = vec![origin, Position::new(1, 1), firing_position];
        let field = app
            .world()
            .resource::<SelectedMoveField>()
            .0
            .clone()
            .expect("the tank has a movement field");
        let targets = app.world().resource::<AttackTargets>().clone();
        app.world_mut().resource_mut::<ProposedMovePath>().path = path.clone();

        update_automatic_move_path(
            origin,
            Position::new(4, 2),
            false,
            false,
            &field,
            &mut app.world_mut().resource_mut::<ProposedMovePath>(),
        );
        assert_eq!(app.world().resource::<ProposedMovePath>().path, path);

        let approach = attack_approach(second_target, &path, &targets, Some(&field))
            .expect("the existing firing position can target the second unit");
        update_automatic_move_path(
            origin,
            approach,
            true,
            true,
            &field,
            &mut app.world_mut().resource_mut::<ProposedMovePath>(),
        );
        assert_eq!(app.world().resource::<ProposedMovePath>().path, path);
    }

    #[test]
    fn attack_target_reticle_rotates_and_pulses() {
        let mut expanded = Transform::default();
        apply_attack_target_reticle_pose(
            1.0 / (4.0 * TARGET_RETICLE_PULSES_PER_SECOND),
            &mut expanded,
        );
        assert!(expanded.scale.x > 1.0);
        assert_ne!(expanded.rotation, Quat::IDENTITY);

        let mut contracted = Transform::default();
        apply_attack_target_reticle_pose(
            3.0 / (4.0 * TARGET_RETICLE_PULSES_PER_SECOND),
            &mut contracted,
        );
        assert!(contracted.scale.x < 1.0);
        assert_ne!(contracted.rotation, expanded.rotation);
    }

    #[test]
    fn friendly_units_are_in_the_preview_but_enemies_block_routes() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 6, 1);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        spawn_unit(
            &mut app,
            Position::new(0, 0),
            awbrn_types::Unit::Recon,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        spawn_unit(
            &mut app,
            Position::new(1, 0),
            awbrn_types::Unit::Recon,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        spawn_unit(
            &mut app,
            Position::new(3, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
            true,
            Some(99),
        );

        click_tile(&mut app, Position::new(0, 0));

        let range = &app.world().resource::<MoveRange>().tiles;
        assert!(range.contains_key(&Position::new(1, 0)));
        assert!(range.contains_key(&Position::new(2, 0)));
        assert!(!range.contains_key(&Position::new(3, 0)));
        assert!(!range.contains_key(&Position::new(4, 0)));

        click_tile(&mut app, Position::new(1, 0));
        assert_eq!(
            app.world()
                .resource::<PendingMoveDestination>()
                .0
                .as_ref()
                .map(|pending| pending.destination),
            Some(Position::new(1, 0)),
            "an occupied friendly tile must reach the action query"
        );
        assert_eq!(
            app.world().resource::<ProposedMovePath>().path,
            vec![Position::new(0, 0), Position::new(1, 0)],
            "the arrow must follow the route to an occupied friendly tile"
        );
    }

    #[test]
    fn clicking_reachable_tile_sets_pending_destination() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let unit = spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );

        click_tile(&mut app, Position::new(2, 2));
        click_tile(&mut app, Position::new(2, 1));

        assert_eq!(
            app.world().resource::<PendingMoveDestination>().0,
            Some(PendingMoveDestinationSelection {
                unit,
                origin: Position::new(2, 2),
                destination: Position::new(2, 1),
                path: vec![Position::new(2, 2), Position::new(2, 1)],
                attack_intent: None,
            })
        );
        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::DestinationSelected
        );
    }

    #[test]
    fn clicking_selected_units_tile_offers_in_place_actions_without_a_ghost() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let origin = Position::new(2, 2);
        let unit = spawn_unit(
            &mut app,
            origin,
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        app.world_mut()
            .entity_mut(unit)
            .insert((Sprite::default(), MOVE_RANGE_SPRITE_SIZE));

        click_tile(&mut app, origin);
        click_tile(&mut app, origin);

        assert_eq!(
            app.world().resource::<PendingMoveDestination>().0,
            Some(PendingMoveDestinationSelection {
                unit,
                origin,
                destination: origin,
                path: vec![origin],
                attack_intent: None,
            })
        );
        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::DestinationSelected
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<DestinationGhost>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn clicking_a_teleporter_unit_offers_delete_when_it_cannot_stay() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let origin = Position::new(2, 2);
        let teleporter = app
            .world()
            .resource::<BoardIndex>()
            .terrain_entity(origin)
            .unwrap();
        app.world_mut()
            .entity_mut(teleporter)
            .insert(awbrn_game::world::TerrainTile {
                terrain: GraphicalTerrain::Teleporter,
            });
        app.world_mut()
            .resource_mut::<GameMap>()
            .set_terrain(origin, GraphicalTerrain::Teleporter);

        let unit = spawn_unit(
            &mut app,
            origin,
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        app.world_mut()
            .insert_resource(EventSink::<UnitActionsChanged>::new(|_| {}));

        click_tile(&mut app, origin);
        click_tile(&mut app, origin);

        assert_eq!(
            app.world().resource::<PendingMoveDestination>().0,
            Some(PendingMoveDestinationSelection {
                unit,
                origin,
                destination: origin,
                path: vec![origin],
                attack_intent: None,
            })
        );
        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::DestinationSelected
        );
        assert!(
            app.world()
                .resource::<OfferedActions>()
                .0
                .iter()
                .any(|option| option.action == UnitOrder::Delete)
        );
    }

    #[test]
    fn clicking_capturing_units_tile_offers_capture_again() {
        let fixture = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../spec/fixtures/capture/capture-city-partial.json"),
        )
        .unwrap();
        let mut fixture: serde_json::Value = serde_json::from_str(&fixture).unwrap();
        let state = &mut fixture["initial_state"];
        state["board"]["tiles"][0][0]["capture_points"] = serde_json::json!(10);
        state["players"][0]["id"] = serde_json::json!("0");
        state["units"][0]["owner"] = serde_json::json!("0");
        state["turn"]["active_player"] = serde_json::json!("0");
        state["turn"]["order"][0] = serde_json::json!("0");
        let state: State = serde_json::from_value(state.clone()).unwrap();
        let observation = observe(&AwbwVisibility, &state, &state.players[0].id).unwrap();

        let mut app = play_test_app();
        app.world_mut().remove_resource::<TestObservationSync>();
        set_plain_map(&mut app, 1, 1);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);
        spawn_unit(
            &mut app,
            Position::new(0, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        let mut observations = awbrn_game::replay::RecipientObservations::default();
        observations.set(vec![observation]);
        app.world_mut().insert_resource(observations);
        app.world_mut()
            .insert_resource(awbrn_game::replay::ReplayViewpoint::Player(
                awbrn_types::AwbwGamePlayerId::new(0),
            ));
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_by_sink = received.clone();
        app.world_mut()
            .insert_resource(EventSink::<UnitActionsChanged>::new(move |event| {
                received_by_sink.lock().unwrap().push(event);
            }));

        click_tile(&mut app, Position::new(0, 0));
        click_tile(&mut app, Position::new(0, 0));

        let received = received.lock().unwrap();
        let menu = received.last().expect("unit action menu");
        assert_eq!(menu.destination, Some(Position::new(0, 0)));
        assert!(menu.options.iter().any(|option| option.action
            == UnitOrder::Move {
                action: PostMoveAction::Capture,
            }));
    }

    #[test]
    fn choosing_an_order_sends_it_and_keeps_a_way_back() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let unit = spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        app.world_mut()
            .entity_mut(unit)
            .insert(AwbwUnitId(awbrn_types::AwbwUnitId::new(42)));
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_by_sink = received.clone();
        app.world_mut()
            .insert_resource(EventSink::<MoveCommandRequested>::new(move |event| {
                received_by_sink.lock().unwrap().push(event);
            }));

        click_tile(&mut app, Position::new(2, 2));
        click_tile(&mut app, Position::new(2, 1));

        // A destination on its own commits nothing. That is the whole point of
        // the menu: arriving somewhere is not an order.
        assert!(received.lock().unwrap().is_empty());
        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::DestinationSelected
        );

        app.world_mut()
            .resource_mut::<OfferedActions>()
            .0
            .push(UnitActionOption::plain(
                "Wait",
                UnitOrder::Move {
                    action: PostMoveAction::Wait,
                },
            ));
        app.world_mut()
            .resource_mut::<Messages<UnitActionChosen>>()
            .write(UnitActionChosen { index: 0 });
        app.update();

        assert_eq!(
            received.lock().unwrap().as_slice(),
            &[MoveCommandRequested {
                unit_id: 42,
                path: vec![Position::new(2, 2), Position::new(2, 1)],
                action: PostMoveAction::Wait,
            }]
        );
        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::AwaitingServer
        );
        assert!(app.world().resource::<MoveRange>().tiles.is_empty());
        assert!(app.world().resource::<CommittedCommand>().0.is_some());
    }

    #[test]
    fn choosing_unload_sends_a_standalone_command() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 3, 3);
        let transport = spawn_unit(
            &mut app,
            Position::new(1, 1),
            awbrn_types::Unit::Apc,
            PlayerFaction::OrangeStar,
            false,
            Some(99),
        );
        app.world_mut()
            .entity_mut(transport)
            .insert(AwbwUnitId(awbrn_types::AwbwUnitId::new(42)));

        app.world_mut().resource_mut::<SelectedUnit>().0 = Some(SelectedUnitSelection {
            entity: transport,
            origin: Position::new(1, 1),
        });
        app.world_mut().resource_mut::<PendingMoveDestination>().0 =
            Some(PendingMoveDestinationSelection {
                unit: transport,
                origin: Position::new(1, 1),
                destination: Position::new(1, 1),
                path: vec![Position::new(1, 1)],
                attack_intent: None,
            });
        app.world_mut()
            .resource_mut::<OfferedActions>()
            .0
            .push(UnitActionOption::plain(
                "Unload #7",
                UnitOrder::Unload {
                    cargo_id: 7,
                    position: Position::new(1, 0),
                },
            ));

        let received = Arc::new(Mutex::new(Vec::new()));
        let received_by_sink = received.clone();
        app.world_mut()
            .insert_resource(EventSink::<UnloadCommandRequested>::new(move |event| {
                received_by_sink.lock().unwrap().push(event);
            }));
        app.world_mut()
            .resource_mut::<Messages<UnitActionChosen>>()
            .write(UnitActionChosen { index: 0 });
        app.update();

        assert_eq!(
            received.lock().unwrap().as_slice(),
            &[UnloadCommandRequested {
                transport_id: 42,
                cargo_id: 7,
                position: Position::new(1, 0),
            }]
        );
    }

    #[test]
    fn ready_unit_offers_delete_and_sends_a_standalone_command() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 3, 3);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);
        let unit = spawn_unit(
            &mut app,
            Position::new(1, 1),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        app.world_mut()
            .entity_mut(unit)
            .insert(AwbwUnitId(awbrn_types::AwbwUnitId::new(42)));

        let menus = Arc::new(Mutex::new(Vec::new()));
        let menus_by_sink = menus.clone();
        app.world_mut()
            .insert_resource(EventSink::<UnitActionsChanged>::new(move |event| {
                menus_by_sink.lock().unwrap().push(event);
            }));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let commands_by_sink = commands.clone();
        app.world_mut()
            .insert_resource(EventSink::<DeleteUnitCommandRequested>::new(move |event| {
                commands_by_sink.lock().unwrap().push(event);
            }));

        click_tile(&mut app, Position::new(1, 1));
        click_tile(&mut app, Position::new(1, 1));

        let delete_index = app
            .world()
            .resource::<OfferedActions>()
            .0
            .iter()
            .position(|option| option.action == UnitOrder::Delete)
            .expect("the current-tile menu must offer deletion");
        assert!(menus.lock().unwrap().iter().any(|menu| {
            menu.options
                .iter()
                .any(|option| option.action == UnitOrder::Delete)
        }));

        app.world_mut()
            .resource_mut::<Messages<UnitActionChosen>>()
            .write(UnitActionChosen {
                index: delete_index,
            });
        app.update();

        assert_eq!(
            commands.lock().unwrap().as_slice(),
            &[DeleteUnitCommandRequested { unit_id: 42 }]
        );
        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::AwaitingServer
        );
    }

    #[test]
    fn spent_loaded_transport_offers_standalone_unload() {
        let fixture = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../spec/fixtures/transport/unload-infantry-from-apc.json"),
        )
        .unwrap();
        let fixture: serde_json::Value = serde_json::from_str(&fixture).unwrap();
        let mut state: State = serde_json::from_value(fixture["initial_state"].clone()).unwrap();
        state.players[0].id = "0".into();
        state.players[0].commanders[0].id = awvm::semantic::CommanderId::Neutral;
        state.turn.active_player = "0".into();
        state.turn.order = vec!["0".into()];
        state.units[0].owner = "0".into();
        state.units[0].action = awvm::semantic::UnitAction::Spent;
        state.units[1].owner = "0".into();
        let observation = observe(&AwbwVisibility, &state, &state.players[0].id).unwrap();

        let mut app = play_test_app();
        app.world_mut().remove_resource::<TestObservationSync>();
        set_plain_map(&mut app, 2, 1);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);
        let transport = spawn_unit(
            &mut app,
            Position::new(1, 0),
            awbrn_types::Unit::Apc,
            PlayerFaction::OrangeStar,
            false,
            Some(70),
        );
        app.world_mut().spawn((
            Unit(awbrn_types::Unit::Infantry),
            Faction(PlayerFaction::OrangeStar),
            AwbwUnitId(awbrn_types::AwbwUnitId::new(1)),
            CarriedBy(transport),
        ));
        let mut observations = awbrn_game::replay::RecipientObservations::default();
        observations.set(vec![observation]);
        app.world_mut().insert_resource(observations);
        app.world_mut()
            .insert_resource(awbrn_game::replay::ReplayViewpoint::Player(
                awbrn_types::AwbwGamePlayerId::new(0),
            ));
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_by_sink = received.clone();
        app.world_mut()
            .insert_resource(EventSink::<UnitActionsChanged>::new(move |event| {
                received_by_sink.lock().unwrap().push(event);
            }));

        send(
            &mut app,
            PointerGestureKind::DragStart,
            Some(Position::new(1, 0)),
        );
        assert_eq!(*app.world().resource::<DragOwner>(), DragOwner::Camera);
        assert_eq!(app.world().resource::<SelectedUnit>().0, None);

        click_tile(&mut app, Position::new(1, 0));
        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::UnitSelected
        );
        click_tile(&mut app, Position::new(1, 0));

        let received = received.lock().unwrap();
        let menu = received.last().expect("unload menu");
        assert_eq!(menu.destination, Some(Position::new(1, 0)));
        assert_eq!(
            menu.options,
            vec![UnitActionOption::plain(
                "Unload Infantry",
                UnitOrder::Unload {
                    cargo_id: 1,
                    position: Position::new(0, 0),
                },
            )]
        );
    }

    /// A refusal used to cost the player the unit, the range, and the route.
    #[test]
    fn a_refused_command_restores_the_selection_at_the_origin() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let unit = spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        app.world_mut()
            .entity_mut(unit)
            .insert(AwbwUnitId(awbrn_types::AwbwUnitId::new(42)));
        app.world_mut()
            .insert_resource(EventSink::<MoveCommandRequested>::new(|_| {}));

        click_tile(&mut app, Position::new(2, 2));
        click_tile(&mut app, Position::new(2, 1));
        let target = Position::new(3, 1);
        app.world_mut()
            .resource_mut::<PendingMoveDestination>()
            .0
            .as_mut()
            .unwrap()
            .attack_intent = Some(target);
        app.world_mut()
            .resource_mut::<ProposedMovePath>()
            .attack_intent = Some(target);
        app.world_mut()
            .resource_mut::<OfferedActions>()
            .0
            .push(UnitActionOption::plain(
                "Wait",
                UnitOrder::Move {
                    action: PostMoveAction::Wait,
                },
            ));
        app.world_mut()
            .resource_mut::<Messages<UnitActionChosen>>()
            .write(UnitActionChosen { index: 0 });
        app.update();
        assert_eq!(
            app.world().resource::<ProposedMovePath>().attack_intent,
            None
        );

        app.world_mut()
            .resource_mut::<Messages<PendingCommandRejected>>()
            .write(PendingCommandRejected);
        app.update();

        assert_eq!(
            app.world().resource::<SelectedUnit>().0,
            Some(SelectedUnitSelection {
                entity: unit,
                origin: Position::new(2, 2),
            })
        );
        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::UnitSelected
        );
        assert!(
            app.world()
                .resource::<MoveRange>()
                .tiles
                .contains_key(&Position::new(2, 1)),
            "the range the player was working with comes back"
        );
        assert_eq!(
            app.world().resource::<ProposedMovePath>().path,
            vec![Position::new(2, 2), Position::new(2, 1)],
            "and so does the route, so a retry is one tap"
        );
        assert_eq!(
            app.world().resource::<ProposedMovePath>().attack_intent,
            Some(target)
        );
    }

    /// Dismissing steps back to the unit; it never drops it. A mis-tap that
    /// used to cost the whole selection now costs the destination only.
    #[test]
    fn dismissing_and_escape_step_back_to_the_unit() {
        for dismiss_with_escape in [false, true] {
            let mut app = play_test_app();
            set_plain_map(&mut app, 5, 5);
            app.world_mut()
                .resource_mut::<FriendlyFactions>()
                .0
                .insert(PlayerFaction::OrangeStar);

            let unit = spawn_unit(
                &mut app,
                Position::new(2, 2),
                awbrn_types::Unit::Infantry,
                PlayerFaction::OrangeStar,
                true,
                Some(99),
            );

            click_tile(&mut app, Position::new(2, 2));
            click_tile(&mut app, Position::new(2, 1));

            if dismiss_with_escape {
                app.world_mut()
                    .resource_mut::<ButtonInput<KeyCode>>()
                    .press(KeyCode::Escape);
            } else {
                app.world_mut()
                    .resource_mut::<Messages<UnitActionDismissed>>()
                    .write(UnitActionDismissed);
            }
            app.update();

            assert_eq!(
                app.world().resource::<SelectedUnit>().0,
                Some(SelectedUnitSelection {
                    entity: unit,
                    origin: Position::new(2, 2),
                }),
                "escape: {dismiss_with_escape}"
            );
            assert_eq!(
                *app.world().resource::<PlayUiPhase>(),
                PlayUiPhase::UnitSelected
            );
            assert_eq!(app.world().resource::<PendingMoveDestination>().0, None);
        }
    }

    #[test]
    fn same_frame_dismissal_does_not_discard_a_new_destination() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        click_tile(&mut app, Position::new(2, 2));
        click_tile(&mut app, Position::new(2, 1));

        app.world_mut()
            .resource_mut::<Messages<UnitActionDismissed>>()
            .write(UnitActionDismissed);
        app.world_mut()
            .resource_mut::<Messages<PointerGesture>>()
            .write(gesture(PointerGestureKind::Tap, Some(Position::new(1, 2))));
        app.update();

        assert_eq!(
            app.world()
                .resource::<PendingMoveDestination>()
                .0
                .as_ref()
                .map(|pending| pending.destination),
            Some(Position::new(1, 2))
        );
        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::DestinationSelected
        );
    }

    /// Dragging is the other way in, and it must reach the same place a pair of
    /// taps reaches.
    #[test]
    fn dragging_a_unit_proposes_the_tile_it_was_released_on() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let unit = spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );

        drag_unit(
            &mut app,
            Position::new(2, 2),
            &[Position::new(2, 1)],
            Position::new(2, 0),
        );

        assert_eq!(*app.world().resource::<DragOwner>(), DragOwner::Camera);
        assert_eq!(
            app.world().resource::<PendingMoveDestination>().0,
            Some(PendingMoveDestinationSelection {
                unit,
                origin: Position::new(2, 2),
                destination: Position::new(2, 0),
                path: vec![
                    Position::new(2, 2),
                    Position::new(2, 1),
                    Position::new(2, 0)
                ],
                attack_intent: None,
            })
        );
    }

    /// A drag that leaves the range holds at the last tile it may end on. It
    /// does not cancel, because overshoot is the commonest drag error and
    /// cancelling punishes it hardest.
    #[test]
    fn a_drag_past_the_range_holds_at_the_last_reachable_tile() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 9, 3);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        spawn_unit(
            &mut app,
            Position::new(0, 1),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );

        // Infantry reach three tiles, so tile 8 is far outside the range.
        drag_unit(
            &mut app,
            Position::new(0, 1),
            &[Position::new(3, 1)],
            Position::new(8, 1),
        );

        let pending = app
            .world()
            .resource::<PendingMoveDestination>()
            .0
            .clone()
            .expect("the drag still proposes something");
        assert_eq!(
            pending.destination,
            Position::new(3, 1),
            "the route holds where it was still legal"
        );
    }

    /// Releasing on an enemy is explicit attack intent, so it resolves onto a
    /// tile the pointer never touched and says which enemy it meant.
    #[test]
    fn releasing_on_an_enemy_approaches_it_and_records_the_aim() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 6, 3);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        spawn_unit(
            &mut app,
            Position::new(1, 1),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        spawn_unit(
            &mut app,
            Position::new(3, 1),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
            true,
            Some(99),
        );
        spawn_unit(
            &mut app,
            Position::new(2, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
            true,
            Some(99),
        );
        let menus = Arc::new(Mutex::new(Vec::new()));
        let menus_by_sink = menus.clone();
        app.world_mut()
            .insert_resource(EventSink::<UnitActionsChanged>::new(move |event| {
                menus_by_sink.lock().unwrap().push(event);
            }));

        drag_unit(
            &mut app,
            Position::new(1, 1),
            &[Position::new(2, 1)],
            Position::new(3, 1),
        );

        let pending = app
            .world()
            .resource::<PendingMoveDestination>()
            .0
            .clone()
            .expect("an approach was proposed");
        assert_eq!(
            pending.destination,
            Position::new(2, 1),
            "it stops on the cheapest tile it can fire from"
        );
        assert_eq!(
            pending.attack_intent,
            Some(Position::new(3, 1)),
            "and remembers which enemy, so the menu can open on Fire"
        );
        let initial_menus = menus.lock().unwrap();
        let menu = initial_menus.last().expect("the attack menu opens");
        assert_eq!(menu.options.len(), 1);
        assert_eq!(menu.preselected, Some(0));
        assert_eq!(
            menu.options[0].action,
            UnitOrder::Move {
                action: PostMoveAction::Attack {
                    target: Position::new(3, 1),
                },
            }
        );
        drop(initial_menus);

        app.world_mut()
            .resource_mut::<PendingMoveDestination>()
            .0
            .as_mut()
            .expect("the destination remains pending")
            .attack_intent = Some(Position::new(5, 2));
        app.update();

        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::DestinationSelected
        );
        let menus = menus.lock().unwrap();
        let menu = menus.last().expect("the full action menu stays open");
        assert_eq!(menu.options.len(), 3);
        assert_eq!(menu.preselected, None);
    }

    /// A press that lands on nothing in particular belongs to the camera, and a
    /// drag from there must not disturb the board.
    #[test]
    fn a_drag_from_empty_ground_is_left_to_the_camera() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );

        send(
            &mut app,
            PointerGestureKind::DragStart,
            Some(Position::new(0, 0)),
        );
        assert_eq!(*app.world().resource::<DragOwner>(), DragOwner::Camera);

        send(
            &mut app,
            PointerGestureKind::DragMove,
            Some(Position::new(1, 0)),
        );
        send(
            &mut app,
            PointerGestureKind::DragEnd,
            Some(Position::new(2, 2)),
        );

        assert_eq!(
            app.world().resource::<SelectedUnit>().0,
            None,
            "panning across a unit must never pick it up"
        );
        assert_eq!(app.world().resource::<PendingMoveDestination>().0, None);
    }

    /// The reachable set is already computed, so a near miss can be pulled onto
    /// it. This is what lets the touch floor sit below the platform minimums.
    #[test]
    fn a_near_miss_resolves_onto_the_nearest_reachable_tile() {
        let mut range = MoveRange::default();
        range.tiles.insert(Position::new(2, 1), 1);
        let mut game_map = GameMap::default();
        game_map.set(awbrn_map::AwbrnMap::new(5, 5, GraphicalTerrain::Plain));

        let center =
            position_to_world_translation(&MOVE_RANGE_SPRITE_SIZE, Position::new(2, 1), &game_map)
                .truncate();

        // Just over the border into the tile above, but still within the slop
        // of the reachable tile below it.
        let just_outside = center + Vec2::new(0.0, TILE_SIZE * 0.6);
        assert_eq!(
            resolve_tap_target(
                Position::new(2, 0),
                Some(just_outside),
                &range,
                &game_map,
                None,
            ),
            Position::new(2, 1),
        );

        // Far enough away that the player meant the tile they hit.
        let clearly_elsewhere = center + Vec2::new(0.0, TILE_SIZE * 1.4);
        assert_eq!(
            resolve_tap_target(
                Position::new(2, 0),
                Some(clearly_elsewhere),
                &range,
                &game_map,
                None,
            ),
            Position::new(2, 0),
        );

        assert_eq!(
            resolve_tap_target(
                Position::new(2, 0),
                Some(just_outside),
                &range,
                &game_map,
                Some(Position::new(2, 0)),
            ),
            Position::new(2, 0),
            "the selected unit's tile must not be pulled onto a neighbour"
        );
    }

    /// Below the floor a tap may select but not commit, and the board comes
    /// back up to a size the finger can work at.
    #[test]
    fn a_tap_below_the_touch_floor_asks_for_the_camera_instead_of_committing() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);
        spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        app.world_mut()
            .resource_mut::<CameraScale>()
            .set_clamped(0.5, 0.2);

        click_tile(&mut app, Position::new(2, 2));
        app.world_mut()
            .resource_mut::<Messages<PointerGesture>>()
            .write(PointerGesture {
                kind: PointerGestureKind::Tap,
                viewport: Vec2::ZERO,
                delta: Vec2::ZERO,
                tile: Some(Position::new(2, 1)),
                coarse: true,
            });
        app.update();

        assert_eq!(
            app.world().resource::<PendingMoveDestination>().0,
            None,
            "nothing is committed at a size the finger cannot hit"
        );
        assert!(
            app.world().resource::<SelectedUnit>().0.is_some(),
            "but the selection survives"
        );
        assert!(
            !app.world()
                .resource::<Messages<ReturnToTouchFloor>>()
                .is_empty(),
            "and the board is asked to come back up"
        );
    }

    #[test]
    fn destination_confirmation_uses_the_field_cached_at_selection() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 7, 7);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let unit = spawn_unit(
            &mut app,
            Position::new(3, 3),
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
            true,
            Some(2),
        );

        click_tile(&mut app, Position::new(3, 3));
        assert!(
            app.world()
                .resource::<MoveRange>()
                .tiles
                .contains_key(&Position::new(5, 3))
        );

        app.world_mut().entity_mut(unit).insert(Fuel(1));
        click_tile(&mut app, Position::new(5, 3));

        assert!(app.world().resource::<PendingMoveDestination>().0.is_some());
        assert!(app.world().resource::<SelectedUnit>().0.is_some());
    }

    #[test]
    fn a_new_blocker_does_not_leak_into_an_existing_advisory_field() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );

        click_tile(&mut app, Position::new(2, 2));
        assert!(
            app.world()
                .resource::<MoveRange>()
                .tiles
                .contains_key(&Position::new(2, 1))
        );

        spawn_unit(
            &mut app,
            Position::new(2, 1),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
            true,
            Some(99),
        );
        click_tile(&mut app, Position::new(2, 1));

        assert!(app.world().resource::<PendingMoveDestination>().0.is_some());
        assert!(app.world().resource::<SelectedUnit>().0.is_some());
    }

    #[test]
    fn moving_selected_unit_clears_selection_and_range() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let unit = spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );

        click_tile(&mut app, Position::new(2, 2));
        assert!(app.world().resource::<SelectedUnit>().0.is_some());
        assert!(!app.world().resource::<MoveRange>().tiles.is_empty());

        app.world_mut()
            .entity_mut(unit)
            .insert(MapPosition::from(Position::new(2, 1)));
        app.update();

        assert_eq!(app.world().resource::<SelectedUnit>().0, None);
        assert_eq!(app.world().resource::<PendingMoveDestination>().0, None);
        assert!(app.world().resource::<MoveRange>().tiles.is_empty());
    }

    #[test]
    fn accepted_command_discards_its_rejection_snapshot() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let unit = spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        click_tile(&mut app, Position::new(2, 2));
        *app.world_mut().resource_mut::<PlayUiPhase>() = PlayUiPhase::AwaitingServer;
        let range = app.world().resource::<MoveRange>().tiles.clone();
        app.world_mut().resource_mut::<CommittedCommand>().0 = Some(CommittedSnapshot {
            unit,
            origin: Position::new(2, 2),
            range,
            attack_targets: HashMap::new(),
            path: vec![Position::new(2, 2), Position::new(2, 1)],
            destination: Position::new(2, 1),
            attack_intent: None,
            kind: CommittedKind::Move,
        });

        app.world_mut()
            .entity_mut(unit)
            .insert(MapPosition::from(Position::new(2, 1)));
        app.update();

        assert!(app.world().resource::<CommittedCommand>().0.is_none());
        assert_eq!(app.world().resource::<SelectedUnit>().0, None);
    }

    #[test]
    fn unrelated_transition_preserves_move_rejection_snapshot() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let unit = spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );
        click_tile(&mut app, Position::new(2, 2));
        *app.world_mut().resource_mut::<PlayUiPhase>() = PlayUiPhase::AwaitingServer;
        let range = app.world().resource::<MoveRange>().tiles.clone();
        let snapshot = CommittedSnapshot {
            unit,
            origin: Position::new(2, 2),
            range,
            attack_targets: HashMap::new(),
            path: vec![Position::new(2, 2), Position::new(2, 1)],
            destination: Position::new(2, 1),
            attack_intent: None,
            kind: CommittedKind::Move,
        };
        app.world_mut().resource_mut::<CommittedCommand>().0 = Some(snapshot.clone());

        let observation = app
            .world()
            .resource::<awbrn_game::replay::RecipientObservations>()
            .for_player(awbrn_types::AwbwGamePlayerId::new(0))
            .unwrap()
            .clone();
        app.world_mut()
            .insert_resource(PendingLiveTransitions(std::collections::VecDeque::from([
                awvm::semantic::ObservedTransition {
                    post: observation,
                    events: Vec::new(),
                },
            ])));
        app.update();

        assert_eq!(app.world().resource::<CommittedCommand>().0, Some(snapshot));
    }

    #[test]
    fn clicking_unreachable_tile_or_escape_clears_selection() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );

        click_tile(&mut app, Position::new(2, 2));
        click_tile(&mut app, Position::new(4, 4));

        assert_eq!(app.world().resource::<SelectedUnit>().0, None);
        assert!(app.world().resource::<MoveRange>().tiles.is_empty());

        click_tile(&mut app, Position::new(2, 2));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();

        assert_eq!(app.world().resource::<SelectedUnit>().0, None);
        assert!(app.world().resource::<MoveRange>().tiles.is_empty());
    }

    #[test]
    fn terrain_costs_are_respected() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 5, 5);
        app.world_mut()
            .resource_mut::<FriendlyFactions>()
            .0
            .insert(PlayerFaction::OrangeStar);

        let mountain_entity = app
            .world()
            .resource::<BoardIndex>()
            .terrain_entity(Position::new(2, 1))
            .unwrap();
        app.world_mut()
            .entity_mut(mountain_entity)
            .insert(awbrn_game::world::TerrainTile {
                terrain: GraphicalTerrain::Mountain,
            });
        app.world_mut()
            .resource_mut::<GameMap>()
            .set_terrain(Position::new(2, 1), GraphicalTerrain::Mountain);

        spawn_unit(
            &mut app,
            Position::new(2, 2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            true,
            Some(99),
        );

        click_tile(&mut app, Position::new(2, 2));

        assert_eq!(
            app.world()
                .resource::<MoveRange>()
                .tiles
                .get(&Position::new(2, 1)),
            Some(&2)
        );
    }
}
