use bevy::prelude::*;

use crate::command::{GameCommand, PostMoveAction};
use crate::player::PlayerRegistry;
use crate::state::{ServerGameState, TurnPhase};
use crate::unit_id::ServerUnitId;
use awbrn_game::MapPosition;
use awbrn_game::world::{Capturing, Fuel, StrongIdMap, UnitActive};
use awbrn_types::PlayerFaction;

/// The set of world mutations that occurred from applying a command.
/// Used by the view layer to build per-player updates.
pub(crate) enum ApplyOutcome {
    UnitMoved {
        unit_id: ServerUnitId,
        entity: Entity,
        from: awbrn_map::Position,
        to: awbrn_map::Position,
        path: Vec<awbrn_map::Position>,
        faction: PlayerFaction,
    },
    TurnEnded {
        new_active_player: crate::player::PlayerId,
        new_day: Option<u32>,
    },
}

/// Apply a validated command to the world. Returns the outcome for view generation.
pub(crate) fn apply_command(world: &mut World, command: &GameCommand) -> ApplyOutcome {
    match command {
        GameCommand::MoveUnit {
            unit_id,
            path,
            action,
        } => apply_move_unit(world, *unit_id, path, action.as_ref()),
        GameCommand::Build { .. } => {
            unreachable!("build should have been rejected by validation")
        }
        GameCommand::EndTurn => apply_end_turn(world),
    }
}

fn apply_move_unit(
    world: &mut World,
    unit_id: ServerUnitId,
    path: &[awbrn_map::Position],
    _action: Option<&PostMoveAction>,
) -> ApplyOutcome {
    let entity = world
        .resource::<StrongIdMap<ServerUnitId>>()
        .get(&unit_id)
        .expect("validated unit must exist");

    let from = world
        .entity(entity)
        .get::<MapPosition>()
        .expect("unit must have position")
        .position();

    let faction = world
        .entity(entity)
        .get::<awbrn_game::world::Faction>()
        .expect("unit must have faction")
        .0;

    let to = *path.last().expect("validated path is non-empty");

    // Consume fuel: one per tile moved (path includes start, so subtract 1).
    let tiles_moved = path.len().saturating_sub(1) as u32;
    if tiles_moved > 0 {
        let current_fuel = world.entity(entity).get::<Fuel>().map(|f| f.0).unwrap_or(0);
        let new_fuel = current_fuel.saturating_sub(tiles_moved);
        world.entity_mut(entity).insert(Fuel(new_fuel));
    }

    // Update position if it changed.
    if from != to {
        // Clear capturing status when moving away.
        world.entity_mut(entity).remove::<Capturing>();
        world.entity_mut(entity).insert(MapPosition::from(to));
    }

    // Deactivate the unit (it has acted this turn).
    world.entity_mut(entity).remove::<UnitActive>();

    ApplyOutcome::UnitMoved {
        unit_id,
        entity,
        from,
        to,
        path: path.to_vec(),
        faction,
    }
}

fn apply_end_turn(world: &mut World) -> ApplyOutcome {
    let current_player = world.resource::<ServerGameState>().active_player;

    // Compute next player and both indices before any mutations to avoid borrow conflicts.
    let (next_player_opt, current_idx, next_idx) = {
        let registry = world.resource::<PlayerRegistry>();
        let next = registry.next_active_player_after(current_player);
        let current_idx = registry.player_index(current_player).unwrap_or(0);
        let next_idx = next.and_then(|p| registry.player_index(p)).unwrap_or(0);
        (next, current_idx, next_idx)
    };

    let Some(next_player) = next_player_opt else {
        // No active players remain -- game over.
        world.resource_mut::<ServerGameState>().phase = TurnPhase::GameOver { winner: None };
        return ApplyOutcome::TurnEnded {
            new_active_player: current_player,
            new_day: None,
        };
    };

    // A new day begins when the turn order wraps back to an earlier index.
    let new_day = if next_idx <= current_idx {
        let mut state = world.resource_mut::<ServerGameState>();
        state.day += 1;
        Some(state.day)
    } else {
        None
    };

    world.resource_mut::<ServerGameState>().active_player = next_player;

    // Reactivate all units belonging to the new active player.
    let new_faction = world
        .resource::<PlayerRegistry>()
        .faction_for_player(next_player)
        .expect("next player must have a faction");

    activate_player_units(world, new_faction);

    ApplyOutcome::TurnEnded {
        new_active_player: next_player,
        new_day,
    }
}

/// Mark all units belonging to the given faction as active.
fn activate_player_units(world: &mut World, faction: PlayerFaction) {
    let batch: Vec<(Entity, UnitActive)> = world
        .query_filtered::<(Entity, &awbrn_game::world::Faction), Without<UnitActive>>()
        .iter(world)
        .filter(|(_, f)| f.0 == faction)
        .map(|(e, _)| (e, UnitActive))
        .collect();

    world.insert_batch(batch);
}
