//! Replay command system for processing AWBW replay actions.
//!
//! Uses a custom Bevy Command to get direct `&mut World` access, enabling
//! immediate mutations that are visible to subsequent queries within the same
//! command execution.

use awbrn_core::{GraphicalTerrain, PlayerFaction, Property};
use awbrn_map::Position;
use awbw_replay::turn_models::{Action, CaptureAction, MoveAction, UnitMap, UpdatedInfo};
use bevy::log;
use bevy::prelude::*;

use crate::{
    AwbwUnitId, Capturing, CurrentWeather, Faction, LoadedReplay, MapPosition, StrongIdMap,
    TerrainTile, Unit,
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
            Action::End { updated_info } => Self::apply_end(updated_info, world),
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
