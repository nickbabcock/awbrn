use std::collections::HashSet;

use bevy::ecs::system::{SystemParam, SystemState};
use bevy::prelude::*;

use crate::command::{GameCommand, PostMoveAction};
use crate::error::CommandError;
use crate::player::{PlayerId, PlayerRegistry};
use crate::state::{ServerGameState, TurnPhase};
use crate::unit_id::ServerUnitId;
use awbrn_game::MapPosition;
use awbrn_game::replay::PowerMovementBoosts;
use awbrn_game::world::{Ammo, BoardIndex, Faction, Fuel, GameMap, StrongIdMap, Unit, UnitActive};
use awbrn_map::Position;
use awbrn_types::{
    Faction as TerrainFaction, GraphicalTerrain, MovementCost, MovementTerrain, PlayerFaction,
};

#[derive(SystemParam)]
struct MovementValidationWorld<'w, 's> {
    game_map: Res<'w, GameMap>,
    board_index: Res<'w, BoardIndex>,
    power_movement_boosts: Res<'w, PowerMovementBoosts>,
    factions: Query<'w, 's, &'static Faction>,
}

impl<'w, 's> MovementValidationWorld<'w, 's> {
    fn movement_budget_for(&self, faction: PlayerFaction, unit: awbrn_types::Unit) -> u32 {
        let power_boost = self
            .power_movement_boosts
            .0
            .get(&faction)
            .copied()
            .unwrap_or_default();
        (i32::from(unit.movement_range()) + power_boost).max(0) as u32
    }
}

/// Validate a command before applying it. Returns `Ok(())` if the command is legal.
pub(crate) fn validate_command(
    world: &mut World,
    player: PlayerId,
    command: &GameCommand,
) -> Result<(), CommandError> {
    let game_state = world.resource::<ServerGameState>();

    // Check game is still active.
    if matches!(game_state.phase, TurnPhase::GameOver { .. }) {
        return Err(CommandError::GameOver);
    }

    // Check it's this player's turn.
    if game_state.active_player != player {
        return Err(CommandError::NotYourTurn);
    }

    match command {
        GameCommand::MoveUnit {
            unit_id,
            path,
            action,
        } => validate_move_unit(world, player, *unit_id, path, action.as_ref()),
        GameCommand::Build { .. } => {
            // Build validation will be added in a future phase.
            Err(CommandError::InvalidAction {
                reason: "build not yet implemented".into(),
            })
        }
        GameCommand::EndTurn => Ok(()),
    }
}

