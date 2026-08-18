//! Units acting on other units: carrying, joining, supplying, repairing.
//!
//! Normative source:
//! * `spec/semantics/transport.md`
//! * `spec/semantics/join.md`
//! * `spec/semantics/supply.md`
//! * `spec/semantics/repair.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::commander::{self};
use crate::event::Event;
use crate::ruleset::{self, Relation, TargetSet, TerrainTrait};
use crate::semantic::{
    AwbwVisibility, KnownReason, Location, PlayerId, Pos, State, UnitAction, UnitId, Visibility,
};
use crate::violation::{Action, Violation};

#[derive(Debug)]
pub(super) struct Supply {
    targets: TargetSet,
    destination: AvailableDestination,
}

#[derive(Debug)]
pub(super) struct Repair {
    target: UnitId,
    capability: ruleset::RepairProfile,
    target_index: usize,
    heal_cost: u64,
    max_fuel: u64,
    max_ammo: u64,
    destination: AvailableDestination,
}

#[derive(Debug)]
pub(super) struct Load {
    transport: UnitId,
    slot: usize,
}

#[derive(Debug)]
pub(super) struct Join {
    target: PreparedJoinTarget,
}

#[derive(Debug)]
struct PreparedJoinTarget {
    id: UnitId,
    index: usize,
}

#[derive(Debug)]
pub(super) struct PreparedUnload<'a> {
    cargo: PreparedUnloadCargo<'a>,
    destination: Pos,
}

pub(crate) fn execute_move_supply(
    turn: &ActiveTurn<'_>,
    unit_id: UnitId,
    path: Vec<Pos>,
) -> Result<Execution, ExecuteError> {
    let movement = turn.prepare_move(unit_id, path)?;
    let prepared = prepare_supply(movement.prepare_destination())?;
    Ok(execute_prepared_supply(prepared))
}

pub(super) fn prepare_supply<'a, V>(
    destination: PreparedDestination<'a, V>,
) -> Result<Prepared<'a, Supply>, ExecuteError>
where
    V: std::borrow::Borrow<AwbwView<'a>>,
{
    let action = validate_supply(&destination)?;
    Ok(Prepared {
        movement: destination.into_movement(),
        action,
    })
}

pub(super) fn validate_supply<'a, V>(
    destination: &PreparedDestination<'a, V>,
) -> Result<Supply, ExecuteError>
where
    V: std::borrow::Borrow<AwbwView<'a>>,
{
    let movement = destination.movement();
    let state = movement.state();
    let plan = movement.plan();
    let unit = &state.units[plan.unit_index()];
    let supply = ruleset::profile(unit.kind).supply;
    let Some(supply) = supply.filter(|supply| supply.relation == Relation::Adjacent) else {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::MoveSupply,
        }));
    };
    let available = destination.available_destination()?;
    Ok(Supply {
        targets: supply.targets,
        destination: available,
    })
}

pub(super) fn execute_prepared_supply(prepared: Prepared<'_, Supply>) -> Execution {
    let Prepared {
        movement,
        action: Supply {
            targets,
            destination: _destination,
        },
    } = prepared;
    let state = movement.state();
    let unit_id = movement.unit();
    let plan = movement.plan();
    let unit = &state.units[plan.unit_index()];
    let mut outcome = execute_planned_movement(state, unit_id, plan);
    if outcome.trapped {
        return Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        };
    }
    let actual_destination =
        board_position(&outcome.state.units[plan.unit_index()]).expect("mover remains on board");
    let mut supply_ids: Vec<_> = state
        .units
        .iter()
        .filter(|target| {
            target.id != unit_id
                && supply_target_eligible(
                    state,
                    &unit.owner,
                    plan.actor_team(),
                    &target.owner,
                    targets,
                )
                && board_position(target).is_some_and(|position| {
                    position.x.abs_diff(actual_destination.x)
                        + position.y.abs_diff(actual_destination.y)
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
            .get_mut(id)
            .expect("supply target remains present");
        let profile = ruleset::profile(target.kind);
        let max_fuel = profile.max_fuel;
        let max_ammo = profile.max_ammo;
        let fuel_before = target.fuel;
        let ammo_before = target.ammo;
        target.fuel = max_fuel;
        target.ammo = max_ammo;
        if fuel_before != max_fuel || ammo_before != max_ammo {
            outcome.events.push(Event::UnitResourced {
                unit: id,
                fuel_before,
                fuel_after: max_fuel,
                ammo_before,
                ammo_after: max_ammo,
                reason: KnownReason::UnitSupply.into(),
            });
        }
    }
    Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    }
}

pub(crate) fn supply_target_eligible(
    state: &State,
    source_owner: &PlayerId,
    source_team: &crate::semantic::TeamId,
    target_owner: &PlayerId,
    targets: TargetSet,
) -> bool {
    match targets {
        TargetSet::OwnedUnits => target_owner == source_owner,
        TargetSet::FriendlyUnits => state
            .find_player(target_owner)
            .is_some_and(|owner| owner.team == source_team),
    }
}

pub(crate) fn execute_move_repair(
    turn: &ActiveTurn<'_>,
    unit_id: UnitId,
    path: Vec<Pos>,
    target_id: UnitId,
) -> Result<Execution, ExecuteError> {
    let movement = turn.prepare_move(unit_id, path)?;
    let prepared = prepare_repair(movement.prepare_destination(), target_id)?;
    execute_prepared_repair(prepared)
}

pub(super) fn prepare_repair<'a, V>(
    destination: PreparedDestination<'a, V>,
    target_id: UnitId,
) -> Result<Prepared<'a, Repair>, ExecuteError>
where
    V: std::borrow::Borrow<AwbwView<'a>>,
{
    let action = validate_repair(&destination, target_id)?;
    Ok(Prepared {
        movement: destination.into_movement(),
        action,
    })
}

