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
use crate::semantic::{KnownReason, Location, PlayerIdx, Pos, State, UnitAction, UnitId};
use crate::violation::{Action, Violation};

#[derive(Debug)]
pub(super) struct Supply;

#[derive(Debug)]
pub(super) struct SupplyProof {
    targets: TargetSet,
    destination: AvailableDestination,
}

#[derive(Debug)]
pub(super) struct Repair(pub(super) UnitId);

#[derive(Debug)]
pub(super) struct RepairProof {
    target: UnitId,
    capability: ruleset::RepairProfile,
    target_index: usize,
    heal_cost: u64,
    max_fuel: u64,
    max_ammo: u64,
    destination: AvailableDestination,
}

#[derive(Debug)]
pub(super) struct Load(pub(super) UnitId);

#[derive(Debug)]
pub(super) struct LoadProof {
    transport: UnitId,
    slot: usize,
}

#[derive(Debug)]
pub(super) struct Join(pub(super) UnitId);

#[derive(Debug)]
pub(super) struct JoinProof {
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

impl<'a> DestinationAction<'a> for Supply {
    type Proof = SupplyProof;

    fn validate<M>(
        &self,
        destination: &PreparedDestination<'a, M>,
    ) -> Result<Self::Proof, ExecuteError>
    where
        M: std::borrow::Borrow<crate::query::TurnMaps<'a>>,
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
        Ok(SupplyProof {
            targets: supply.targets,
            destination: available,
        })
    }

    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a> {
        PreparedCommandKind::Supply(bound)
    }
}

pub(super) fn execute_prepared_supply(prepared: MovementAction<'_, SupplyProof>) -> Execution {
    let MovementAction {
        movement,
        trap,
        action: SupplyProof {
            targets,
            destination: _destination,
        },
    } = prepared;
    let state = movement.state();
    let unit_id = movement.unit();
    let plan = movement.plan();
    let unit = &state.units[plan.unit_index()];
    let mut outcome = execute_planned_movement(state, unit_id, plan, trap);
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
                    unit.owner,
                    plan.actor_team(),
                    target.owner,
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
    source_owner: PlayerIdx,
    source_team: &crate::semantic::TeamId,
    target_owner: PlayerIdx,
    targets: TargetSet,
) -> bool {
    match targets {
        TargetSet::OwnedUnits => target_owner == source_owner,
        TargetSet::FriendlyUnits => state
            .players
            .get(target_owner.get())
            .is_some_and(|owner| owner.team == source_team),
    }
}

impl<'a> DestinationAction<'a> for Repair {
    type Proof = RepairProof;

    fn validate<M>(
        &self,
        destination: &PreparedDestination<'a, M>,
    ) -> Result<Self::Proof, ExecuteError>
    where
        M: std::borrow::Borrow<crate::query::TurnMaps<'a>>,
    {
        let target_id = self.0;
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
        let target_team = state
            .players
            .get(target.owner.get())
            .map(|owner| &owner.team);
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

        Ok(RepairProof {
            target: target_id,
            capability: repair,
            target_index,
            heal_cost,
            max_fuel: target_profile.max_fuel,
            max_ammo: target_profile.max_ammo,
            destination: available,
        })
    }

    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a> {
        PreparedCommandKind::Repair(bound)
    }
}