fn validate_move_unit(
    world: &mut World,
    player: PlayerId,
    unit_id: ServerUnitId,
    path: &[awbrn_map::Position],
    action: Option<&PostMoveAction>,
) -> Result<(), CommandError> {
    let (entity, player_faction, friendly_factions) = {
        let registry = world.resource::<PlayerRegistry>();
        let units = world.resource::<StrongIdMap<ServerUnitId>>();

        let entity = units
            .get(&unit_id)
            .ok_or(CommandError::InvalidUnit(unit_id))?;
        let player_faction = registry
            .faction_for_player(player)
            .ok_or(CommandError::InvalidUnit(unit_id))?;
        let friendly_factions = registry.friendly_factions_for_player(player);
        (entity, player_faction, friendly_factions)
    };

    // Check unit is owned by this player.
    let faction = world
        .entity(entity)
        .get::<Faction>()
        .copied()
        .ok_or(CommandError::InvalidUnit(unit_id))?;
    if faction.0 != player_faction {
        return Err(CommandError::InvalidUnit(unit_id));
    }

    // Check unit hasn't already acted.
    if !world.entity(entity).contains::<UnitActive>() {
        return Err(CommandError::UnitAlreadyActed(unit_id));
    }

    // Path must have at least the destination.
    if path.is_empty() {
        return Err(CommandError::InvalidPath {
            reason: "path is empty".into(),
        });
    }

    // Verify the path starts at the unit's current position.
    let current_position = world
        .entity(entity)
        .get::<MapPosition>()
        .map(MapPosition::position)
        .ok_or(CommandError::InvalidUnit(unit_id))?;
    if path[0] != current_position {
        return Err(invalid_path(
            "path does not start at unit's current position",
        ));
    }

    let unit = world
        .entity(entity)
        .get::<Unit>()
        .copied()
        .ok_or(CommandError::InvalidUnit(unit_id))?;
    let mut movement_world_state: SystemState<MovementValidationWorld> = SystemState::new(world);
    let movement_world = movement_world_state.get(world);
    let movement_budget = movement_world.movement_budget_for(faction.0, unit.0);
    let fuel_budget = world
        .entity(entity)
        .get::<Fuel>()
        .map_or(u32::MAX, |fuel| fuel.0);

    let mut movement_cost = 0u32;
    let mut fuel_cost = 0u32;
    let mut previous = current_position;
    let destination = *path.last().expect("validated path is non-empty");

    for &step in &path[1..] {
        if previous.manhattan(&step) != 1 {
            return Err(invalid_path(format!(
                "path step {previous:?} -> {step:?} is not adjacent"
            )));
        }

        let Some(terrain) = movement_world.game_map.terrain_at(step) else {
            return Err(invalid_path(format!(
                "path step {step:?} is outside the map bounds"
            )));
        };
        let step_cost = step_movement_cost(unit.0, terrain, step)?;
        movement_cost += u32::from(step_cost);
        fuel_cost += 1;

        let occupied_by = movement_world
            .board_index
            .unit_entity(step)
            .map_err(|_| invalid_path(format!("path step {step:?} is outside the map bounds")))?;
        if let Some(occupant) = occupied_by
            && occupant != entity
        {
            if step == destination {
                return Err(invalid_path(format!(
                    "path destination {step:?} is occupied"
                )));
            }

            let occupant_faction = movement_world
                .factions
                .get(occupant)
                .map_err(|_| invalid_path(format!("occupant at {step:?} is missing faction")))?;

            // Advance Wars movement allows traversing friendly/allied units,
            // but any enemy unit blocks the path and no move may end on an
            // occupied tile.
            if !friendly_factions.contains(&occupant_faction.0) {
                return Err(invalid_path(format!(
                    "path step {step:?} is blocked by an enemy unit"
                )));
            }
        }

        previous = step;
    }

    if movement_cost > movement_budget {
        return Err(invalid_path(format!(
            "path costs {movement_cost} movement but unit only has {movement_budget}"
        )));
    }

    if fuel_cost > fuel_budget {
        return Err(invalid_path(format!(
            "path consumes {fuel_cost} fuel but unit only has {fuel_budget}"
        )));
    }

    // Validate that the action is consistent (basic checks).
    if let Some(action) = action {
        validate_post_move_action(
            world,
            entity,
            current_position,
            destination,
            &friendly_factions,
            action,
        )?;
    }

    Ok(())
}

fn step_movement_cost(
    unit: awbrn_types::Unit,
    terrain: GraphicalTerrain,
    position: Position,
) -> Result<u8, CommandError> {
    let movement_terrain = MovementTerrain::from(terrain);
    MovementCost::from_terrain(&movement_terrain)
        .cost(unit.movement_type())
        .ok_or_else(|| {
            invalid_path(format!(
                "{:?} cannot move onto {:?} at {position:?}",
                unit, terrain
            ))
        })
}

fn invalid_path(reason: impl Into<String>) -> CommandError {
    CommandError::InvalidPath {
        reason: reason.into(),
    }
}

