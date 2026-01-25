//! Replay command system for processing AWBW replay actions.
//!
//! Uses a custom Bevy Command to get direct `&mut World` access, enabling
//! immediate mutations that are visible to subsequent queries within the same
//! command execution.

use awbrn_core::{GraphicalTerrain, PlayerFaction, Property};
use awbrn_map::Position;
use awbw_replay::turn_models::{
    Action, CaptureAction, CombatUnit, FireAction, LoadAction, MoveAction, UnitMap, UpdatedInfo,
};
use bevy::log;
use bevy::prelude::*;

use crate::{
    AwbwUnitId, Capturing, CurrentWeather, Faction, GraphicalHp, HasCargo, LoadedReplay,
    MapPosition, StrongIdMap, TerrainTile, Unit, UnitActive,
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

            // Mark unit as inactive (has acted this turn)
            world.entity_mut(entity).remove::<UnitActive>();
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

            let unit_name = format!(
                "{} - {} - {}",
                faction.country_code(),
                unit.units_name.name(),
                unit.units_id.as_u32()
            );

            world.spawn((
                Name::new(unit_name),
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

    /// Processes an End turn action, updating the day counter, and re-activating units
    fn apply_end(updated_info: &UpdatedInfo, world: &mut World) {
        let current_day = {
            let replay_state = world.resource::<ReplayState>();
            replay_state.day
        };

        if updated_info.day != current_day {
            world.resource_mut::<ReplayState>().day = updated_info.day;
        }

        Self::activate_all_units(world);
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
        let mut attacker_entity = None;

        // Iterate over all players' combat vision
        for (_player, combat_vision) in fire_action.combat_info_vision.iter() {
            let combat_info = &combat_vision.combat_info;

            // Process attacker HP update and track the entity
            if let Some(attacker_unit) = combat_info.attacker.get_value() {
                let entity = Self::update_unit_hp(world, attacker_unit);
                if attacker_entity.is_none() {
                    attacker_entity = entity;
                }
            }

            // Process defender HP update
            if let Some(defender_unit) = combat_info.defender.get_value() {
                Self::update_unit_hp(world, defender_unit);
            }
        }

        // Mark attacker as inactive (has acted this turn)
        if let Some(entity) = attacker_entity {
            world.entity_mut(entity).remove::<UnitActive>();
        }
    }

    /// Helper to update a single unit's HP from CombatUnit data.
    /// Returns the entity if found, None otherwise.
    fn update_unit_hp(world: &mut World, combat_unit: &CombatUnit) -> Option<Entity> {
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
            return None;
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

        Some(entity)
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

    /// Activates all units by adding UnitActive component.
    /// Called at the start of a new turn.
    fn activate_all_units(world: &mut World) {
        let unit_entities: Vec<Entity> = {
            let mut query = world.query_filtered::<Entity, With<Unit>>();
            query.iter(world).collect()
        };

        for entity in &unit_entities {
            world.entity_mut(*entity).insert(UnitActive);
        }

        if !unit_entities.is_empty() {
            log::info!("Activated {} units for new turn", unit_entities.len());
        }
    }
}
