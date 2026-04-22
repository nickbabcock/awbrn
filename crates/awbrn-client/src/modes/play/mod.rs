use std::collections::{HashMap, HashSet};

use crate::core::coords::{TILE_SIZE, position_to_world_translation};
use crate::core::{AppState, GameMode, RenderLayer, SpriteSize};
use crate::features::input::TileClicked;
use awbrn_game::MapPosition;
use awbrn_game::world::{
    BoardIndex, CarriedBy, Faction, FriendlyFactions, Fuel, GameMap, Unit, UnitActive,
};
use awbrn_map::{MovementMap, PathFinder, Position, TerrainCosts};
use awbrn_types::{GraphicalTerrain, MovementCost, MovementTerrain, UnitMovement};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

const MOVE_RANGE_COLOR: Color = Color::srgba(0.1, 0.9, 0.75, 0.42);

const MOVE_RANGE_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: TILE_SIZE,
    height: TILE_SIZE,
    z_index: RenderLayer::MOVE_RANGE_OVERLAY,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingMoveDestinationSelection {
    pub unit: Entity,
    pub origin: Position,
    pub destination: Position,
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PendingMoveDestination(pub Option<PendingMoveDestinationSelection>);

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayUiPhase {
    #[default]
    Idle,
    UnitSelected,
    DestinationSelected,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveRangeHighlight;

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
) -> HashMap<Position, u8> {
    let width = game_map.width();
    let height = game_map.height();
    let mut blocked = vec![false; width * height];
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
    let mut pathfinder = PathFinder::new(map);
    pathfinder
        .reachable(origin, movement_budget(unit, fuel), costs)
        .into_positions()
        .filter(|(position, _)| *position != origin)
        .filter(|(position, _)| !friendly_occupied.contains(position))
        .collect()
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
    range: HashMap<Position, u8>,
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

    if !range.contains_key(&destination) {
        clear_selection_state(selection);
        return false;
    }

    selection.move_range.tiles = range;
    selection.pending_destination.0 = Some(PendingMoveDestinationSelection {
        unit: selected_unit.entity,
        origin: selected_unit.origin,
        destination,
    });
    *selection.phase = PlayUiPhase::DestinationSelected;
    true
}

pub(crate) fn handle_play_tile_clicks(
    board_index: Res<BoardIndex>,
    game_map: Res<GameMap>,
    friendly_factions: Res<FriendlyFactions>,
    mut click_reader: MessageReader<TileClicked>,
    units: Query<UnitSelectionQueryItem<'_>, With<Unit>>,
    occupancy: Query<OccupancyQueryItem<'_>, With<Unit>>,
    mut selection: PlaySelectionState<'_>,
) {
    let Some(TileClicked { position }) = click_reader.read().last().copied() else {
        return;
    };

    if selection.selected.0.is_some() {
        if selection.move_range.tiles.contains_key(&position) {
            confirm_selected_destination(
                position,
                &game_map,
                &friendly_factions,
                &units,
                &occupancy,
                &mut selection,
            );
            return;
        }

        clear_selection_state(&mut selection);
        return;
    }

    let Ok(Some(unit_entity)) = board_index.unit_entity(position) else {
        return;
    };
    let Ok((unit, faction, map_position, fuel, is_active, is_carried)) = units.get(unit_entity)
    else {
        return;
    };

    if !unit_is_selectable(*faction, is_active, is_carried, &friendly_factions) {
        return;
    }

    let origin = map_position.position();
    let range = compute_move_range(
        &game_map,
        unit_entity,
        origin,
        unit.0,
        fuel,
        &friendly_factions,
        &occupancy,
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
    highlights: Query<Entity, With<MoveRangeHighlight>>,
) {
    clear_selection_state(&mut selection);

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
            .add_systems(
                Update,
                (
                    handle_play_tile_clicks
                        .after(crate::features::input::detect_map_clicks)
                        .after(crate::features::input::detect_touch_taps),
                    clear_selection_on_escape,
                    clear_invalid_selection,
                    sync_move_range_highlights,
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
                .get(&Position::new(2, 1)),
            Some(&2)
        );
    }
}
