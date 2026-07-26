//! Small authoritative reducer surface used by the conformance protocol.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::combat::{self, Side, Weapon};
use crate::commander::{
    self, AreaStrikeCenterTarget, AreaStrikePolicy, CombatContext, Combatant, CommanderSlotTarget,
    FriendlyContribution, ImmobilizationDuration, InstantEffect, OccupiedTileHandling,
    PlayerTarget, PowerLevel, PropertyOrder, PropertyTarget, SpawnAction, SpawnConcealment,
    SpawnResources, SpawnUnitLimit, Strike, TargetedAreaStrikePolicy, TargetedUnitValue,
    UnitTarget, WeatherDuration, WeatherEffectKind,
};
use crate::ruleset::{self, Domain, FireMode, Relation, TargetSet, TerrainTrait, UnitKind};
use crate::semantic::{
    AwbwVisibility, Concealment, Location, Match, Outcome, Phase, PlayerId, PlayerStatus, Position,
    PowerState, Silo, State, TeamStatus, TerrainId, Unit, UnitAction, UnitId, UnitKindId,
    Visibility, WeatherKind, WeatherSetting,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Command {
    MoveWait {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
    },
    MoveAttack {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
        target: AttackTarget,
    },
    MoveLaunch {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
        target: Position,
    },
    MoveExplode {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
    },
    DeleteUnit {
        player: PlayerId,
        unit: UnitId,
    },
    MoveHide {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
    },
    MoveReveal {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
    },
    MoveCapture {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
    },
    ProduceUnit {
        player: PlayerId,
        position: Position,
        kind: UnitKindId,
    },
    MoveJoin {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
        target: UnitId,
    },
    MoveSupply {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
    },
    MoveRepair {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
        target: UnitId,
    },
    MoveLoad {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Position>,
        transport: UnitId,
    },
    Unload {
        player: PlayerId,
        transport: UnitId,
        cargo: UnitId,
        destination: Position,
    },
    ActivatePower {
        player: PlayerId,
        level: PowerLevel,
    },
    Tag {
        player: PlayerId,
    },
    EndTurn {
        player: PlayerId,
    },
    Resign {
        player: PlayerId,
    },
    #[serde(other)]
    Unsupported,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AttackTarget {
    Unit { unit: UnitId },
    Tile { position: Position },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Execution {
    pub state: State,
    pub events: Vec<Value>,
    pub random_consumed: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecuteError {
    UnsupportedCommand,
    Violation(Value),
    UnsupportedRuleset,
    InvalidState(String),
    InvalidRandom(String),
}

pub fn execute(
    state: &State,
    command: Command,
    random: &[Value],
) -> Result<Execution, ExecuteError> {
    match command {
        Command::MoveWait { player, unit, path } => {
            execute_move_wait(state, &player, unit, path, random)
        }
        Command::MoveAttack {
            player,
            unit,
            path,
            target,
        } => execute_move_attack(state, &player, unit, path, target, random),
        Command::MoveLaunch {
            player,
            unit,
            path,
            target,
        } => execute_move_launch(state, &player, unit, path, target),
        Command::MoveExplode { player, unit, path } => {
            execute_move_explode(state, &player, unit, path)
        }
        Command::DeleteUnit { player, unit } => execute_delete_unit(state, &player, unit),
        Command::MoveHide { player, unit, path } => {
            execute_move_concealment(state, &player, unit, path, true)
        }
        Command::MoveReveal { player, unit, path } => {
            execute_move_concealment(state, &player, unit, path, false)
        }
        Command::MoveCapture { player, unit, path } => {
            execute_move_capture(state, &player, unit, path)
        }
        Command::ProduceUnit {
            player,
            position,
            kind,
        } => execute_produce_unit(state, &player, position, kind),
        Command::MoveJoin {
            player,
            unit,
            path,
            target,
        } => execute_move_join(state, &player, unit, path, target),
        Command::MoveSupply { player, unit, path } => {
            execute_move_supply(state, &player, unit, path)
        }
        Command::MoveRepair {
            player,
            unit,
            path,
            target,
        } => execute_move_repair(state, &player, unit, path, target),
        Command::MoveLoad {
            player,
            unit,
            path,
            transport,
        } => execute_move_load(state, &player, unit, path, transport),
        Command::Unload {
            player,
            transport,
            cargo,
            destination,
        } => execute_unload(state, &player, transport, cargo, destination),
        Command::ActivatePower { player, level } => execute_activate_power(state, &player, level),
        Command::Tag { player } => execute_tag(state, &player, random),
        Command::EndTurn { player } => execute_end_turn(state, &player, random),
        Command::Resign { player } => execute_resign(state, &player, random),
        Command::Unsupported => Err(ExecuteError::UnsupportedCommand),
    }
}

struct MovementPlan {
    unit_index: usize,
    origin: Position,
    path: Vec<Position>,
    entry_costs: Vec<u64>,
    actor_team: crate::semantic::TeamId,
}

struct MovementOutcome {
    state: State,
    events: Vec<Value>,
    trapped: bool,
}

fn validate_movement_prefix(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
) -> Result<MovementPlan, ExecuteError> {
    if state.ruleset.id != "awbw" || state.ruleset.revision != "2026-07-10" {
        return Err(ExecuteError::UnsupportedRuleset);
    }
    if matches!(state.match_state, Match::Finished { .. }) {
        return Err(violation(json!({"code":"MATCH_FINISHED"})));
    }
    if state.turn.phase != Phase::UnitAction {
        return Err(violation(json!({
            "code":"WRONG_PHASE", "expected":"unit-action", "actual":state.turn.phase
        })));
    }
    if state.turn.active_player != player {
        return Err(violation(
            json!({"code":"NOT_ACTIVE_PLAYER","player":player}),
        ));
    }
    let unit_index = state
        .units
        .iter()
        .position(|unit| unit.id == unit_id)
        .ok_or_else(|| violation(json!({"code":"UNIT_NOT_FOUND","unit":unit_id})))?;
    let unit = &state.units[unit_index];
    if unit.owner != player {
        return Err(violation(
            json!({"code":"UNIT_NOT_OWNED","unit":unit_id,"player":player}),
        ));
    }
    let Location::Board { position: origin } = unit.location else {
        return Err(violation(
            json!({"code":"UNIT_NOT_ON_BOARD","unit":unit_id}),
        ));
    };
    if unit.action != UnitAction::Ready {
        return Err(violation(
            json!({"code":"UNIT_ALREADY_ACTED","unit":unit_id}),
        ));
    }
    let actual_origin = path.first().copied().unwrap_or(origin);
    if path.first() != Some(&origin) {
        return Err(violation(
            json!({"code":"PATH_ORIGIN_MISMATCH","expected":origin,"actual":actual_origin}),
        ));
    }
    for (index, pair) in path.windows(2).enumerate() {
        if pair[0][0].abs_diff(pair[1][0]) + pair[0][1].abs_diff(pair[1][1]) != 1 {
            return Err(violation(json!({
                "code":"PATH_NON_ADJACENT", "index":index + 1,
                "from":pair[0], "to":pair[1]
            })));
        }
    }
    for (index, position) in path.iter().copied().enumerate() {
        if let Some(first_index) = path[..index].iter().position(|seen| *seen == position) {
            return Err(violation(json!({
                "code":"PATH_REPEATED_POSITION", "index":index,
                "position":position, "first_index":first_index
            })));
        }
    }
    for (index, position) in path.iter().copied().enumerate() {
        if position[0] >= state.board.width || position[1] >= state.board.height {
            return Err(violation(
                json!({"code":"PATH_OUT_OF_BOUNDS","index":index,"position":position}),
            ));
        }
    }
    let profile = ruleset::profile(unit.kind);
    let movement =
        commander::effective_move(state, unit, profile.movement, profile.domain.as_str());
    let weather = commander::effective_weather(state, unit);
    let mut entry_costs = vec![0];
    for (index, position) in path.iter().copied().enumerate().skip(1) {
        let terrain = state.board.tiles[position[1]][position[0]].terrain;
        let cost = commander::effective_movement_cost(
            state,
            unit,
            ruleset::movement_cost(terrain, weather, profile.movement_class),
        )
        .ok_or_else(|| {
            violation(json!({
                "code":"TERRAIN_IMPASSABLE","index":index,"position":position
            }))
        })?;
        entry_costs.push(cost);
    }
    let actor_team = state
        .players
        .iter()
        .find(|candidate| candidate.id == player)
        .map(|candidate| candidate.team.clone())
        .ok_or_else(|| ExecuteError::InvalidState(format!("unknown active player {player}")))?;
    let visibility = AwbwVisibility;
    for (index, position) in path
        .iter()
        .copied()
        .enumerate()
        .skip(1)
        .take(path.len().saturating_sub(2))
    {
        if state.units.iter().any(|other| {
            other.id != unit_id
                && board_position(other) == Some(position)
                && occupancy_is_disclosed(&visibility, state, &actor_team, other)
        }) {
            return Err(violation(
                json!({"code":"PATH_OCCUPIED","index":index,"position":position}),
            ));
        }
    }
    let intended_cost: u64 = entry_costs.iter().sum();
    if intended_cost > movement {
        return Err(violation(json!({
            "code":"INSUFFICIENT_MOVEMENT","required":intended_cost,"available":movement
        })));
    }
    if intended_cost > unit.fuel {
        return Err(violation(json!({
            "code":"INSUFFICIENT_FUEL","required":intended_cost,"available":unit.fuel
        })));
    }
    Ok(MovementPlan {
        unit_index,
        origin,
        path,
        entry_costs,
        actor_team,
    })
}

fn execute_planned_movement(
    state: &State,
    unit_id: UnitId,
    plan: &MovementPlan,
) -> MovementOutcome {
    let visibility = AwbwVisibility;
    let trap = plan
        .path
        .iter()
        .copied()
        .enumerate()
        .skip(1)
        .find_map(|(index, position)| {
            state
                .units
                .iter()
                .find(|other| {
                    other.id != unit_id
                        && board_position(other) == Some(position)
                        && !occupancy_is_disclosed(&visibility, state, &plan.actor_team, other)
                })
                .map(|blocker| (index, position, blocker.id))
        });
    let actual_length = trap
        .as_ref()
        .map_or(plan.path.len(), |(index, _, _)| *index);
    let actual_path = plan.path[..actual_length].to_vec();
    let destination = *actual_path.last().expect("actual path includes origin");
    let fuel_spent: u64 = plan.entry_costs[..actual_length].iter().sum();
    let mut next = state.clone();
    let mut events = Vec::new();
    reset_capture_on_departure(&mut next, unit_id, plan.origin, &actual_path, &mut events);
    next.units[plan.unit_index].fuel -= fuel_spent;
    next.units[plan.unit_index].action = UnitAction::Spent;
    next.units[plan.unit_index].location = Location::Board {
        position: destination,
    };
    events.push(json!({
        "type":"unit-moved", "unit":unit_id, "from":plan.origin, "to":destination,
        "path":actual_path, "fuel_spent":fuel_spent
    }));
    let trapped = if let Some((_, position, blocker)) = trap {
        events.push(json!({
            "type":"movement-trapped", "unit":unit_id,
            "blocker":blocker, "position":position
        }));
        true
    } else {
        false
    };
    MovementOutcome {
        state: next,
        events,
        trapped,
    }
}

fn execute_move_supply(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
) -> Result<Execution, ExecuteError> {
    let plan = validate_movement_prefix(state, player, unit_id, path)?;
    let unit = &state.units[plan.unit_index];
    let supply = ruleset::profile(unit.kind).supply;
    let Some(supply) = supply.filter(|supply| supply.relation == Relation::Adjacent) else {
        return Err(violation(
            json!({"code":"ACTION_NOT_SUPPORTED","action":"move-supply"}),
        ));
    };
    let destination = *plan.path.last().expect("origin was checked");
    let visibility = AwbwVisibility;
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(destination)
            && occupancy_is_disclosed(&visibility, state, &plan.actor_team, other)
    }) {
        return Err(violation(
            json!({"code":"DESTINATION_OCCUPIED","position":destination}),
        ));
    }

    let mut outcome = execute_planned_movement(state, unit_id, &plan);
    if outcome.trapped {
        return Ok(Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        });
    }
    let actual_destination =
        board_position(&outcome.state.units[plan.unit_index]).expect("mover remains on board");
    let mut supply_ids: Vec<_> = state
        .units
        .iter()
        .filter(|target| {
            target.id != unit_id
                && supply_target_eligible(
                    state,
                    &unit.owner,
                    &plan.actor_team,
                    &target.owner,
                    supply.targets,
                )
                && board_position(target).is_some_and(|position| {
                    position[0].abs_diff(actual_destination[0])
                        + position[1].abs_diff(actual_destination[1])
                        == 1
                })
        })
        .map(|target| target.id)
        .collect();
    supply_ids.sort();
    for id in supply_ids {
        let target = outcome
            .state
            .units
            .iter_mut()
            .find(|target| target.id == id)
            .expect("supply target remains present");
        let profile = ruleset::profile(target.kind);
        let max_fuel = profile.max_fuel;
        let max_ammo = profile.max_ammo;
        let fuel_before = target.fuel;
        let ammo_before = target.ammo;
        target.fuel = max_fuel;
        target.ammo = max_ammo;
        if fuel_before != max_fuel || ammo_before != max_ammo {
            outcome.events.push(json!({
                "type":"unit-resourced", "unit":id,
                "fuel_before":fuel_before, "fuel_after":max_fuel,
                "ammo_before":ammo_before, "ammo_after":max_ammo,
                "reason":"unit-supply"
            }));
        }
    }
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

fn supply_target_eligible(
    state: &State,
    source_owner: &str,
    source_team: &str,
    target_owner: &str,
    targets: TargetSet,
) -> bool {
    match targets {
        TargetSet::OwnedUnits => target_owner == source_owner,
        TargetSet::FriendlyUnits => state
            .players
            .iter()
            .find(|owner| owner.id == target_owner)
            .is_some_and(|owner| owner.team == source_team),
    }
}

fn execute_move_repair(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
    target_id: UnitId,
) -> Result<Execution, ExecuteError> {
    let plan = validate_movement_prefix(state, player, unit_id, path)?;
    let unit = &state.units[plan.unit_index];
    let repair = ruleset::profile(unit.kind).repair;
    let Some(repair) = repair.filter(|repair| repair.relation == Relation::Adjacent) else {
        return Err(violation(
            json!({"code":"ACTION_NOT_SUPPORTED","action":"move-repair"}),
        ));
    };
    let target_index = state.units.iter().position(|target| target.id == target_id);
    let target = target_index.and_then(|index| state.units.get(index));
    let target_team = target.and_then(|target| {
        state
            .players
            .iter()
            .find(|owner| owner.id == target.owner)
            .map(|owner| owner.team.as_str())
    });
    let target_position = target.and_then(board_position);
    if !target.is_some_and(|target| {
        target.id != unit_id
            && target_team == Some(plan.actor_team.as_str())
            && target_position.is_some()
    }) {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":target_id}),
        ));
    }
    let destination = *plan.path.last().expect("origin was checked");
    let target_position = target_position.expect("target validity established its position");
    if target_position[0].abs_diff(destination[0]) + target_position[1].abs_diff(destination[1])
        != 1
    {
        return Err(violation(
            json!({"code":"TARGET_OUT_OF_RANGE","target":target_id}),
        ));
    }
    let visibility = AwbwVisibility;
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(destination)
            && occupancy_is_disclosed(&visibility, state, &plan.actor_team, other)
    }) {
        return Err(violation(
            json!({"code":"DESTINATION_OCCUPIED","position":destination}),
        ));
    }

    let target_index = target_index.expect("target validity established its index");
    let exact_hp = repair.exact_hp;
    let target_profile = ruleset::profile(target.expect("target exists").kind);
    let max_fuel = target_profile.max_fuel;
    let max_ammo = target_profile.max_ammo;
    let heal_cost = target_profile
        .cost
        .checked_mul(repair.cost_percent)
        .and_then(|cost| cost.checked_div(100))
        .ok_or(ExecuteError::UnsupportedRuleset)?;

    let mut outcome = execute_planned_movement(state, unit_id, &plan);
    if outcome.trapped {
        return Ok(Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        });
    }
    let target = &mut outcome.state.units[target_index];
    let fuel_before = target.fuel;
    let ammo_before = target.ammo;
    target.fuel = max_fuel;
    target.ammo = max_ammo;
    if fuel_before != max_fuel || ammo_before != max_ammo {
        outcome.events.push(json!({
            "type":"unit-resourced", "unit":target_id,
            "fuel_before":fuel_before, "fuel_after":max_fuel,
            "ammo_before":ammo_before, "ammo_after":max_ammo,
            "reason":"unit-repair"
        }));
    }
    let visual_hp = target.hp.div_ceil(exact_hp);
    if visual_hp < 10 {
        let player_index = outcome
            .state
            .players
            .iter()
            .position(|candidate| candidate.id == player)
            .ok_or_else(|| ExecuteError::InvalidState(format!("unknown active player {player}")))?;
        let funds_before = outcome.state.players[player_index].funds;
        if heal_cost <= funds_before {
            let hp_before = outcome.state.units[target_index].hp;
            let hp_after = (visual_hp + 1).min(10) * exact_hp;
            outcome.state.players[player_index].funds -= heal_cost;
            outcome.events.push(json!({
                "type":"funds-changed", "player":player, "from":funds_before,
                "to":funds_before - heal_cost, "reason":"unit-repair"
            }));
            outcome.state.units[target_index].hp = hp_after;
            outcome.events.push(json!({
                "type":"unit-repaired", "unit":target_id, "from_hp":hp_before,
                "to_hp":hp_after, "reason":"unit-repair"
            }));
        }
    }
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

fn execute_move_load(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
    transport_id: UnitId,
) -> Result<Execution, ExecuteError> {
    let plan = validate_movement_prefix(state, player, unit_id, path)?;
    let mover = &state.units[plan.unit_index];
    let transport_index = state
        .units
        .iter()
        .position(|transport| transport.id == transport_id);
    let transport = transport_index.and_then(|index| state.units.get(index));
    let transport_capability =
        transport.and_then(|transport| ruleset::profile(transport.kind).transport);
    let capacity = transport_capability.map(|capability| capability.capacity);
    let cargo_kind_allowed =
        transport_capability.is_some_and(|capability| capability.cargo.contains(mover.kind));
    let occupied_slots: Vec<_> = state
        .units
        .iter()
        .filter_map(|cargo| match &cargo.location {
            Location::Cargo { transport, slot } if *transport == transport_id => Some(*slot),
            _ => None,
        })
        .collect();
    let target_valid = transport.is_some_and(|transport| {
        transport.id != unit_id
            && transport.owner == player
            && board_position(transport).is_some()
            && cargo_kind_allowed
            && capacity.is_some_and(|capacity| occupied_slots.len() < capacity)
    });
    if !target_valid {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":transport_id}),
        ));
    }
    let transport_position =
        board_position(transport.expect("target validity established transport position"))
            .expect("target validity established transport position");
    let destination = *plan.path.last().expect("origin was checked");
    if destination != transport_position {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":destination}),
        ));
    }
    let capacity = capacity.expect("target validity established capacity");
    let slot = (0..capacity)
        .find(|slot| !occupied_slots.contains(slot))
        .ok_or_else(|| ExecuteError::InvalidState(format!("transport {transport_id} is full")))?;

    let mut outcome = execute_planned_movement(state, unit_id, &plan);
    if outcome.trapped {
        return Ok(Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        });
    }
    outcome.state.units[plan.unit_index].location = Location::Cargo {
        transport: transport_id,
        slot,
    };
    outcome.events.push(json!({
        "type":"unit-loaded", "unit":unit_id, "transport":transport_id, "slot":slot
    }));
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