pub(super) fn validate_repair<'a, V>(
    destination: &PreparedDestination<'a, V>,
    target_id: UnitId,
) -> Result<Repair, ExecuteError>
where
    V: std::borrow::Borrow<AwbwView<'a>>,
{
    let movement = destination.movement();
    let state = movement.state();
    let unit_id = movement.unit();
    let plan = movement.plan();
    let unit = &state.units[plan.unit_index()];
    let repair = ruleset::profile(unit.kind).repair;
    let Some(repair) = repair.filter(|repair| repair.relation == Relation::Adjacent) else {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::MoveRepair,
        }));
    };
    let Some(target_index) = state.units.index_of(target_id) else {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    };
    let target = &state.units[target_index];
    let target_team = state.find_player(&target.owner).map(|owner| &owner.team);
    let Some(target_position) = board_position(target) else {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    };
    if target.id == unit_id || target_team != Some(plan.actor_team()) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    }
    let position = plan.destination();
    if target_position.x.abs_diff(position.x) + target_position.y.abs_diff(position.y) != 1 {
        return Err(violation(Violation::TargetOutOfRange {
            target: Some(target_id.into()),
        }));
    }
    let available = destination.available_destination()?;

    let target_profile = ruleset::profile(target.kind);
    let heal_cost = target_profile
        .cost
        .checked_mul(repair.cost_percent)
        .and_then(|cost| cost.checked_div(100))
        .ok_or(ExecuteError::UnsupportedRuleset)?;

    Ok(Repair {
        target: target_id,
        capability: repair,
        target_index,
        heal_cost,
        max_fuel: target_profile.max_fuel,
        max_ammo: target_profile.max_ammo,
        destination: available,
    })
}

pub(super) fn execute_prepared_repair(
    prepared: Prepared<'_, Repair>,
) -> Result<Execution, ExecuteError> {
    let Prepared {
        movement,
        action:
            Repair {
                target: target_id,
                capability,
                target_index,
                heal_cost,
                max_fuel,
                max_ammo,
                destination: _destination,
            },
    } = prepared;
    let state = movement.state();
    let player = &state.turn.active_player;
    let unit_id = movement.unit();
    let plan = movement.plan();
    let exact_hp = capability.exact_hp;

    let mut outcome = execute_planned_movement(state, unit_id, plan);
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
        outcome.events.push(Event::UnitResourced {
            unit: target_id,
            fuel_before,
            fuel_after: max_fuel,
            ammo_before,
            ammo_after: max_ammo,
            reason: KnownReason::UnitRepair.into(),
        });
    }
    let visual_hp = target.hp.div_ceil(exact_hp);
    if visual_hp < 10 {
        let player_index = outcome.state.player_index(player).ok_or_else(|| {
            ExecuteError::InvalidState(format!("unknown active player {player}").into())
        })?;
        let funds_before = outcome.state.player_mut(player_index).funds;
        if heal_cost <= funds_before {
            let hp_before = outcome.state.units[target_index].hp;
            let hp_after = (visual_hp + 1).min(10) * exact_hp;
            outcome.state.player_mut(player_index).funds -= heal_cost;
            outcome.events.push(Event::FundsChanged {
                player: player.clone(),
                from: funds_before,
                to: funds_before - heal_cost,
                reason: KnownReason::UnitRepair.into(),
            });
            outcome.state.units[target_index].hp = hp_after;
            outcome.events.push(Event::UnitRepaired {
                unit: target_id,
                from_hp: hp_before,
                to_hp: hp_after,
                reason: KnownReason::UnitRepair.into(),
            });
        }
    }
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

