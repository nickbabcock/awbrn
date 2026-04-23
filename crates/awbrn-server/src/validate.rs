use std::collections::HashSet;

use bevy::ecs::system::{SystemParam, SystemState};
use bevy::prelude::*;

use crate::adjacency::adjacent_self_owned_units;
use crate::command::{GameCommand, PostMoveAction};
use crate::error::CommandError;
use crate::player::{PlayerId, PlayerRegistry};
use crate::state::{ServerGameState, TurnPhase};
use crate::unit_id::ServerUnitId;
use awbrn_game::MapPosition;
use awbrn_game::replay::PowerMovementBoosts;
use awbrn_game::world::{
    Ammo, BoardIndex, CarriedBy, Faction, Fuel, GameMap, HasCargo, Hiding, StrongIdMap, Unit,
    UnitActive,
};
use awbrn_map::Position;
use awbrn_types::{
    Faction as TerrainFaction, GraphicalTerrain, MovementCost, MovementTerrain, PlayerFaction,
    PropertyKind, UnitDomain,
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
        GameCommand::Build {
            position,
            unit_type,
        } => validate_build(world, player, *position, *unit_type),
        GameCommand::EndTurn => Ok(()),
    }
}

fn validate_build(
    world: &World,
    player: PlayerId,
    position: Position,
    unit_type: awbrn_types::Unit,
) -> Result<(), CommandError> {
    let player_faction = world
        .resource::<PlayerRegistry>()
        .faction_for_player(player)
        .ok_or(CommandError::InvalidBuildLocation)?;

    let terrain = world
        .resource::<GameMap>()
        .terrain_at(position)
        .ok_or(CommandError::InvalidBuildLocation)?;

    let GraphicalTerrain::Property(property) = terrain else {
        return Err(CommandError::InvalidBuildLocation);
    };

    if property.faction() != TerrainFaction::Player(player_faction) {
        return Err(CommandError::InvalidBuildLocation);
    }

    let required_domain = match property.kind() {
        PropertyKind::Base => UnitDomain::Ground,
        PropertyKind::Airport => UnitDomain::Air,
        PropertyKind::Port => UnitDomain::Sea,
        _ => return Err(CommandError::InvalidBuildLocation),
    };

    if unit_type.domain() != required_domain {
        return Err(CommandError::InvalidBuildLocation);
    }

    let occupied = world
        .resource::<BoardIndex>()
        .unit_entity(position)
        .map_err(|_| CommandError::InvalidBuildLocation)?;
    if occupied.is_some() {
        return Err(CommandError::InvalidBuildLocation);
    }

    let cost = unit_type.base_cost();
    let available = world
        .resource::<PlayerRegistry>()
        .get(player)
        .map(|slot| slot.funds)
        .ok_or(CommandError::InvalidBuildLocation)?;
    if available < cost {
        return Err(CommandError::InsufficientFunds { cost, available });
    }

    Ok(())
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
    let fuel_budget = world
        .entity(entity)
        .get::<Fuel>()
        .map_or(u32::MAX, |fuel| fuel.0);

    let destination = *path.last().expect("validated path is non-empty");
    let mut movement_cost = 0u32;
    let mut fuel_cost = 0u32;
    let mut previous = current_position;

    {
        let mut movement_world_state: SystemState<MovementValidationWorld> =
            SystemState::new(world);
        let movement_world = movement_world_state.get(world);
        let movement_budget = movement_world.movement_budget_for(faction.0, unit.0);

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

            let occupied_by = movement_world.board_index.unit_entity(step).map_err(|_| {
                invalid_path(format!("path step {step:?} is outside the map bounds"))
            })?;
            if let Some(occupant) = occupied_by
                && occupant != entity
            {
                if step == destination {
                    if !action_allows_occupied_destination(action) {
                        return Err(invalid_path(format!(
                            "path destination {step:?} is occupied"
                        )));
                    }
                } else {
                    let occupant_faction = movement_world.factions.get(occupant).map_err(|_| {
                        invalid_path(format!("occupant at {step:?} is missing faction"))
                    })?;

                    // Advance Wars movement allows traversing friendly/allied units,
                    // but any enemy unit blocks the path and no move may end on an
                    // occupied tile except for validated Load/Join actions.
                    if !friendly_factions.contains(&occupant_faction.0) {
                        return Err(invalid_path(format!(
                            "path step {step:?} is blocked by an enemy unit"
                        )));
                    }
                }
            }

            previous = step;
        }

        if movement_cost > movement_budget {
            return Err(invalid_path(format!(
                "path costs {movement_cost} movement but unit only has {movement_budget}"
            )));
        }
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
            player_faction,
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
    world: &mut World,
    entity: Entity,
    from: Position,
    destination: Position,
    player_faction: PlayerFaction,
    friendly_factions: &HashSet<PlayerFaction>,
    action: &PostMoveAction,
) -> Result<(), CommandError> {
    match action {
        PostMoveAction::Wait => Ok(()),
        PostMoveAction::Attack { target } => {
            validate_attack(world, entity, from, destination, friendly_factions, *target)
        }
        PostMoveAction::Capture => validate_capture(world, entity, destination, friendly_factions),
        PostMoveAction::Load { transport_id } => validate_load(
            world,
            entity,
            destination,
            player_faction,
            friendly_factions,
            *transport_id,
        ),
        PostMoveAction::Unload { cargo_id, position } => {
            validate_unload(world, entity, from, destination, *cargo_id, *position)
        }
        PostMoveAction::Supply => validate_supply(world, entity, player_faction, destination),
        PostMoveAction::Hide => validate_hide(world, entity),
        PostMoveAction::Unhide => validate_unhide(world, entity),
        PostMoveAction::Join { target_id } => {
            validate_join(world, entity, destination, player_faction, *target_id)
        }
    }
}

