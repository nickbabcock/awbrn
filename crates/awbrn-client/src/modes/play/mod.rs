use std::collections::{HashMap, HashSet};

use crate::core::coords::{TILE_SIZE, position_to_world_translation};
use crate::core::{AppState, GameMode, RenderLayer, SpriteSize};
use crate::features::event_bus::{
    ActionMenuAction, ActionMenuEvent, ClientCommandReady, EventSink,
};
use crate::features::input::TileClicked;
use awbrn_game::MapPosition;
use awbrn_game::world::{
    Ammo, BoardIndex, CarriedBy, Faction, FriendlyFactions, Fuel, GameMap, StrongIdMap, Unit,
    UnitActive,
};
use awbrn_map::{MovementMap, Position, TerrainCosts};
use awbrn_types::{GraphicalTerrain, MovementCost, MovementTerrain, UnitMovement};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde_json::json;

const MOVE_RANGE_COLOR: Color = Color::srgba(0.1, 0.9, 0.75, 0.42);
const ATTACK_TARGET_COLOR: Color = Color::srgba(0.95, 0.3, 0.3, 0.5);

const MOVE_RANGE_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::MOVE_RANGE_OVERLAY,
};

const ATTACK_TARGET_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::MOVE_RANGE_OVERLAY + 1,
};

pub mod live_match;
pub use live_match::{
    MatchGameStateWire, MatchParticipantWire, MatchPlayerUpdateWire, PendingMatchState,
    PendingMatchUpdates, ServerUnitId, VisibleCargo,
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
    pub tiles: HashMap<Position, MoveDestinationCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMoveDestinationSelection {
    pub unit: Entity,
    pub origin: Position,
    pub destination: Position,
    pub path: Vec<Position>,
}

#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct PendingMoveDestination(pub Option<PendingMoveDestinationSelection>);

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayUiPhase {
    #[default]
    Idle,
    UnitSelected,
    DestinationSelected,
    AttackTargeting,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveRangeHighlight;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackTargetHighlight;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveDestinationCandidate {
    pub movement_cost: u8,
    pub path: Vec<Position>,
}

#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct CurrentActionMenu {
    pub state: Option<ActionMenuState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionMenuState {
    pub unit_entity: Entity,
    pub unit_id: u64,
    pub origin: Position,
    pub destination: Position,
    pub path: Vec<Position>,
    pub actions: Vec<ActionMenuAction>,
    pub attack_targets: Vec<AttackTarget>,
    pub anchor: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackTarget {
    pub unit_entity: Entity,
    pub unit_id: u64,
    pub position: Position,
}

#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct CurrentAttackTargets {
    pub origin: Option<Position>,
    pub destination: Option<Position>,
    pub unit_id: Option<u64>,
    pub path: Vec<Position>,
    pub targets: Vec<AttackTarget>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChooseActionMenuAction {
    pub action: ActionMenuAction,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelActionMenu;

#[derive(SystemParam)]
pub(crate) struct PlaySelectionState<'w> {
    selected: ResMut<'w, SelectedUnit>,
    move_range: ResMut<'w, MoveRange>,
    pending_destination: ResMut<'w, PendingMoveDestination>,
    phase: ResMut<'w, PlayUiPhase>,
}

#[derive(Clone)]
struct ClientMovementMap {
    width: usize,
    height: usize,
    terrain: Vec<MovementTerrain>,
    blocked: Vec<bool>,
}

impl ClientMovementMap {
    fn flat_index(&self, position: Position) -> Option<usize> {
        if position.x < self.width && position.y < self.height {
            Some(position.y * self.width + position.x)
        } else {
            None
        }
    }
}

impl MovementMap for ClientMovementMap {
    fn terrain_at(&self, pos: Position) -> Option<MovementTerrain> {
        self.flat_index(pos).map(|idx| self.terrain[idx])
    }

    fn terrain_at_flat(&self, flat_idx: usize) -> MovementTerrain {
        self.terrain[flat_idx]
    }

    fn is_blocked_flat(&self, flat_idx: usize) -> bool {
        self.blocked[flat_idx]
    }

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }
}

struct UnitMovementCosts {
    movement_type: UnitMovement,
}

impl TerrainCosts for UnitMovementCosts {
    fn cost(&self, terrain: MovementTerrain) -> Option<u8> {
        MovementCost::from_terrain(&terrain).cost(self.movement_type)
    }
}

type UnitSelectionQueryItem<'a> = (
    &'a Unit,
    &'a Faction,
    &'a MapPosition,
    Option<&'a Fuel>,
    Has<UnitActive>,
    Has<CarriedBy>,
);

type OccupancyQueryItem<'a> = (Entity, &'a MapPosition, &'a Faction, Has<CarriedBy>);
type SelectionValidityQueryItem<'a> = (
    &'a Faction,
    &'a MapPosition,
    Has<UnitActive>,
    Has<CarriedBy>,
);
type ActionQueryItem<'a> = (
    Entity,
    &'a Unit,
    &'a Faction,
    &'a MapPosition,
    Has<UnitActive>,
    Has<CarriedBy>,
    Option<&'a Fuel>,
    Option<&'a Ammo>,
    Has<awbrn_game::world::Hiding>,
    Option<&'a VisibleCargo>,
    Option<&'a ServerUnitId>,
);
type CleanupHighlightFilter = Or<(With<MoveRangeHighlight>, With<AttackTargetHighlight>)>;
type CleanupHighlightsQuery<'w, 's> = Query<'w, 's, Entity, CleanupHighlightFilter>;