fn validate_post_move_action(
    world: &World,
    entity: Entity,
    from: Position,
    destination: Position,
    friendly_factions: &HashSet<PlayerFaction>,
    action: &PostMoveAction,
) -> Result<(), CommandError> {
    match action {
        PostMoveAction::Wait => Ok(()),
        PostMoveAction::Attack { target } => {
            validate_attack(world, entity, from, destination, friendly_factions, *target)
        }
        PostMoveAction::Capture => validate_capture(world, entity, destination, friendly_factions),
        PostMoveAction::Load { .. }
        | PostMoveAction::Unload { .. }
        | PostMoveAction::Supply
        | PostMoveAction::Hide
        | PostMoveAction::Unhide
        | PostMoveAction::Join { .. } => Err(CommandError::InvalidAction {
            reason: format!("action {action:?} not yet implemented"),
        }),
    }
}

fn validate_capture(
    world: &World,
    entity: Entity,
    destination: Position,
    friendly_factions: &HashSet<PlayerFaction>,
) -> Result<(), CommandError> {
    let unit = world
        .entity(entity)
        .get::<Unit>()
        .copied()
        .expect("validated unit must have Unit component");

    if !matches!(
        unit.0,
        awbrn_types::Unit::Infantry | awbrn_types::Unit::Mech
    ) {
        return Err(CommandError::InvalidAction {
            reason: "only infantry and mech units can capture".into(),
        });
    }

    let terrain = world
        .resource::<GameMap>()
        .terrain_at(destination)
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "capture destination is outside the map".into(),
        })?;

    let GraphicalTerrain::Property(property) = terrain else {
        return Err(CommandError::InvalidAction {
            reason: "capture destination is not a property".into(),
        });
    };

    match property.faction() {
        TerrainFaction::Neutral => Ok(()),
        TerrainFaction::Player(faction) if !friendly_factions.contains(&faction) => Ok(()),
        TerrainFaction::Player(_) => Err(CommandError::InvalidAction {
            reason: "cannot capture a friendly property".into(),
        }),
    }
}