fn execute_unload(
    state: &State,
    player: &str,
    transport_id: UnitId,
    cargo_id: UnitId,
    destination: Position,
) -> Result<Execution, ExecuteError> {
    if state.ruleset.id != "awbw" || state.ruleset.revision != "2026-07-10" {
        return Err(ExecuteError::UnsupportedRuleset);
    }
    if matches!(state.match_state, Match::Finished { .. }) {
        return Err(violation(json!({"code":"MATCH_FINISHED"})));
    }
    if state.turn.phase != Phase::UnitAction {
        return Err(violation(json!({
            "code":"WRONG_PHASE", "expected":"unit-action", "actual":state.turn.phase
        })));
    }
    if state.turn.active_player != player {
        return Err(violation(
            json!({"code":"NOT_ACTIVE_PLAYER","player":player}),
        ));
    }
    let transport_index = state
        .units
        .iter()
        .position(|transport| transport.id == transport_id);
    let transport = transport_index.and_then(|index| state.units.get(index));
    let transport_position = transport.and_then(board_position);
    if !transport.is_some_and(|transport| {
        transport.owner == player
            && transport_position.is_some()
            && ruleset::profile(transport.kind).transport.is_some()
    }) {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":transport_id}),
        ));
    }
    let cargo_index = state.units.iter().position(|cargo| cargo.id == cargo_id);
    let cargo = cargo_index.and_then(|index| state.units.get(index));
    let cargo_slot = cargo.and_then(|cargo| match &cargo.location {
        Location::Cargo { transport, slot } if *transport == transport_id => Some(*slot),
        _ => None,
    });
    if cargo_slot.is_none() {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":cargo_id}),
        ));
    }
    let transport_position = transport_position.expect("transport validity established position");
    if transport_position[0].abs_diff(destination[0])
        + transport_position[1].abs_diff(destination[1])
        != 1
    {
        return Err(violation(
            json!({"code":"TARGET_OUT_OF_RANGE","target":destination}),
        ));
    }
    let cargo = cargo.expect("cargo validity established unit");
    let movement_class = ruleset::profile(cargo.kind).movement_class;
    let weather = commander::effective_weather(state, cargo);
    let destination_tile = state
        .board
        .tiles
        .get(destination[1])
        .and_then(|row| row.get(destination[0]));
    let passable = destination_tile.is_some_and(|tile| {
        commander::effective_movement_cost(
            state,
            cargo,
            ruleset::movement_cost(tile.terrain, weather, movement_class),
        )
        .is_some()
    });
    if !passable {
        return Err(violation(
            json!({"code":"TERRAIN_IMPASSABLE","position":destination}),
        ));
    }
    if state
        .units
        .iter()
        .any(|unit| board_position(unit) == Some(destination))
    {
        return Err(violation(
            json!({"code":"DESTINATION_OCCUPIED","position":destination}),
        ));
    }

    let cargo_index = cargo_index.expect("cargo validity established index");
    let vacated_slot = cargo_slot.expect("cargo validity established slot");
    let mut next = state.clone();
    next.units[cargo_index].location = Location::Board {
        position: destination,
    };
    next.units[cargo_index].action = UnitAction::Spent;
    for unit in &mut next.units {
        if let Location::Cargo { transport, slot } = &mut unit.location
            && *transport == transport_id
            && *slot > vacated_slot
        {
            *slot -= 1;
        }
    }
    Ok(Execution {
        state: next,
        events: vec![json!({
            "type":"unit-unloaded", "unit":cargo_id,
            "transport":transport_id, "position":destination
        })],
        random_consumed: 0,
    })
}

fn execute_move_join(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
    target_id: UnitId,
) -> Result<Execution, ExecuteError> {
    if state.ruleset.id != "awbw" || state.ruleset.revision != "2026-07-10" {
        return Err(ExecuteError::UnsupportedRuleset);
    }
    if matches!(state.match_state, Match::Finished { .. }) {
        return Err(violation(json!({"code":"MATCH_FINISHED"})));
    }
    if state.turn.phase != Phase::UnitAction {
        return Err(violation(json!({
            "code":"WRONG_PHASE", "expected":"unit-action", "actual":state.turn.phase
        })));
    }
    if state.turn.active_player != player {
        return Err(violation(
            json!({"code":"NOT_ACTIVE_PLAYER","player":player}),
        ));
    }
    let mover_index = state
        .units
        .iter()
        .position(|unit| unit.id == unit_id)
        .ok_or_else(|| violation(json!({"code":"UNIT_NOT_FOUND","unit":unit_id})))?;
    let mover = &state.units[mover_index];
    if mover.owner != player {
        return Err(violation(
            json!({"code":"UNIT_NOT_OWNED","unit":unit_id,"player":player}),
        ));
    }
    let Location::Board { position: origin } = mover.location else {
        return Err(violation(
            json!({"code":"UNIT_NOT_ON_BOARD","unit":unit_id}),
        ));
    };
    if mover.action != UnitAction::Ready {
        return Err(violation(
            json!({"code":"UNIT_ALREADY_ACTED","unit":unit_id}),
        ));
    }
    let actual_origin = path.first().copied().unwrap_or(origin);
    if path.first() != Some(&origin) {
        return Err(violation(
            json!({"code":"PATH_ORIGIN_MISMATCH","expected":origin,"actual":actual_origin}),
        ));
    }
    for (index, pair) in path.windows(2).enumerate() {
        if pair[0][0].abs_diff(pair[1][0]) + pair[0][1].abs_diff(pair[1][1]) != 1 {
            return Err(violation(json!({
                "code":"PATH_NON_ADJACENT", "index":index + 1,
                "from":pair[0], "to":pair[1]
            })));
        }
    }
    for (index, position) in path.iter().copied().enumerate() {
        if let Some(first_index) = path[..index].iter().position(|seen| *seen == position) {
            return Err(violation(json!({
                "code":"PATH_REPEATED_POSITION", "index":index,
                "position":position, "first_index":first_index
            })));
        }
    }
    for (index, position) in path.iter().copied().enumerate() {
        if position[0] >= state.board.width || position[1] >= state.board.height {
            return Err(violation(
                json!({"code":"PATH_OUT_OF_BOUNDS","index":index,"position":position}),
            ));
        }
    }

    let mover_profile = ruleset::profile(mover.kind);
    let movement = commander::effective_move(
        state,
        mover,
        mover_profile.movement,
        mover_profile.domain.as_str(),
    );
    let weather = commander::effective_weather(state, mover);
    let mut entry_costs = vec![0];
    for (index, position) in path.iter().copied().enumerate().skip(1) {
        let terrain = state.board.tiles[position[1]][position[0]].terrain;
        let cost = commander::effective_movement_cost(
            state,
            mover,
            ruleset::movement_cost(terrain, weather, mover_profile.movement_class),
        )
        .ok_or_else(|| {
            violation(json!({
                "code":"TERRAIN_IMPASSABLE","index":index,"position":position
            }))
        })?;
        entry_costs.push(cost);
    }
    let actor_team = state
        .players
        .iter()
        .find(|candidate| candidate.id == player)
        .map(|candidate| candidate.team.as_str())
        .ok_or_else(|| ExecuteError::InvalidState(format!("unknown active player {player}")))?;
    let visibility = AwbwVisibility;
    for (index, position) in path
        .iter()
        .copied()
        .enumerate()
        .skip(1)
        .take(path.len().saturating_sub(2))
    {
        if state.units.iter().any(|other| {
            other.id != unit_id
                && board_position(other) == Some(position)
                && occupancy_is_disclosed(&visibility, state, actor_team, other)
        }) {
            return Err(violation(
                json!({"code":"PATH_OCCUPIED","index":index,"position":position}),
            ));
        }
    }
    let intended_cost: u64 = entry_costs.iter().sum();
    if intended_cost > movement {
        return Err(violation(json!({
            "code":"INSUFFICIENT_MOVEMENT","required":intended_cost,"available":movement
        })));
    }
    if intended_cost > mover.fuel {
        return Err(violation(json!({
            "code":"INSUFFICIENT_FUEL","required":intended_cost,"available":mover.fuel
        })));
    }

    let target_index = state.units.iter().position(|unit| unit.id == target_id);
    let target = target_index.and_then(|index| state.units.get(index));
    let target_owner_team = target.and_then(|target| {
        state
            .players
            .iter()
            .find(|owner| owner.id == target.owner)
            .map(|owner| owner.team.as_str())
    });
    let target_position = target.and_then(board_position);
    let target_valid = target.is_some_and(|target| {
        target.id != mover.id
            && target.kind == mover.kind
            && target_owner_team == Some(actor_team)
            && target_position.is_some()
    });
    if !target_valid {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":target_id}),
        ));
    }
    let target = target.expect("target validity established the unit");
    let target_index = target_index.expect("target validity established the index");
    let destination = *path.last().expect("origin was checked");
    if target_position != Some(destination) {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":destination}),
        ));
    }
    if target.hp.div_ceil(10) == 10 {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":target_id}),
        ));
    }
    let target_carries_cargo = state.units.iter().any(
        |cargo| matches!(&cargo.location, Location::Cargo { transport, .. } if *transport == target_id),
    );
    if target_carries_cargo {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":target_id}),
        ));
    }
    let mover_carries_cargo = state.units.iter().any(
        |cargo| matches!(&cargo.location, Location::Cargo { transport, .. } if *transport == unit_id),
    );
    if mover_carries_cargo {
        return Err(violation(json!({"code":"INVALID_TARGET","target":unit_id})));
    }

    // Only an undisclosed intermediate enemy can trap a well-formed join: the
    // allied destination target is always disclosed and explicitly licensed.
    let trap = path
        .iter()
        .copied()
        .enumerate()
        .skip(1)
        .take(path.len().saturating_sub(2))
        .find_map(|(index, position)| {
            state
                .units
                .iter()
                .find(|other| {
                    other.id != unit_id
                        && board_position(other) == Some(position)
                        && !occupancy_is_disclosed(&visibility, state, actor_team, other)
                })
                .map(|blocker| (index, position, blocker.id))
        });
    let actual_length = trap.as_ref().map_or(path.len(), |(index, _, _)| *index);
    let actual_path = path[..actual_length].to_vec();
    let actual_destination = *actual_path.last().expect("actual path includes origin");
    let fuel_spent: u64 = entry_costs[..actual_length].iter().sum();
    let mut next = state.clone();
    let mut events = Vec::new();
    reset_capture_on_departure(&mut next, unit_id, origin, &actual_path, &mut events);
    next.units[mover_index].fuel -= fuel_spent;
    next.units[mover_index].action = UnitAction::Spent;
    next.units[mover_index].location = Location::Board {
        position: actual_destination,
    };
    events.push(json!({
        "type":"unit-moved", "unit":unit_id, "from":origin, "to":actual_destination,
        "path":actual_path, "fuel_spent":fuel_spent
    }));
    if let Some((_, position, blocker)) = trap {
        events.push(json!({
            "type":"movement-trapped", "unit":unit_id,
            "blocker":blocker, "position":position
        }));
        return Ok(Execution {
            state: next,
            events,
            random_consumed: 0,
        });
    }

    let combined_visual_hp = mover.hp.div_ceil(10) + target.hp.div_ceil(10);
    let moved_fuel = mover.fuel - fuel_spent;
    let max_fuel = mover_profile.max_fuel;
    let max_ammo = mover_profile.max_ammo;
    let cost = mover_profile.cost;
    next.units[target_index].hp = combined_visual_hp.min(10) * 10;
    next.units[target_index].fuel = (moved_fuel + target.fuel).min(max_fuel);
    next.units[target_index].ammo = (mover.ammo + target.ammo).min(max_ammo);
    next.units[target_index].action = UnitAction::Spent;
    next.units.remove(mover_index);
    events.push(json!({"type":"units-joined","source":unit_id,"target":target_id}));

    if combined_visual_hp > 10 {
        let refund = (cost / 10) * u64::from(combined_visual_hp - 10);
        let player_index = next
            .players
            .iter()
            .position(|candidate| candidate.id == player)
            .ok_or_else(|| ExecuteError::InvalidState(format!("unknown active player {player}")))?;
        let funds_before = next.players[player_index].funds;
        next.players[player_index].funds = funds_before
            .checked_add(refund)
            .ok_or_else(|| ExecuteError::InvalidState("join refund overflow".into()))?;
        events.push(json!({
            "type":"funds-changed", "player":player, "from":funds_before,
            "to":next.players[player_index].funds, "reason":"unit-join"
        }));
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: 0,
    })
}

fn execute_produce_unit(
    state: &State,
    player: &str,
    position: Position,
    kind: UnitKind,
) -> Result<Execution, ExecuteError> {
    if state.ruleset.id != "awbw" || state.ruleset.revision != "2026-07-10" {
        return Err(ExecuteError::UnsupportedRuleset);
    }
    if matches!(state.match_state, Match::Finished { .. }) {
        return Err(violation(json!({"code":"MATCH_FINISHED"})));
    }
    if state.turn.phase != Phase::UnitAction {
        return Err(violation(json!({
            "code":"WRONG_PHASE", "expected":"unit-action", "actual":state.turn.phase
        })));
    }
    if state.turn.active_player != player {
        return Err(violation(
            json!({"code":"NOT_ACTIVE_PLAYER","player":player}),
        ));
    }
    let player_index = state
        .players
        .iter()
        .position(|candidate| candidate.id == player)
        .ok_or_else(|| ExecuteError::InvalidState(format!("unknown active player {player}")))?;
    let profile = ruleset::profile(kind);

    // Site validation precedes requested-kind validation: whether the player
    // owns a facility here does not depend on what they asked it to build.
    let tile = state
        .board
        .tiles
        .get(position[1])
        .and_then(|row| row.get(position[0]));
    let site_valid = tile.is_some_and(|tile| {
        let terrain = ruleset::terrain(tile.terrain);
        let domain = profile.domain.as_str();
        let commander_facility =
            commander::commander_production_site(state, player, tile.terrain, Some(domain));
        let is_facility = terrain.produces_any() || commander_facility;
        let owned = tile.owner.as_ref().and_then(Option::as_deref) == Some(player);
        let domain_matches = terrain.has(profile.domain.produces())
            || commander::commander_production_site(state, player, tile.terrain, Some(domain));
        is_facility && owned && domain_matches
    });
    if !site_valid {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":position}),
        ));
    }
    if state.settings.unit_bans.contains(&kind) {
        return Err(violation(json!({"code":"INVALID_TARGET","target":kind})));
    }
    if state.settings.lab_units.contains(&kind) && !player_owns_lab(state, player) {
        return Err(violation(json!({"code":"INVALID_TARGET","target":kind})));
    }
    if state
        .units
        .iter()
        .any(|unit| board_position(unit) == Some(position))
    {
        return Err(violation(
            json!({"code":"DESTINATION_OCCUPIED","position":position}),
        ));
    }
    let current = owned_unit_count(state, player)?;
    if let Some(limit) = state.settings.unit_limit
        && current >= limit
    {
        return Err(violation(json!({
            "code":"UNIT_LIMIT_REACHED","current":current,"limit":limit
        })));
    }
    let cost = commander::effective_build_cost(state, player, profile.cost)
        .ok_or_else(|| ExecuteError::InvalidState("commander build cost overflow".into()))?;
    let funds = state.players[player_index].funds;
    if cost > funds {
        return Err(violation(json!({
            "code":"INSUFFICIENT_FUNDS","required":cost,"available":funds
        })));
    }
    let next_id = state
        .next_unit_id
        .ok_or_else(|| ExecuteError::InvalidState("production requires next_unit_id".into()))?;
    let allocated_id = UnitId::new(next_id);
    if state.units.iter().any(|unit| unit.id == allocated_id) {
        return Err(ExecuteError::InvalidState(format!(
            "next_unit_id {next_id} is not fresh"
        )));
    }
    let max_fuel = profile.max_fuel;
    let max_ammo = profile.max_ammo;
    let incremented_id = next_id
        .checked_add(1)
        .ok_or_else(|| ExecuteError::InvalidState("next_unit_id overflow".into()))?;

    let mut next = state.clone();
    next.players[player_index].funds -= cost;
    next.next_unit_id = Some(incremented_id);
    next.units.push(Unit {
        id: allocated_id,
        kind,
        owner: player.into(),
        hp: 100,
        fuel: max_fuel,
        ammo: max_ammo,
        action: UnitAction::Spent,
        concealment: Concealment::Exposed,
        location: Location::Board { position },
    });
    Ok(Execution {
        state: next,
        events: vec![
            json!({
                "type":"funds-changed", "player":player, "from":funds,
                "to":funds - cost, "reason":"unit-production"
            }),
            json!({
                "type":"unit-created", "unit":allocated_id, "kind":kind,
                "owner":player, "position":position
            }),
        ],
        random_consumed: 0,
    })
}

fn player_owns_lab(state: &State, player: &str) -> bool {
    state.board.tiles.iter().flatten().any(|tile| {
        tile.terrain == TerrainId::Lab
            && tile.owner.as_ref().and_then(Option::as_deref) == Some(player)
    })
}

fn execute_move_capture(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
) -> Result<Execution, ExecuteError> {
    if state.ruleset.id != "awbw" || state.ruleset.revision != "2026-07-10" {
        return Err(ExecuteError::UnsupportedRuleset);
    }
    if matches!(state.match_state, Match::Finished { .. }) {
        return Err(violation(json!({"code":"MATCH_FINISHED"})));
    }
    if state.turn.phase != Phase::UnitAction {
        return Err(violation(json!({
            "code":"WRONG_PHASE", "expected":"unit-action", "actual":state.turn.phase
        })));
    }
    if state.turn.active_player != player {
        return Err(violation(
            json!({"code":"NOT_ACTIVE_PLAYER","player":player}),
        ));
    }
    let unit_index = state
        .units
        .iter()
        .position(|unit| unit.id == unit_id)
        .ok_or_else(|| violation(json!({"code":"UNIT_NOT_FOUND","unit":unit_id})))?;
    let unit = &state.units[unit_index];
    if unit.owner != player {
        return Err(violation(
            json!({"code":"UNIT_NOT_OWNED","unit":unit_id,"player":player}),
        ));
    }
    let Location::Board { position: origin } = unit.location else {
        return Err(violation(
            json!({"code":"UNIT_NOT_ON_BOARD","unit":unit_id}),
        ));
    };
    if unit.action != UnitAction::Ready {
        return Err(violation(
            json!({"code":"UNIT_ALREADY_ACTED","unit":unit_id}),
        ));
    }
    let actual_origin = path.first().copied().unwrap_or(origin);
    if path.first() != Some(&origin) {
        return Err(violation(
            json!({"code":"PATH_ORIGIN_MISMATCH","expected":origin,"actual":actual_origin}),
        ));
    }
    for (index, pair) in path.windows(2).enumerate() {
        if pair[0][0].abs_diff(pair[1][0]) + pair[0][1].abs_diff(pair[1][1]) != 1 {
            return Err(violation(json!({
                "code":"PATH_NON_ADJACENT", "index":index + 1,
                "from":pair[0], "to":pair[1]
            })));
        }
    }
    for (index, position) in path.iter().copied().enumerate() {
        if let Some(first_index) = path[..index].iter().position(|seen| *seen == position) {
            return Err(violation(json!({
                "code":"PATH_REPEATED_POSITION", "index":index,
                "position":position, "first_index":first_index
            })));
        }
    }
    for (index, position) in path.iter().copied().enumerate() {
        if position[0] >= state.board.width || position[1] >= state.board.height {
            return Err(violation(
                json!({"code":"PATH_OUT_OF_BOUNDS","index":index,"position":position}),
            ));
        }
    }

    let profile = ruleset::profile(unit.kind);
    let movement =
        commander::effective_move(state, unit, profile.movement, profile.domain.as_str());
    let weather = commander::effective_weather(state, unit);
    let mut entry_costs = vec![0];
    for (index, position) in path.iter().copied().enumerate().skip(1) {
        let terrain = state.board.tiles[position[1]][position[0]].terrain;
        let cost = commander::effective_movement_cost(
            state,
            unit,
            ruleset::movement_cost(terrain, weather, profile.movement_class),
        )
        .ok_or_else(|| {
            violation(json!({
                "code":"TERRAIN_IMPASSABLE","index":index,"position":position
            }))
        })?;
        entry_costs.push(cost);
    }

    let actor_team = state
        .players
        .iter()
        .find(|candidate| candidate.id == player)
        .map(|candidate| candidate.team.as_str())
        .ok_or(ExecuteError::UnsupportedRuleset)?;
    let visibility = AwbwVisibility;
    for (index, position) in path
        .iter()
        .copied()
        .enumerate()
        .skip(1)
        .take(path.len().saturating_sub(2))
    {
        if state.units.iter().any(|other| {
            other.id != unit_id
                && board_position(other) == Some(position)
                && occupancy_is_disclosed(&visibility, state, actor_team, other)
        }) {
            return Err(violation(
                json!({"code":"PATH_OCCUPIED","index":index,"position":position}),
            ));
        }
    }
    let intended_cost: u64 = entry_costs.iter().sum();
    if intended_cost > movement {
        return Err(violation(json!({
            "code":"INSUFFICIENT_MOVEMENT","required":intended_cost,"available":movement
        })));
    }
    if intended_cost > unit.fuel {
        return Err(violation(json!({
            "code":"INSUFFICIENT_FUEL","required":intended_cost,"available":unit.fuel
        })));
    }

    if !profile.can_capture {
        return Err(violation(
            json!({"code":"ACTION_NOT_SUPPORTED","action":"capture"}),
        ));
    }
    let destination = *path.last().expect("origin was checked");
    let destination_tile = &state.board.tiles[destination[1]][destination[0]];
    let capturable = ruleset::terrain_has(destination_tile.terrain, TerrainTrait::Capturable);
    let owner = destination_tile.owner.as_ref().and_then(Option::as_deref);
    let owner_is_hostile = owner.is_none_or(|owner| {
        state
            .players
            .iter()
            .find(|candidate| candidate.id == owner)
            .is_some_and(|candidate| candidate.team != actor_team)
    });
    if !capturable || !owner_is_hostile {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":destination}),
        ));
    }
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(destination)
            && occupancy_is_disclosed(&visibility, state, actor_team, other)
    }) {
        return Err(violation(
            json!({"code":"DESTINATION_OCCUPIED","position":destination}),
        ));
    }

    // An undisclosed enemy is not a validation fact. It truncates execution
    // before the occupied tile and suppresses capture.
    let trap = path
        .iter()
        .copied()
        .enumerate()
        .skip(1)
        .find_map(|(index, position)| {
            state
                .units
                .iter()
                .find(|other| {
                    other.id != unit_id
                        && board_position(other) == Some(position)
                        && !occupancy_is_disclosed(&visibility, state, actor_team, other)
                })
                .map(|blocker| (index, position, blocker.id))
        });
    let actual_length = trap.as_ref().map_or(path.len(), |(index, _, _)| *index);
    let actual_path = path[..actual_length].to_vec();
    let actual_destination = *actual_path.last().expect("actual path includes origin");
    let fuel_spent: u64 = entry_costs[..actual_length].iter().sum();
    let mut next = state.clone();
    let mut events = Vec::new();
    reset_capture_on_departure(&mut next, unit_id, origin, &actual_path, &mut events);
    next.units[unit_index].fuel -= fuel_spent;
    next.units[unit_index].action = UnitAction::Spent;
    next.units[unit_index].location = Location::Board {
        position: actual_destination,
    };
    events.push(json!({
        "type":"unit-moved", "unit":unit_id, "from":origin, "to":actual_destination,
        "path":actual_path, "fuel_spent":fuel_spent
    }));
    if let Some((_, position, blocker)) = trap {
        events.push(json!({
            "type":"movement-trapped", "unit":unit_id,
            "blocker":blocker, "position":position
        }));
        return Ok(Execution {
            state: next,
            events,
            random_consumed: 0,
        });
    }

    let tile = &mut next.board.tiles[destination[1]][destination[0]];
    let before = tile
        .capture_points
        .ok_or(ExecuteError::UnsupportedRuleset)?;
    let capture_strength =
        commander::effective_capture_points(state, unit, u64::from(unit.hp.div_ceil(10)));
    if u64::from(before) > capture_strength {
        let after = u8::try_from(u64::from(before) - capture_strength)
            .map_err(|_| ExecuteError::InvalidState("capture result overflow".into()))?;
        tile.capture_points = Some(after);
        events.push(json!({
            "type":"capture-changed","position":destination,"from":before,"to":after
        }));
    } else {
        let previous_owner = tile.owner.as_ref().and_then(Clone::clone);
        events.push(json!({
            "type":"capture-changed","position":destination,"from":before,"to":0
        }));
        tile.owner = Some(Some(player.into()));
        events.push(json!({
            "type":"tile-owner-changed","position":destination,
            "from":previous_owner,"to":player
        }));
        tile.capture_points = Some(20);
        events.push(json!({
            "type":"capture-changed","position":destination,"from":0,"to":20
        }));
        let captured_terrain = tile.terrain;
        let captured_profile = ruleset::terrain(captured_terrain);
        let counts_toward_capture_limit =
            captured_profile.has(TerrainTrait::CountsTowardCaptureLimit);
        if counts_toward_capture_limit
            && next
                .settings
                .capture_limit
                .is_some_and(|limit| capture_limit_count(&next, player) >= limit)
        {
            let winning_team = next
                .players
                .iter()
                .find(|candidate| candidate.id == player)
                .map(|candidate| candidate.team.clone())
                .ok_or_else(|| ExecuteError::InvalidState("capturing player is absent".into()))?;
            complete_match(
                &mut next,
                Outcome::Victory {
                    winners: vec![winning_team],
                    reason: "capture-limit".into(),
                },
                &mut events,
            );
            return Ok(Execution {
                state: next,
                events,
                random_consumed: 0,
            });
        }
        let defeats_owner = captured_profile.has(TerrainTrait::CaptureDefeatsOwner);
        let no_hq_on_map = !next
            .board
            .tiles
            .iter()
            .flatten()
            .any(|candidate| candidate.terrain == TerrainId::Hq);
        let is_lab = captured_profile.has(TerrainTrait::LabVictory);
        let last_owned_lab_lost = previous_owner.as_deref().is_some_and(|owner| {
            !next.board.tiles.iter().flatten().any(|candidate| {
                candidate.terrain == TerrainId::Lab
                    && candidate
                        .owner
                        .as_ref()
                        .and_then(Option::as_deref)
                        .is_some_and(|candidate_owner| candidate_owner == owner)
            })
        });
        if (defeats_owner || (no_hq_on_map && is_lab && last_owned_lab_lost))
            && let Some(previous_owner) = previous_owner
        {
            let cause = if defeats_owner {
                "hq-capture"
            } else {
                "lab-capture"
            };
            eliminate_player(
                &mut next,
                &previous_owner,
                cause,
                Some(player),
                Some(destination),
                &mut events,
            )?;
        }
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: 0,
    })
}