#[derive(SystemParam)]
pub(crate) struct PlayBoardContext<'w, 's> {
    board_index: Res<'w, BoardIndex>,
    game_map: Res<'w, GameMap>,
    friendly_factions: Res<'w, FriendlyFactions>,
    units: Query<'w, 's, UnitSelectionQueryItem<'static>, With<Unit>>,
    occupancy: Query<'w, 's, OccupancyQueryItem<'static>, With<Unit>>,
    current_menu: Res<'w, CurrentActionMenu>,
    current_targets: Res<'w, CurrentAttackTargets>,
}

#[derive(SystemParam)]
pub(crate) struct ActionMenuContext<'w, 's> {
    game_map: Res<'w, GameMap>,
    friendly_factions: Res<'w, FriendlyFactions>,
    actions: Query<'w, 's, ActionQueryItem<'static>, With<Unit>>,
    occupancy: Query<'w, 's, OccupancyQueryItem<'static>, With<Unit>>,
    camera_q: Query<'w, 's, (&'static Camera, &'static GlobalTransform)>,
    sink: Option<Res<'w, EventSink<ActionMenuEvent>>>,
}

fn unit_is_selectable(
    faction: Faction,
    is_active: bool,
    is_carried: bool,
    friendly_factions: &FriendlyFactions,
) -> bool {
    is_active && !is_carried && friendly_factions.0.contains(&faction.0)
}

fn movement_budget(unit: awbrn_types::Unit, fuel: Option<&Fuel>) -> u8 {
    let fuel = fuel.map_or(unit.max_fuel(), Fuel::value);
    unit.movement_range()
        .min(fuel.min(u32::from(u8::MAX)) as u8)
}

fn collect_terrain(game_map: &GameMap) -> Vec<MovementTerrain> {
    let mut terrain = Vec::with_capacity(game_map.width() * game_map.height());

    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            let position = Position::new(x, y);
            let graphical = game_map
                .terrain_at(position)
                .unwrap_or(GraphicalTerrain::Plain);
            terrain.push(MovementTerrain::from(graphical));
        }
    }

    terrain
}

fn compute_move_range(
    game_map: &GameMap,
    moving_entity: Entity,
    origin: Position,
    unit: awbrn_types::Unit,
    fuel: Option<&Fuel>,
    friendly_factions: &FriendlyFactions,
    occupancy: &Query<OccupancyQueryItem<'_>, With<Unit>>,
) -> HashMap<Position, MoveDestinationCandidate> {
    let width = game_map.width();
    let height = game_map.height();
    let tile_count = width * height;
    let mut blocked = vec![false; tile_count];
    let mut friendly_occupied = HashSet::new();

    for (entity, map_position, faction, is_carried) in occupancy {
        if is_carried || entity == moving_entity {
            continue;
        }

        let position = map_position.position();
        if position.x >= width || position.y >= height {
            continue;
        }

        if friendly_factions.0.contains(&faction.0) {
            friendly_occupied.insert(position);
        } else {
            blocked[position.y * width + position.x] = true;
        }
    }

    let map = ClientMovementMap {
        width,
        height,
        terrain: collect_terrain(game_map),
        blocked,
    };
    let costs = UnitMovementCosts {
        movement_type: unit.movement_type(),
    };
    let movement_budget = movement_budget(unit, fuel);

    let mut best_costs = vec![u8::MAX; tile_count];
    let mut predecessors = vec![None; tile_count];
    let mut buckets = vec![Vec::<usize>::new(); movement_budget as usize + 1];
    let origin_idx = origin.y * width + origin.x;
    best_costs[origin_idx] = 0;
    buckets[0].push(origin_idx);

    for current_cost in 0..=movement_budget as usize {
        while let Some(flat_idx) = buckets[current_cost].pop() {
            if best_costs[flat_idx] as usize != current_cost {
                continue;
            }

            let x = flat_idx % width;
            let y = flat_idx / width;
            let neighbors = [
                (x + 1 < width).then_some(flat_idx + 1),
                if x > 0 { Some(flat_idx - 1) } else { None },
                (y + 1 < height).then_some(flat_idx + width),
                if y > 0 { Some(flat_idx - width) } else { None },
            ];

            for neighbor in neighbors.into_iter().flatten() {
                if map.is_blocked_flat(neighbor) {
                    continue;
                }

                let terrain = map.terrain_at_flat(neighbor);
                let Some(step_cost) = costs.cost(terrain) else {
                    continue;
                };
                let next_cost = current_cost + step_cost as usize;
                if next_cost > movement_budget as usize {
                    continue;
                }

                if next_cost < best_costs[neighbor] as usize {
                    best_costs[neighbor] = next_cost as u8;
                    predecessors[neighbor] = Some(flat_idx);
                    buckets[next_cost].push(neighbor);
                }
            }
        }
    }

    let mut reachable = HashMap::new();
    for (flat_idx, &cost) in best_costs.iter().enumerate().take(tile_count) {
        if cost == u8::MAX || flat_idx == origin_idx {
            continue;
        }

        let position = Position::new(flat_idx % width, flat_idx / width);
        if friendly_occupied.contains(&position) {
            continue;
        }

        let mut path = Vec::new();
        let mut cursor = Some(flat_idx);
        while let Some(index) = cursor {
            path.push(Position::new(index % width, index / width));
            cursor = predecessors[index];
        }
        path.reverse();

        reachable.insert(
            position,
            MoveDestinationCandidate {
                movement_cost: cost,
                path,
            },
        );
    }

    reachable
}