fn action_allows_occupied_destination(action: Option<&PostMoveAction>) -> bool {
    matches!(
        action,
        Some(PostMoveAction::Load { .. } | PostMoveAction::Join { .. })
    )
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

fn validate_supply(
    world: &World,
    supplier_entity: Entity,
    player_faction: PlayerFaction,
    destination: Position,
) -> Result<(), CommandError> {
    let supplier_unit = world
        .entity(supplier_entity)
        .get::<Unit>()
        .copied()
        .expect("validated unit must have Unit component")
        .0;

    if !matches!(
        supplier_unit,
        awbrn_types::Unit::APC | awbrn_types::Unit::BlackBoat
    ) {
        return Err(CommandError::InvalidAction {
            reason: "only APC and Black Boat units can supply".into(),
        });
    }

    if adjacent_self_owned_units(world, destination, player_faction)
        .next()
        .is_none()
    {
        return Err(CommandError::InvalidAction {
            reason: "no adjacent self-owned units to supply".into(),
        });
    }

    Ok(())
}

fn validate_load(
    world: &mut World,
    cargo_entity: Entity,
    destination: Position,
    player_faction: PlayerFaction,
    friendly_factions: &HashSet<PlayerFaction>,
    transport_id: ServerUnitId,
) -> Result<(), CommandError> {
    let transport_entity = world
        .resource::<StrongIdMap<ServerUnitId>>()
        .get(&transport_id)
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "transport unit does not exist".into(),
        })?;

    if cargo_entity == transport_entity {
        return Err(CommandError::InvalidAction {
            reason: "unit cannot load into itself".into(),
        });
    }

    let occupied = world
        .resource::<BoardIndex>()
        .unit_entity(destination)
        .map_err(|_| CommandError::InvalidAction {
            reason: "load destination is outside the map".into(),
        })?;
    if occupied != Some(transport_entity) {
        return Err(CommandError::InvalidAction {
            reason: "selected transport is not at the load destination".into(),
        });
    }

    let cargo = world.entity(cargo_entity);
    if cargo.contains::<CarriedBy>() {
        return Err(CommandError::InvalidAction {
            reason: "cargo unit is already loaded".into(),
        });
    }
    let cargo_unit = cargo
        .get::<Unit>()
        .copied()
        .expect("validated cargo must have Unit component")
        .0;
    let cargo_faction = cargo
        .get::<Faction>()
        .copied()
        .expect("validated cargo must have Faction component")
        .0;

    let transport = world.entity(transport_entity);
    let transport_unit = transport
        .get::<Unit>()
        .copied()
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "transport entity is missing unit data".into(),
        })?
        .0;
    let transport_faction = transport
        .get::<Faction>()
        .copied()
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "transport entity is missing faction".into(),
        })?
        .0;

    if cargo_faction != player_faction || !friendly_factions.contains(&transport_faction) {
        return Err(CommandError::InvalidAction {
            reason: "cargo and transport must be friendly units".into(),
        });
    }

    if !can_transport(transport_unit, cargo_unit) {
        return Err(CommandError::InvalidAction {
            reason: format!(
                "{} cannot transport {}",
                transport_unit.name(),
                cargo_unit.name()
            ),
        });
    }

    let capacity =
        transport_capacity(transport_unit).ok_or_else(|| CommandError::InvalidAction {
            reason: "selected unit is not a transport".into(),
        })?;
    if transport
        .get::<HasCargo>()
        .is_some_and(|cargo| cargo.len() >= capacity)
    {
        return Err(CommandError::InvalidAction {
            reason: "transport is full".into(),
        });
    }

    Ok(())
}