fn occupancy_is_disclosed(
    visibility: &impl Visibility,
    state: &State,
    actor_team: &str,
    unit: &Unit,
) -> bool {
    let owner_is_ally = state
        .players
        .iter()
        .find(|player| player.id == unit.owner)
        .is_some_and(|player| player.team == actor_team);
    owner_is_ally || visibility.visible_unit(state, actor_team, unit)
}

fn board_position(unit: &Unit) -> Option<Position> {
    match unit.location {
        Location::Board { position } => Some(position),
        Location::Cargo { .. } => None,
    }
}

fn owned_unit_count(state: &State, player: &str) -> Result<u64, ExecuteError> {
    u64::try_from(
        state
            .units
            .iter()
            .filter(|unit| unit.owner == player)
            .count(),
    )
    .map_err(|_| ExecuteError::InvalidState("owned unit count exceeds u64".into()))
}

fn reset_capture_on_departure(
    state: &mut State,
    unit_id: UnitId,
    origin: Position,
    actual_path: &[Position],
    events: &mut Vec<Value>,
) {
    if actual_path.len() < 2 || !state.units.iter().any(|unit| unit.id == unit_id) {
        return;
    }
    let tile = &mut state.board.tiles[origin[1]][origin[0]];
    if let Some(before) = tile.capture_points.filter(|points| *points < 20) {
        tile.capture_points = Some(20);
        events.push(json!({
            "type":"capture-changed","position":origin,"from":before,"to":20
        }));
    }
}

fn complete_match(state: &mut State, outcome: Outcome, events: &mut Vec<Value>) {
    state.match_state = Match::Finished {
        outcome: outcome.clone(),
    };
    state.turn.phase = Phase::Finished;
    events.push(json!({"type":"match-completed","outcome":outcome}));
}

fn capture_limit_count(state: &State, player: &str) -> u64 {
    state
        .board
        .tiles
        .iter()
        .flatten()
        .filter(|tile| {
            tile.owner
                .as_ref()
                .and_then(Option::as_deref)
                .is_some_and(|owner| owner == player)
                && ruleset::terrain_has(tile.terrain, TerrainTrait::CountsTowardCaptureLimit)
        })
        .count() as u64
}

fn day_limit_outcome(state: &State) -> Result<Outcome, ExecuteError> {
    let mut scores = Vec::new();
    for player in state
        .players
        .iter()
        .filter(|player| player.status == PlayerStatus::Active)
    {
        let properties = state
            .board
            .tiles
            .iter()
            .flatten()
            .filter(|tile| {
                tile.owner
                    .as_ref()
                    .and_then(Option::as_deref)
                    .is_some_and(|owner| player.id == owner)
            })
            .count();
        scores.push((player.team.clone(), properties));
    }
    let maximum = scores
        .iter()
        .map(|(_, properties)| *properties)
        .max()
        .ok_or_else(|| ExecuteError::InvalidState("day limit has no active players".into()))?;
    let mut leading_teams: Vec<_> = scores
        .into_iter()
        .filter(|(_, properties)| *properties == maximum)
        .map(|(team, _)| team)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    leading_teams.sort();
    Ok(if leading_teams.len() == 1 {
        Outcome::Victory {
            winners: leading_teams,
            reason: "day-limit".into(),
        }
    } else {
        Outcome::Draw {
            teams: leading_teams,
            reason: "day-limit".into(),
        }
    })
}

fn eliminate_player(
    state: &mut State,
    defeated_player: &str,
    cause: &str,
    beneficiary: Option<&str>,
    trigger_hq: Option<Position>,
    events: &mut Vec<Value>,
) -> Result<bool, ExecuteError> {
    let player_index = state
        .players
        .iter()
        .position(|player| player.id == defeated_player)
        .ok_or(ExecuteError::UnsupportedRuleset)?;
    let defeated_team = state.players[player_index].team.clone();
    let previous_status = state.players[player_index].status.clone();
    state.players[player_index].status = if cause == "resignation" {
        PlayerStatus::Resigned
    } else {
        PlayerStatus::Eliminated
    };
    events.push(json!({
        "type":"player-status-changed","player":defeated_player,
        "from":previous_status,"to":state.players[player_index].status
    }));
    if state
        .players
        .iter()
        .filter(|player| player.team == defeated_team)
        .all(|player| player.status != PlayerStatus::Active)
    {
        let team = state
            .teams
            .iter_mut()
            .find(|team| team.id == defeated_team)
            .ok_or(ExecuteError::UnsupportedRuleset)?;
        team.status = TeamStatus::Eliminated;
        events.push(json!({
            "type":"team-eliminated","team":defeated_team,"reason":cause
        }));
    }
    let mut surviving_teams: Vec<_> = state
        .teams
        .iter()
        .filter(|team| {
            state
                .players
                .iter()
                .any(|player| player.team == team.id && player.status == PlayerStatus::Active)
        })
        .map(|team| team.id.clone())
        .collect();
    surviving_teams.sort();
    if surviving_teams.len() == 1 {
        let outcome = Outcome::Victory {
            winners: surviving_teams,
            reason: cause.into(),
        };
        complete_match(state, outcome, events);
        return Ok(true);
    }

    let mut unit_ids: Vec<_> = state
        .units
        .iter()
        .filter(|unit| unit.owner == defeated_player)
        .map(|unit| unit.id)
        .collect();
    unit_ids.sort();
    for unit_id in unit_ids {
        let unit_index = state
            .units
            .iter()
            .position(|unit| unit.id == unit_id)
            .expect("elimination unit remains present until its pass");
        if let Some(position) = board_position(&state.units[unit_index]) {
            let tile = &mut state.board.tiles[position[1]][position[0]];
            if let Some(before) = tile.capture_points.filter(|points| *points < 20) {
                tile.capture_points = Some(20);
                events.push(json!({
                    "type":"capture-changed","position":position,"from":before,"to":20
                }));
            }
        }
        events.push(json!({
            "type":"unit-removed","unit":unit_id,"reason":"elimination"
        }));
        state.units.remove(unit_index);
    }

    let mut properties = Vec::new();
    for (y, row) in state.board.tiles.iter().enumerate() {
        for (x, tile) in row.iter().enumerate() {
            let position = [x, y];
            let owned = tile
                .owner
                .as_ref()
                .and_then(Option::as_deref)
                .is_some_and(|owner| owner == defeated_player);
            if owned || trigger_hq == Some(position) {
                properties.push(position);
            }
        }
    }
    for position in properties {
        let tile = &mut state.board.tiles[position[1]][position[0]];
        if let Some(before) = tile.capture_points.filter(|points| *points < 20) {
            tile.capture_points = Some(20);
            events.push(json!({
                "type":"capture-changed","position":position,"from":before,"to":20
            }));
        }
        if let Some(replacement) = ruleset::terrain(tile.terrain).elimination_replacement {
            let from = tile.terrain;
            tile.terrain = replacement;
            events.push(json!({
                "type":"tile-terrain-changed","position":position,
                "from":from,"to":replacement,"reason":"elimination"
            }));
        }
        let previous_owner = tile.owner.as_ref().and_then(Clone::clone);
        let next_owner = beneficiary.map(PlayerId::from);
        if previous_owner != next_owner {
            tile.owner = Some(next_owner.clone());
            events.push(json!({
                "type":"tile-owner-changed","position":position,
                "from":previous_owner,"to":next_owner
            }));
        }
    }
    Ok(false)
}

fn execute_move_concealment(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
    hide: bool,
) -> Result<Execution, ExecuteError> {
    let plan = validate_movement_prefix(state, player, unit_id, path)?;
    let original = &state.units[plan.unit_index];
    let supported = ruleset::profile(original.kind).concealment.is_some();
    let target = if hide {
        Concealment::Hidden
    } else {
        Concealment::Exposed
    };
    if !supported || original.concealment == target {
        return Err(violation(json!({
            "code":"ACTION_NOT_SUPPORTED",
            "action":if hide {"move-hide"} else {"move-reveal"}
        })));
    }

    let destination = *plan.path.last().expect("origin was checked");
    let visibility = AwbwVisibility;
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(destination)
            && occupancy_is_disclosed(&visibility, state, &plan.actor_team, other)
    }) {
        return Err(violation(
            json!({"code":"DESTINATION_OCCUPIED","position":destination}),
        ));
    }

    let mut outcome = execute_planned_movement(state, unit_id, &plan);
    if outcome.trapped {
        return Ok(Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        });
    }
    let unit = &mut outcome.state.units[plan.unit_index];
    let from = unit.concealment.clone();
    unit.concealment = target.clone();
    outcome.events.push(json!({
        "type":"concealment-changed",
        "unit":unit_id,
        "from":from,
        "to":target
    }));
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

fn turns_until_player_selection(state: &State, player: &str) -> Result<u64, ExecuteError> {
    let order_len = state.turn.order.len();
    if order_len == 0 || state.turn.position >= order_len {
        return Err(ExecuteError::InvalidState(
            "turn order or position is invalid".into(),
        ));
    }
    let mut index = state.turn.position;
    let mut selections = 0_u64;
    for _ in 0..order_len {
        index = (index + 1) % order_len;
        let candidate_id = &state.turn.order[index];
        let candidate = state
            .players
            .iter()
            .find(|candidate| candidate.id == *candidate_id)
            .ok_or_else(|| {
                ExecuteError::InvalidState("turn order names a missing player".into())
            })?;
        if candidate.status != PlayerStatus::Active {
            continue;
        }
        selections = selections.checked_add(1).ok_or_else(|| {
            ExecuteError::InvalidState("weather duration selection count overflow".into())
        })?;
        if candidate.id == player {
            return Ok(selections);
        }
    }
    Err(ExecuteError::InvalidState(
        "active player is not selectable in turn order".into(),
    ))
}

fn area_strike_centers(
    state: &State,
    player: &str,
    radius: usize,
    policies: &[AreaStrikePolicy],
) -> Result<Vec<Position>, ExecuteError> {
    let actor_team = state
        .players
        .iter()
        .find(|candidate| candidate.id == player)
        .map(|candidate| candidate.team.as_str())
        .ok_or_else(|| ExecuteError::InvalidState("area-strike actor is missing".into()))?;
    let mut priced_units = Vec::new();
    for unit in &state.units {
        let Location::Board { position } = unit.location else {
            continue;
        };
        let base_cost = ruleset::profile(unit.kind).cost;
        let cost = commander::effective_build_cost(state, &unit.owner, base_cost)
            .ok_or_else(|| ExecuteError::InvalidState("area-strike cost overflow".into()))?;
        let friendly = state
            .players
            .iter()
            .find(|candidate| candidate.id == unit.owner)
            .is_some_and(|owner| owner.team == actor_team);
        let capturing = matches!(unit.kind.as_str(), "infantry" | "mech")
            && state.board.tiles[position[1]][position[0]]
                .capture_points
                .is_some_and(|points| points < 20);
        priced_units.push((unit, position, cost, friendly, capturing));
    }

    let mut centers = Vec::with_capacity(policies.len());
    for policy in policies {
        let mut best: Option<(i128, i128, Position)> = None;
        for y in 0..state.board.height {
            for x in 0..state.board.width {
                let center = [x, y];
                let mut score = 0_i128;
                let mut enemy_tiebreak = 0_i128;
                for (unit, position, cost, friendly, capturing) in &priced_units {
                    if center[0].abs_diff(position[0]) + center[1].abs_diff(position[1]) > radius {
                        continue;
                    }
                    let exact_hp = i128::from(unit.hp);
                    let capped_hp = exact_hp.clamp(1, 30);
                    let cost = i128::from(*cost);
                    let value = match policy {
                        AreaStrikePolicy::InfantryHp => {
                            let multiplier = if matches!(unit.kind.as_str(), "infantry" | "mech")
                                && unit.hp > 10
                            {
                                if *capturing { 8 } else { 4 }
                            } else {
                                1
                            };
                            capped_hp.checked_mul(multiplier).ok_or_else(|| {
                                ExecuteError::InvalidState(
                                    "area-strike infantry score overflow".into(),
                                )
                            })?
                        }
                        AreaStrikePolicy::UnitValue => {
                            if unit.hp < 10 {
                                2
                            } else {
                                capped_hp.checked_mul(cost).ok_or_else(|| {
                                    ExecuteError::InvalidState(
                                        "area-strike cost score overflow".into(),
                                    )
                                })?
                            }
                        }
                        AreaStrikePolicy::UnitHp => capped_hp,
                    };
                    score = if *friendly {
                        score.checked_sub(value)
                    } else {
                        score.checked_add(value)
                    }
                    .ok_or_else(|| {
                        ExecuteError::InvalidState("area-strike score overflow".into())
                    })?;
                    if !friendly {
                        let tie_value = match policy {
                            AreaStrikePolicy::InfantryHp => 0,
                            AreaStrikePolicy::UnitValue => {
                                exact_hp.checked_mul(cost).ok_or_else(|| {
                                    ExecuteError::InvalidState(
                                        "area-strike cost tiebreak overflow".into(),
                                    )
                                })?
                            }
                            AreaStrikePolicy::UnitHp => exact_hp,
                        };
                        enemy_tiebreak =
                            enemy_tiebreak.checked_add(tie_value).ok_or_else(|| {
                                ExecuteError::InvalidState("area-strike tiebreak overflow".into())
                            })?;
                    }
                }
                if best.as_ref().is_none_or(|(best_score, best_tie, _)| {
                    score > *best_score || (score == *best_score && enemy_tiebreak > *best_tie)
                }) {
                    best = Some((score, enemy_tiebreak, center));
                }
            }
        }
        centers.push(
            best.map(|(_, _, center)| center)
                .ok_or_else(|| ExecuteError::InvalidState("area-strike board is empty".into()))?,
        );
    }
    Ok(centers)
}