fn clear_selection_state(selection: &mut PlaySelectionState<'_>) {
    selection.selected.0 = None;
    selection.move_range.tiles.clear();
    selection.pending_destination.0 = None;
    *selection.phase = PlayUiPhase::Idle;
}

fn select_unit(
    entity: Entity,
    origin: Position,
    range: HashMap<Position, MoveDestinationCandidate>,
    selection: &mut PlaySelectionState<'_>,
) {
    selection.selected.0 = Some(SelectedUnitSelection { entity, origin });
    selection.move_range.tiles = range;
    selection.pending_destination.0 = None;
    *selection.phase = PlayUiPhase::UnitSelected;
}

fn confirm_selected_destination(
    destination: Position,
    game_map: &GameMap,
    friendly_factions: &FriendlyFactions,
    units: &Query<UnitSelectionQueryItem<'_>, With<Unit>>,
    occupancy: &Query<OccupancyQueryItem<'_>, With<Unit>>,
    selection: &mut PlaySelectionState<'_>,
) -> bool {
    let Some(selected_unit) = selection.selected.0 else {
        return false;
    };
    let Ok((unit, faction, map_position, fuel, is_active, is_carried)) =
        units.get(selected_unit.entity)
    else {
        clear_selection_state(selection);
        return false;
    };

    if !unit_is_selectable(*faction, is_active, is_carried, friendly_factions)
        || map_position.position() != selected_unit.origin
    {
        clear_selection_state(selection);
        return false;
    }

    let range = compute_move_range(
        game_map,
        selected_unit.entity,
        selected_unit.origin,
        unit.0,
        fuel,
        friendly_factions,
        occupancy,
    );

    let Some(candidate) = range.get(&destination).cloned() else {
        clear_selection_state(selection);
        return false;
    };

    selection.move_range.tiles = range;
    selection.pending_destination.0 = Some(PendingMoveDestinationSelection {
        unit: selected_unit.entity,
        origin: selected_unit.origin,
        destination,
        path: candidate.path,
    });
    *selection.phase = PlayUiPhase::DestinationSelected;
    true
}

fn transport_capacity(unit: awbrn_types::Unit) -> Option<usize> {
    match unit {
        awbrn_types::Unit::APC | awbrn_types::Unit::TCopter => Some(1),
        awbrn_types::Unit::BlackBoat | awbrn_types::Unit::Cruiser | awbrn_types::Unit::Lander => {
            Some(2)
        }
        _ => None,
    }
}

fn can_transport(transport: awbrn_types::Unit, cargo: awbrn_types::Unit) -> bool {
    match transport {
        awbrn_types::Unit::APC | awbrn_types::Unit::TCopter | awbrn_types::Unit::BlackBoat => {
            matches!(cargo, awbrn_types::Unit::Infantry | awbrn_types::Unit::Mech)
        }
        awbrn_types::Unit::Lander => cargo.domain() == awbrn_types::UnitDomain::Ground,
        awbrn_types::Unit::Cruiser => cargo.domain() == awbrn_types::UnitDomain::Air,
        _ => false,
    }
}

fn destination_anchor(
    destination: Position,
    game_map: &GameMap,
    camera_q: &Query<(&Camera, &GlobalTransform)>,
) -> Option<Vec2> {
    let Ok((camera, camera_transform)) = camera_q.single() else {
        return None;
    };
    let world = position_to_world_translation(&MOVE_RANGE_SPRITE_SIZE, destination, game_map);
    camera.world_to_viewport(camera_transform, world).ok()
}

fn derive_attack_targets(
    destination: Position,
    origin: Position,
    unit: awbrn_types::Unit,
    friendly_factions: &FriendlyFactions,
    actions: &Query<ActionQueryItem<'_>, With<Unit>>,
) -> Vec<AttackTarget> {
    if unit.is_indirect() && origin != destination {
        return Vec::new();
    }

    let min_range = unit.attack_range_min() as usize;
    let max_range = unit.attack_range_max() as usize;

    actions
        .iter()
        .filter_map(
            |(
                entity,
                _target_unit,
                target_faction,
                position,
                _active,
                is_carried,
                _fuel,
                _ammo,
                _hiding,
                _cargo,
                unit_id,
            )| {
                if is_carried || friendly_factions.0.contains(&target_faction.0) {
                    return None;
                }

                let distance = destination.manhattan(&position.position());
                if distance < min_range || distance > max_range {
                    return None;
                }

                Some(AttackTarget {
                    unit_entity: entity,
                    unit_id: unit_id.map(|id| id.0).unwrap_or_default(),
                    position: position.position(),
                })
            },
        )
        .collect()
}