pub(super) fn execute_prepared_repair(
    prepared: MovementAction<'_, RepairProof>,
) -> Result<Execution, ExecuteError> {
    let MovementAction {
        movement,
        trap,
        action:
            RepairProof {
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

    let mut outcome = execute_planned_movement(state, unit_id, plan, trap);
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

/// How many cargo slots the bitmask below can name.
///
/// Every ruleset gives a transport a single-digit capacity, so the mask holds
/// every slot a valid state can name.
const CARGO_SLOTS: usize = u32::BITS as usize;

/// Which of `transport`'s cargo slots are taken, as a bitmask.
///
/// Enumeration asks this for every destination a loadable unit can reach, and
/// the answer used to be a `Vec<usize>` that was then searched linearly. A
/// mask removes the allocation and turns the search for a free slot into
/// `trailing_ones`.
fn occupied_slots(state: &State, transport: UnitId, capacity: usize) -> Result<u32, ExecuteError> {
    if capacity > CARGO_SLOTS {
        return Err(ExecuteError::UnsupportedRuleset);
    }
    let mut occupied = 0_u32;
    for cargo in state.units.iter() {
        let Location::Cargo {
            transport: carrier,
            slot,
        } = cargo.location
        else {
            continue;
        };
        if carrier != transport {
            continue;
        }
        if slot >= CARGO_SLOTS {
            return Err(ExecuteError::InvalidState(
                format!(
                    "unit {} rides slot {slot} of transport {transport}",
                    cargo.id
                )
                .into(),
            ));
        }
        occupied |= 1 << slot;
    }
    Ok(occupied)
}

impl<'a> DestinationAction<'a> for Load {
    type Proof = LoadProof;

    fn validate<M>(
        &self,
        destination: &PreparedDestination<'a, M>,
    ) -> Result<Self::Proof, ExecuteError>
    where
        M: std::borrow::Borrow<crate::query::TurnMaps<'a>>,
    {
        let transport_id = self.0;
        let movement = destination.movement();
        let state = movement.state();
        let unit_id = movement.unit();
        let plan = movement.plan();
        let mover = &state.units[plan.unit_index()];
        // The mover is the active unit, so its own seat is the active seat.
        let seat = mover.owner;
        let transport_index = state.units.index_of(transport_id);
        let transport = transport_index.and_then(|index| state.units.at(index));
        let transport_capability =
            transport.and_then(|transport| ruleset::profile(transport.kind).transport);
        let capacity = transport_capability.map(|capability| capability.capacity);
        let cargo_kind_allowed =
            transport_capability.is_some_and(|capability| capability.cargo.contains(mover.kind));
        // Everything above is a lookup. The cargo scan below walks every unit,
        // so it runs only for a transport that has already passed the cheap
        // rejections that most candidate destinations fail.
        let target_valid = transport.is_some_and(|transport| {
            transport.id != unit_id
                && transport.owner == seat
                && board_position(transport).is_some()
                && cargo_kind_allowed
                && capacity.is_some()
        });
        if !target_valid {
            return Err(violation(Violation::InvalidTarget {
                target: Some(transport_id.into()),
            }));
        }
        let capacity = capacity.expect("target validity established capacity");
        let occupied = occupied_slots(state, transport_id, capacity)?;
        if usize::try_from(occupied.count_ones()).unwrap_or(usize::MAX) >= capacity {
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
        let slot = occupied.trailing_ones() as usize;
        if slot >= capacity {
            return Err(ExecuteError::InvalidState(
                format!("transport {transport_id} is full").into(),
            ));
        }

        Ok(LoadProof {
            transport: transport_id,
            slot,
        })
    }

    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a> {
        PreparedCommandKind::Load(bound)
    }
}

pub(super) fn execute_prepared_load(prepared: MovementAction<'_, LoadProof>) -> Execution {
    let MovementAction {
        movement,
        trap,
        action: LoadProof {
            transport: transport_id,
            slot,
        },
    } = prepared;
    let state = movement.state();
    let unit_id = movement.unit();
    let plan = movement.plan();
    let mut outcome = execute_planned_movement(state, unit_id, plan, trap);
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

pub(super) fn prepare_unload_transport<'a>(
    turn: &ActiveTurn<'a>,
    transport_id: UnitId,
) -> Result<PreparedUnloadTransport<'a>, ExecuteError> {
    let state = turn.state();
    let seat = turn.seat();
    let transport_index = state.units.index_of(transport_id);
    let transport = transport_index.and_then(|index| state.units.at(index));
    let transport_position = transport.and_then(board_position);
    if !transport.is_some_and(|transport| {
        transport.owner == seat
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

impl<'a> DestinationAction<'a> for Join {
    type Proof = JoinProof;

    fn validate<M>(
        &self,
        destination: &PreparedDestination<'a, M>,
    ) -> Result<Self::Proof, ExecuteError>
    where
        M: std::borrow::Borrow<crate::query::TurnMaps<'a>>,
    {
        let target_id = self.0;
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
        let target_owner_team = state
            .players
            .get(target.owner.get())
            .map(|owner| &owner.team);
        let Some(target_position) = board_position(target) else {
            return Err(violation(Violation::InvalidTarget {
                target: Some(target_id.into()),
            }));
        };
        if target.id == unit.id || target.kind != unit.kind || target_owner_team != Some(actor_team)
        {
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

        Ok(JoinProof {
            target: PreparedJoinTarget {
                id: target_id,
                index: target_index,
            },
        })
    }

    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a> {
        PreparedCommandKind::Join(bound)
    }
}

pub(super) fn execute_prepared_join(
    prepared: MovementAction<'_, JoinProof>,
) -> Result<Execution, ExecuteError> {
    let MovementAction {
        movement,
        trap,
        action:
            JoinProof {
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
    let unit_index = plan.unit_index();
    let entry_costs = plan.entry_costs();
    let target = &state.units[target_index];

    // Only an undisclosed intermediate enemy can trap a well-formed join, and
    // the shared trap check reports exactly those: the allied destination
    // target is always disclosed, so it never appears as one.
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