fn execute_activate_power(
    state: &State,
    player: &str,
    level: PowerLevel,
) -> Result<Execution, ExecuteError> {
    if state.ruleset.id != "awbw" || state.ruleset.revision != "2026-07-10" {
        return Err(ExecuteError::UnsupportedRuleset);
    }
    if matches!(state.match_state, Match::Finished { .. }) {
        return Err(violation(json!({"code":"MATCH_FINISHED"})));
    }
    if state.turn.phase != Phase::UnitAction {
        return Err(violation(json!({
            "code":"WRONG_PHASE", "expected":"unit-action", "actual":state.turn.phase
        })));
    }
    if state.turn.active_player != player {
        return Err(violation(
            json!({"code":"NOT_ACTIVE_PLAYER","player":player}),
        ));
    }
    let player_index = state
        .players
        .iter()
        .position(|candidate| candidate.id == player)
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent from players".into()))?;
    let actor = &state.players[player_index];
    let active_slot = actor
        .commanders
        .iter()
        .position(|candidate| candidate.active)
        .ok_or_else(|| ExecuteError::InvalidState("player has no active commander".into()))?;
    let active_slot = u8::try_from(active_slot)
        .map_err(|_| ExecuteError::InvalidState("active commander slot exceeds u8".into()))?;
    let active_commander = &actor.commanders[usize::from(active_slot)];
    if state.settings.powers != crate::semantic::Toggle::Enabled
        || !matches!(actor.power_state, crate::semantic::PowerState::None)
    {
        return Err(violation(
            json!({"code":"ACTION_NOT_SUPPORTED","action":"activate-power"}),
        ));
    }
    let activation =
        commander::power_activation(active_commander.id, level, active_commander.power_uses)
            .map_err(|error| {
                ExecuteError::InvalidState(format!(
                    "commander power profile cannot activate: {error:?}"
                ))
            })?
            .ok_or_else(|| {
                violation(json!({
                    "code":"ACTION_NOT_SUPPORTED", "action":"activate-power"
                }))
            })?;
    let cost = activation.cost;
    if active_commander.power_charge < cost {
        return Err(violation(json!({
            "code":"INSUFFICIENT_POWER",
            "required":cost,
            "available":active_commander.power_charge
        })));
    }

    let mut next = state.clone();
    let commander = &mut next.players[player_index].commanders[usize::from(active_slot)];
    commander.power_charge -= cost;
    commander.power_uses = commander
        .power_uses
        .checked_add(1)
        .ok_or_else(|| ExecuteError::InvalidState("commander power uses overflow".into()))?;
    next.players[player_index].power_state = match level {
        PowerLevel::Cop => crate::semantic::PowerState::Cop {
            commander_slot: active_slot,
        },
        PowerLevel::Scop => crate::semantic::PowerState::Scop {
            commander_slot: active_slot,
        },
    };
    let mut events = vec![json!({
        "type":"power-activated",
        "player":player,
        "commander":active_commander.id,
        "power":level
    })];
    for effect in activation.instant_effects {
        match effect {
            InstantEffect::HealVisualHp {
                target: UnitTarget::Owned,
                amount,
            } => {
                let mut targets: Vec<_> = next
                    .units
                    .iter()
                    .filter(|unit| unit.owner == player)
                    .map(|unit| unit.id)
                    .collect();
                targets.sort();
                for target_id in targets {
                    let target = next
                        .units
                        .iter_mut()
                        .find(|unit| unit.id == target_id)
                        .expect("power target remains present");
                    let from_hp = target.hp;
                    let visual_hp = from_hp.div_ceil(10);
                    let to_hp = visual_hp.saturating_add(amount).min(10) * 10;
                    if to_hp == from_hp {
                        continue;
                    }
                    target.hp = to_hp;
                    events.push(json!({
                        "type":"unit-repaired", "unit":target_id,
                        "from_hp":from_hp, "to_hp":to_hp,
                        "reason":"commander-power"
                    }));
                }
            }
            InstantEffect::HealExactHp {
                target: UnitTarget::Owned,
                amount,
            } => {
                let mut targets: Vec<_> = next
                    .units
                    .iter()
                    .filter(|unit| unit.owner == player)
                    .map(|unit| unit.id)
                    .collect();
                targets.sort();
                for target_id in targets {
                    let target = next
                        .units
                        .iter_mut()
                        .find(|unit| unit.id == target_id)
                        .expect("power target remains present");
                    let from_hp = target.hp;
                    let to_hp = from_hp.saturating_add(amount).min(100);
                    if to_hp == from_hp {
                        continue;
                    }
                    target.hp = to_hp;
                    events.push(json!({
                        "type":"unit-repaired", "unit":target_id,
                        "from_hp":from_hp, "to_hp":to_hp,
                        "reason":"commander-power"
                    }));
                }
            }
            InstantEffect::DamageExactHp {
                target: target @ (UnitTarget::Enemy | UnitTarget::EnemyOnProperties),
                amount,
                minimum_hp,
            } => {
                let actor_team = next.players[player_index].team.clone();
                let properties_only = target == UnitTarget::EnemyOnProperties;
                let enemy_owners: HashSet<_> = next
                    .players
                    .iter()
                    .filter(|candidate| candidate.team != actor_team)
                    .map(|candidate| candidate.id.as_str())
                    .collect();
                let mut targets: Vec<_> = next
                    .units
                    .iter()
                    .filter(|unit| {
                        enemy_owners.contains(unit.owner.as_str())
                            && (!properties_only
                                || match unit.location {
                                    Location::Board { position } => ruleset::terrain_has(
                                        next.board.tiles[position[1]][position[0]].terrain,
                                        TerrainTrait::Capturable,
                                    ),
                                    Location::Cargo { .. } => false,
                                })
                    })
                    .map(|unit| unit.id)
                    .collect();
                targets.sort();
                for target_id in targets {
                    let target = next
                        .units
                        .iter_mut()
                        .find(|unit| unit.id == target_id)
                        .expect("power target remains present");
                    let from_hp = target.hp;
                    let to_hp = from_hp.saturating_sub(amount).max(minimum_hp);
                    if to_hp == from_hp {
                        continue;
                    }
                    target.hp = to_hp;
                    events.push(json!({
                        "type":"unit-damaged", "unit":target_id,
                        "from_hp":from_hp, "to_hp":to_hp,
                        "reason":"commander-power"
                    }));
                }
            }
            InstantEffect::SetWeather {
                kind,
                duration: WeatherDuration::UntilOwnerNextTurn,
            } => {
                let remaining_turns = turns_until_player_selection(&next, player)?;
                let from = next.weather.kind;
                let to = match kind {
                    WeatherEffectKind::Clear => WeatherKind::Clear,
                    WeatherEffectKind::Rain => WeatherKind::Rain,
                    WeatherEffectKind::Snow => WeatherKind::Snow,
                };
                if next.weather.kind == to && next.weather.remaining_turns == remaining_turns {
                    continue;
                }
                next.weather.kind = to;
                next.weather.remaining_turns = remaining_turns;
                events.push(json!({
                    "type":"weather-changed", "from":from, "to":next.weather.kind,
                    "remaining_turns":remaining_turns, "reason":"commander-power"
                }));
            }
            InstantEffect::DrainCurrentFuelRatio {
                target: UnitTarget::Enemy,
                numerator,
                denominator,
            } => {
                let actor_team = next.players[player_index].team.clone();
                let enemy_owners: HashSet<_> = next
                    .players
                    .iter()
                    .filter(|candidate| candidate.team != actor_team)
                    .map(|candidate| candidate.id.as_str())
                    .collect();
                let mut targets: Vec<_> = next
                    .units
                    .iter()
                    .filter(|unit| enemy_owners.contains(unit.owner.as_str()))
                    .map(|unit| unit.id)
                    .collect();
                targets.sort();
                for target_id in targets {
                    let target = next
                        .units
                        .iter_mut()
                        .find(|unit| unit.id == target_id)
                        .expect("power target remains present");
                    let fuel_before = target.fuel;
                    let drained = fuel_before
                        .checked_mul(numerator)
                        .and_then(|value| value.checked_div(denominator))
                        .ok_or_else(|| {
                            ExecuteError::InvalidState(
                                "fuel-drain ratio arithmetic overflow".into(),
                            )
                        })?;
                    let fuel_after = fuel_before.saturating_sub(drained);
                    if fuel_after == fuel_before {
                        continue;
                    }
                    target.fuel = fuel_after;
                    events.push(json!({
                        "type":"unit-resourced", "unit":target_id,
                        "fuel_before":fuel_before, "fuel_after":fuel_after,
                        "ammo_before":target.ammo, "ammo_after":target.ammo,
                        "reason":"commander-power"
                    }));
                }
            }
            InstantEffect::FireAreaStrikes {
                target: UnitTarget::AllBoard,
                radius,
                damage,
                minimum_hp,
                selection_policies,
                friendly_contribution: FriendlyContribution::Subtract,
            } => {
                let centers = area_strike_centers(state, player, radius, &selection_policies)?;
                for (strike, (policy, center)) in
                    selection_policies.into_iter().zip(centers).enumerate()
                {
                    events.push(json!({
                        "type":"area-strike-resolved",
                        "strike":strike,
                        "policy":policy,
                        "center":center,
                        "radius":radius,
                        "damage":damage
                    }));
                    let mut targets: Vec<_> = next
                        .units
                        .iter()
                        .filter_map(|unit| match unit.location {
                            Location::Board { position }
                                if center[0].abs_diff(position[0])
                                    + center[1].abs_diff(position[1])
                                    <= radius =>
                            {
                                Some(unit.id)
                            }
                            _ => None,
                        })
                        .collect();
                    targets.sort();
                    for target_id in targets {
                        let target = next
                            .units
                            .iter_mut()
                            .find(|unit| unit.id == target_id)
                            .expect("area-strike target remains present");
                        let from_hp = target.hp;
                        let to_hp = from_hp.saturating_sub(damage).max(minimum_hp);
                        if to_hp == from_hp {
                            continue;
                        }
                        target.hp = to_hp;
                        events.push(json!({
                            "type":"unit-damaged", "unit":target_id,
                            "from_hp":from_hp, "to_hp":to_hp,
                            "reason":"commander-power"
                        }));
                    }
                }
            }
            InstantEffect::ReducePowerChargeByFundsRatio {
                target: CommanderSlotTarget::EnemyCommanderSlots,
                funds_per_full_bar,
            } => {
                let actor_team = next.players[player_index].team.clone();
                let actor_funds = next.players[player_index].funds;
                let mut target_players: Vec<_> = next
                    .players
                    .iter()
                    .enumerate()
                    .filter(|(_, candidate)| candidate.team != actor_team)
                    .map(|(index, candidate)| (index, candidate.id.clone()))
                    .collect();
                target_players.sort_by(|left, right| left.1.cmp(&right.1));
                for (target_player_index, target_player_id) in target_players {
                    for commander_slot in 0..next.players[target_player_index].commanders.len() {
                        let target = &next.players[target_player_index].commanders[commander_slot];
                        let from = target.power_charge;
                        if from == 0 {
                            continue;
                        }
                        let full_bar =
                            commander::maximum_power_charge(target.id, target.power_uses)
                                .map_err(|error| {
                                    ExecuteError::InvalidState(format!(
                                        "enemy power profile cannot compute full bar: {error:?}"
                                    ))
                                })?
                                .ok_or_else(|| {
                                    ExecuteError::InvalidState(format!(
                                        "enemy commander {} has no complete power profile",
                                        target.id
                                    ))
                                })?;
                        let reduction = actor_funds
                            .checked_mul(full_bar)
                            .and_then(|value| value.checked_div(funds_per_full_bar))
                            .ok_or_else(|| {
                                ExecuteError::InvalidState(
                                    "power-charge reduction arithmetic overflow".into(),
                                )
                            })?;
                        let to = from.saturating_sub(reduction);
                        if to == from {
                            continue;
                        }
                        next.players[target_player_index].commanders[commander_slot].power_charge =
                            to;
                        events.push(json!({
                            "type":"power-charge-changed",
                            "player":target_player_id,
                            "commander_slot":commander_slot,
                            "from":from,
                            "to":to,
                            "reason":"commander-power"
                        }));
                    }
                }
            }
            InstantEffect::RefreshUnitAction {
                target: UnitTarget::Owned,
                exclude_unit_kinds,
            } => {
                let mut targets: Vec<_> = next
                    .units
                    .iter()
                    .filter(|unit| unit.owner == player && !exclude_unit_kinds.contains(&unit.kind))
                    .map(|unit| unit.id)
                    .collect();
                targets.sort();
                for target_id in targets {
                    let target = next
                        .units
                        .iter_mut()
                        .find(|unit| unit.id == target_id)
                        .expect("power target remains present");
                    if target.action != UnitAction::Spent {
                        continue;
                    }
                    let from = target.action.clone();
                    target.action = UnitAction::Ready;
                    events.push(json!({
                        "type":"unit-action-changed", "unit":target_id,
                        "from":from, "to":"ready", "reason":"commander-power"
                    }));
                }
            }
            InstantEffect::ResupplyUnits {
                target: UnitTarget::Owned,
            } => {
                let mut targets: Vec<_> = next
                    .units
                    .iter()
                    .filter(|unit| unit.owner == player)
                    .map(|unit| unit.id)
                    .collect();
                targets.sort();
                for target_id in targets {
                    let target = next
                        .units
                        .iter_mut()
                        .find(|unit| unit.id == target_id)
                        .expect("power target remains present");
                    let fuel_before = target.fuel;
                    let ammo_before = target.ammo;
                    if !refill_unit(target) {
                        continue;
                    }
                    events.push(json!({
                        "type":"unit-resourced", "unit":target_id,
                        "fuel_before":fuel_before, "fuel_after":target.fuel,
                        "ammo_before":ammo_before, "ammo_after":target.ammo,
                        "reason":"commander-power"
                    }));
                }
            }
            InstantEffect::SpawnUnitsOnOwnedProperties {
                target: PropertyTarget::OwnedProperties,
                property_kinds,
                unit_kind,
                hp,
                resources: SpawnResources::UnitMaxima,
                action: SpawnAction::Ready,
                concealment: SpawnConcealment::Exposed,
                occupied_tiles: OccupiedTileHandling::Skip,
                order: PropertyOrder::YThenX,
                unit_limit: SpawnUnitLimit::Settings,
            } => {
                let profile = ruleset::profile(unit_kind);
                let max_fuel = profile.max_fuel;
                let max_ammo = profile.max_ammo;
                let mut positions = Vec::new();
                let mut owned_unit_count = owned_unit_count(&next, player)?;
                'rows: for (y, row) in next.board.tiles.iter().enumerate() {
                    for (x, tile) in row.iter().enumerate() {
                        if next
                            .settings
                            .unit_limit
                            .is_some_and(|limit| owned_unit_count >= limit)
                        {
                            break 'rows;
                        }
                        if tile.owner.as_ref().and_then(Option::as_deref) != Some(player) {
                            continue;
                        }
                        let property_kind = ruleset::terrain(tile.terrain).property_kind;
                        if !property_kind.is_some_and(|kind| property_kinds.contains(&kind)) {
                            continue;
                        }
                        let position = [x, y];
                        if next
                            .units
                            .iter()
                            .any(|unit| board_position(unit) == Some(position))
                        {
                            continue;
                        }
                        positions.push(position);
                        owned_unit_count = owned_unit_count.checked_add(1).ok_or_else(|| {
                            ExecuteError::InvalidState("owned unit count overflow".into())
                        })?;
                    }
                }
                if positions.is_empty() {
                    continue;
                }
                let first_id = next.next_unit_id.ok_or_else(|| {
                    ExecuteError::InvalidState("unit-spawning power requires next_unit_id".into())
                })?;
                let count = u32::try_from(positions.len())
                    .map_err(|_| ExecuteError::InvalidState("spawn count exceeds u32".into()))?;
                let after_id = first_id
                    .checked_add(count)
                    .ok_or_else(|| ExecuteError::InvalidState("next_unit_id overflow".into()))?;
                for offset in 0..count {
                    let allocated_id = UnitId::new(first_id + offset);
                    if next.units.iter().any(|unit| unit.id == allocated_id) {
                        return Err(ExecuteError::InvalidState(format!(
                            "next_unit_id {} is not fresh",
                            first_id + offset
                        )));
                    }
                }
                next.next_unit_id = Some(after_id);
                for (offset, position) in positions.into_iter().enumerate() {
                    let offset = u32::try_from(offset).expect("spawn offset fits validated count");
                    let allocated_id = UnitId::new(first_id + offset);
                    next.units.push(Unit {
                        id: allocated_id,
                        kind: unit_kind,
                        owner: player.into(),
                        hp,
                        fuel: max_fuel,
                        ammo: max_ammo,
                        action: UnitAction::Ready,
                        concealment: Concealment::Exposed,
                        location: Location::Board { position },
                    });
                    events.push(json!({
                        "type":"unit-created", "unit":allocated_id,
                        "kind":unit_kind, "owner":player, "position":position
                    }));
                }
            }
            InstantEffect::FireTargetedAreaStrike {
                target: AreaStrikeCenterTarget::EnemyUnitCenters,
                radius,
                damage,
                minimum_hp,
                selection_policy: TargetedAreaStrikePolicy::UnitValue,
                friendly_contribution: FriendlyContribution::Subtract,
                unit_value: TargetedUnitValue::BaseBuildCost,
            } => {
                let actor_team = next.players[player_index].team.clone();
                let enemy_owners: HashSet<_> = next
                    .players
                    .iter()
                    .filter(|candidate| candidate.team != actor_team)
                    .map(|candidate| candidate.id.as_str())
                    .collect();
                let mut candidates: Vec<_> = next
                    .units
                    .iter()
                    .filter_map(|unit| match unit.location {
                        Location::Board { position }
                            if enemy_owners.contains(unit.owner.as_str()) =>
                        {
                            Some(position)
                        }
                        _ => None,
                    })
                    .collect();
                candidates.sort_by_key(|position| (position[1], position[0]));
                let mut best: Option<(i128, Position)> = None;
                for center in candidates {
                    let mut score = 0_i128;
                    for unit in &next.units {
                        let Location::Board { position } = unit.location else {
                            continue;
                        };
                        if center[0].abs_diff(position[0]) + center[1].abs_diff(position[1])
                            > radius
                        {
                            continue;
                        }
                        let cost = ruleset::profile(unit.kind).cost;
                        let value = i128::from(unit.hp.div_ceil(10))
                            .checked_mul(i128::from(cost))
                            .ok_or_else(|| {
                                ExecuteError::InvalidState(
                                    "targeted area-strike score overflow".into(),
                                )
                            })?;
                        let friendly = next
                            .players
                            .iter()
                            .find(|candidate| candidate.id == unit.owner)
                            .is_some_and(|owner| owner.team == actor_team);
                        score = if friendly {
                            score.checked_sub(value)
                        } else {
                            score.checked_add(value)
                        }
                        .ok_or_else(|| {
                            ExecuteError::InvalidState("targeted area-strike score overflow".into())
                        })?;
                    }
                    if best
                        .as_ref()
                        .is_none_or(|(best_score, _)| score > *best_score)
                    {
                        best = Some((score, center));
                    }
                }
                let Some((_, center)) = best else {
                    continue;
                };
                events.push(json!({
                    "type":"area-strike-resolved", "strike":0,
                    "policy":"unit-value", "center":center,
                    "radius":radius, "damage":damage
                }));
                let mut targets: Vec<_> = next
                    .units
                    .iter()
                    .filter_map(|unit| match unit.location {
                        Location::Board { position }
                            if center[0].abs_diff(position[0])
                                + center[1].abs_diff(position[1])
                                <= radius =>
                        {
                            Some(unit.id)
                        }
                        _ => None,
                    })
                    .collect();
                targets.sort();
                for target_id in targets {
                    let target = next
                        .units
                        .iter_mut()
                        .find(|unit| unit.id == target_id)
                        .expect("targeted area-strike target remains present");
                    let from_hp = target.hp;
                    let to_hp = from_hp.saturating_sub(damage).max(minimum_hp);
                    if to_hp == from_hp {
                        continue;
                    }
                    target.hp = to_hp;
                    events.push(json!({
                        "type":"unit-damaged", "unit":target_id,
                        "from_hp":from_hp, "to_hp":to_hp,
                        "reason":"commander-power"
                    }));
                }
            }
            InstantEffect::FireImmobilizingAreaStrike {
                target: UnitTarget::Enemy,
                radius,
                damage,
                minimum_hp,
                selection_policy: TargetedAreaStrikePolicy::UnitValue,
                friendly_contribution: FriendlyContribution::Subtract,
                unit_value: TargetedUnitValue::BaseBuildCost,
                duration: ImmobilizationDuration::ThroughTargetNextTurn,
            } => {
                let actor_team = next.players[player_index].team.clone();
                let enemy_owners: HashSet<_> = next
                    .players
                    .iter()
                    .filter(|candidate| candidate.team != actor_team)
                    .map(|candidate| candidate.id.as_str())
                    .collect();
                let mut priced_units = Vec::new();
                for unit in &state.units {
                    let Location::Board { position } = unit.location else {
                        continue;
                    };
                    let cost = ruleset::profile(unit.kind).cost;
                    let friendly = state
                        .players
                        .iter()
                        .find(|candidate| candidate.id == unit.owner)
                        .is_some_and(|owner| owner.team == actor_team);
                    priced_units.push((unit, position, cost, friendly));
                }
                let mut best: Option<(i128, i128, Position)> = None;
                for y in 0..state.board.height {
                    for x in 0..state.board.width {
                        let center = [x, y];
                        let mut score = 0_i128;
                        let mut enemy_tiebreak = 0_i128;
                        for (unit, position, cost, friendly) in &priced_units {
                            if center[0].abs_diff(position[0]) + center[1].abs_diff(position[1])
                                > radius
                            {
                                continue;
                            }
                            let exact_hp = i128::from(unit.hp);
                            let cost = i128::from(*cost);
                            let value = if unit.hp < 10 {
                                2
                            } else {
                                exact_hp.clamp(1, 30).checked_mul(cost).ok_or_else(|| {
                                    ExecuteError::InvalidState(
                                        "immobilizing area-strike score overflow".into(),
                                    )
                                })?
                            };
                            score = if *friendly {
                                score.checked_sub(value)
                            } else {
                                score.checked_add(value)
                            }
                            .ok_or_else(|| {
                                ExecuteError::InvalidState(
                                    "immobilizing area-strike score overflow".into(),
                                )
                            })?;
                            if !friendly {
                                enemy_tiebreak = enemy_tiebreak
                                    .checked_add(exact_hp.checked_mul(cost).ok_or_else(|| {
                                        ExecuteError::InvalidState(
                                            "immobilizing area-strike tiebreak overflow".into(),
                                        )
                                    })?)
                                    .ok_or_else(|| {
                                        ExecuteError::InvalidState(
                                            "immobilizing area-strike tiebreak overflow".into(),
                                        )
                                    })?;
                            }
                        }
                        if best.as_ref().is_none_or(|(best_score, best_tie, _)| {
                            score > *best_score
                                || (score == *best_score && enemy_tiebreak > *best_tie)
                        }) {
                            best = Some((score, enemy_tiebreak, center));
                        }
                    }
                }
                let center = best.map(|(_, _, center)| center).ok_or_else(|| {
                    ExecuteError::InvalidState("immobilizing area-strike board is empty".into())
                })?;
                events.push(json!({
                    "type":"area-strike-resolved", "strike":0,
                    "policy":"unit-value", "center":center,
                    "radius":radius, "damage":damage
                }));
                let mut targets: Vec<_> = next
                    .units
                    .iter()
                    .filter_map(|unit| match unit.location {
                        Location::Board { position }
                            if enemy_owners.contains(unit.owner.as_str())
                                && center[0].abs_diff(position[0])
                                    + center[1].abs_diff(position[1])
                                    <= radius =>
                        {
                            Some(unit.id)
                        }
                        _ => None,
                    })
                    .collect();
                targets.sort();
                for target_id in targets {
                    let target = next
                        .units
                        .iter_mut()
                        .find(|unit| unit.id == target_id)
                        .expect("immobilizing area-strike target remains present");
                    let from_hp = target.hp;
                    let to_hp = from_hp.saturating_sub(damage).max(minimum_hp);
                    if to_hp != from_hp {
                        target.hp = to_hp;
                        events.push(json!({
                            "type":"unit-damaged", "unit":target_id,
                            "from_hp":from_hp, "to_hp":to_hp,
                            "reason":"commander-power"
                        }));
                    }
                    if target.action != UnitAction::Immobilized {
                        let from = target.action.clone();
                        target.action = UnitAction::Immobilized;
                        events.push(json!({
                            "type":"unit-action-changed", "unit":target_id,
                            "from":from, "to":"immobilized",
                            "reason":"commander-power"
                        }));
                    }
                }
            }
            InstantEffect::MultiplyFundsRatio {
                target: PlayerTarget::ActivatingPlayer,
                numerator,
                denominator,
            } => {
                let from = next.players[player_index].funds;
                let exact = u128::from(from)
                    .checked_mul(u128::from(numerator))
                    .and_then(|value| value.checked_div(u128::from(denominator)))
                    .ok_or_else(|| {
                        ExecuteError::InvalidState("invalid funds multiplier ratio".into())
                    })?;
                let to = u64::try_from(exact)
                    .map_err(|_| ExecuteError::InvalidState("player funds overflow".into()))?;
                if to == from {
                    continue;
                }
                next.players[player_index].funds = to;
                events.push(json!({
                    "type":"funds-changed", "player":player,
                    "from":from, "to":to, "reason":"commander-power"
                }));
            }
            unsupported => {
                return Err(ExecuteError::InvalidState(format!(
                    "unsupported instant-effect target combination: {unsupported:?}"
                )));
            }
        }
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: 0,
    })
}