fn unload_actions_available(
    destination: Position,
    cargo_units: &[awbrn_types::Unit],
    game_map: &GameMap,
    occupancy: &Query<OccupancyQueryItem<'_>, With<Unit>>,
) -> bool {
    let neighbors = [
        destination
            .x
            .checked_add(1)
            .map(|x| Position::new(x, destination.y)),
        destination
            .x
            .checked_sub(1)
            .map(|x| Position::new(x, destination.y)),
        destination
            .y
            .checked_add(1)
            .map(|y| Position::new(destination.x, y)),
        destination
            .y
            .checked_sub(1)
            .map(|y| Position::new(destination.x, y)),
    ];

    neighbors.into_iter().flatten().any(|target| {
        let target_occupied = occupancy
            .iter()
            .any(|(_, position, _, is_carried)| !is_carried && position.position() == target);
        if target_occupied {
            return false;
        }

        let Some(terrain) = game_map.terrain_at(target) else {
            return false;
        };

        cargo_units.iter().any(|cargo| {
            MovementCost::from_terrain(&MovementTerrain::from(terrain))
                .cost(cargo.movement_type())
                .is_some()
        })
    })
}

fn derive_valid_actions(
    pending: &PendingMoveDestinationSelection,
    game_map: &GameMap,
    friendly_factions: &FriendlyFactions,
    actions: &Query<ActionQueryItem<'_>, With<Unit>>,
    occupancy: &Query<OccupancyQueryItem<'_>, With<Unit>>,
) -> Option<(u64, Vec<ActionMenuAction>, Vec<AttackTarget>)> {
    let (
        entity,
        unit,
        faction,
        map_position,
        is_active,
        is_carried,
        _fuel,
        _ammo,
        is_hiding,
        cargo,
        unit_id,
    ) = actions.get(pending.unit).ok()?;

    if entity != pending.unit
        || !is_active
        || is_carried
        || map_position.position() != pending.origin
        || !friendly_factions.0.contains(&faction.0)
    {
        return None;
    }

    let mut valid = Vec::new();

    let attack_targets = derive_attack_targets(
        pending.destination,
        pending.origin,
        unit.0,
        friendly_factions,
        actions,
    );
    if !attack_targets.is_empty() {
        valid.push(ActionMenuAction::Attack);
    }

    if matches!(
        unit.0,
        awbrn_types::Unit::Infantry | awbrn_types::Unit::Mech
    ) && let Some(GraphicalTerrain::Property(property)) =
        game_map.terrain_at(pending.destination)
    {
        match property.faction() {
            awbrn_types::Faction::Neutral => valid.push(ActionMenuAction::Capture),
            awbrn_types::Faction::Player(owner) if owner != faction.0 => {
                valid.push(ActionMenuAction::Capture)
            }
            awbrn_types::Faction::Player(_) => {}
        }
    }

    if unit.0 == awbrn_types::Unit::APC {
        let has_adjacent_friendly = actions.iter().any(
            |(
                other_entity,
                _other_unit,
                other_faction,
                other_position,
                _active,
                other_carried,
                ..,
            )| {
                other_entity != pending.unit
                    && !other_carried
                    && other_faction.0 == faction.0
                    && pending.destination.manhattan(&other_position.position()) == 1
            },
        );
        if has_adjacent_friendly {
            valid.push(ActionMenuAction::Supply);
        }
    }

    if matches!(unit.0, awbrn_types::Unit::Sub | awbrn_types::Unit::Stealth) {
        valid.push(if is_hiding {
            ActionMenuAction::Unhide
        } else {
            ActionMenuAction::Hide
        });
    }

    let occupant = actions.iter().find(
        |(
            other_entity,
            _other_unit,
            _other_faction,
            other_position,
            _active,
            other_carried,
            ..,
        )| {
            *other_entity != pending.unit
                && !*other_carried
                && other_position.position() == pending.destination
        },
    );

    if let Some((_, other_unit, other_faction, _other_position, _active, _other_carried, ..)) =
        occupant
    {
        if other_faction.0 == faction.0 && other_unit.0 == unit.0 {
            valid.push(ActionMenuAction::Join);
        }

        if other_faction.0 == faction.0
            && let Some(capacity) = transport_capacity(other_unit.0)
        {
            let cargo_count = cargo.map(|cargo| cargo.0.len()).unwrap_or_default();
            if cargo_count < capacity && can_transport(other_unit.0, unit.0) {
                valid.push(ActionMenuAction::Load);
            }
        }
    }

    if let Some(cargo) = cargo
        && transport_capacity(unit.0).is_some()
        && unload_actions_available(pending.destination, &cargo.0, game_map, occupancy)
    {
        valid.push(ActionMenuAction::Unload);
    }

    valid.push(ActionMenuAction::Wait);

    Some((
        unit_id.map(|id| id.0).unwrap_or_default(),
        valid,
        attack_targets,
    ))
}

fn emit_action_menu(action_menu: &ActionMenuState, sink: Option<&Res<EventSink<ActionMenuEvent>>>) {
    if let Some(sink) = sink {
        sink.emit(ActionMenuEvent {
            unit_id: action_menu.unit_id,
            origin_x: action_menu.origin.x,
            origin_y: action_menu.origin.y,
            destination_x: action_menu.destination.x,
            destination_y: action_menu.destination.y,
            anchor_x: action_menu.anchor.x,
            anchor_y: action_menu.anchor.y,
            actions: action_menu.actions.clone(),
        });
    }
}

