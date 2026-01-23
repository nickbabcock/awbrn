//! Replay command system for processing AWBW replay actions.
//!
//! Uses a custom Bevy Command to get direct `&mut World` access, enabling
//! immediate mutations that are visible to subsequent queries within the same
//! command execution.

use awbrn_core::{AwbwTerrain, GraphicalTerrain, PlayerFaction, Property};
use awbrn_map::Position;
use awbw_replay::turn_models::{
    Action, CaptureAction, CombatUnit, FireAction, LoadAction, MoveAction, UnitMap, UpdatedInfo,
};
use bevy::log;
use bevy::prelude::*;

use crate::{
    AwbwUnitId, Capturing, CurrentWeather, Faction, GraphicalHp, HasCargo, LoadedReplay,
    MapPosition, StrongIdMap, TerrainTile, Unit,
};

/// Resource tracking the current state of replay playback.
#[derive(Resource)]
pub struct ReplayState {
    pub turn: u32,
    pub day: u32,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self { turn: 0, day: 1 }
    }
}

/// A custom Command for processing replay turn actions.
///
/// This command gets `&mut World` access, allowing immediate mutations that
/// are visible to subsequent queries within the same `apply()` call. This
/// eliminates the need for workarounds like `EntityHashMap` to track deferred
/// position changes.
pub struct ReplayTurnCommand {
    pub action: Action,
}

impl Command for ReplayTurnCommand {
    fn apply(self, world: &mut World) {
        // Apply movement first (many actions have move_action)
        if let Some(mov) = self.action.move_action() {
            Self::apply_move(mov, world);
        }

        // Dispatch to action-specific handler
        match &self.action {
            Action::Build { new_unit, .. } => Self::apply_build(new_unit, world),
            Action::Capt { capture_action, .. } => Self::apply_capture(capture_action, world),
            Action::Load { load_action, .. } => Self::apply_load(load_action, world),
            Action::Unload {
                unit, transport_id, ..
            } => Self::apply_unload(unit, *transport_id, world),
            Action::End { updated_info } => Self::apply_end(updated_info, world),
            Action::Fire {
                move_action: _move_action,
                fire_action,
            } => Self::apply_fire(fire_action, world),
            Action::Move(_) => {} // Already handled via move_action()
            _ => log::warn!("Unhandled action: {:?}", self.action),
        }
    }
}

impl ReplayTurnCommand {
    /// Processes unit movement from a MoveAction.
    fn apply_move(move_action: &MoveAction, world: &mut World) {
        for (_player, unit_data) in move_action.unit.iter() {
            let Some(unit) = unit_data.get_value() else {
                continue;
            };

            let Some(x) = unit.units_x else { continue };
            let Some(y) = unit.units_y else { continue };

            // Get entity from resource (borrow ends after this block)
            let entity = {
                let units = world.resource::<StrongIdMap<AwbwUnitId>>();
                units.get(&AwbwUnitId(unit.units_id))
            };

            let Some(entity) = entity else {
                log::warn!(
                    "Unit with ID {} not found in unit storage",
                    unit.units_id.as_u32()
                );
                continue;
            };

            // Immediate mutation - visible to subsequent queries!
            let new_position = MapPosition::new(x as usize, y as usize);
            world.entity_mut(entity).insert(new_position);
        }
    }

    /// Spawns new units from a Build action.
    fn apply_build(new_unit: &UnitMap, world: &mut World) {
        // Get the loaded replay to look up player factions
        let players = {
            let loaded_replay = world.resource::<LoadedReplay>();
            loaded_replay
                .0
                .games
                .first()
                .map(|g| g.players.clone())
                .unwrap_or_default()
        };

        for (_player, unit_data) in new_unit.iter() {
            let Some(unit) = unit_data.get_value() else {
                continue;
            };

            let Some(x) = unit.units_x else { continue };
            let Some(y) = unit.units_y else { continue };

            // Get player faction from replay data
            let faction = players
                .iter()
                .find(|p| p.id.as_u32() == unit.units_players_id)
                .map(|p| p.faction)
                .unwrap_or(PlayerFaction::OrangeStar);

            world.spawn((
                MapPosition::new(x as usize, y as usize),
                Faction(faction),
                AwbwUnitId(unit.units_id),
                Unit(unit.units_name),
            ));
        }
    }