fn execute_end_turn(
    state: &State,
    player: &str,
    random: &[Value],
) -> Result<Execution, ExecuteError> {
    execute_turn_boundary(state, player, BoundaryCommand::EndTurn, random)
}

fn execute_tag(state: &State, player: &str, random: &[Value]) -> Result<Execution, ExecuteError> {
    execute_turn_boundary(state, player, BoundaryCommand::Tag, random)
}

fn execute_resign(
    state: &State,
    player: &str,
    random: &[Value],
) -> Result<Execution, ExecuteError> {
    execute_turn_boundary(state, player, BoundaryCommand::Resign, random)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundaryCommand {
    EndTurn,
    Tag,
    Resign,
}

fn execute_turn_boundary(
    state: &State,
    player: &str,
    command: BoundaryCommand,
    random: &[Value],
) -> Result<Execution, ExecuteError> {
    if state.ruleset.id != "awbw" || state.ruleset.revision != "2026-07-10" {
        return Err(ExecuteError::UnsupportedRuleset);
    }
    if matches!(state.match_state, Match::Finished { .. }) {
        return Err(violation(json!({"code":"MATCH_FINISHED"})));
    }
    if state.turn.phase != Phase::UnitAction {
        return Err(violation(json!({
            "code":"WRONG_PHASE", "expected":"unit-action", "actual":state.turn.phase
        })));
    }
    if state.turn.active_player != player {
        return Err(violation(
            json!({"code":"NOT_ACTIVE_PLAYER","player":player}),
        ));
    }
    let player_index = state
        .players
        .iter()
        .position(|candidate| candidate.id == player)
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent".into()))?;
    if command == BoundaryCommand::Tag && !state.settings.tags {
        return Err(violation(
            json!({"code":"ACTION_NOT_SUPPORTED","action":"tag"}),
        ));
    }
    if command == BoundaryCommand::Tag
        && (state.players[player_index].commanders.len() != 2
            || state.players[player_index]
                .commanders
                .iter()
                .filter(|commander| commander.active)
                .count()
                != 1)
    {
        return Err(ExecuteError::InvalidState(
            "tag player must have two commander slots and exactly one active slot".into(),
        ));
    }

    let mut next = state.clone();
    let mut random_consumed = 0;
    let mut events = vec![json!({
        "type":"phase-changed", "player":player,
        "from":"unit-action", "to":"turn-end"
    })];
    next.turn.phase = Phase::TurnEnd;
    if command == BoundaryCommand::Tag {
        let from_slot = next.players[player_index]
            .commanders
            .iter()
            .position(|commander| commander.active)
            .expect("tag commander shape was checked");
        let to_slot = 1 - from_slot;
        let active_power = match next.players[player_index].power_state {
            PowerState::None => None,
            PowerState::Cop { commander_slot } => Some((commander_slot, PowerLevel::Cop)),
            PowerState::Scop { commander_slot } => Some((commander_slot, PowerLevel::Scop)),
        };
        if let Some((commander_slot, power)) = active_power {
            if usize::from(commander_slot) != from_slot {
                return Err(ExecuteError::InvalidState(
                    "power state does not name the active commander slot".into(),
                ));
            }
            let commander_id = next.players[player_index].commanders[from_slot].id;
            next.players[player_index].power_state = PowerState::None;
            events.push(json!({
                "type":"power-ended", "player":player,
                "commander":commander_id, "power":power
            }));
        }
        next.players[player_index].commanders[from_slot].active = false;
        next.players[player_index].commanders[to_slot].active = true;
        events.push(json!({
            "type":"commander-swapped", "player":player,
            "from_slot":from_slot, "to_slot":to_slot
        }));
    }
    if command == BoundaryCommand::Resign
        && eliminate_player(&mut next, player, "resignation", None, None, &mut events)?
    {
        return Ok(Execution {
            state: next,
            events,
            random_consumed,
        });
    }

    loop {
        let order_len = next.turn.order.len();
        if order_len == 0 || next.turn.position >= order_len {
            return Err(ExecuteError::InvalidState(
                "turn order and position do not identify an active player".into(),
            ));
        }
        if next.turn.order[next.turn.position] != next.turn.active_player {
            return Err(ExecuteError::InvalidState(
                "turn active_player does not equal order[position]".into(),
            ));
        }
        let successor = (1..=order_len).find_map(|offset| {
            let position = (next.turn.position + offset) % order_len;
            let id = &next.turn.order[position];
            next.players
                .iter()
                .find(|candidate| candidate.id == *id)
                .filter(|candidate| candidate.status == PlayerStatus::Active)
                .map(|_| (position, id.clone()))
        });
        let (successor_position, successor_id) = successor.ok_or_else(|| {
            ExecuteError::InvalidState("turn order contains no active successor".into())
        })?;
        let crossed_round_boundary = successor_position <= next.turn.position;
        if crossed_round_boundary
            && next
                .settings
                .day_limit
                .is_some_and(|limit| limit > 0 && limit == next.turn.day)
        {
            let outcome = day_limit_outcome(&next)?;
            complete_match(&mut next, outcome, &mut events);
            return Ok(Execution {
                state: next,
                events,
                random_consumed,
            });
        }

        let successor_player_index = next
            .players
            .iter()
            .position(|candidate| candidate.id == successor_id)
            .expect("successor selection established the player");

        if crossed_round_boundary {
            let from = next.turn.day;
            next.turn.day = next
                .turn
                .day
                .checked_add(1)
                .ok_or_else(|| ExecuteError::InvalidState("turn day overflow".into()))?;
            events.push(json!({"type":"day-advanced","from":from,"to":next.turn.day}));
        }
        next.turn.position = successor_position;
        next.turn.active_player = successor_id.clone();
        events.push(json!({
            "type":"turn-selected", "player":successor_id, "position":successor_position
        }));
        if next.turn.phase == Phase::TurnEnd {
            next.turn.phase = Phase::TurnStart;
            events.push(json!({
                "type":"phase-changed", "player":successor_id,
                "from":"turn-end", "to":"turn-start"
            }));
        }

        let expired_power = match next.players[successor_player_index].power_state {
            crate::semantic::PowerState::None => None,
            crate::semantic::PowerState::Cop { commander_slot } => {
                Some((commander_slot, PowerLevel::Cop))
            }
            crate::semantic::PowerState::Scop { commander_slot } => {
                Some((commander_slot, PowerLevel::Scop))
            }
        };
        if let Some((commander_slot, power)) = expired_power {
            let commander = next.players[successor_player_index]
                .commanders
                .get(usize::from(commander_slot))
                .ok_or_else(|| {
                    ExecuteError::InvalidState("power state names a missing commander slot".into())
                })?;
            if !commander.active {
                return Err(ExecuteError::InvalidState(
                    "power state names an inactive commander slot".into(),
                ));
            }
            let commander_id = commander.id;
            next.players[successor_player_index].power_state = crate::semantic::PowerState::None;
            events.push(json!({
                "type":"power-ended", "player":successor_id,
                "commander":commander_id, "power":power
            }));
        }

        if next.weather.remaining_turns > 0 {
            let from = next.weather.kind;
            next.weather.remaining_turns -= 1;
            if next.weather.remaining_turns == 0 {
                next.weather.kind = match next.settings.weather {
                    WeatherSetting::Clear => WeatherKind::Clear,
                    WeatherSetting::Rain => WeatherKind::Rain,
                    WeatherSetting::Snow => WeatherKind::Snow,
                    WeatherSetting::Random => WeatherKind::Clear,
                };
            }
            events.push(json!({
                "type":"weather-changed", "from":from, "to":next.weather.kind,
                "remaining_turns":next.weather.remaining_turns, "reason":"expiry"
            }));
        } else if next.settings.weather == WeatherSetting::Random {
            let token = random.get(random_consumed).ok_or_else(|| {
                ExecuteError::InvalidRandom("missing weather-selection token".into())
            })?;
            if token["type"] != "weather-selection" {
                return Err(ExecuteError::InvalidRandom(
                    "expected weather-selection token".into(),
                ));
            }
            let outcome = token["value"].as_str().ok_or_else(|| {
                ExecuteError::InvalidRandom("weather-selection value must be a string".into())
            })?;
            let selected = match outcome {
                "clear" => WeatherKind::Clear,
                "rain" => WeatherKind::Rain,
                "snow" => WeatherKind::Snow,
                _ => {
                    return Err(ExecuteError::InvalidRandom(
                        "weather-selection value is outside the AWBW domain".into(),
                    ));
                }
            };
            random_consumed += 1;
            events.push(json!({
                "type":"random-outcome", "kind":"weather-selection", "outcome":outcome
            }));
            if next.weather.kind != selected {
                let from = next.weather.kind;
                next.weather.kind = selected;
                events.push(json!({
                    "type":"weather-changed", "from":from, "to":next.weather.kind,
                    "remaining_turns":0, "reason":"random-weather"
                }));
            }
        }

        let income_tiles = next
            .board
            .tiles
            .iter()
            .flatten()
            .filter(|tile| {
                tile.owner
                    .as_ref()
                    .and_then(Option::as_ref)
                    .is_some_and(|owner| owner == &successor_id)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::Income)
            })
            .count();
        let income_per_property = commander::effective_income_per_property(&next, &successor_id);
        let income = u64::try_from(income_tiles)
            .ok()
            .and_then(|count| count.checked_mul(income_per_property))
            .ok_or_else(|| ExecuteError::InvalidState("turn-start income overflow".into()))?;
        if income > 0 {
            let funds_before = next.players[successor_player_index].funds;
            let funds_after = funds_before
                .checked_add(income)
                .ok_or_else(|| ExecuteError::InvalidState("player funds overflow".into()))?;
            next.players[successor_player_index].funds = funds_after;
            events.push(json!({
                "type":"funds-changed", "player":successor_id,
                "from":funds_before, "to":funds_after, "reason":"turn-start-income"
            }));
        }

        let mut property_sources = Vec::new();
        for (y, row) in next.board.tiles.iter().enumerate() {
            for (x, tile) in row.iter().enumerate() {
                let position = [x, y];
                let Some(unit) = next.units.iter().find(|unit| {
                    unit.owner == successor_id && board_position(unit) == Some(position)
                }) else {
                    continue;
                };
                if tile
                    .owner
                    .as_ref()
                    .and_then(Option::as_ref)
                    .is_some_and(|owner| owner == &successor_id)
                    && terrain_repairs_unit(tile.terrain, unit.kind)
                {
                    property_sources.push((position, unit.id));
                }
            }
        }
        let mut resupplied = HashSet::new();

        for (position, unit_id) in &property_sources {
            resupplied.insert(*unit_id);
            let unit = next
                .units
                .iter_mut()
                .find(|unit| unit.id == *unit_id)
                .expect("property supply unit remains present");
            if refill_unit(unit) {
                events.push(json!({
                    "type":"automatic-supply", "source":position, "units":[unit_id]
                }));
            }
        }

        let mut apc_ids: Vec<_> = next
            .units
            .iter()
            .filter(|unit| {
                unit.owner == successor_id
                    && ruleset::profile(unit.kind)
                        .supply
                        .is_some_and(|supply| supply.relation == Relation::Adjacent)
                    && board_position(unit).is_some()
            })
            .map(|unit| unit.id)
            .collect();
        apc_ids.sort();
        for apc_id in apc_ids {
            let source = next
                .units
                .iter()
                .find(|unit| unit.id == apc_id)
                .expect("APC source remains on board");
            let source_position = board_position(source).expect("APC source remains on board");
            let source_owner = source.owner.clone();
            let source_team = next.players[successor_player_index].team.clone();
            let supply_targets = ruleset::profile(source.kind)
                .supply
                .ok_or(ExecuteError::UnsupportedRuleset)?
                .targets;
            let mut target_ids: Vec<_> = next
                .units
                .iter()
                .filter(|unit| {
                    unit.id != apc_id
                        && supply_target_eligible(
                            &next,
                            &source_owner,
                            &source_team,
                            &unit.owner,
                            supply_targets,
                        )
                        && board_position(unit).is_some_and(|position| {
                            position[0].abs_diff(source_position[0])
                                + position[1].abs_diff(source_position[1])
                                == 1
                        })
                })
                .map(|unit| unit.id)
                .collect();
            target_ids.sort();
            let mut changed = Vec::new();
            for target_id in target_ids {
                resupplied.insert(target_id);
                let target = next
                    .units
                    .iter_mut()
                    .find(|unit| unit.id == target_id)
                    .expect("APC supply target remains present");
                if refill_unit(target) {
                    changed.push(target_id);
                }
            }
            if !changed.is_empty() {
                events.push(json!({
                    "type":"automatic-supply", "source":apc_id, "units":changed
                }));
            }
        }

        let mut cargo_supply_ids: Vec<_> = next
            .units
            .iter()
            .filter(|unit| {
                unit.owner == successor_id
                    && ruleset::profile(unit.kind)
                        .supply
                        .is_some_and(|supply| supply.relation == Relation::Cargo)
            })
            .map(|unit| unit.id)
            .collect();
        cargo_supply_ids.sort();
        for transport_id in cargo_supply_ids {
            let mut cargo_ids: Vec<_> = next
                .units
                .iter()
                .filter(|unit| {
                    unit.owner == successor_id
                        && matches!(
                            &unit.location,
                            Location::Cargo { transport, .. } if transport == &transport_id
                        )
                })
                .map(|unit| unit.id)
                .collect();
            cargo_ids.sort();
            let mut changed = Vec::new();
            for cargo_id in cargo_ids {
                resupplied.insert(cargo_id);
                let cargo = next
                    .units
                    .iter_mut()
                    .find(|unit| unit.id == cargo_id)
                    .expect("cargo supply target remains present");
                if refill_unit(cargo) {
                    changed.push(cargo_id);
                }
            }
            if !changed.is_empty() {
                events.push(json!({
                    "type":"automatic-supply", "source":transport_id, "units":changed
                }));
            }
        }

        if next.turn.day >= 2 {
            let mut upkeep_ids: Vec<_> = next
                .units
                .iter()
                .filter(|unit| {
                    unit.owner == successor_id
                        && !resupplied.contains(&unit.id)
                        && matches!(
                            ruleset::profile(unit.kind).domain,
                            Domain::Air | Domain::Sea
                        )
                })
                .map(|unit| unit.id)
                .collect();
            upkeep_ids.sort();
            for unit_id in upkeep_ids {
                let unit_snapshot = next
                    .units
                    .iter()
                    .find(|unit| unit.id == unit_id)
                    .expect("upkeep unit remains present")
                    .clone();
                let profile = ruleset::profile(unit_snapshot.kind);
                let base_upkeep = if unit_snapshot.concealment == Concealment::Hidden {
                    profile
                        .fuel_per_turn
                        .hidden
                        .unwrap_or(profile.fuel_per_turn.normal)
                } else {
                    profile.fuel_per_turn.normal
                };
                let upkeep = commander::effective_upkeep(
                    &next,
                    &unit_snapshot,
                    base_upkeep,
                    profile.domain.as_str(),
                );
                let unit = next
                    .units
                    .iter_mut()
                    .find(|unit| unit.id == unit_id)
                    .expect("upkeep unit remains present");
                let fuel_before = unit.fuel;
                unit.fuel = unit.fuel.saturating_sub(upkeep);
                if unit.fuel > 0 && unit.fuel < fuel_before {
                    events.push(json!({
                        "type":"unit-resourced", "unit":unit_id,
                        "fuel_before":fuel_before, "fuel_after":unit.fuel,
                        "ammo_before":unit.ammo, "ammo_after":unit.ammo,
                        "reason":"fuel-upkeep"
                    }));
                }
            }
        }

        let mut crash_ids: Vec<_> = next
            .units
            .iter()
            .filter(|unit| {
                unit.owner == successor_id
                    && unit.fuel == 0
                    && matches!(
                        ruleset::profile(unit.kind).domain,
                        Domain::Air | Domain::Sea
                    )
            })
            .map(|unit| unit.id)
            .collect();
        crash_ids.sort();
        let removed_units = !crash_ids.is_empty();
        for unit_id in crash_ids {
            if !next.units.iter().any(|unit| unit.id == unit_id) {
                continue;
            }
            let mut cargo: Vec<_> = next
                .units
                .iter()
                .filter_map(|unit| match &unit.location {
                    Location::Cargo { transport, slot } if transport == &unit_id => {
                        Some((*slot, unit.id))
                    }
                    _ => None,
                })
                .collect();
            cargo.sort();
            next.units.retain(|unit| {
                unit.id != unit_id
                    && !matches!(
                        &unit.location,
                        Location::Cargo { transport, .. } if transport == &unit_id
                    )
            });
            events.push(json!({
                "type":"unit-removed", "unit":unit_id, "reason":"fuel-depleted"
            }));
            for (_, cargo_id) in cargo {
                events.push(json!({
                    "type":"unit-removed", "unit":cargo_id, "reason":"carrier-lost"
                }));
            }
        }

        let mut repair_units: Vec<_> = property_sources
            .iter()
            .filter(|(_, unit_id)| next.units.iter().any(|unit| unit.id == *unit_id))
            .map(|(position, unit_id)| (*unit_id, *position))
            .collect();
        repair_units.sort_by_key(|left| left.0);
        for (unit_id, position) in repair_units {
            let unit_index = next
                .units
                .iter()
                .position(|unit| unit.id == unit_id)
                .expect("repair unit remains present");
            let hp_before = next.units[unit_index].hp;
            let visual_hp = u64::from(hp_before).div_ceil(10);
            let missing_bars = 10 - visual_hp;
            if missing_bars == 0 {
                continue;
            }
            let heal_cost = ruleset::profile(next.units[unit_index].kind)
                .cost
                .checked_div(10)
                .ok_or(ExecuteError::UnsupportedRuleset)?;
            let affordable_bars = next.players[successor_player_index]
                .funds
                .checked_div(heal_cost)
                .unwrap_or(missing_bars);
            let bars = commander::effective_repair_bars(&next, &successor_id)
                .min(missing_bars)
                .min(affordable_bars);
            if bars == 0 {
                continue;
            }
            let cost = bars.checked_mul(heal_cost).ok_or_else(|| {
                ExecuteError::InvalidState("property repair cost overflow".into())
            })?;
            next.players[successor_player_index].funds -= cost;
            let hp_after = u8::try_from((visual_hp + bars).min(10) * 10)
                .map_err(|_| ExecuteError::InvalidState("property repair HP overflow".into()))?;
            next.units[unit_index].hp = hp_after;
            events.push(json!({
                "type":"automatic-repair", "unit":unit_id, "position":position,
                "hp_restored":hp_after - hp_before, "cost":cost
            }));
        }

        if removed_units && !next.units.iter().any(|unit| unit.owner == successor_id) {
            if eliminate_player(&mut next, &successor_id, "rout", None, None, &mut events)? {
                return Ok(Execution {
                    state: next,
                    events,
                    random_consumed,
                });
            }
            continue;
        }

        let mut unit_indices: Vec<_> = next
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| unit.owner == successor_id && unit.action != UnitAction::Ready)
            .map(|(index, unit)| (unit.id, index))
            .collect();
        unit_indices.sort_by_key(|left| left.0);
        for (unit_id, index) in unit_indices {
            let from = next.units[index].action.clone();
            next.units[index].action = if from == UnitAction::Immobilized {
                UnitAction::Spent
            } else {
                UnitAction::Ready
            };
            events.push(json!({
                "type":"unit-action-changed", "unit":unit_id,
                "from":from, "to":next.units[index].action, "reason":"turn-start"
            }));
        }
        next.turn.phase = Phase::UnitAction;
        events.push(json!({
            "type":"phase-changed", "player":successor_id,
            "from":"turn-start", "to":"unit-action"
        }));

        return Ok(Execution {
            state: next,
            events,
            random_consumed,
        });
    }
}