pub(crate) fn execute_move_load(
    turn: &ActiveTurn<'_>,
    unit_id: UnitId,
    path: Vec<Pos>,
    transport_id: UnitId,
) -> Result<Execution, ExecuteError> {
    let movement = turn.prepare_move(unit_id, path)?;
    let prepared = prepare_load(movement.prepare_destination(), transport_id)?;
    Ok(execute_prepared_load(prepared))
}

pub(super) fn prepare_load<'a, V>(
    destination: PreparedDestination<'a, V>,
    transport_id: UnitId,
) -> Result<Prepared<'a, Load>, ExecuteError>
where
    V: std::borrow::Borrow<AwbwView<'a>>,
{
    let action = validate_load(&destination, transport_id)?;
    Ok(Prepared {
        movement: destination.into_movement(),
        action,
    })
}

pub(super) fn validate_load<'a, V>(
    destination: &PreparedDestination<'a, V>,
    transport_id: UnitId,
) -> Result<Load, ExecuteError>
where
    V: std::borrow::Borrow<AwbwView<'a>>,
{
    let movement = destination.movement();
    let state = movement.state();
    let unit_id = movement.unit();
    let plan = movement.plan();
    let player = &state.turn.active_player;
    let mover = &state.units[plan.unit_index()];
    let transport_index = state.units.index_of(transport_id);
    let transport = transport_index.and_then(|index| state.units.at(index));
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
        return Err(violation(Violation::InvalidTarget {
            target: Some(transport_id.into()),
        }));
    }
    let transport_position =
        board_position(transport.expect("target validity established transport position"))
            .expect("target validity established transport position");
    let destination = plan.destination();
    if destination != transport_position {
        return Err(violation(Violation::InvalidTarget {
            target: Some(destination.into()),
        }));
    }
    let capacity = capacity.expect("target validity established capacity");
    let slot = (0..capacity)
        .find(|slot| !occupied_slots.contains(slot))
        .ok_or_else(|| {
            ExecuteError::InvalidState(format!("transport {transport_id} is full").into())
        })?;

    Ok(Load {
        transport: transport_id,
        slot,
    })
}

pub(super) fn execute_prepared_load(prepared: Prepared<'_, Load>) -> Execution {
    let Prepared {
        movement,
        action: Load {
            transport: transport_id,
            slot,
        },
    } = prepared;
    let state = movement.state();
    let unit_id = movement.unit();
    let plan = movement.plan();
    let mut outcome = execute_planned_movement(state, unit_id, plan);
    if outcome.trapped {
        return Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        };
    }
    outcome.state.units[plan.unit_index()].location = Location::Cargo {
        transport: transport_id,
        slot,
    };
    outcome.events.push(Event::UnitLoaded {
        unit: unit_id,
        transport: transport_id,
        slot,
    });
    Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    }
}

pub(crate) fn execute_unload(
    turn: &ActiveTurn<'_>,
    transport_id: UnitId,
    cargo_id: UnitId,
    destination: Pos,
) -> Result<Execution, ExecuteError> {
    let transport = prepare_unload_transport(turn, transport_id)?;
    let cargo = prepare_unload_cargo(transport, cargo_id)?;
    let prepared = prepare_unload(cargo, destination)?;
    Ok(execute_prepared_unload(prepared))
}

pub(super) fn prepare_unload_transport<'a>(
    turn: &ActiveTurn<'a>,
    transport_id: UnitId,
) -> Result<PreparedUnloadTransport<'a>, ExecuteError> {
    let state = turn.state();
    let player = turn.player();
    let transport_index = state.units.index_of(transport_id);
    let transport = transport_index.and_then(|index| state.units.at(index));
    let transport_position = transport.and_then(board_position);
    if !transport.is_some_and(|transport| {
        transport.owner == player
            && transport_position.is_some()
            && ruleset::profile(transport.kind).transport.is_some()
    }) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(transport_id.into()),
        }));
    }
    Ok(PreparedUnloadTransport {
        state,
        transport: transport_id,
        position: transport_position.expect("transport validity established position"),
    })
}

