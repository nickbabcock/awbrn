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
use crate::ruleset::{self, Relation, TargetSet};
use crate::semantic::{
    AwbwVisibility, KnownReason, Location, PlayerId, Pos, State, UnitAction, UnitId,
};
use crate::violation::{Action, Violation};

pub(crate) fn execute_move_supply(
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    path: Vec<Pos>,
) -> Result<Execution, ExecuteError> {
    let turn = ActiveTurn::open(state, player)?;
    let plan = turn.plan_move(unit_id, path)?;
    let unit = &state.units[plan.unit_index()];
    let supply = ruleset::profile(unit.kind).supply;
    let Some(supply) = supply.filter(|supply| supply.relation == Relation::Adjacent) else {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::MoveSupply,
        }));
    };
    let destination = plan.destination();
    let visibility = AwbwVisibility;
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(destination)
            && occupancy_is_disclosed(&visibility, state, plan.actor_team(), other)
    }) {
        return Err(violation(Violation::DestinationOccupied {
            position: destination,
        }));
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
                    supply.targets,
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
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
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
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    path: Vec<Pos>,
    target_id: UnitId,
) -> Result<Execution, ExecuteError> {
    let turn = ActiveTurn::open(state, player)?;
    let plan = turn.plan_move(unit_id, path)?;
    let unit = &state.units[plan.unit_index()];
    let repair = ruleset::profile(unit.kind).repair;
    let Some(repair) = repair.filter(|repair| repair.relation == Relation::Adjacent) else {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::MoveRepair,
        }));
    };
    let target_index = state.units.index_of(target_id);
    let target = target_index.and_then(|index| state.units.at(index));
    let target_team =
        target.and_then(|target| state.find_player(&target.owner).map(|owner| &owner.team));
    let target_position = target.and_then(board_position);
    if !target.is_some_and(|target| {
        target.id != unit_id && target_team == Some(plan.actor_team()) && target_position.is_some()
    }) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    }
    let destination = plan.destination();
    let target_position = target_position.expect("target validity established its position");
    if target_position.x.abs_diff(destination.x) + target_position.y.abs_diff(destination.y) != 1 {
        return Err(violation(Violation::TargetOutOfRange {
            target: Some(target_id.into()),
        }));
    }
    let visibility = AwbwVisibility;
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(destination)
            && occupancy_is_disclosed(&visibility, state, plan.actor_team(), other)
    }) {
        return Err(violation(Violation::DestinationOccupied {
            position: destination,
        }));
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
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    path: Vec<Pos>,
    transport_id: UnitId,
) -> Result<Execution, ExecuteError> {
    let turn = ActiveTurn::open(state, player)?;
    let plan = turn.plan_move(unit_id, path)?;
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

    let mut outcome = execute_planned_movement(state, unit_id, &plan);
    if outcome.trapped {
        return Ok(Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        });
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
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

pub(crate) fn execute_unload(
    state: &State,
    player: &PlayerId,
    transport_id: UnitId,
    cargo_id: UnitId,
    destination: Pos,
) -> Result<Execution, ExecuteError> {
    let _turn = ActiveTurn::open(state, player)?;
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
    let cargo_index = state.units.index_of(cargo_id);
    let cargo = cargo_index.and_then(|index| state.units.at(index));
    let cargo_slot = cargo.and_then(|cargo| match &cargo.location {
        Location::Cargo { transport, slot } if *transport == transport_id => Some(*slot),
        _ => None,
    });
    if cargo_slot.is_none() {
        return Err(violation(Violation::InvalidTarget {
            target: Some(cargo_id.into()),
        }));
    }
    let transport_position = transport_position.expect("transport validity established position");
    if transport_position.x.abs_diff(destination.x) + transport_position.y.abs_diff(destination.y)
        != 1
    {
        return Err(violation(Violation::TargetOutOfRange {
            target: Some(destination.into()),
        }));
    }
    let cargo = cargo.expect("cargo validity established unit");
    let movement_class = ruleset::profile(cargo.kind).movement_class;
    let weather = commander::effective_weather(state, cargo);
    let destination_tile = state.board.get(destination);
    let passable = destination_tile.is_some_and(|tile| {
        commander::effective_movement_cost(
            state,
            cargo,
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
        events: vec![Event::UnitUnloaded {
            unit: cargo_id,
            transport: transport_id,
            position: destination,
        }],
        random_consumed: 0,
    })
}

pub(crate) fn execute_move_join(
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    path: Vec<Pos>,
    target_id: UnitId,
) -> Result<Execution, ExecuteError> {
    let turn = ActiveTurn::open(state, player)?;
    let plan = turn.plan_move(unit_id, path)?;
    let unit = &state.units[plan.unit_index()];
    let origin = plan.origin();
    let path = plan.path();
    let profile = ruleset::profile(unit.kind);
    let actor_team = plan.actor_team();
    let unit_index = plan.unit_index();
    let entry_costs = plan.entry_costs();
    let visibility = AwbwVisibility;

    let target_index = state.units.index_of(target_id);
    let target = target_index.and_then(|index| state.units.at(index));
    let target_owner_team =
        target.and_then(|target| state.find_player(&target.owner).map(|owner| &owner.team));
    let target_position = target.and_then(board_position);
    let target_valid = target.is_some_and(|target| {
        target.id != unit.id
            && target.kind == unit.kind
            && target_owner_team == Some(actor_team)
            && target_position.is_some()
    });
    if !target_valid {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    }
    let target = target.expect("target validity established the unit");
    let target_index = target_index.expect("target validity established the index");
    let destination = *path.last().expect("origin was checked");
    if target_position != Some(destination) {
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