fn validate_unload(
    world: &World,
    transport_entity: Entity,
    from: Position,
    destination: Position,
    cargo_id: ServerUnitId,
    target: Position,
) -> Result<(), CommandError> {
    let transport_unit = world
        .entity(transport_entity)
        .get::<Unit>()
        .copied()
        .expect("validated transport must have Unit component")
        .0;

    if transport_capacity(transport_unit).is_none() {
        return Err(CommandError::InvalidAction {
            reason: "acting unit is not a transport".into(),
        });
    }

    let cargo_entity = world
        .resource::<StrongIdMap<ServerUnitId>>()
        .get(&cargo_id)
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "cargo unit does not exist".into(),
        })?;
    let carried_by = world
        .entity(cargo_entity)
        .get::<CarriedBy>()
        .copied()
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "cargo unit is not loaded".into(),
        })?;
    if carried_by.0 != transport_entity {
        return Err(CommandError::InvalidAction {
            reason: "cargo unit is not carried by this transport".into(),
        });
    }

    if destination.manhattan(&target) != 1 {
        return Err(CommandError::InvalidAction {
            reason: "unload target is not adjacent to the transport".into(),
        });
    }

    let cargo_unit = world
        .entity(cargo_entity)
        .get::<Unit>()
        .copied()
        .expect("cargo must have Unit component")
        .0;
    let terrain = world
        .resource::<GameMap>()
        .terrain_at(target)
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "unload target is outside the map".into(),
        })?;
    step_movement_cost(cargo_unit, terrain, target)?;

    let occupied = world
        .resource::<BoardIndex>()
        .unit_entity(target)
        .map_err(|_| CommandError::InvalidAction {
            reason: "unload target is outside the map".into(),
        })?;
    let transport_leaves_target = target == from && from != destination;
    if occupied.is_some() && !(occupied == Some(transport_entity) && transport_leaves_target) {
        return Err(CommandError::InvalidAction {
            reason: "unload target is occupied".into(),
        });
    }

    Ok(())
}

fn validate_hide(world: &World, entity: Entity) -> Result<(), CommandError> {
    let unit = world
        .entity(entity)
        .get::<Unit>()
        .copied()
        .expect("validated unit must have Unit component")
        .0;

    if !matches!(unit, awbrn_types::Unit::Sub | awbrn_types::Unit::Stealth) {
        return Err(CommandError::InvalidAction {
            reason: "only Sub and Stealth units can hide".into(),
        });
    }

    if world.entity(entity).contains::<Hiding>() {
        return Err(CommandError::InvalidAction {
            reason: "unit is already hidden".into(),
        });
    }

    Ok(())
}

fn validate_unhide(world: &World, entity: Entity) -> Result<(), CommandError> {
    let unit = world
        .entity(entity)
        .get::<Unit>()
        .copied()
        .expect("validated unit must have Unit component")
        .0;

    if !matches!(unit, awbrn_types::Unit::Sub | awbrn_types::Unit::Stealth) {
        return Err(CommandError::InvalidAction {
            reason: "only Sub and Stealth units can unhide".into(),
        });
    }

    if !world.entity(entity).contains::<Hiding>() {
        return Err(CommandError::InvalidAction {
            reason: "unit is not hidden".into(),
        });
    }

    Ok(())
}

fn validate_join(
    world: &World,
    source_entity: Entity,
    destination: Position,
    player_faction: PlayerFaction,
    target_id: ServerUnitId,
) -> Result<(), CommandError> {
    let target_entity = world
        .resource::<StrongIdMap<ServerUnitId>>()
        .get(&target_id)
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "join target does not exist".into(),
        })?;

    if source_entity == target_entity {
        return Err(CommandError::InvalidAction {
            reason: "unit cannot join into itself".into(),
        });
    }

    let occupied = world
        .resource::<BoardIndex>()
        .unit_entity(destination)
        .map_err(|_| CommandError::InvalidAction {
            reason: "join destination is outside the map".into(),
        })?;
    if occupied != Some(target_entity) {
        return Err(CommandError::InvalidAction {
            reason: "join target is not at the destination".into(),
        });
    }

    let source = world.entity(source_entity);
    let target = world.entity(target_entity);

    if source.contains::<CarriedBy>() || target.contains::<CarriedBy>() {
        return Err(CommandError::InvalidAction {
            reason: "carried units cannot join".into(),
        });
    }
    if source.contains::<HasCargo>() || target.contains::<HasCargo>() {
        return Err(CommandError::InvalidAction {
            reason: "transport carrying cargo cannot join".into(),
        });
    }

    let source_unit = source
        .get::<Unit>()
        .copied()
        .expect("source must have Unit component")
        .0;
    let target_unit = target
        .get::<Unit>()
        .copied()
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "join target is missing unit data".into(),
        })?
        .0;
    if source_unit != target_unit {
        return Err(CommandError::InvalidAction {
            reason: "joined units must have the same type".into(),
        });
    }

    let source_faction = source
        .get::<Faction>()
        .copied()
        .expect("source must have Faction component")
        .0;
    let target_faction = target
        .get::<Faction>()
        .copied()
        .ok_or_else(|| CommandError::InvalidAction {
            reason: "join target is missing faction".into(),
        })?
        .0;
    if source_faction != player_faction || target_faction != player_faction {
        return Err(CommandError::InvalidAction {
            reason: "joined units must have the same owner".into(),
        });
    }

    Ok(())
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
        awbrn_types::Unit::Lander => cargo.domain() == UnitDomain::Ground,
        awbrn_types::Unit::Cruiser => cargo.domain() == UnitDomain::Air,
        _ => false,
    }
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