    /// Processes a capture action, updating the capturing unit and potentially flipping the building.
    fn apply_capture(capture_action: &CaptureAction, world: &mut World) {
        let building_pos = Position::new(
            capture_action.building_info.buildings_x as usize,
            capture_action.building_info.buildings_y as usize,
        );

        // Find unit at the building position - query sees updated positions from apply_move!
        let capturing_unit = {
            let mut query = world.query::<(Entity, &MapPosition, &Faction)>();
            query
                .iter(world)
                .find(|(_, pos, _)| pos.position() == building_pos)
                .map(|(e, _, f)| (e, f.0))
        };

        let Some((entity, faction)) = capturing_unit else {
            log::warn!("No unit found at capture position {:?}", building_pos);
            return;
        };

        if capture_action.building_info.buildings_capture >= 20 {
            // Capture complete - remove Capturing component and flip building
            world.entity_mut(entity).remove::<Capturing>();
            Self::flip_building(world, building_pos, faction);
        } else {
            // Capture in progress - add Capturing component
            world.entity_mut(entity).insert(Capturing);
        }
    }

    /// Processes an End turn action, updating the day counter.
    fn apply_end(updated_info: &UpdatedInfo, world: &mut World) {
        let current_day = {
            let replay_state = world.resource::<ReplayState>();
            replay_state.day
        };

        if updated_info.day != current_day {
            world.resource_mut::<ReplayState>().day = updated_info.day;
        }
    }

    /// Processes a load action, hiding the loaded unit and marking the transport as carrying cargo.
    fn apply_load(load_action: &LoadAction, world: &mut World) {
        // Extract loaded unit ID (from awbrn_core)
        let loaded_unit_id = load_action
            .loaded
            .values()
            .find_map(|hidden| hidden.get_value().copied());

        // Extract transport unit ID (from awbrn_core)
        let transport_unit_id = load_action
            .transport
            .values()
            .find_map(|hidden| hidden.get_value().copied());

        let Some(loaded_id_core) = loaded_unit_id else {
            log::warn!("No loaded unit ID found in load action");
            return;
        };

        let Some(transport_id_core) = transport_unit_id else {
            log::warn!("No transport unit ID found in load action");
            return;
        };

        // Wrap in crate's AwbwUnitId for lookup
        let loaded_id = AwbwUnitId(loaded_id_core);
        let transport_id = AwbwUnitId(transport_id_core);

        // Get entities from resource
        let (loaded_entity, transport_entity) = {
            let units = world.resource::<StrongIdMap<AwbwUnitId>>();
            (units.get(&loaded_id), units.get(&transport_id))
        };

        let Some(loaded_entity) = loaded_entity else {
            log::warn!(
                "Loaded unit entity not found for ID: {}",
                loaded_id_core.as_u32()
            );
            return;
        };

        let Some(transport_entity) = transport_entity else {
            log::warn!(
                "Transport unit entity not found for ID: {}",
                transport_id_core.as_u32()
            );
            return;
        };

        // Hide the loaded unit
        world.entity_mut(loaded_entity).insert(Visibility::Hidden);

        // Add or update HasCargo component on transport
        let mut transport_mut = world.entity_mut(transport_entity);
        let success = if let Some(mut has_cargo) = transport_mut.get_mut::<HasCargo>() {
            has_cargo.add_cargo(loaded_id)
        } else {
            let mut has_cargo = HasCargo::new();
            let success = has_cargo.add_cargo(loaded_id);
            transport_mut.insert(has_cargo);
            success
        };

        if success {
            log::info!(
                "Loaded unit {} into transport {}",
                loaded_id_core.as_u32(),
                transport_id_core.as_u32()
            );
        } else {
            log::warn!(
                "Transport {} is at full capacity (2 units), could not load unit {}",
                transport_id_core.as_u32(),
                loaded_id_core.as_u32()
            );
        }
    }