/// The domain a unit presents to commander combat predicates.
///
/// `commander-combat.json` discriminates more finely than `units.json` does:
/// it separates foot soldiers from other ground units and transports from
/// combatants. Only the transport half is derivable from a table, so the foot
/// kinds are named.
fn combat_domain(profile: &ruleset::UnitProfile) -> &'static str {
    match profile.kind {
        UnitKind::Infantry | UnitKind::Mech => "foot",
        _ if profile.transport.is_some() => "transport",
        _ => match profile.domain {
            Domain::Ground => "ground-vehicle",
            Domain::Air => "air",
            Domain::Sea => "naval",
        },
    }
}

fn terrain_repairs_unit(terrain: TerrainId, kind: UnitKindId) -> bool {
    ruleset::terrain_has(terrain, ruleset::profile(kind).domain.repairs())
}

fn refill_unit(unit: &mut Unit) -> bool {
    let profile = ruleset::profile(unit.kind);
    let changed = unit.fuel != profile.max_fuel || unit.ammo != profile.max_ammo;
    unit.fuel = profile.max_fuel;
    unit.ammo = profile.max_ammo;
    changed
}

#[allow(clippy::too_many_arguments)]
fn apply_strike_funds(
    state: &State,
    next: &mut State,
    events: &mut Vec<Value>,
    striker: &str,
    target_owner: &str,
    target_kind: UnitKindId,
    from_hp: u8,
    to_hp: u8,
) -> Result<(), ExecuteError> {
    let base_value = ruleset::profile(target_kind).cost;
    let target_value = commander::effective_build_cost(state, target_owner, base_value)
        .ok_or_else(|| ExecuteError::InvalidState("strike target value overflow".into()))?;
    let gain =
        commander::strike_funds_gain(state, striker, target_owner, from_hp, to_hp, target_value)
            .ok_or_else(|| {
                ExecuteError::InvalidState("strike funds profile or arithmetic is invalid".into())
            })?;
    if gain == 0 {
        return Ok(());
    }
    let player = next
        .players
        .iter_mut()
        .find(|candidate| candidate.id == striker)
        .ok_or_else(|| ExecuteError::InvalidState("strike owner is absent".into()))?;
    let from = player.funds;
    let to = from
        .checked_add(gain)
        .ok_or_else(|| ExecuteError::InvalidState("strike funds overflow".into()))?;
    player.funds = to;
    events.push(json!({
        "type":"funds-changed", "player":striker,
        "from":from, "to":to, "reason":"commander-power"
    }));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_strike_power_charge(
    state: &State,
    next: &mut State,
    events: &mut Vec<Value>,
    striker: &str,
    target_owner: &str,
    target_kind: UnitKindId,
    from_hp: u8,
    to_hp: u8,
    reason: &str,
) -> Result<(), ExecuteError> {
    let visual_damage = u64::from(from_hp.div_ceil(10).saturating_sub(to_hp.div_ceil(10)));
    if visual_damage == 0 {
        return Ok(());
    }
    let base_value = ruleset::profile(target_kind).cost;
    let target_value = commander::effective_build_cost(state, target_owner, base_value)
        .ok_or_else(|| ExecuteError::InvalidState("power charge unit value overflow".into()))?;
    let dealt_gain = target_value
        .checked_mul(visual_damage)
        .and_then(|value| value.checked_div(20))
        .ok_or_else(|| ExecuteError::InvalidState("dealt power charge overflow".into()))?;
    let received_gain = target_value
        .checked_mul(visual_damage)
        .and_then(|value| value.checked_div(10))
        .ok_or_else(|| ExecuteError::InvalidState("received power charge overflow".into()))?;
    for (player_id, gain) in [(striker, dealt_gain), (target_owner, received_gain)] {
        if gain == 0 {
            continue;
        }
        let player_index = next
            .players
            .iter()
            .position(|player| player.id == player_id)
            .ok_or_else(|| ExecuteError::InvalidState("combat owner is absent".into()))?;
        if !matches!(next.players[player_index].power_state, PowerState::None) {
            continue;
        }
        let active_slot = next.players[player_index]
            .commanders
            .iter()
            .position(|commander| commander.active)
            .ok_or_else(|| ExecuteError::InvalidState("active commander is absent".into()))?;
        let commander_slots = if state.settings.tags {
            if next.players[player_index].commanders.len() != 2 {
                return Err(ExecuteError::InvalidState(
                    "tag player does not have two commander slots".into(),
                ));
            }
            vec![active_slot, 1 - active_slot]
        } else {
            vec![active_slot]
        };
        for commander_slot in commander_slots {
            let slot_gain = if commander_slot == active_slot {
                gain
            } else {
                gain / 2
            };
            if slot_gain == 0 {
                continue;
            }
            let commander = &next.players[player_index].commanders[commander_slot];
            let Some(maximum) = commander::maximum_power_charge(commander.id, commander.power_uses)
                .map_err(|_| ExecuteError::InvalidState("maximum power charge overflow".into()))?
            else {
                continue;
            };
            let from = commander.power_charge;
            if from >= maximum {
                continue;
            }
            let to = from
                .checked_add(slot_gain)
                .ok_or_else(|| ExecuteError::InvalidState("power charge overflow".into()))?
                .min(maximum);
            if to == from {
                continue;
            }
            next.players[player_index].commanders[commander_slot].power_charge = to;
            events.push(json!({
                "type":"power-charge-changed",
                "player":player_id,
                "commander_slot":commander_slot,
                "from":from,
                "to":to,
                "reason":reason
            }));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_tile_attack(
    state: &State,
    player: &str,
    unit_id: UnitId,
    attacker_index: usize,
    attacker: &Unit,
    origin: Position,
    position: Position,
) -> Result<Execution, ExecuteError> {
    let tile = state
        .board
        .tiles
        .get(position[1])
        .and_then(|row| row.get(position[0]))
        .ok_or_else(|| violation(json!({"code":"INVALID_TARGET","target":position})))?;
    if state
        .units
        .iter()
        .any(|unit| board_position(unit) == Some(position))
    {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":position}),
        ));
    }

    let Some(destructible) = ruleset::terrain(tile.terrain).destructible else {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":position}),
        ));
    };
    let from_hp = tile
        .destructible_hp
        .ok_or_else(|| ExecuteError::InvalidState("destructible tile has no HP".into()))?;
    if from_hp > destructible.maximum_hp {
        return Err(ExecuteError::InvalidState(
            "destructible tile HP exceeds its maximum".into(),
        ));
    }
    let from_hp = u8::try_from(from_hp)
        .map_err(|_| ExecuteError::InvalidState("destructible tile HP overflow".into()))?;
    let target_kind = destructible.target_kind;
    let destruction_replacement = destructible.destruction_replacement;

    let actor_team = state
        .players
        .iter()
        .find(|candidate| candidate.id == player)
        .map(|candidate| candidate.team.as_str())
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent".into()))?;
    if state.settings.fog && !AwbwVisibility.visible_position(state, actor_team, position) {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":position}),
        ));
    }

    let profile = ruleset::profile(attacker.kind);
    let fire_mode = profile.fire_mode;
    if fire_mode == FireMode::None {
        return Err(violation(
            json!({"code":"ACTION_NOT_SUPPORTED","action":"attack"}),
        ));
    }
    let distance = origin[0].abs_diff(position[0]) + origin[1].abs_diff(position[1]);
    if let Some(range) = profile.indirect_range {
        let minimum = range.minimum as usize;
        let maximum = usize::try_from(commander::effective_attack_range(
            state,
            attacker,
            range.maximum,
            profile.domain.as_str(),
            "indirect",
        ))
        .map_err(|_| ExecuteError::InvalidState("attack range overflow".into()))?;
        if distance < minimum || distance > maximum {
            return Err(violation(
                json!({"code":"TARGET_OUT_OF_RANGE","target":position}),
            ));
        }
    } else if distance != 1 {
        return Err(violation(
            json!({"code":"TARGET_OUT_OF_RANGE","target":position}),
        ));
    }
    if combat::select_weapon(attacker.kind, target_kind, attacker.ammo).is_none() {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":position}),
        ));
    }

    let unit_domain = combat_domain(profile);
    let tower_count = state
        .board
        .tiles
        .iter()
        .flatten()
        .filter(|candidate| {
            candidate
                .owner
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|owner| owner == player)
                && ruleset::terrain_has(candidate.terrain, TerrainTrait::CommunicationBonus)
        })
        .count() as i64;
    let owned_properties = state
        .board
        .tiles
        .iter()
        .flatten()
        .filter(|candidate| {
            candidate
                .owner
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|owner| owner == player)
                && ruleset::terrain_has(candidate.terrain, TerrainTrait::Capturable)
        })
        .count() as u64;
    let attacker_terrain = state.board.tiles[origin[1]][origin[0]].terrain;
    let attacker_stars = ruleset::defense_stars(attacker_terrain);
    let combat_weather = state.weather.kind;
    let no_capabilities = HashSet::new();
    let context = Combatant {
        kind: attacker.kind,
        domain: unit_domain,
        fire_mode: fire_mode.as_str(),
        terrain: attacker_terrain,
        weather: combat_weather,
        property: ruleset::terrain_has(attacker_terrain, TerrainTrait::Capturable),
        capabilities: &no_capabilities,
    };
    let (attack, _, _, _, _) = commander::effective_combat(
        state,
        player,
        context,
        Strike::Initial,
        CombatContext {
            tower_count,
            funds: state.players[attacker_index].funds,
            owned_properties,
            base_terrain_stars: i64::from(attacker_stars),
        },
    )
    .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
    let hit = combat::damage(
        Side {
            kind: attacker.kind,
            hp: attacker.hp,
            ammo: attacker.ammo,
            attack,
            defense: 100,
            terrain_stars: attacker_stars,
        },
        Side {
            kind: target_kind,
            hp: from_hp,
            ammo: 0,
            attack: 100,
            defense: 100,
            terrain_stars: 0,
        },
        0,
    )
    .expect("tile weapon was validated");
    let to_hp = from_hp.saturating_sub(hit.damage);
    let weapon = match hit.weapon.weapon {
        Weapon::Ammo => "ammo",
        Weapon::Unlimited => "unlimited",
    };

    let mut next = state.clone();
    let mut events = Vec::new();
    if hit.weapon.ammo_cost > 0 {
        let before = next.units[attacker_index].ammo;
        next.units[attacker_index].ammo -= hit.weapon.ammo_cost;
        events.push(json!({
            "type":"unit-resourced", "unit":unit_id,
            "fuel_before":attacker.fuel, "fuel_after":attacker.fuel,
            "ammo_before":before, "ammo_after":next.units[attacker_index].ammo,
            "reason":"combat"
        }));
    }
    events.push(json!({
        "type":"attack-resolved", "attacker":unit_id, "weapon":weapon,
        "target":{"type":"tile","position":position}
    }));
    events.push(json!({
        "type":"destructible-damaged", "position":position,
        "from_hp":from_hp, "to_hp":to_hp
    }));
    if to_hp == 0 {
        next.board.tiles[position[1]][position[0]].terrain = destruction_replacement;
        next.board.tiles[position[1]][position[0]].destructible_hp = None;
        events.push(json!({
            "type":"tile-terrain-changed", "position":position,
            "from":tile.terrain, "to":destruction_replacement, "reason":"combat"
        }));
    } else {
        next.board.tiles[position[1]][position[0]].destructible_hp = Some(u64::from(to_hp));
    }
    next.units[attacker_index].action = UnitAction::Spent;
    events.push(json!({
        "type":"unit-action-changed", "unit":unit_id,
        "from":"ready", "to":"spent", "reason":"attack"
    }));
    Ok(Execution {
        state: next,
        events,
        random_consumed: 0,
    })
}