pub(super) fn prepare_unload_cargo(
    transport: PreparedUnloadTransport<'_>,
    cargo_id: UnitId,
) -> Result<PreparedUnloadCargo<'_>, ExecuteError> {
    let state = transport.state;
    let cargo_index = state.units.index_of(cargo_id);
    let cargo = cargo_index.and_then(|index| state.units.at(index));
    let cargo_slot = cargo.and_then(|cargo| match &cargo.location {
        Location::Cargo {
            transport: carried_by,
            slot,
        } if *carried_by == transport.transport => Some(*slot),
        _ => None,
    });
    if cargo_slot.is_none() {
        return Err(violation(Violation::InvalidTarget {
            target: Some(cargo_id.into()),
        }));
    }
    Ok(PreparedUnloadCargo {
        transport,
        cargo: cargo_id,
        cargo_index: cargo_index.expect("cargo validity established index"),
        cargo_slot: cargo_slot.expect("cargo validity established slot"),
    })
}

pub(super) fn prepare_unload(
    cargo: PreparedUnloadCargo<'_>,
    destination: Pos,
) -> Result<PreparedUnload<'_>, ExecuteError> {
    let state = cargo.transport.state;
    let transport_position = cargo.transport.position;
    if transport_position.x.abs_diff(destination.x) + transport_position.y.abs_diff(destination.y)
        != 1
    {
        return Err(violation(Violation::TargetOutOfRange {
            target: Some(destination.into()),
        }));
    }
    let unit = &state.units[cargo.cargo_index];
    let movement_class = ruleset::profile(unit.kind).movement_class;
    let weather = commander::effective_weather(state, unit);
    let destination_tile = state.board.get(destination);
    let passable = destination_tile.is_some_and(|tile| {
        !ruleset::terrain_has(tile.terrain, TerrainTrait::Teleporter)
            && commander::effective_movement_cost(
                state,
                unit,
                ruleset::movement_cost(tile.terrain, weather, movement_class),
            )
            .is_some()
    });
    if !passable {
        return Err(violation(Violation::TerrainImpassable {
            index: None,
            position: destination,
        }));
    }
    if state
        .units
        .iter()
        .any(|unit| board_position(unit) == Some(destination))
    {
        return Err(violation(Violation::DestinationOccupied {
            position: destination,
        }));
    }

    Ok(PreparedUnload { cargo, destination })
}

pub(super) fn execute_prepared_unload(prepared: PreparedUnload<'_>) -> Execution {
    let PreparedUnload { cargo, destination } = prepared;
    let state = cargo.transport.state;
    let transport_id = cargo.transport.transport;
    let cargo_id = cargo.cargo;
    let mut next = state.clone();
    next.units[cargo.cargo_index].location = Location::Board {
        position: destination,
    };
    next.units[cargo.cargo_index].action = UnitAction::Spent;
    for unit in &mut next.units {
        if let Location::Cargo { transport, slot } = &mut unit.location
            && *transport == transport_id
            && *slot > cargo.cargo_slot
        {
            *slot -= 1;
        }
    }
    Execution {
        state: next,
        events: vec![Event::UnitUnloaded {
            unit: cargo_id,
            transport: transport_id,
            position: destination,
        }],
        random_consumed: 0,
    }
}

pub(crate) fn execute_move_join(
    turn: &ActiveTurn<'_>,
    unit_id: UnitId,
    path: Vec<Pos>,
    target_id: UnitId,
) -> Result<Execution, ExecuteError> {
    let movement = turn.prepare_move(unit_id, path)?;
    let prepared = prepare_join(movement.prepare_destination(), target_id)?;
    execute_prepared_join(prepared)
}

pub(super) fn prepare_join<'a, V>(
    destination: PreparedDestination<'a, V>,
    target_id: UnitId,
) -> Result<Prepared<'a, Join>, ExecuteError>
where
    V: std::borrow::Borrow<AwbwView<'a>>,
{
    let action = validate_join(&destination, target_id)?;
    Ok(Prepared {
        movement: destination.into_movement(),
        action,
    })
}