    /// Processes an unload action, making the unloaded unit visible and removing it from cargo.
    fn apply_unload(
        unit_map: &UnitMap,
        transport_id_core: awbrn_core::AwbwUnitId,
        world: &mut World,
    ) {
        // Extract unloaded unit data
        let unloaded_unit = unit_map.values().find_map(|hidden| hidden.get_value());

        let Some(unit) = unloaded_unit else {
            log::warn!("No unloaded unit found in unload action");
            return;
        };

        let Some(x) = unit.units_x else {
            log::warn!("Unloaded unit has no x coordinate");
            return;
        };

        let Some(y) = unit.units_y else {
            log::warn!("Unloaded unit has no y coordinate");
            return;
        };

        // Wrap IDs in crate's AwbwUnitId for lookup
        let unloaded_id = AwbwUnitId(unit.units_id);
        let transport_id = AwbwUnitId(transport_id_core);

        // Get entities from resource
        let (unloaded_entity, transport_entity) = {
            let units = world.resource::<StrongIdMap<AwbwUnitId>>();
            (units.get(&unloaded_id), units.get(&transport_id))
        };

        let Some(unloaded_entity) = unloaded_entity else {
            log::warn!(
                "Unloaded unit entity not found for ID: {}",
                unit.units_id.as_u32()
            );
            return;
        };

        let Some(transport_entity) = transport_entity else {
            log::warn!(
                "Transport unit entity not found for ID: {}",
                transport_id_core.as_u32()
            );
            return;
        };

        // Make the unit visible and update its position
        world
            .entity_mut(unloaded_entity)
            .insert(Visibility::Inherited)
            .insert(MapPosition::new(x as usize, y as usize));

        // Remove unit from transport's cargo
        let mut transport_mut = world.entity_mut(transport_entity);
        if let Some(mut has_cargo) = transport_mut.get_mut::<HasCargo>() {
            let removed = has_cargo.remove_cargo(unloaded_id);

            if removed {
                log::info!(
                    "Unloaded unit {} from transport {} at ({}, {})",
                    unit.units_id.as_u32(),
                    transport_id_core.as_u32(),
                    x,
                    y
                );
                // Note: Cleanup of empty HasCargo is handled by cleanup_empty_cargo system
            } else {
                log::warn!(
                    "Unit {} was not in transport {}'s cargo",
                    unit.units_id.as_u32(),
                    transport_id_core.as_u32()
                );
            }
        } else {
            log::warn!(
                "Transport {} does not have HasCargo component",
                transport_id_core.as_u32()
            );
        }
    }

    /// Processes a fire action, updating unit health from combat results.
    fn apply_fire(fire_action: &FireAction, world: &mut World) {
        // Iterate over all players' combat vision
        for (_player, combat_vision) in fire_action.combat_info_vision.iter() {
            let combat_info = &combat_vision.combat_info;

            // Process attacker HP update
            if let Some(attacker_unit) = combat_info.attacker.get_value() {
                Self::update_unit_hp(world, attacker_unit);
            }

            // Process defender HP update
            if let Some(defender_unit) = combat_info.defender.get_value() {
                Self::update_unit_hp(world, defender_unit);
            }
        }
    }

    /// Helper to update a single unit's HP from CombatUnit data
    fn update_unit_hp(world: &mut World, combat_unit: &CombatUnit) {
        let unit_id = AwbwUnitId(combat_unit.units_id);

        // Get entity from StrongIdMap
        let entity = {
            let units = world.resource::<StrongIdMap<AwbwUnitId>>();
            units.get(&unit_id)
        };

        let Some(entity) = entity else {
            log::warn!(
                "Unit entity not found for ID: {}",
                combat_unit.units_id.as_u32()
            );
            return;
        };

        // Extract HP value if present
        if let Some(hp_display) = combat_unit.units_hit_points {
            let hp_value = hp_display.value();

            // Insert or update GraphicalHp component
            world.entity_mut(entity).insert(GraphicalHp(hp_value));

            log::info!(
                "Updated unit {} HP to {}",
                combat_unit.units_id.as_u32(),
                hp_value
            );
        }
    }