fn validate_attack(
    world: &World,
    attacker_entity: Entity,
    from: Position,
    destination: Position,
    friendly_factions: &HashSet<PlayerFaction>,
    target: Position,
) -> Result<(), CommandError> {
    let unit = world
        .entity(attacker_entity)
        .get::<Unit>()
        .copied()
        .expect("validated unit must have Unit component");

    // Indirect units cannot attack after moving.
    if unit.0.is_indirect() && from != destination {
        return Err(CommandError::InvalidAction {
            reason: "indirect units cannot attack after moving".into(),
        });
    }

    // Target must be within attack range.
    let dist = destination.manhattan(&target);
    if dist < unit.0.attack_range_min() as usize || dist > unit.0.attack_range_max() as usize {
        return Err(CommandError::InvalidAction {
            reason: format!(
                "target is out of range (distance {dist}, range {}-{})",
                unit.0.attack_range_min(),
                unit.0.attack_range_max()
            ),
        });
    }

    // Target tile must be occupied by a unit.
    let board_index = world.resource::<BoardIndex>();
    let defender_entity = board_index
        .unit_entity(target)
        .ok()
        .flatten()
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "no unit at target position".into(),
        })?;

    // Target must be an enemy (not allied).
    let defender_faction = world
        .entity(defender_entity)
        .get::<Faction>()
        .copied()
        .expect("defender must have Faction component");
    if friendly_factions.contains(&defender_faction.0) {
        return Err(CommandError::InvalidAction {
            reason: "cannot attack a friendly unit".into(),
        });
    }

    // Attacker must have a weapon effective against the defender unit type.
    let defender_unit = world
        .entity(defender_entity)
        .get::<Unit>()
        .copied()
        .expect("defender must have Unit component");
    let attacker_ammo = world
        .entity(attacker_entity)
        .get::<Ammo>()
        .map(|a| a.0)
        .unwrap_or(0);
    if crate::damage::base_damage(unit.0, defender_unit.0, attacker_ammo).is_none() {
        return Err(CommandError::InvalidAction {
            reason: "attacker has no weapon effective against this unit type".into(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::GameCommand;
    use crate::setup::{GameSetup, PlayerSetup, initialize_server_world};
    use awbrn_game::world::{Ammo, UnitHp, VisionRange};
    use awbrn_map::AwbrnMap;
    use awbrn_types::{Co, GraphicalTerrain, PlayerFaction};
    use std::num::NonZeroU8;

    fn test_world(map: AwbrnMap) -> World {
        test_world_with_players(
            map,
            vec![PlayerSetup {
                faction: PlayerFaction::OrangeStar,
                team: None,
                starting_funds: 1000,
                co: Co::Andy,
            }],
        )
    }

    fn test_world_with_players(map: AwbrnMap, players: Vec<PlayerSetup>) -> World {
        initialize_server_world(GameSetup {
            map,
            players,
            fog_enabled: false,
            rng_seed: 0,
        })
        .unwrap()
    }

    fn spawn_unit(
        world: &mut World,
        raw_id: u32,
        position: Position,
        unit_type: awbrn_types::Unit,
        faction: PlayerFaction,
        fuel: u32,
    ) -> ServerUnitId {
        let unit_id = ServerUnitId(raw_id.into());
        world.spawn((
            MapPosition::from(position),
            Unit(unit_type),
            Faction(faction),
            UnitHp(awbrn_types::ExactHp::new(100)),
            Fuel(fuel),
            Ammo(unit_type.max_ammo()),
            VisionRange(unit_type.base_vision()),
            UnitActive,
            unit_id,
        ));
        unit_id
    }

    fn move_command(unit_id: ServerUnitId, path: Vec<Position>) -> GameCommand {
        GameCommand::MoveUnit {
            unit_id,
            path,
            action: Some(PostMoveAction::Wait),
        }
    }

    #[test]
    fn rejects_non_adjacent_path_steps() {
        let mut world = test_world(AwbrnMap::new(5, 1, GraphicalTerrain::Plain));
        let unit_id = spawn_unit(
            &mut world,
            1,
            Position::new(0, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            99,
        );

        let err = validate_command(
            &mut world,
            PlayerId(0),
            &move_command(unit_id, vec![Position::new(0, 0), Position::new(2, 0)]),
        )
        .unwrap_err();

        assert!(matches!(err, CommandError::InvalidPath { .. }));
    }

    #[test]
    fn rejects_paths_ending_on_friendly_occupied_tiles() {
        let mut world = test_world(AwbrnMap::new(5, 1, GraphicalTerrain::Plain));
        let unit_id = spawn_unit(
            &mut world,
            1,
            Position::new(0, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            99,
        );
        spawn_unit(
            &mut world,
            2,
            Position::new(1, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            99,
        );

        let err = validate_command(
            &mut world,
            PlayerId(0),
            &move_command(unit_id, vec![Position::new(0, 0), Position::new(1, 0)]),
        )
        .unwrap_err();

        assert!(matches!(err, CommandError::InvalidPath { .. }));
    }

    #[test]
    fn rejects_paths_through_enemy_occupied_tiles() {
        let mut world = test_world_with_players(
            AwbrnMap::new(5, 1, GraphicalTerrain::Plain),
            vec![
                PlayerSetup {
                    faction: PlayerFaction::OrangeStar,
                    team: None,
                    starting_funds: 1000,
                    co: Co::Andy,
                },
                PlayerSetup {
                    faction: PlayerFaction::BlueMoon,
                    team: None,
                    starting_funds: 1000,
                    co: Co::Andy,
                },
            ],
        );
        let unit_id = spawn_unit(
            &mut world,
            1,
            Position::new(0, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            99,
        );
        spawn_unit(
            &mut world,
            2,
            Position::new(1, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
            99,
        );

        let err = validate_command(
            &mut world,
            PlayerId(0),
            &move_command(
                unit_id,
                vec![
                    Position::new(0, 0),
                    Position::new(1, 0),
                    Position::new(2, 0),
                ],
            ),
        )
        .unwrap_err();

        assert!(matches!(err, CommandError::InvalidPath { .. }));
    }

    #[test]
    fn allows_paths_through_allied_units() {
        let mut world = test_world_with_players(
            AwbrnMap::new(5, 1, GraphicalTerrain::Plain),
            vec![
                PlayerSetup {
                    faction: PlayerFaction::OrangeStar,
                    team: Some(NonZeroU8::new(1).unwrap()),
                    starting_funds: 1000,
                    co: Co::Andy,
                },
                PlayerSetup {
                    faction: PlayerFaction::BlueMoon,
                    team: Some(NonZeroU8::new(1).unwrap()),
                    starting_funds: 1000,
                    co: Co::Andy,
                },
            ],
        );
        let unit_id = spawn_unit(
            &mut world,
            1,
            Position::new(0, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            99,
        );
        spawn_unit(
            &mut world,
            2,
            Position::new(1, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
            99,
        );

        let result = validate_command(
            &mut world,
            PlayerId(0),
            &move_command(
                unit_id,
                vec![
                    Position::new(0, 0),
                    Position::new(1, 0),
                    Position::new(2, 0),
                ],
            ),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_impassable_terrain() {
        let mut map = AwbrnMap::new(3, 1, GraphicalTerrain::Plain);
        map.set_terrain(Position::new(1, 0), GraphicalTerrain::Mountain);
        let mut world = test_world(map);
        let unit_id = spawn_unit(
            &mut world,
            1,
            Position::new(0, 0),
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
            70,
        );

        let err = validate_command(
            &mut world,
            PlayerId(0),
            &move_command(unit_id, vec![Position::new(0, 0), Position::new(1, 0)]),
        )
        .unwrap_err();

        assert!(matches!(err, CommandError::InvalidPath { .. }));
    }

    #[test]
    fn rejects_paths_exceeding_movement_budget() {
        let mut map = AwbrnMap::new(3, 1, GraphicalTerrain::Plain);
        map.set_terrain(Position::new(1, 0), GraphicalTerrain::Mountain);
        map.set_terrain(Position::new(2, 0), GraphicalTerrain::Mountain);
        let mut world = test_world(map);
        let unit_id = spawn_unit(
            &mut world,
            1,
            Position::new(0, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            99,
        );

        let err = validate_command(
            &mut world,
            PlayerId(0),
            &move_command(
                unit_id,
                vec![
                    Position::new(0, 0),
                    Position::new(1, 0),
                    Position::new(2, 0),
                ],
            ),
        )
        .unwrap_err();

        assert!(matches!(err, CommandError::InvalidPath { .. }));
    }

    #[test]
    fn rejects_paths_exceeding_fuel_budget() {
        let mut world = test_world(AwbrnMap::new(3, 1, GraphicalTerrain::Plain));
        let unit_id = spawn_unit(
            &mut world,
            1,
            Position::new(0, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            1,
        );

        let err = validate_command(
            &mut world,
            PlayerId(0),
            &move_command(
                unit_id,
                vec![
                    Position::new(0, 0),
                    Position::new(1, 0),
                    Position::new(2, 0),
                ],
            ),
        )
        .unwrap_err();

        assert!(matches!(err, CommandError::InvalidPath { .. }));
    }

    #[test]
    fn power_movement_boosts_extend_movement_budget() {
        let mut map = AwbrnMap::new(3, 1, GraphicalTerrain::Plain);
        map.set_terrain(Position::new(1, 0), GraphicalTerrain::Mountain);
        map.set_terrain(Position::new(2, 0), GraphicalTerrain::Mountain);
        let mut world = test_world(map);
        let unit_id = spawn_unit(
            &mut world,
            1,
            Position::new(0, 0),
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
            99,
        );
        world
            .resource_mut::<PowerMovementBoosts>()
            .0
            .insert(PlayerFaction::OrangeStar, 1);

        let result = validate_command(
            &mut world,
            PlayerId(0),
            &move_command(
                unit_id,
                vec![
                    Position::new(0, 0),
                    Position::new(1, 0),
                    Position::new(2, 0),
                ],
            ),
        );

        assert!(result.is_ok());
    }
}