fn close_action_menu(
    sink: Option<&Res<EventSink<ActionMenuEvent>>>,
    current_menu: &mut ResMut<CurrentActionMenu>,
) {
    if current_menu.state.take().is_some()
        && let Some(sink) = sink
    {
        sink.emit(ActionMenuEvent {
            unit_id: 0,
            origin_x: 0,
            origin_y: 0,
            destination_x: 0,
            destination_y: 0,
            anchor_x: -1.0,
            anchor_y: -1.0,
            actions: Vec::new(),
        });
    }
}

fn queue_move_unit_command(
    unit_id: u64,
    path: Vec<Position>,
    action: serde_json::Value,
    sink: Option<&Res<EventSink<ClientCommandReady>>>,
) {
    let Some(sink) = sink else {
        return;
    };

    let command = json!({
        "type": "moveUnit",
        "unit_id": unit_id,
        "path": path.into_iter().map(|position| json!({ "x": position.x, "y": position.y })).collect::<Vec<_>>(),
        "action": action,
    });
    sink.emit(ClientCommandReady { command });
}

pub(crate) fn sync_action_menu_state(
    context: ActionMenuContext<'_, '_>,
    selection: PlaySelectionState<'_>,
    mut current_menu: ResMut<CurrentActionMenu>,
    mut current_targets: ResMut<CurrentAttackTargets>,
) {
    if *selection.phase == PlayUiPhase::AttackTargeting {
        close_action_menu(context.sink.as_ref(), &mut current_menu);
        return;
    }

    if *selection.phase != PlayUiPhase::DestinationSelected {
        current_targets.targets.clear();
        close_action_menu(context.sink.as_ref(), &mut current_menu);
        return;
    }

    let Some(pending) = selection.pending_destination.0.as_ref() else {
        current_targets.targets.clear();
        close_action_menu(context.sink.as_ref(), &mut current_menu);
        return;
    };

    let Some(anchor) =
        destination_anchor(pending.destination, &context.game_map, &context.camera_q)
    else {
        return;
    };
    let Some((unit_id, valid_actions, attack_targets)) = derive_valid_actions(
        pending,
        &context.game_map,
        &context.friendly_factions,
        &context.actions,
        &context.occupancy,
    ) else {
        current_targets.targets.clear();
        close_action_menu(context.sink.as_ref(), &mut current_menu);
        return;
    };

    current_targets.targets = attack_targets.clone();
    current_targets.unit_id = Some(unit_id);
    current_targets.origin = Some(pending.origin);
    current_targets.destination = Some(pending.destination);
    current_targets.path = pending.path.clone();

    let next = ActionMenuState {
        unit_entity: pending.unit,
        unit_id,
        origin: pending.origin,
        destination: pending.destination,
        path: pending.path.clone(),
        actions: valid_actions,
        attack_targets,
        anchor,
    };

    if current_menu.state.as_ref() != Some(&next) {
        emit_action_menu(&next, context.sink.as_ref());
        current_menu.state = Some(next);
    }
}