fn execute_move_attack(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
    target: AttackTarget,
    random: &[Value],
) -> Result<Execution, ExecuteError> {
    let plan = validate_movement_prefix(state, player, unit_id, path)?;
    let ai = plan.unit_index;
    let attacker = &state.units[ai];
    let origin = plan.origin;

    if plan.path.len() > 1 {
        match ruleset::profile(attacker.kind).fire_mode {
            FireMode::Indirect => {
                return Err(violation(
                    json!({"code":"ACTION_NOT_SUPPORTED","action":"move-and-fire"}),
                ));
            }
            FireMode::None => {
                return Err(violation(
                    json!({"code":"ACTION_NOT_SUPPORTED","action":"attack"}),
                ));
            }
            FireMode::Direct => {}
        }

        let destination = *plan.path.last().expect("origin was checked");
        let visibility = AwbwVisibility;
        if state.units.iter().any(|other| {
            other.id != unit_id
                && board_position(other) == Some(destination)
                && occupancy_is_disclosed(&visibility, state, &plan.actor_team, other)
        }) {
            return Err(violation(
                json!({"code":"DESTINATION_OCCUPIED","position":destination}),
            ));
        }

        let mut movement = execute_planned_movement(state, unit_id, &plan);
        if movement.trapped {
            return Ok(Execution {
                state: movement.state,
                events: movement.events,
                random_consumed: 0,
            });
        }

        // Movement spends the unit for movement-only actions. Restore readiness
        // internally so the atomic follow-up can resolve and emit the single
        // attack action transition.
        movement.state.units[plan.unit_index].action = UnitAction::Ready;
        let mut combat = execute_move_attack(
            &movement.state,
            player,
            unit_id,
            vec![destination],
            target,
            random,
        )?;
        movement.events.append(&mut combat.events);
        combat.events = movement.events;
        return Ok(combat);
    }
    let target_id = match target {
        AttackTarget::Unit { unit } => unit,
        AttackTarget::Tile { position } => {
            return execute_tile_attack(state, player, unit_id, ai, attacker, origin, position);
        }
    };
    let di = state
        .units
        .iter()
        .position(|u| u.id == target_id)
        .ok_or_else(|| violation(json!({"code":"INVALID_TARGET","target":target_id})))?;
    let defender = &state.units[di];
    let defender_owner = defender.owner.clone();
    let Location::Board { position: dp } = defender.location else {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":target_id}),
        ));
    };
    if defender.owner == player {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":target_id}),
        ));
    }
    let actor_team = state
        .players
        .iter()
        .find(|candidate| candidate.id == player)
        .map(|candidate| candidate.team.as_str())
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent".into()))?;
    if !AwbwVisibility.visible_unit(state, actor_team, defender) {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":target_id}),
        ));
    }
    let concealed_target_compatible = match (
        defender.concealment.clone(),
        defender.kind.as_str(),
        attacker.kind.as_str(),
    ) {
        (Concealment::Hidden, "sub", "sub" | "cruiser")
        | (Concealment::Hidden, "stealth", "fighter" | "stealth") => true,
        (Concealment::Hidden, "sub" | "stealth", _) => false,
        _ => true,
    };
    if !concealed_target_compatible {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":target_id}),
        ));
    }
    let profile = ruleset::profile(attacker.kind);
    if profile.fire_mode == FireMode::None {
        return Err(violation(
            json!({"code":"ACTION_NOT_SUPPORTED","action":"attack"}),
        ));
    }
    let distance = origin[0].abs_diff(dp[0]) + origin[1].abs_diff(dp[1]);
    if let Some(range) = profile.indirect_range {
        let min = range.minimum as usize;
        let max = usize::try_from(commander::effective_attack_range(
            state,
            attacker,
            range.maximum,
            profile.domain.as_str(),
            "indirect",
        ))
        .map_err(|_| ExecuteError::InvalidState("attack range overflow".into()))?;
        if distance < min || distance > max {
            return Err(violation(
                json!({"code":"TARGET_OUT_OF_RANGE","target":target_id}),
            ));
        }
    } else if distance != 1 {
        return Err(violation(
            json!({"code":"TARGET_OUT_OF_RANGE","target":target_id}),
        ));
    }
    if combat::select_weapon(attacker.kind, defender.kind, attacker.ammo).is_none() {
        return Err(violation(
            json!({"code":"INVALID_TARGET","target":target_id}),
        ));
    }
    let token = |i: usize, kind: &str, domain: commander::Domain| -> Result<i64, ExecuteError> {
        let v = random.get(i).ok_or(ExecuteError::UnsupportedCommand)?;
        if v["type"] != kind {
            return Err(ExecuteError::UnsupportedCommand);
        }
        v["value"]
            .as_i64()
            .filter(|x| (domain.minimum..=domain.maximum).contains(x))
            .ok_or(ExecuteError::UnsupportedCommand)
    };
    let stars = |p: Position| ruleset::defense_stars(state.board.tiles[p[1]][p[0]].terrain);
    let unit_domain = |kind: UnitKindId| combat_domain(ruleset::profile(kind));
    let fire_mode = |kind: UnitKindId| ruleset::profile(kind).fire_mode.as_str();
    let tower_count = |owner: &str| {
        state
            .board
            .tiles
            .iter()
            .flatten()
            .filter(|tile| {
                tile.owner
                    .as_ref()
                    .and_then(Option::as_ref)
                    .is_some_and(|value| value == owner)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::CommunicationBonus)
            })
            .count() as i64
    };
    let is_property = |terrain: TerrainId| ruleset::terrain_has(terrain, TerrainTrait::Capturable);
    let combat_context = |owner: &str, position: Position| CombatContext {
        tower_count: tower_count(owner),
        funds: state
            .players
            .iter()
            .find(|player| player.id == owner)
            .map_or(0, |player| player.funds),
        owned_properties: state
            .board
            .tiles
            .iter()
            .flatten()
            .filter(|tile| {
                tile.owner
                    .as_ref()
                    .and_then(Option::as_ref)
                    .is_some_and(|value| value == owner)
                    && is_property(tile.terrain)
            })
            .count() as u64,
        base_terrain_stars: i64::from(stars(position)),
    };
    let no_capabilities = HashSet::new();
    let combat_weather = state.weather.kind;
    let attacker_context = Combatant {
        kind: attacker.kind,
        domain: unit_domain(attacker.kind),
        fire_mode: fire_mode(attacker.kind),
        terrain: state.board.tiles[origin[1]][origin[0]].terrain,
        weather: combat_weather,
        property: is_property(state.board.tiles[origin[1]][origin[0]].terrain),
        capabilities: &no_capabilities,
    };
    let defender_context = Combatant {
        kind: defender.kind,
        domain: unit_domain(defender.kind),
        fire_mode: fire_mode(attacker.kind),
        terrain: state.board.tiles[dp[1]][dp[0]].terrain,
        weather: combat_weather,
        property: is_property(state.board.tiles[dp[1]][dp[0]].terrain),
        capabilities: &no_capabilities,
    };
    let (attacker_attack, _, _, attacker_good, attacker_bad) = commander::effective_combat(
        state,
        &attacker.owner,
        attacker_context,
        Strike::Initial,
        combat_context(&attacker.owner, origin),
    )
    .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
    let (_, defender_defense, defender_stars, _, _) = commander::effective_combat(
        state,
        &defender.owner,
        defender_context,
        Strike::Initial,
        combat_context(&defender.owner, dp),
    )
    .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
    let defender_stars = commander::effective_enemy_terrain_stars(
        state,
        &attacker.owner,
        attacker_context,
        Strike::Initial,
        defender_stars,
    )
    .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
    let defender_stars = u8::try_from(defender_stars)
        .map_err(|_| ExecuteError::InvalidState("terrain stars overflow".into()))?;
    let attacker_side = Side {
        kind: attacker.kind,
        hp: attacker.hp,
        ammo: attacker.ammo,
        attack: attacker_attack,
        defense: 100,
        terrain_stars: stars(origin),
    };
    let defender_side = Side {
        kind: defender.kind,
        hp: defender.hp,
        ammo: defender.ammo,
        attack: 100,
        defense: defender_defense,
        terrain_stars: defender_stars,
    };
    let weapon_name = |w| match w {
        Weapon::Ammo => "ammo",
        Weapon::Unlimited => "unlimited",
    };
    let defender_direct = ruleset::profile(defender.kind).fire_mode == FireMode::Direct;
    let counter_first_context = Combatant {
        kind: defender.kind,
        domain: unit_domain(defender.kind),
        fire_mode: fire_mode(defender.kind),
        terrain: state.board.tiles[dp[1]][dp[0]].terrain,
        weather: combat_weather,
        property: is_property(state.board.tiles[dp[1]][dp[0]].terrain),
        capabilities: &no_capabilities,
    };
    let counter_first = distance == 1
        && defender_direct
        && combat::select_weapon(defender.kind, attacker.kind, defender.ammo).is_some()
        && commander::counter_first(
            state,
            &defender.owner,
            counter_first_context,
            Strike::Counter,
        );
    if counter_first {
        let countered_context = Combatant {
            kind: attacker.kind,
            domain: unit_domain(attacker.kind),
            fire_mode: fire_mode(defender.kind),
            terrain: state.board.tiles[origin[1]][origin[0]].terrain,
            weather: combat_weather,
            property: is_property(state.board.tiles[origin[1]][origin[0]].terrain),
            capabilities: &no_capabilities,
        };
        let (counter_attack, _, _, counter_good, counter_bad) = commander::effective_combat(
            state,
            &defender.owner,
            counter_first_context,
            Strike::Counter,
            combat_context(&defender.owner, dp),
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let (_, countered_defense, countered_stars, _, _) = commander::effective_combat(
            state,
            &attacker.owner,
            countered_context,
            Strike::Counter,
            combat_context(&attacker.owner, origin),
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let countered_stars = commander::effective_enemy_terrain_stars(
            state,
            &defender.owner,
            counter_first_context,
            Strike::Counter,
            countered_stars,
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let countered_stars = u8::try_from(countered_stars)
            .map_err(|_| ExecuteError::InvalidState("terrain stars overflow".into()))?;
        let counter_luck =
            token(0, "combat-good-luck", counter_good)? - token(1, "combat-bad-luck", counter_bad)?;
        let preemptive = combat::damage(
            Side {
                attack: counter_attack,
                ..defender_side
            },
            Side {
                defense: countered_defense,
                terrain_stars: countered_stars,
                ..attacker_side
            },
            counter_luck,
        )
        .expect("counter-first eligibility selected a weapon");
        let attacker_remaining = attacker.hp.saturating_sub(preemptive.damage);
        let initiating = if attacker_remaining > 0 {
            let attack_luck = token(2, "combat-good-luck", attacker_good)?
                - token(3, "combat-bad-luck", attacker_bad)?;
            Some(
                combat::damage(
                    Side {
                        hp: attacker_remaining,
                        ..attacker_side
                    },
                    defender_side,
                    attack_luck,
                )
                .expect("initiating weapon was validated"),
            )
        } else {
            None
        };

        let mut next = state.clone();
        let mut events = Vec::new();
        if preemptive.weapon.ammo_cost > 0 {
            let index = next
                .units
                .iter()
                .position(|unit| unit.id == target_id)
                .expect("counter-first defender remains present");
            let before = next.units[index].ammo;
            next.units[index].ammo -= preemptive.weapon.ammo_cost;
            events.push(json!({"type":"unit-resourced","unit":target_id,"fuel_before":defender.fuel,"fuel_after":defender.fuel,"ammo_before":before,"ammo_after":next.units[index].ammo,"reason":"combat-counter"}));
        }
        events.push(json!({"type":"attack-resolved","attacker":target_id,"weapon":weapon_name(preemptive.weapon.weapon),"target":{"type":"unit","unit":unit_id}}));
        events.push(json!({"type":"unit-damaged","unit":unit_id,"from_hp":attacker.hp,"to_hp":attacker_remaining,"reason":"combat-counter"}));
        apply_strike_funds(
            state,
            &mut next,
            &mut events,
            &defender.owner,
            &attacker.owner,
            attacker.kind,
            attacker.hp,
            attacker_remaining,
        )?;
        apply_strike_power_charge(
            state,
            &mut next,
            &mut events,
            &defender.owner,
            &attacker.owner,
            attacker.kind,
            attacker.hp,
            attacker_remaining,
            "combat-counter",
        )?;
        if attacker_remaining == 0 {
            events.push(json!({"type":"unit-removed","unit":unit_id,"reason":"combat-counter"}));
            next.units.remove(ai);
            if !next.units.iter().any(|unit| unit.owner == attacker.owner) {
                eliminate_player(&mut next, &attacker.owner, "rout", None, None, &mut events)?;
            }
            return Ok(Execution {
                state: next,
                events,
                random_consumed: 2,
            });
        }
        next.units[ai].hp = attacker_remaining;

        let hit = initiating.expect("surviving attacker performs initiating strike");
        let next_ai = next
            .units
            .iter()
            .position(|unit| unit.id == unit_id)
            .expect("surviving attacker remains present");
        if hit.weapon.ammo_cost > 0 {
            let before = next.units[next_ai].ammo;
            next.units[next_ai].ammo -= hit.weapon.ammo_cost;
            events.push(json!({"type":"unit-resourced","unit":unit_id,"fuel_before":attacker.fuel,"fuel_after":attacker.fuel,"ammo_before":before,"ammo_after":next.units[next_ai].ammo,"reason":"combat"}));
        }
        let defender_remaining = defender.hp.saturating_sub(hit.damage);
        events.push(json!({"type":"attack-resolved","attacker":unit_id,"weapon":weapon_name(hit.weapon.weapon),"target":{"type":"unit","unit":target_id}}));
        events.push(json!({"type":"unit-damaged","unit":target_id,"from_hp":defender.hp,"to_hp":defender_remaining,"reason":"combat"}));
        apply_strike_funds(
            state,
            &mut next,
            &mut events,
            &attacker.owner,
            &defender.owner,
            defender.kind,
            defender.hp,
            defender_remaining,
        )?;
        apply_strike_power_charge(
            state,
            &mut next,
            &mut events,
            &attacker.owner,
            &defender.owner,
            defender.kind,
            defender.hp,
            defender_remaining,
            "combat",
        )?;
        if defender_remaining == 0 {
            events.push(json!({"type":"unit-removed","unit":target_id,"reason":"combat"}));
            let next_di = next
                .units
                .iter()
                .position(|unit| unit.id == target_id)
                .expect("lethal target remains until removal");
            next.units.remove(next_di);
        } else {
            let next_di = next
                .units
                .iter()
                .position(|unit| unit.id == target_id)
                .expect("surviving target remains present");
            next.units[next_di].hp = defender_remaining;
        }
        let next_ai = next
            .units
            .iter()
            .position(|unit| unit.id == unit_id)
            .expect("acting attacker survives counter-first engagement");
        next.units[next_ai].action = UnitAction::Spent;
        events.push(json!({"type":"unit-action-changed","unit":unit_id,"from":"ready","to":"spent","reason":"attack"}));
        if defender_remaining == 0 && !next.units.iter().any(|unit| unit.owner == defender_owner) {
            eliminate_player(&mut next, &defender_owner, "rout", None, None, &mut events)?;
        }
        return Ok(Execution {
            state: next,
            events,
            random_consumed: 4,
        });
    }
    let attack_luck =
        token(0, "combat-good-luck", attacker_good)? - token(1, "combat-bad-luck", attacker_bad)?;
    let first = combat::damage(attacker_side, defender_side, attack_luck)
        .ok_or_else(|| violation(json!({"code":"INVALID_TARGET","target":target_id})))?;
    let remaining = defender.hp.saturating_sub(first.damage);
    let counter = if remaining > 0
        && distance == 1
        && defender_direct
        && combat::select_weapon(defender.kind, attacker.kind, defender.ammo).is_some()
    {
        let counter_context = Combatant {
            kind: defender.kind,
            domain: unit_domain(defender.kind),
            fire_mode: fire_mode(defender.kind),
            terrain: state.board.tiles[dp[1]][dp[0]].terrain,
            weather: combat_weather,
            property: is_property(state.board.tiles[dp[1]][dp[0]].terrain),
            capabilities: &no_capabilities,
        };
        let countered_context = Combatant {
            kind: attacker.kind,
            domain: unit_domain(attacker.kind),
            fire_mode: fire_mode(defender.kind),
            terrain: state.board.tiles[origin[1]][origin[0]].terrain,
            weather: combat_weather,
            property: is_property(state.board.tiles[origin[1]][origin[0]].terrain),
            capabilities: &no_capabilities,
        };
        let (counter_attack, _, _, counter_good, counter_bad) = commander::effective_combat(
            state,
            &defender.owner,
            counter_context,
            Strike::Counter,
            combat_context(&defender.owner, dp),
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let (_, countered_defense, countered_stars, _, _) = commander::effective_combat(
            state,
            &attacker.owner,
            countered_context,
            Strike::Counter,
            combat_context(&attacker.owner, origin),
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let countered_stars = commander::effective_enemy_terrain_stars(
            state,
            &defender.owner,
            counter_context,
            Strike::Counter,
            countered_stars,
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let countered_stars = u8::try_from(countered_stars)
            .map_err(|_| ExecuteError::InvalidState("terrain stars overflow".into()))?;
        let luck =
            token(2, "combat-good-luck", counter_good)? - token(3, "combat-bad-luck", counter_bad)?;
        combat::damage(
            Side {
                hp: remaining,
                attack: counter_attack,
                ..defender_side
            },
            Side {
                defense: countered_defense,
                terrain_stars: countered_stars,
                ..attacker_side
            },
            luck,
        )
    } else {
        None
    };
    let mut next = state.clone();
    let mut events = Vec::new();
    if first.weapon.ammo_cost > 0 {
        let before = next.units[ai].ammo;
        next.units[ai].ammo -= first.weapon.ammo_cost;
        events.push(json!({"type":"unit-resourced","unit":unit_id,"fuel_before":attacker.fuel,"fuel_after":attacker.fuel,"ammo_before":before,"ammo_after":next.units[ai].ammo,"reason":"combat"}));
    }
    events.push(json!({"type":"attack-resolved","attacker":unit_id,"weapon":weapon_name(first.weapon.weapon),"target":{"type":"unit","unit":target_id}}));
    events.push(json!({"type":"unit-damaged","unit":target_id,"from_hp":defender.hp,"to_hp":remaining,"reason":"combat"}));
    apply_strike_funds(
        state,
        &mut next,
        &mut events,
        &attacker.owner,
        &defender.owner,
        defender.kind,
        defender.hp,
        remaining,
    )?;
    apply_strike_power_charge(
        state,
        &mut next,
        &mut events,
        &attacker.owner,
        &defender.owner,
        defender.kind,
        defender.hp,
        remaining,
        "combat",
    )?;
    if remaining > 0 {
        next.units[di].hp = remaining;
    }
    if let Some(hit) = counter {
        let before = next.units[di].ammo;
        if hit.weapon.ammo_cost > 0 {
            next.units[di].ammo -= hit.weapon.ammo_cost;
            events.push(json!({"type":"unit-resourced","unit":target_id,"fuel_before":defender.fuel,"fuel_after":defender.fuel,"ammo_before":before,"ammo_after":next.units[di].ammo,"reason":"combat-counter"}));
        }
        let ahp = attacker.hp.saturating_sub(hit.damage);
        events.push(json!({"type":"attack-resolved","attacker":target_id,"weapon":weapon_name(hit.weapon.weapon),"target":{"type":"unit","unit":unit_id}}));
        events.push(json!({"type":"unit-damaged","unit":unit_id,"from_hp":attacker.hp,"to_hp":ahp,"reason":"combat-counter"}));
        apply_strike_funds(
            state,
            &mut next,
            &mut events,
            &defender.owner,
            &attacker.owner,
            attacker.kind,
            attacker.hp,
            ahp,
        )?;
        apply_strike_power_charge(
            state,
            &mut next,
            &mut events,
            &defender.owner,
            &attacker.owner,
            attacker.kind,
            attacker.hp,
            ahp,
            "combat-counter",
        )?;
        next.units[ai].hp = ahp;
    }
    if remaining == 0 {
        events.push(json!({"type":"unit-removed","unit":target_id,"reason":"combat"}));
        next.units.remove(di);
    }
    let next_ai = next
        .units
        .iter()
        .position(|u| u.id == unit_id)
        .expect("attacker survives this slice");
    next.units[next_ai].action = UnitAction::Spent;
    events.push(json!({"type":"unit-action-changed","unit":unit_id,"from":"ready","to":"spent","reason":"attack"}));
    if remaining == 0 && !next.units.iter().any(|unit| unit.owner == defender_owner) {
        eliminate_player(&mut next, &defender_owner, "rout", None, None, &mut events)?;
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: if counter.is_some() { 4 } else { 2 },
    })
}

fn execute_move_launch(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
    target: Position,
) -> Result<Execution, ExecuteError> {
    let plan = validate_movement_prefix(state, player, unit_id, path)?;
    if target[0] >= state.board.width || target[1] >= state.board.height {
        return Err(violation(json!({
            "code":"INVALID_TARGET", "target":target
        })));
    }

    let unit = &state.units[plan.unit_index];
    if !matches!(unit.kind.as_str(), "infantry" | "mech") {
        return Err(violation(json!({
            "code":"ACTION_NOT_SUPPORTED", "action":"move-launch"
        })));
    }
    let silo_position = *plan.path.last().expect("origin was checked");
    let silo = &state.board.tiles[silo_position[1]][silo_position[0]].silo;
    if silo != &Some(Silo::Ready) {
        return Err(violation(json!({
            "code":"INVALID_TARGET", "target":silo_position
        })));
    }
    let visibility = AwbwVisibility;
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(silo_position)
            && occupancy_is_disclosed(&visibility, state, &plan.actor_team, other)
    }) {
        return Err(violation(json!({
            "code":"DESTINATION_OCCUPIED", "position":silo_position
        })));
    }

    let mut outcome = execute_planned_movement(state, unit_id, &plan);
    if outcome.trapped {
        return Ok(Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        });
    }

    // AWBW's silo missile is three visual bars (30 exact HP), nonlethal, and
    // affects every board unit, including allies. Derive the list after the
    // move and sort it so event order is independent of state-vector order.
    outcome.events.push(json!({
        "type":"area-strike-resolved", "strike":0, "policy":"unit-hp",
        "center":target, "radius":3, "damage":30
    }));
    let mut affected: Vec<UnitId> = outcome
        .state
        .units
        .iter()
        .filter(|unit| {
            board_position(unit).is_some_and(|position| {
                position[0].abs_diff(target[0]) + position[1].abs_diff(target[1]) <= 3
            })
        })
        .map(|unit| unit.id)
        .collect();
    affected.sort();
    for id in affected {
        let unit = outcome
            .state
            .units
            .iter_mut()
            .find(|unit| unit.id == id)
            .expect("launch target remains present");
        let from_hp = unit.hp;
        let to_hp = from_hp.saturating_sub(30).max(1);
        if to_hp != from_hp {
            unit.hp = to_hp;
            outcome.events.push(json!({
                "type":"unit-damaged", "unit":id, "from_hp":from_hp,
                "to_hp":to_hp, "reason":"missile-silo"
            }));
        }
    }
    outcome.state.board.tiles[silo_position[1]][silo_position[0]].silo = Some(Silo::Spent);
    outcome.events.push(json!({
        "type":"silo-changed", "position":silo_position,
        "from":"ready", "to":"spent"
    }));
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

fn execute_move_explode(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
) -> Result<Execution, ExecuteError> {
    let plan = validate_movement_prefix(state, player, unit_id, path)?;
    let unit = &state.units[plan.unit_index];
    if unit.kind != UnitKind::BlackBomb {
        return Err(violation(json!({
            "code":"ACTION_NOT_SUPPORTED", "action":"move-explode"
        })));
    }
    let destination = *plan.path.last().expect("origin was checked");
    let visibility = AwbwVisibility;
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(destination)
            && occupancy_is_disclosed(&visibility, state, &plan.actor_team, other)
    }) {
        return Err(violation(json!({
            "code":"DESTINATION_OCCUPIED", "position":destination
        })));
    }

    let mut outcome = execute_planned_movement(state, unit_id, &plan);
    if outcome.trapped {
        return Ok(Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        });
    }

    outcome.events.push(json!({
        "type":"area-strike-resolved", "strike":0, "policy":"unit-hp",
        "center":destination, "radius":3, "damage":50
    }));
    let mut affected: Vec<UnitId> = outcome
        .state
        .units
        .iter()
        .filter(|unit| unit.id != unit_id)
        .filter(|unit| {
            board_position(unit).is_some_and(|position| {
                position[0].abs_diff(destination[0]) + position[1].abs_diff(destination[1]) <= 3
            })
        })
        .map(|unit| unit.id)
        .collect();
    affected.sort();
    for id in affected {
        let unit = outcome
            .state
            .units
            .iter_mut()
            .find(|unit| unit.id == id)
            .expect("explosion target remains present");
        let from_hp = unit.hp;
        let to_hp = from_hp.saturating_sub(50).max(1);
        if to_hp == from_hp {
            continue;
        }
        unit.hp = to_hp;
        outcome.events.push(json!({
            "type":"unit-damaged", "unit":id, "from_hp":from_hp,
            "to_hp":to_hp, "reason":"explode"
        }));
    }

    let exploding_owner = outcome.state.units[plan.unit_index].owner.clone();
    outcome.state.units.remove(plan.unit_index);
    outcome.events.push(json!({
        "type":"unit-removed", "unit":unit_id, "reason":"explode"
    }));
    if !outcome
        .state
        .units
        .iter()
        .any(|unit| unit.owner == exploding_owner)
    {
        eliminate_player(
            &mut outcome.state,
            &exploding_owner,
            "rout",
            None,
            None,
            &mut outcome.events,
        )?;
    }
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

fn execute_delete_unit(
    state: &State,
    player: &str,
    unit_id: UnitId,
) -> Result<Execution, ExecuteError> {
    if state.ruleset.id != "awbw" || state.ruleset.revision != "2026-07-10" {
        return Err(ExecuteError::UnsupportedRuleset);
    }
    if matches!(state.match_state, Match::Finished { .. }) {
        return Err(violation(json!({"code":"MATCH_FINISHED"})));
    }
    if state.turn.phase != Phase::UnitAction {
        return Err(violation(json!({
            "code":"WRONG_PHASE", "expected":"unit-action", "actual":state.turn.phase
        })));
    }
    if state.turn.active_player != player {
        return Err(violation(json!({
            "code":"NOT_ACTIVE_PLAYER", "player":player
        })));
    }
    let unit_index = state
        .units
        .iter()
        .position(|unit| unit.id == unit_id)
        .ok_or_else(|| violation(json!({"code":"UNIT_NOT_FOUND", "unit":unit_id})))?;
    let unit = &state.units[unit_index];
    if unit.owner != player {
        return Err(violation(json!({
            "code":"UNIT_NOT_OWNED", "unit":unit_id, "player":player
        })));
    }
    let position = board_position(unit)
        .ok_or_else(|| violation(json!({"code":"UNIT_NOT_ON_BOARD", "unit":unit_id})))?;

    let mut next = state.clone();
    let mut events = Vec::new();
    if let Some(before) = next.board.tiles[position[1]][position[0]]
        .capture_points
        .filter(|points| *points < 20)
    {
        next.board.tiles[position[1]][position[0]].capture_points = Some(20);
        events.push(json!({
            "type":"capture-changed", "position":position, "from":before, "to":20
        }));
    }
    next.units.remove(unit_index);
    events.push(json!({
        "type":"unit-removed", "unit":unit_id, "reason":"delete"
    }));
    if !next.units.iter().any(|unit| unit.owner == player) {
        eliminate_player(&mut next, player, "rout", None, None, &mut events)?;
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: 0,
    })
}

fn execute_move_wait(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Position>,
    _random: &[Value],
) -> Result<Execution, ExecuteError> {
    let plan = validate_movement_prefix(state, player, unit_id, path)?;
    let destination = *plan.path.last().expect("origin was checked");
    let visibility = AwbwVisibility;
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(destination)
            && occupancy_is_disclosed(&visibility, state, &plan.actor_team, other)
    }) {
        return Err(violation(
            json!({"code":"DESTINATION_OCCUPIED","position":destination}),
        ));
    }
    let outcome = execute_planned_movement(state, unit_id, &plan);
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