pub(super) fn validate_join<'a, V>(
    destination: &PreparedDestination<'a, V>,
    target_id: UnitId,
) -> Result<Join, ExecuteError>
where
    V: std::borrow::Borrow<AwbwView<'a>>,
{
    let movement = destination.movement();
    let state = movement.state();
    let unit_id = movement.unit();
    let plan = movement.plan();
    let unit = &state.units[plan.unit_index()];
    let actor_team = plan.actor_team();

    let Some(target_index) = state.units.index_of(target_id) else {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    };
    let target = &state.units[target_index];
    let target_owner_team = state.find_player(&target.owner).map(|owner| &owner.team);
    let Some(target_position) = board_position(target) else {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    };
    if target.id == unit.id || target.kind != unit.kind || target_owner_team != Some(actor_team) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    }
    let destination = plan.destination();
    if target_position != destination {
        return Err(violation(Violation::InvalidTarget {
            target: Some(destination.into()),
        }));
    }
    if target.hp.div_ceil(10) == 10 {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    }
    let target_carries_cargo = state.units.iter().any(
        |cargo| matches!(&cargo.location, Location::Cargo { transport, .. } if *transport == target_id),
    );
    if target_carries_cargo {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    }
    let mover_carries_cargo = state.units.iter().any(
        |cargo| matches!(&cargo.location, Location::Cargo { transport, .. } if *transport == unit_id),
    );
    if mover_carries_cargo {
        return Err(violation(Violation::InvalidTarget {
            target: Some(unit_id.into()),
        }));
    }

    Ok(Join {
        target: PreparedJoinTarget {
            id: target_id,
            index: target_index,
        },
    })
}

pub(super) fn execute_prepared_join(
    prepared: Prepared<'_, Join>,
) -> Result<Execution, ExecuteError> {
    let Prepared {
        movement,
        action:
            Join {
                target:
                    PreparedJoinTarget {
                        id: target_id,
                        index: target_index,
                    },
            },
    } = prepared;
    let state = movement.state();
    let player = &state.turn.active_player;
    let unit_id = movement.unit();
    let plan = movement.plan();
    let unit = &state.units[plan.unit_index()];
    let origin = plan.origin();
    let path = plan.path();
    let profile = ruleset::profile(unit.kind);
    let actor_team = plan.actor_team();
    let unit_index = plan.unit_index();
    let entry_costs = plan.entry_costs();
    let view = AwbwVisibility.view(state, actor_team);
    let target = &state.units[target_index];

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
                        && !occupancy_is_disclosed(&view, other)
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
    events.push(Event::UnitMoved {
        unit: unit_id,
        from: origin,
        to: actual_destination,
        path: actual_path,
        fuel_spent,
    });
    if let Some((_, position, blocker)) = trap {
        events.push(Event::MovementTrapped {
            unit: unit_id,
            blocker,
            position,
        });
        return Ok(Execution {
            state: next,
            events,
            random_consumed: 0,
        });
    }

    let combined_visual_hp = unit.hp.div_ceil(10) + target.hp.div_ceil(10);
    let moved_fuel = unit.fuel - fuel_spent;
    let max_fuel = profile.max_fuel;
    let max_ammo = profile.max_ammo;
    let cost = profile.cost;
    next.units[target_index].hp = combined_visual_hp.min(10) * 10;
    next.units[target_index].fuel = (moved_fuel + target.fuel).min(max_fuel);
    next.units[target_index].ammo = (unit.ammo + target.ammo).min(max_ammo);
    next.units[target_index].action = UnitAction::Spent;
    next.units.remove(unit_index);
    events.push(Event::UnitsJoined {
        source: unit_id,
        target: target_id,
    });

    if combined_visual_hp > 10 {
        let refund = (cost / 10) * u64::from(combined_visual_hp - 10);
        let player_index = next.player_index(player).ok_or_else(|| {
            ExecuteError::InvalidState(format!("unknown active player {player}").into())
        })?;
        let funds_before = next.player_mut(player_index).funds;
        next.player_mut(player_index).funds = funds_before
            .checked_add(refund)
            .ok_or_else(|| ExecuteError::InvalidState("join refund overflow".into()))?;
        events.push(Event::FundsChanged {
            player: player.clone(),
            from: funds_before,
            to: next.player_mut(player_index).funds,
            reason: KnownReason::UnitJoin.into(),
        });
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: 0,
    })
}