    /// Flips a building to a new faction after capture completion.
    fn flip_building(world: &mut World, pos: Position, faction: PlayerFaction) {
        // Get current weather for sprite lookup
        let weather = world.resource::<CurrentWeather>().weather();

        // Find and update the terrain tile at this position
        let mut query = world.query::<(Entity, &mut TerrainTile, &mut Sprite)>();
        for (_terrain_entity, mut terrain_tile, mut sprite) in query.iter_mut(world) {
            if terrain_tile.position != pos {
                continue;
            }

            // Check if this is a property that can be captured
            if let GraphicalTerrain::Property(property) = terrain_tile.terrain {
                let new_property = match property {
                    Property::City(_) => Property::City(awbrn_core::Faction::Player(faction)),
                    Property::Base(_) => Property::Base(awbrn_core::Faction::Player(faction)),
                    Property::Airport(_) => Property::Airport(awbrn_core::Faction::Player(faction)),
                    Property::Port(_) => Property::Port(awbrn_core::Faction::Player(faction)),
                    Property::ComTower(_) => {
                        Property::ComTower(awbrn_core::Faction::Player(faction))
                    }
                    Property::Lab(_) => Property::Lab(awbrn_core::Faction::Player(faction)),
                    Property::HQ(_) => Property::HQ(faction),
                };

                terrain_tile.terrain = GraphicalTerrain::Property(new_property);

                // Update sprite to show new faction
                let sprite_index = awbrn_core::spritesheet_index(weather, terrain_tile.terrain);
                if let Some(atlas) = &mut sprite.texture_atlas {
                    atlas.index = sprite_index.index() as usize;
                }

                log::info!("Captured building at {:?} flipped to {:?}", pos, faction);
                break;
            }
        }
    }
}

/// A command to reset the replay to its initial state.
///
/// This despawns all units, clears the unit ID map, resets terrain ownership,
/// and resets the replay state to turn 0/day 1.
pub struct ResetReplayCommand;

impl Command for ResetReplayCommand {
    fn apply(self, world: &mut World) {
        // 1. Despawn all unit entities
        let unit_entities: Vec<Entity> = {
            let mut query = world.query_filtered::<Entity, With<AwbwUnitId>>();
            query.iter(world).collect()
        };

        for entity in unit_entities {
            world.entity_mut(entity).despawn();
        }

        // 2. Clear StrongIdMap<AwbwUnitId>
        world.resource_mut::<StrongIdMap<AwbwUnitId>>().clear();

        // 3. Reset terrain tiles to initial ownership
        Self::reset_terrain(world);

        // 4. Reset ReplayState
        world.resource_mut::<ReplayState>().turn = 0;
        world.resource_mut::<ReplayState>().day = 1;

        // 5. Re-spawn initial units
        Self::spawn_initial_units(world);

        log::info!("Replay reset to initial state");
    }
}

impl ResetReplayCommand {
    /// Reset terrain tiles to their initial ownership based on replay buildings data.
    fn reset_terrain(world: &mut World) {
        // Build a map of initial building ownership from replay data
        let initial_buildings: std::collections::HashMap<Position, AwbwTerrain> = {
            let loaded_replay = world.resource::<LoadedReplay>();
            loaded_replay
                .0
                .games
                .first()
                .map(|game| {
                    game.buildings
                        .iter()
                        .map(|b| (Position::new(b.x as usize, b.y as usize), b.terrain_id))
                        .collect()
                })
                .unwrap_or_default()
        };

        if initial_buildings.is_empty() {
            return;
        }

        // Get current weather for sprite updates
        let weather = world.resource::<CurrentWeather>().weather();

        // Query and update terrain tiles
        let mut query = world.query::<(&mut TerrainTile, &mut Sprite)>();
        for (mut terrain_tile, mut sprite) in query.iter_mut(world) {
            // Only reset properties that have initial state in replay data
            if let Some(&AwbwTerrain::Property(property)) =
                initial_buildings.get(&terrain_tile.position)
            {
                terrain_tile.terrain = GraphicalTerrain::Property(property);

                // Update sprite to match initial ownership
                let sprite_index = awbrn_core::spritesheet_index(weather, terrain_tile.terrain);
                if let Some(atlas) = &mut sprite.texture_atlas {
                    atlas.index = sprite_index.index() as usize;
                }
            }
        }
    }