fn violation(value: Value) -> ExecuteError {
    ExecuteError::Violation(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct_combat_state(width: usize) -> State {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/movement/infantry-plain-move.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.board.width = width;
        let plain = state.board.tiles[0][0].clone();
        state.board.tiles[0] = vec![plain; width];
        state.teams.push(crate::semantic::Team {
            id: "blue-team".into(),
            status: crate::semantic::TeamStatus::Active,
        });
        let mut blue = state.players[0].clone();
        blue.id = "blue".into();
        blue.team = "blue-team".into();
        blue.commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.players[0].commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.players.push(blue);
        state.units[0].id = UnitId::new(0);
        state
    }

    #[test]
    fn scalar_power_activation_validates_availability_and_scaled_cost() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/commander/adder-power-activation.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.players[0].commanders[0].power_uses = 1;
        state.players[0].commanders[0].power_charge = 21_599;
        let activate = || {
            serde_json::from_value(json!({
                "type":"activate-power", "player":"red", "level":"cop"
            }))
            .unwrap()
        };

        assert_eq!(
            execute(&state, activate(), &[]),
            Err(violation(json!({
                "code":"INSUFFICIENT_POWER", "required":21600, "available":21599
            })))
        );

        state.settings.powers = crate::semantic::Toggle::Disabled;
        assert_eq!(
            execute(&state, activate(), &[]),
            Err(violation(json!({
                "code":"ACTION_NOT_SUPPORTED", "action":"activate-power"
            })))
        );
    }

    #[test]
    fn random_weather_rejects_missing_wrong_and_out_of_domain_tokens() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/turn-hooks/random-weather-outcomes.json"
        ))
        .unwrap();
        let state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        let command: Command = serde_json::from_value(case["steps"][0]["command"].clone()).unwrap();

        for random in [
            vec![],
            vec![json!({"type":"combat-good-luck","value":0})],
            vec![json!({"type":"weather-selection","value":"sandstorm"})],
        ] {
            assert!(
                matches!(
                    execute(&state, command.clone(), &random),
                    Err(ExecuteError::InvalidRandom(_))
                ),
                "unexpectedly accepted random input {random:?}"
            );
        }
    }

    #[test]
    fn cargo_is_supplied_before_crashing_transport_removes_it() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/turn-hooks/fuel-upkeep-and-crash.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.units[0].kind = UnitKindId::Carrier;
        state.units[0].fuel = 0;
        state.units[0].ammo = 9;
        state.units[1].kind = UnitKindId::Fighter;
        state.units[1].fuel = 30;
        state.units[1].ammo = 2;
        state.units[1].location = Location::Cargo {
            transport: UnitId::new(0),
            slot: 0,
        };
        let command: Command = serde_json::from_value(case["steps"][0]["command"].clone()).unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert!(
            !result
                .state
                .units
                .iter()
                .any(|unit| unit.id == UnitId::new(0))
        );
        assert!(
            !result
                .state
                .units
                .iter()
                .any(|unit| unit.id == UnitId::new(1))
        );
        assert_eq!(
            result.events[4..7],
            [
                json!({"type":"automatic-supply","source":0,"units":[1]}),
                json!({"type":"unit-removed","unit":0,"reason":"fuel-depleted"}),
                json!({"type":"unit-removed","unit":1,"reason":"carrier-lost"}),
            ]
        );
    }

    #[test]
    fn capture_move_resets_origin_before_attempting_destination() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-partial.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.board.width = 2;
        let mut destination = state.board.tiles[0][0].clone();
        destination.capture_points = Some(20);
        state.board.tiles[0][0].capture_points = Some(10);
        state.board.tiles[0].push(destination);
        let command: Command = serde_json::from_value(json!({
            "type":"move-capture", "player":"red", "unit":0,
            "path":[[0,0],[1,0]]
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert_eq!(result.state.board.tiles[0][0].capture_points, Some(20));
        assert_eq!(result.state.board.tiles[0][1].capture_points, Some(10));
        assert_eq!(
            result.events,
            vec![
                json!({"type":"capture-changed","position":[0,0],"from":10,"to":20}),
                json!({"type":"unit-moved","unit":0,"from":[0,0],"to":[1,0],"path":[[0,0],[1,0]],"fuel_spent":1}),
                json!({"type":"capture-changed","position":[1,0],"from":20,"to":10}),
            ]
        );
    }

    #[test]
    fn hidden_destination_occupant_traps_and_suppresses_capture() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-complete.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.settings.fog = true;
        state.board.width = 4;
        let mut plain = state.board.tiles[0][0].clone();
        plain.terrain = TerrainId::Plain;
        plain.owner = None;
        plain.capture_points = None;
        let destination = state.board.tiles[0][0].clone();
        state.board.tiles[0] = vec![plain.clone(), plain.clone(), plain, destination];
        let mut blocker = state.units[0].clone();
        blocker.id = UnitId::new(1);
        blocker.kind = UnitKindId::Tank;
        blocker.owner = "blue".into();
        blocker.location = Location::Board { position: [3, 0] };
        state.units[0].location = Location::Board { position: [0, 0] };
        state.units.push(blocker);
        let command: Command = serde_json::from_value(json!({
            "type":"move-capture", "player":"red", "unit":0,
            "path":[[0,0],[1,0],[2,0],[3,0]]
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert_eq!(
            board_position(
                result
                    .state
                    .units
                    .iter()
                    .find(|unit| unit.id == UnitId::new(0))
                    .unwrap()
            ),
            Some([2, 0])
        );
        assert_eq!(result.state.board.tiles[0][3].capture_points, Some(10));
        assert_eq!(
            result.events,
            vec![
                json!({"type":"unit-moved","unit":0,"from":[0,0],"to":[2,0],"path":[[0,0],[1,0],[2,0]],"fuel_spent":2}),
                json!({"type":"movement-trapped","unit":0,"blocker":1,"position":[3,0]}),
            ]
        );
    }

    #[test]
    fn hidden_destination_occupant_traps_and_suppresses_concealment() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-complete.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.settings.fog = true;
        state.board.width = 7;
        let mut plain = state.board.tiles[0][0].clone();
        plain.terrain = TerrainId::Plain;
        plain.owner = None;
        plain.capture_points = None;
        state.board.tiles[0] = vec![plain; 7];
        state.units[0].kind = UnitKindId::Stealth;
        state.units[0].fuel = 55;
        state.units[0].ammo = 6;
        state.units[0].location = Location::Board { position: [0, 0] };
        let mut blocker = state.units[0].clone();
        blocker.id = UnitId::new(1);
        blocker.kind = UnitKindId::Tank;
        blocker.owner = "blue".into();
        blocker.concealment = Concealment::Exposed;
        blocker.location = Location::Board { position: [6, 0] };
        state.units.push(blocker);
        let command: Command = serde_json::from_value(json!({
            "type":"move-hide", "player":"red", "unit":0,
            "path":[[0,0],[1,0],[2,0],[3,0],[4,0],[5,0],[6,0]]
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();
        let stealth = result
            .state
            .units
            .iter()
            .find(|unit| unit.id == UnitId::new(0))
            .unwrap();

        assert_eq!(board_position(stealth), Some([5, 0]));
        assert_eq!(stealth.concealment, Concealment::Exposed);
        assert_eq!(
            result.events,
            vec![
                json!({"type":"unit-moved","unit":0,"from":[0,0],"to":[5,0],"path":[[0,0],[1,0],[2,0],[3,0],[4,0],[5,0]],"fuel_spent":5}),
                json!({"type":"movement-trapped","unit":0,"blocker":1,"position":[6,0]}),
            ]
        );
    }

    #[test]
    fn move_launch_damages_all_board_units_in_stable_order_without_charge() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-complete.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.board.width = 6;
        let mut plain = state.board.tiles[0][0].clone();
        plain.terrain = TerrainId::Plain;
        plain.owner = None;
        plain.capture_points = None;
        plain.silo = None;
        let mut silo = plain.clone();
        silo.terrain = TerrainId::MissileSilo;
        silo.silo = Some(Silo::Ready);
        state.board.tiles[0] = vec![
            plain,
            silo,
            state.board.tiles[0][0].clone(),
            state.board.tiles[0][0].clone(),
            state.board.tiles[0][0].clone(),
            state.board.tiles[0][0].clone(),
        ];
        state.units[0].location = Location::Board { position: [0, 0] };
        state.units[0].hp = 20;
        let mut ally = state.units[0].clone();
        ally.id = UnitId::new(1);
        ally.location = Location::Board { position: [2, 0] };
        ally.hp = 100;
        let mut enemy = ally.clone();
        enemy.id = UnitId::new(2);
        enemy.owner = "blue".into();
        enemy.location = Location::Board { position: [5, 0] };
        enemy.hp = 10;
        state.units.extend([ally, enemy]);
        state.settings.fog = true;
        let command: Command = serde_json::from_value(json!({
            "type":"move-launch", "player":"red", "unit":0,
            "path":[[0,0],[1,0]], "target":[4,0]
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert_eq!(result.state.board.tiles[0][1].silo, Some(Silo::Spent));
        assert_eq!(result.state.players[0].commanders[0].power_charge, 0);
        assert_eq!(
            result
                .state
                .units
                .iter()
                .find(|u| u.id == UnitId::new(0))
                .unwrap()
                .hp,
            1
        );
        assert_eq!(
            result
                .state
                .units
                .iter()
                .find(|u| u.id == UnitId::new(1))
                .unwrap()
                .hp,
            70
        );
        assert_eq!(
            result
                .state
                .units
                .iter()
                .find(|u| u.id == UnitId::new(2))
                .unwrap()
                .hp,
            1
        );
        let types: Vec<_> = result
            .events
            .iter()
            .map(|e| e["type"].as_str().unwrap())
            .collect();
        assert_eq!(
            types,
            vec![
                "unit-moved",
                "area-strike-resolved",
                "unit-damaged",
                "unit-damaged",
                "unit-damaged",
                "silo-changed"
            ]
        );
        assert_eq!(result.events[2]["unit"], 0);
        assert_eq!(result.events[3]["unit"], 1);
        assert_eq!(result.events[4]["unit"], 2);
        let observed = crate::semantic::observe_events(
            &AwbwVisibility,
            &state,
            &result.state,
            &result.events,
            "red",
        )
        .unwrap();
        assert!(
            observed
                .iter()
                .all(|event| event["unit"] != json!({"type":"friendly","unit":2}))
        );
    }

    #[test]
    fn move_explode_damages_other_units_then_removes_bomb_without_charge() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-complete.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.board.width = 7;
        let mut plain = state.board.tiles[0][0].clone();
        plain.terrain = TerrainId::Plain;
        plain.owner = None;
        plain.capture_points = None;
        plain.silo = None;
        state.board.tiles[0] = vec![plain; 7];
        state.units[0].id = UnitId::new(0);
        state.units[0].kind = UnitKindId::BlackBomb;
        state.units[0].location = Location::Board { position: [0, 0] };
        let mut ally = state.units[0].clone();
        ally.id = UnitId::new(1);
        ally.kind = UnitKindId::Infantry;
        ally.location = Location::Board { position: [2, 0] };
        ally.hp = 100;
        let mut enemy = ally.clone();
        enemy.id = UnitId::new(2);
        enemy.owner = "blue".into();
        enemy.location = Location::Board { position: [3, 0] };
        enemy.hp = 10;
        let mut reserve = ally.clone();
        reserve.id = UnitId::new(3);
        reserve.location = Location::Board { position: [6, 0] };
        state.units.extend([ally, enemy, reserve]);
        let command: Command = serde_json::from_value(json!({
            "type":"move-explode", "player":"red", "unit":0,
            "path":[[0,0]]
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert!(
            !result
                .state
                .units
                .iter()
                .any(|unit| unit.id == UnitId::new(0))
        );
        assert_eq!(
            result
                .state
                .units
                .iter()
                .find(|u| u.id == UnitId::new(1))
                .unwrap()
                .hp,
            50
        );
        assert_eq!(
            result
                .state
                .units
                .iter()
                .find(|u| u.id == UnitId::new(2))
                .unwrap()
                .hp,
            1
        );
        assert_eq!(
            result
                .state
                .units
                .iter()
                .find(|u| u.id == UnitId::new(3))
                .unwrap()
                .hp,
            100
        );
        assert_eq!(result.state.players[0].commanders[0].power_charge, 0);
        let types: Vec<_> = result
            .events
            .iter()
            .map(|e| e["type"].as_str().unwrap())
            .collect();
        assert_eq!(
            types,
            vec![
                "unit-moved",
                "area-strike-resolved",
                "unit-damaged",
                "unit-damaged",
                "unit-removed"
            ]
        );
        assert_eq!(result.events[2]["unit"], 1);
        assert_eq!(result.events[3]["unit"], 2);
    }

    #[test]
    fn delete_unit_resets_capture_before_removal_without_charge() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-complete.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.board.width = 2;
        state.board.tiles[0][0].capture_points = Some(10);
        let mut plain = state.board.tiles[0][0].clone();
        plain.capture_points = None;
        plain.owner = None;
        state.board.tiles[0].push(plain);
        let mut reserve = state.units[0].clone();
        reserve.id = UnitId::new(1);
        reserve.location = Location::Board { position: [1, 0] };
        state.units.push(reserve);
        let command: Command = serde_json::from_value(json!({
            "type":"delete-unit", "player":"red", "unit":0
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert!(
            !result
                .state
                .units
                .iter()
                .any(|unit| unit.id == UnitId::new(0))
        );
        assert_eq!(result.state.board.tiles[0][0].capture_points, Some(20));
        assert_eq!(result.state.players[0].commanders[0].power_charge, 0);
        assert_eq!(
            result.events,
            vec![
                json!({"type":"capture-changed","position":[0,0],"from":10,"to":20}),
                json!({"type":"unit-removed","unit":0,"reason":"delete"}),
            ]
        );
    }

    #[test]
    fn direct_unit_moves_then_attacks_from_resolved_destination() {
        let mut state = direct_combat_state(3);
        state.board.tiles[0][0].capture_points = Some(10);
        let mut defender = state.units[0].clone();
        defender.id = UnitId::new(1);
        defender.owner = "blue".into();
        defender.location = Location::Board { position: [2, 0] };
        state.units.push(defender);
        let command: Command = serde_json::from_value(json!({
            "type":"move-attack", "player":"red", "unit":0,
            "path":[[0,0],[1,0]],
            "target":{"type":"unit","unit":1}
        }))
        .unwrap();
        let random = vec![
            json!({"type":"combat-good-luck","value":0}),
            json!({"type":"combat-bad-luck","value":0}),
            json!({"type":"combat-good-luck","value":0}),
            json!({"type":"combat-bad-luck","value":0}),
        ];

        let result = execute(&state, command, &random).unwrap();
        let attacker = result
            .state
            .units
            .iter()
            .find(|unit| unit.id == UnitId::new(0))
            .unwrap();

        assert_eq!(board_position(attacker), Some([1, 0]));
        assert_eq!(attacker.fuel, 98);
        assert_eq!(attacker.action, UnitAction::Spent);
        assert_eq!(result.state.board.tiles[0][0].capture_points, Some(20));
        assert_eq!(result.random_consumed, 4);
        assert_eq!(
            result.events[..3],
            [
                json!({"type":"capture-changed","position":[0,0],"from":10,"to":20}),
                json!({"type":"unit-moved","unit":0,"from":[0,0],"to":[1,0],"path":[[0,0],[1,0]],"fuel_spent":1}),
                json!({"type":"attack-resolved","attacker":0,"weapon":"unlimited","target":{"type":"unit","unit":1}}),
            ]
        );
    }

    #[test]
    fn movement_can_reveal_attack_target_under_fog() {
        let mut state = direct_combat_state(3);
        state.settings.fog = true;
        state.board.tiles[0][2].terrain = TerrainId::Wood;
        let mut defender = state.units[0].clone();
        defender.id = UnitId::new(1);
        defender.owner = "blue".into();
        defender.location = Location::Board { position: [2, 0] };
        state.units.push(defender);
        let visibility = AwbwVisibility;
        assert!(!visibility.visible_unit(&state, "red-team", &state.units[1]));
        let command: Command = serde_json::from_value(json!({
            "type":"move-attack", "player":"red", "unit":0,
            "path":[[0,0],[1,0]],
            "target":{"type":"unit","unit":1}
        }))
        .unwrap();
        let random = vec![
            json!({"type":"combat-good-luck","value":0}),
            json!({"type":"combat-bad-luck","value":0}),
            json!({"type":"combat-good-luck","value":0}),
            json!({"type":"combat-bad-luck","value":0}),
        ];

        let result = execute(&state, command, &random).unwrap();

        assert_eq!(result.events[0]["type"], "unit-moved");
        assert_eq!(result.events[1]["type"], "attack-resolved");
    }

    #[test]
    fn hidden_blocker_truncates_combat_movement_and_suppresses_attack() {
        let mut state = direct_combat_state(5);
        state.settings.fog = true;
        let mut blocker = state.units[0].clone();
        blocker.id = UnitId::new(1);
        blocker.kind = UnitKindId::Tank;
        blocker.owner = "blue".into();
        blocker.location = Location::Board { position: [3, 0] };
        let mut target = state.units[0].clone();
        target.id = UnitId::new(2);
        target.owner = "blue".into();
        target.location = Location::Board { position: [4, 0] };
        state.units.extend([blocker, target]);
        let command: Command = serde_json::from_value(json!({
            "type":"move-attack", "player":"red", "unit":0,
            "path":[[0,0],[1,0],[2,0],[3,0]],
            "target":{"type":"unit","unit":2}
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();
        let attacker = result
            .state
            .units
            .iter()
            .find(|unit| unit.id == UnitId::new(0))
            .unwrap();

        assert_eq!(board_position(attacker), Some([2, 0]));
        assert_eq!(attacker.action, UnitAction::Spent);
        assert_eq!(result.random_consumed, 0);
        assert_eq!(
            result.events,
            [
                json!({"type":"unit-moved","unit":0,"from":[0,0],"to":[2,0],"path":[[0,0],[1,0],[2,0]],"fuel_spent":2}),
                json!({"type":"movement-trapped","unit":0,"blocker":1,"position":[3,0]}),
            ]
        );
    }

    #[test]
    fn indirect_units_cannot_move_and_fire() {
        let mut state = direct_combat_state(4);
        state.units[0].kind = UnitKindId::Artillery;
        let mut defender = state.units[0].clone();
        defender.id = UnitId::new(1);
        defender.kind = UnitKindId::Infantry;
        defender.owner = "blue".into();
        defender.location = Location::Board { position: [3, 0] };
        state.units.push(defender);
        let command: Command = serde_json::from_value(json!({
            "type":"move-attack", "player":"red", "unit":0,
            "path":[[0,0],[1,0]],
            "target":{"type":"unit","unit":1}
        }))
        .unwrap();

        assert_eq!(
            execute(&state, command, &[]),
            Err(ExecuteError::Violation(
                json!({"code":"ACTION_NOT_SUPPORTED","action":"move-and-fire"})
            ))
        );
    }

    #[test]
    fn neutral_infantry_combat_consumes_counter_luck() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/movement/infantry-plain-move.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.board.width = 2;
        state.board.tiles[0].truncate(2);
        state.teams.push(crate::semantic::Team {
            id: "blue-team".into(),
            status: crate::semantic::TeamStatus::Active,
        });
        let mut blue = state.players[0].clone();
        blue.id = "blue".into();
        blue.team = "blue-team".into();
        blue.commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.players[0].commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.players.push(blue);
        let mut defender = state.units[0].clone();
        defender.id = UnitId::new(1);
        defender.owner = "blue".into();
        defender.location = Location::Board { position: [1, 0] };
        state.units[0].id = UnitId::new(0);
        state.units.push(defender);
        let command: Command = serde_json::from_value(json!({"type":"move-attack","player":"red","unit":0,"path":[[0,0]],"target":{"type":"unit","unit":1}})).unwrap();
        let random = vec![
            json!({"type":"combat-good-luck","value":0}),
            json!({"type":"combat-bad-luck","value":0}),
            json!({"type":"combat-good-luck","value":0}),
            json!({"type":"combat-bad-luck","value":0}),
        ];
        let result = execute(&state, command, &random).unwrap();
        assert_eq!(result.state.units[0].hp, 75);
        assert_eq!(result.state.units[1].hp, 51);
        assert_eq!(result.random_consumed, 4);
        assert_eq!(result.events[0]["weapon"], "unlimited");
        assert_eq!(result.events[2]["weapon"], "unlimited");

        let attack = |state: &State| {
            let command: Command = serde_json::from_value(json!({"type":"move-attack","player":"red","unit":0,"path":[[0,0]],"target":{"type":"unit","unit":1}})).unwrap();
            execute(state, command, &random).unwrap()
        };

        let mut tank_vs_tank = state.clone();
        tank_vs_tank.units[0].kind = UnitKindId::Tank;
        tank_vs_tank.units[0].ammo = 9;
        tank_vs_tank.units[1].kind = UnitKindId::Tank;
        tank_vs_tank.units[1].ammo = 9;
        let result = attack(&tank_vs_tank);
        assert_eq!(result.events[0]["ammo_before"], 9);
        assert_eq!(result.events[0]["ammo_after"], 8);
        assert_eq!(result.events[1]["weapon"], "ammo");

        let mut tank_vs_infantry = state.clone();
        tank_vs_infantry.units[0].kind = UnitKindId::Tank;
        tank_vs_infantry.units[0].ammo = 9;
        let result = attack(&tank_vs_infantry);
        assert_eq!(result.events[0]["weapon"], "unlimited");
        assert_eq!(
            result
                .state
                .units
                .iter()
                .find(|u| u.id == UnitId::new(0))
                .unwrap()
                .ammo,
            9
        );

        let mut empty_tank_vs_tank = tank_vs_tank;
        empty_tank_vs_tank.units[0].ammo = 0;
        let result = attack(&empty_tank_vs_tank);
        assert_eq!(result.events[0]["weapon"], "unlimited");
    }

    #[test]
    fn lethal_combat_routes_last_unit_owner() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/combat/neutral-infantry-counter.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state
            .units
            .iter_mut()
            .find(|unit| unit.id == UnitId::new(1))
            .unwrap()
            .hp = 1;
        let command: Command = serde_json::from_value(case["steps"][2]["command"].clone()).unwrap();
        let random = vec![
            json!({"type":"combat-good-luck","value":0}),
            json!({"type":"combat-bad-luck","value":0}),
        ];

        let result = execute(&state, command, &random).unwrap();

        assert!(matches!(result.state.match_state, Match::Finished { .. }));
        assert_eq!(result.state.turn.phase, Phase::Finished);
        assert_eq!(
            result.events[result.events.len() - 3..],
            [
                json!({"type":"player-status-changed","player":"blue","from":"active","to":"eliminated"}),
                json!({"type":"team-eliminated","team":"blue-team","reason":"rout"}),
                json!({"type":"match-completed","outcome":{"type":"victory","winners":["red-team"],"reason":"rout"}}),
            ]
        );
    }
}