pub(crate) fn handle_action_menu_choices(
    mut choice_reader: MessageReader<ChooseActionMenuAction>,
    mut current_menu: ResMut<CurrentActionMenu>,
    mut current_targets: ResMut<CurrentAttackTargets>,
    mut selection: PlaySelectionState<'_>,
    occupancy: Query<ActionQueryItem<'_>, With<Unit>>,
    action_menu_sink: Option<Res<EventSink<ActionMenuEvent>>>,
    sink: Option<Res<EventSink<ClientCommandReady>>>,
) {
    let Some(choice) = choice_reader.read().last().copied() else {
        return;
    };
    let Some(menu) = current_menu.state.clone() else {
        return;
    };

    match choice.action {
        ActionMenuAction::Attack => {
            close_action_menu(action_menu_sink.as_ref(), &mut current_menu);
            current_targets.targets = menu.attack_targets.clone();
            current_targets.unit_id = Some(menu.unit_id);
            current_targets.origin = Some(menu.origin);
            current_targets.destination = Some(menu.destination);
            current_targets.path = menu.path.clone();
            *selection.phase = PlayUiPhase::AttackTargeting;
        }
        ActionMenuAction::Wait => {
            queue_move_unit_command(
                menu.unit_id,
                menu.path.clone(),
                json!({ "type": "wait" }),
                sink.as_ref(),
            );
            clear_selection_state(&mut selection);
            close_action_menu(action_menu_sink.as_ref(), &mut current_menu);
            current_targets.targets.clear();
        }
        ActionMenuAction::Capture => {
            queue_move_unit_command(
                menu.unit_id,
                menu.path.clone(),
                json!({ "type": "capture" }),
                sink.as_ref(),
            );
            clear_selection_state(&mut selection);
            close_action_menu(action_menu_sink.as_ref(), &mut current_menu);
            current_targets.targets.clear();
        }
        ActionMenuAction::Supply => {
            queue_move_unit_command(
                menu.unit_id,
                menu.path.clone(),
                json!({ "type": "supply" }),
                sink.as_ref(),
            );
            clear_selection_state(&mut selection);
            close_action_menu(action_menu_sink.as_ref(), &mut current_menu);
            current_targets.targets.clear();
        }
        ActionMenuAction::Hide => {
            queue_move_unit_command(
                menu.unit_id,
                menu.path.clone(),
                json!({ "type": "hide" }),
                sink.as_ref(),
            );
            clear_selection_state(&mut selection);
            close_action_menu(action_menu_sink.as_ref(), &mut current_menu);
            current_targets.targets.clear();
        }
        ActionMenuAction::Unhide => {
            queue_move_unit_command(
                menu.unit_id,
                menu.path.clone(),
                json!({ "type": "unhide" }),
                sink.as_ref(),
            );
            clear_selection_state(&mut selection);
            close_action_menu(action_menu_sink.as_ref(), &mut current_menu);
            current_targets.targets.clear();
        }
        ActionMenuAction::Join => {
            let Some((
                _,
                _unit,
                _faction,
                _position,
                _active,
                _carried,
                _fuel,
                _ammo,
                _hiding,
                _cargo,
                unit_id,
            )) = occupancy.iter().find(
                |(entity, _unit, _faction, position, _active, carried, ..)| {
                    *entity != menu.unit_entity
                        && !*carried
                        && position.position() == menu.destination
                },
            )
            else {
                return;
            };
            let Some(target_id) = unit_id.map(|id| id.0) else {
                return;
            };
            queue_move_unit_command(
                menu.unit_id,
                menu.path.clone(),
                json!({ "type": "join", "target_id": target_id }),
                sink.as_ref(),
            );
            clear_selection_state(&mut selection);
            close_action_menu(action_menu_sink.as_ref(), &mut current_menu);
            current_targets.targets.clear();
        }
        ActionMenuAction::Load => {
            let Some((
                _,
                _transport_unit,
                _transport_faction,
                _position,
                _active,
                _carried,
                _fuel,
                _ammo,
                _hiding,
                _cargo,
                unit_id,
            )) = occupancy.iter().find(
                |(entity, _unit, _faction, position, _active, carried, ..)| {
                    *entity != menu.unit_entity
                        && !*carried
                        && position.position() == menu.destination
                },
            )
            else {
                return;
            };
            let Some(transport_id) = unit_id.map(|id| id.0) else {
                return;
            };
            queue_move_unit_command(
                menu.unit_id,
                menu.path.clone(),
                json!({ "type": "load", "transport_id": transport_id }),
                sink.as_ref(),
            );
            clear_selection_state(&mut selection);
            close_action_menu(action_menu_sink.as_ref(), &mut current_menu);
            current_targets.targets.clear();
        }
        ActionMenuAction::Unload => {}
    }
}

pub(crate) fn cancel_action_menu(
    mut cancel_reader: MessageReader<CancelActionMenu>,
    mut current_menu: ResMut<CurrentActionMenu>,
    mut current_targets: ResMut<CurrentAttackTargets>,
    mut selection: PlaySelectionState<'_>,
    sink: Option<Res<EventSink<ActionMenuEvent>>>,
) {
    if cancel_reader.read().last().is_none() {
        return;
    }

    close_action_menu(sink.as_ref(), &mut current_menu);
    current_targets.targets.clear();
    clear_selection_state(&mut selection);
}

pub(crate) fn handle_attack_target_clicks(
    mut click_reader: MessageReader<TileClicked>,
    mut current_targets: ResMut<CurrentAttackTargets>,
    mut selection: PlaySelectionState<'_>,
    sink: Option<Res<EventSink<ClientCommandReady>>>,
) {
    if *selection.phase != PlayUiPhase::AttackTargeting {
        return;
    }

    let Some(TileClicked { position }) = click_reader.read().last().copied() else {
        return;
    };
    let Some(target) = current_targets
        .targets
        .iter()
        .find(|target| target.position == position)
        .copied()
    else {
        current_targets.targets.clear();
        clear_selection_state(&mut selection);
        return;
    };

    let Some(unit_id) = current_targets.unit_id else {
        return;
    };

    queue_move_unit_command(
        unit_id,
        current_targets.path.clone(),
        json!({
            "type": "attack",
            "target": { "x": target.position.x, "y": target.position.y }
        }),
        sink.as_ref(),
    );
    current_targets.targets.clear();
    clear_selection_state(&mut selection);
}

pub(crate) fn sync_attack_target_highlights(
    mut commands: Commands,
    game_map: Res<GameMap>,
    current_targets: Res<CurrentAttackTargets>,
    highlights: Query<Entity, With<AttackTargetHighlight>>,
) {
    if !current_targets.is_changed() {
        return;
    }

    for entity in &highlights {
        commands.entity(entity).try_despawn();
    }

    let mut positions: Vec<_> = current_targets
        .targets
        .iter()
        .map(|target| target.position)
        .collect();
    positions.sort();

    for position in positions {
        commands.spawn((
            AttackTargetHighlight,
            Sprite::from_color(
                ATTACK_TARGET_COLOR,
                Vec2::new(
                    ATTACK_TARGET_SPRITE_SIZE.width,
                    ATTACK_TARGET_SPRITE_SIZE.height,
                ),
            ),
            ATTACK_TARGET_SPRITE_SIZE,
            Transform::from_translation(position_to_world_translation(
                &ATTACK_TARGET_SPRITE_SIZE,
                position,
                &game_map,
            )),
        ));
    }
}