    /// Spawn initial units from replay data.
    fn spawn_initial_units(world: &mut World) {
        // Extract data needed for spawning
        let (players, replay_units) = {
            let loaded_replay = world.resource::<LoadedReplay>();
            if let Some(first_game) = loaded_replay.0.games.first() {
                (first_game.players.clone(), first_game.units.clone())
            } else {
                return;
            }
        };

        // Spawn each initial unit
        for unit in &replay_units {
            let faction = players
                .iter()
                .find(|p| p.id == unit.players_id)
                .map(|p| p.faction)
                .unwrap_or(PlayerFaction::OrangeStar);

            let mut entity = world.spawn((
                MapPosition::new(unit.x as usize, unit.y as usize),
                Faction(faction),
                AwbwUnitId(unit.id),
                Unit(unit.name),
            ));

            // Handle units that start carried (in transports)
            if unit.carried {
                entity.insert(Visibility::Hidden);
            }

            // Handle units with initial damage
            let hp_value = unit.hit_points.round() as u8;
            if hp_value < 10 {
                entity.insert(GraphicalHp(hp_value));
            }
        }

        // Set up cargo relationships for transports
        for unit in &replay_units {
            let cargo1_id = unit.cargo1_units_id;
            let cargo2_id = unit.cargo2_units_id;

            // Check if this unit has cargo (non-zero cargo IDs indicate cargo)
            if cargo1_id.as_u32() == 0 && cargo2_id.as_u32() == 0 {
                continue;
            }

            // Get the transport entity
            let transport_entity = {
                let units = world.resource::<StrongIdMap<AwbwUnitId>>();
                units.get(&AwbwUnitId(unit.id))
            };

            let Some(transport_entity) = transport_entity else {
                continue;
            };

            // Build cargo component
            let mut has_cargo = HasCargo::new();
            if cargo1_id.as_u32() != 0 {
                has_cargo.add_cargo(AwbwUnitId(cargo1_id));
            }
            if cargo2_id.as_u32() != 0 {
                has_cargo.add_cargo(AwbwUnitId(cargo2_id));
            }

            if !has_cargo.is_empty() {
                world.entity_mut(transport_entity).insert(has_cargo);
            }
        }
    }
}

/// A command to undo the last replay turn.
///
/// This resets to the initial state and replays all turns up to (but not including)
/// the current turn.
pub struct UndoTurnCommand;

impl Command for UndoTurnCommand {
    fn apply(self, world: &mut World) {
        let current_turn = world.resource::<ReplayState>().turn;

        // Nothing to undo at turn 0
        if current_turn == 0 {
            log::info!("Already at the beginning of the replay");
            return;
        }

        let target_turn = current_turn - 1;

        // Clone turns before reset (to avoid borrow issues)
        let turns = world.resource::<LoadedReplay>().0.turns.clone();

        // Reset to initial state
        ResetReplayCommand.apply(world);

        // Replay all turns from 0 to target_turn-1
        for turn_idx in 0..target_turn as usize {
            if let Some(action) = turns.get(turn_idx) {
                ReplayTurnCommand {
                    action: action.clone(),
                }
                .apply(world);
            }
        }

        // Update turn counter (ResetReplayCommand sets it to 0, we need target_turn)
        world.resource_mut::<ReplayState>().turn = target_turn;

        log::info!("Undid turn, now at turn {}", target_turn);
    }
}