pub(crate) fn handle_play_tile_clicks(
    context: PlayBoardContext<'_, '_>,
    mut click_reader: MessageReader<TileClicked>,
    mut selection: PlaySelectionState<'_>,
) {
    let Some(TileClicked { position }) = click_reader.read().last().copied() else {
        return;
    };

    if *selection.phase == PlayUiPhase::AttackTargeting {
        let _ = position;
        return;
    }

    if context.current_menu.state.is_some() {
        let _ = &context.current_targets;
        clear_selection_state(&mut selection);
        return;
    }

    if selection.selected.0.is_some() {
        if selection.move_range.tiles.contains_key(&position) {
            confirm_selected_destination(
                position,
                &context.game_map,
                &context.friendly_factions,
                &context.units,
                &context.occupancy,
                &mut selection,
            );
            return;
        }

        clear_selection_state(&mut selection);
        return;
    }

    let Ok(Some(unit_entity)) = context.board_index.unit_entity(position) else {
        return;
    };
    let Ok((unit, faction, map_position, fuel, is_active, is_carried)) =
        context.units.get(unit_entity)
    else {
        return;
    };

    if !unit_is_selectable(*faction, is_active, is_carried, &context.friendly_factions) {
        return;
    }

    let origin = map_position.position();
    let range = compute_move_range(
        &context.game_map,
        unit_entity,
        origin,
        unit.0,
        fuel,
        &context.friendly_factions,
        &context.occupancy,
    );
    select_unit(unit_entity, origin, range, &mut selection);
}

pub(crate) fn clear_selection_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    mut selection: PlaySelectionState<'_>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }

    clear_selection_state(&mut selection);
}

pub(crate) fn clear_invalid_selection(
    friendly_factions: Res<FriendlyFactions>,
    units: Query<SelectionValidityQueryItem<'_>, With<Unit>>,
    mut selection: PlaySelectionState<'_>,
) {
    let Some(selected_unit) = selection.selected.0 else {
        return;
    };
    let Ok((faction, map_position, is_active, is_carried)) = units.get(selected_unit.entity) else {
        clear_selection_state(&mut selection);
        return;
    };

    if !unit_is_selectable(*faction, is_active, is_carried, &friendly_factions)
        || map_position.position() != selected_unit.origin
    {
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
        commands.spawn((
            MoveRangeHighlight,
            Sprite::from_color(
                MOVE_RANGE_COLOR,
                Vec2::new(MOVE_RANGE_SPRITE_SIZE.width, MOVE_RANGE_SPRITE_SIZE.height),
            ),
            MOVE_RANGE_SPRITE_SIZE,
            Transform::from_translation(position_to_world_translation(
                &MOVE_RANGE_SPRITE_SIZE,
                position,
                &game_map,
            )),
        ));
    }
}

pub(crate) fn cleanup_play_selection(
    mut commands: Commands,
    mut selection: PlaySelectionState<'_>,
    highlights: CleanupHighlightsQuery<'_, '_>,
    mut current_menu: ResMut<CurrentActionMenu>,
    mut current_targets: ResMut<CurrentAttackTargets>,
) {
    clear_selection_state(&mut selection);
    current_menu.state = None;
    current_targets.targets.clear();

    for entity in &highlights {
        commands.entity(entity).try_despawn();
    }
}

pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedUnit>()
            .init_resource::<MoveRange>()
            .init_resource::<PendingMoveDestination>()
            .init_resource::<PlayUiPhase>()
            .init_resource::<CurrentActionMenu>()
            .init_resource::<CurrentAttackTargets>()
            .init_resource::<StrongIdMap<ServerUnitId>>()
            .init_resource::<live_match::MatchViewState>()
            .add_message::<ChooseActionMenuAction>()
            .add_message::<CancelActionMenu>()
            .add_systems(
                Update,
                (
                    live_match::apply_match_sync_system,
                    handle_play_tile_clicks
                        .after(crate::features::input::detect_map_clicks)
                        .after(crate::features::input::detect_touch_taps),
                    handle_attack_target_clicks
                        .after(crate::features::input::detect_map_clicks)
                        .after(crate::features::input::detect_touch_taps),
                    cancel_action_menu,
                    handle_action_menu_choices,
                    sync_action_menu_state,
                    clear_selection_on_escape,
                    clear_invalid_selection,
                    sync_move_range_highlights,
                    sync_attack_target_highlights,
                )
                    .chain()
                    .run_if(in_state(GameMode::Game).and(in_state(AppState::InGame))),
            )
            .add_systems(OnExit(GameMode::Game), cleanup_play_selection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_game::world::initialize_terrain_semantic_world;
    use awbrn_types::PlayerFaction;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    fn play_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.insert_state(AppState::InGame);
        app.insert_state(GameMode::Game);
        app.add_message::<TileClicked>();
        app.init_resource::<BoardIndex>();
        app.init_resource::<GameMap>();
        app.init_resource::<FriendlyFactions>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.add_plugins(PlayPlugin);
        app
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
        let mut entity =
            app.world_mut()
                .spawn((MapPosition::from(position), Unit(unit), Faction(faction)));
        if active {
            entity.insert(UnitActive);
        }
        if let Some(fuel) = fuel {
            entity.insert(Fuel(fuel));
        }
        entity.id()
    }

    fn click_tile(app: &mut App, position: Position) {
        app.world_mut()
            .resource_mut::<Messages<TileClicked>>()
            .write(TileClicked { position });
        app.update();
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
    fn inactive_and_enemy_units_are_not_selectable() {
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
    fn enemy_occupied_tiles_block_and_friendly_tiles_only_block_stopping() {
        let mut app = play_test_app();
        set_plain_map(&mut app, 6, 3);
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

        click_tile(&mut app, Position::new(0, 1));

        let range = &app.world().resource::<MoveRange>().tiles;
        assert!(!range.contains_key(&Position::new(1, 1)));
        assert!(range.contains_key(&Position::new(2, 1)));
        assert!(!range.contains_key(&Position::new(3, 1)));
        assert!(!range.contains_key(&Position::new(4, 1)));
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
            })
        );
        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::DestinationSelected
        );
    }

    #[test]
    fn destination_confirmation_revalidates_current_fuel() {
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

        assert_eq!(app.world().resource::<PendingMoveDestination>().0, None);
        assert_eq!(app.world().resource::<SelectedUnit>().0, None);
        assert!(app.world().resource::<MoveRange>().tiles.is_empty());
    }

    #[test]
    fn destination_confirmation_revalidates_current_occupancy() {
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

        assert_eq!(app.world().resource::<PendingMoveDestination>().0, None);
        assert_eq!(app.world().resource::<SelectedUnit>().0, None);
        assert!(app.world().resource::<MoveRange>().tiles.is_empty());
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
                .get(&Position::new(2, 1))
                .map(|candidate| candidate.movement_cost),
            Some(2)
        );
    }

    #[test]
    fn choosing_wait_emits_move_unit_command() {
        let mut app = play_test_app();
        let sent_commands = Arc::new(Mutex::new(Vec::new()));
        let sent_commands_clone = Arc::clone(&sent_commands);
        app.world_mut()
            .insert_resource(EventSink::<ClientCommandReady>::new(move |event| {
                sent_commands_clone.lock().unwrap().push(event.command);
            }));

        let unit = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<CurrentActionMenu>().state = Some(ActionMenuState {
            unit_entity: unit,
            unit_id: 77,
            origin: Position::new(2, 2),
            destination: Position::new(2, 1),
            path: vec![Position::new(2, 2), Position::new(2, 1)],
            actions: vec![ActionMenuAction::Wait],
            attack_targets: Vec::new(),
            anchor: Vec2::ZERO,
        });

        app.world_mut()
            .resource_mut::<Messages<ChooseActionMenuAction>>()
            .write(ChooseActionMenuAction {
                action: ActionMenuAction::Wait,
            });

        app.update();

        assert_eq!(
            sent_commands.lock().unwrap().as_slice(),
            &[json!({
                "type": "moveUnit",
                "unit_id": 77,
                "path": [
                    { "x": 2, "y": 2 },
                    { "x": 2, "y": 1 }
                ],
                "action": { "type": "wait" }
            })]
        );
        assert!(app.world().resource::<CurrentActionMenu>().state.is_none());
        assert_eq!(*app.world().resource::<PlayUiPhase>(), PlayUiPhase::Idle);
    }

    #[test]
    fn attack_target_click_emits_attack_command() {
        let mut app = play_test_app();
        let sent_commands = Arc::new(Mutex::new(Vec::new()));
        let sent_commands_clone = Arc::clone(&sent_commands);
        app.world_mut()
            .insert_resource(EventSink::<ClientCommandReady>::new(move |event| {
                sent_commands_clone.lock().unwrap().push(event.command);
            }));

        let unit = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<CurrentActionMenu>().state = Some(ActionMenuState {
            unit_entity: unit,
            unit_id: 88,
            origin: Position::new(2, 2),
            destination: Position::new(2, 1),
            path: vec![Position::new(2, 2), Position::new(2, 1)],
            actions: vec![ActionMenuAction::Attack, ActionMenuAction::Wait],
            attack_targets: vec![AttackTarget {
                unit_entity: Entity::PLACEHOLDER,
                unit_id: 99,
                position: Position::new(3, 1),
            }],
            anchor: Vec2::ZERO,
        });

        app.world_mut()
            .resource_mut::<Messages<ChooseActionMenuAction>>()
            .write(ChooseActionMenuAction {
                action: ActionMenuAction::Attack,
            });
        app.update();

        assert_eq!(
            *app.world().resource::<PlayUiPhase>(),
            PlayUiPhase::AttackTargeting
        );
        assert_eq!(
            app.world().resource::<CurrentAttackTargets>().targets,
            vec![AttackTarget {
                unit_entity: Entity::PLACEHOLDER,
                unit_id: 99,
                position: Position::new(3, 1),
            }]
        );

        app.world_mut()
            .resource_mut::<Messages<TileClicked>>()
            .write(TileClicked {
                position: Position::new(3, 1),
            });
        app.update();

        assert_eq!(
            sent_commands.lock().unwrap().as_slice(),
            &[json!({
                "type": "moveUnit",
                "unit_id": 88,
                "path": [
                    { "x": 2, "y": 2 },
                    { "x": 2, "y": 1 }
                ],
                "action": {
                    "type": "attack",
                    "target": { "x": 3, "y": 1 }
                }
            })]
        );
        assert_eq!(*app.world().resource::<PlayUiPhase>(), PlayUiPhase::Idle);
        assert!(
            app.world()
                .resource::<CurrentAttackTargets>()
                .targets
                .is_empty()
        );
    }
}
