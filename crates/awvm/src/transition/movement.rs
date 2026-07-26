//! Movement: path validation, the move itself, and what departing a tile costs.
//!
//! Normative source:
//! * `spec/semantics/movement.md`
//! * `spec/semantics/capture-reset.md`
//! * `spec/semantics/concealment.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::commander::{self};
use crate::event::Event;
use crate::ruleset::{self};
use crate::semantic::{
    AwbwVisibility, Concealment, Location, PlayerId, Pos, State, UnitAction, UnitId,
};
use crate::violation::{Action, Violation};

/// A movement that has been validated, and the numbers that validating it
/// produced.
///
/// The fields are private to this module and [`plan`] is the only constructor,
/// so holding one is proof the path was checked. `execute_move_capture` and
/// `execute_move_join` each carried a verbatim copy of that check; nothing
/// stopped a tenth reducer from carrying a subtly different one.
pub(crate) struct MovedUnit {
    unit_index: usize,
    origin: Pos,
    path: Vec<Pos>,
    entry_costs: Vec<u64>,
    actor_team: crate::semantic::TeamId,
}

impl MovedUnit {
    /// The mover's position in [`State::units`].
    pub(crate) const fn unit_index(&self) -> usize {
        self.unit_index
    }

    /// Where the move started.
    pub(crate) const fn origin(&self) -> Pos {
        self.origin
    }

    /// The requested route, origin first.
    pub(crate) fn path(&self) -> &[Pos] {
        &self.path
    }

    /// Movement points to enter each step of [`MovedUnit::path`]; the first is
    /// always zero.
    pub(crate) fn entry_costs(&self) -> &[u64] {
        &self.entry_costs
    }

    /// The team the mover belongs to, which decides what it can see.
    pub(crate) const fn actor_team(&self) -> &crate::semantic::TeamId {
        &self.actor_team
    }

    /// The destination the mover asked for, which is not where it ends up if a
    /// hidden unit traps it.
    pub(crate) fn destination(&self) -> Pos {
        *self.path.last().expect("a validated path has an origin")
    }
}

pub(crate) struct MovementOutcome {
    pub(crate) state: State,
    pub(crate) events: Vec<Event>,
    pub(crate) trapped: bool,
}

/// Validate a movement, producing the one proof that it was validated.
///
/// The shared prologue is [`ActiveTurn::open`]'s job and has already run; what
/// is left is everything specific to moving a unit along a path.
pub(crate) fn plan(
    turn: &ActiveTurn<'_>,
    unit_id: UnitId,
    path: Vec<Pos>,
) -> Result<MovedUnit, ExecuteError> {
    let state = turn.state();
    let player = turn.player();
    let unit_index = state
        .units
        .index_of(unit_id)
        .ok_or_else(|| violation(Violation::UnitNotFound { unit: unit_id }))?;
    let unit = &state.units[unit_index];
    if unit.owner != player {
        return Err(violation(Violation::UnitNotOwned {
            unit: unit_id,
            player: player.clone(),
        }));
    }
    let Location::Board { position: origin } = unit.location else {
        return Err(violation(Violation::UnitNotOnBoard { unit: unit_id }));
    };
    if unit.action != UnitAction::Ready {
        return Err(violation(Violation::UnitAlreadyActed { unit: unit_id }));
    }
    let actual_origin = path.first().copied().unwrap_or(origin);
    if path.first() != Some(&origin) {
        return Err(violation(Violation::PathOriginMismatch {
            expected: origin,
            actual: actual_origin,
        }));
    }
    for (index, pair) in path.windows(2).enumerate() {
        if pair[0].x.abs_diff(pair[1].x) + pair[0].y.abs_diff(pair[1].y) != 1 {
            return Err(violation(Violation::PathNonAdjacent {
                index: index + 1,
                from: pair[0],
                to: pair[1],
            }));
        }
    }
    for (index, position) in path.iter().copied().enumerate() {
        if let Some(first_index) = path[..index].iter().position(|seen| *seen == position) {
            return Err(violation(Violation::PathRepeatedPosition {
                index,
                position,
                first_index,
            }));
        }
    }
    for (index, position) in path.iter().copied().enumerate() {
        if position.x >= state.board.width() || position.y >= state.board.height() {
            return Err(violation(Violation::PathOutOfBounds { index, position }));
        }
    }
    let profile = ruleset::profile(unit.kind);
    let movement =
        commander::effective_move(state, unit, profile.movement, profile.domain.as_str());
    let weather = commander::effective_weather(state, unit);
    let mut entry_costs = vec![0];
    for (index, position) in path.iter().copied().enumerate().skip(1) {
        let terrain = state.board.tile(position).terrain;
        let cost = commander::effective_movement_cost(
            state,
            unit,
            ruleset::movement_cost(terrain, weather, profile.movement_class),
        )
        .ok_or_else(|| {
            violation(Violation::TerrainImpassable {
                index: Some(index),
                position,
            })
        })?;
        entry_costs.push(cost);
    }
    let actor_team = state
        .find_player(player)
        .map(|candidate| candidate.team.clone())
        .ok_or_else(|| {
            ExecuteError::InvalidState(format!("unknown active player {player}").into())
        })?;
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
            return Err(violation(Violation::PathOccupied { index, position }));
        }
    }
    let intended_cost: u64 = entry_costs.iter().sum();
    if intended_cost > movement {
        return Err(violation(Violation::InsufficientMovement {
            required: intended_cost,
            available: movement,
        }));
    }
    if intended_cost > unit.fuel {
        return Err(violation(Violation::InsufficientFuel {
            required: intended_cost,
            available: unit.fuel,
        }));
    }
    Ok(MovedUnit {
        unit_index,
        origin,
        path,
        entry_costs,
        actor_team,
    })
}

pub(crate) fn execute_planned_movement(
    state: &State,
    unit_id: UnitId,
    plan: &MovedUnit,
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
                        && !occupancy_is_disclosed(&visibility, state, plan.actor_team(), other)
                })
                .map(|blocker| (index, position, blocker.id))
        });
    let actual_length = trap
        .as_ref()
        .map_or(plan.path().len(), |(index, _, _)| *index);
    let actual_path = plan.path()[..actual_length].to_vec();
    let destination = *actual_path.last().expect("actual path includes origin");
    let fuel_spent: u64 = plan.entry_costs()[..actual_length].iter().sum();
    let mut next = state.clone();
    let mut events = Vec::new();
    reset_capture_on_departure(&mut next, unit_id, plan.origin(), &actual_path, &mut events);
    next.units[plan.unit_index()].fuel -= fuel_spent;
    next.units[plan.unit_index()].action = UnitAction::Spent;
    next.units[plan.unit_index()].location = Location::Board {
        position: destination,
    };
    events.push(Event::UnitMoved {
        unit: unit_id,
        from: plan.origin(),
        to: destination,
        path: actual_path,
        fuel_spent,
    });
    let trapped = if let Some((_, position, blocker)) = trap {
        events.push(Event::MovementTrapped {
            unit: unit_id,
            blocker,
            position,
        });
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

pub(crate) fn reset_capture_on_departure(
    state: &mut State,
    unit_id: UnitId,
    origin: Pos,
    actual_path: &[Pos],
    events: &mut Vec<Event>,
) {
    if actual_path.len() < 2 || !state.units.contains(unit_id) {
        return;
    }
    let tile = &mut state.board.tile_mut(origin);
    if let Some(before) = tile.capture_points.filter(|points| *points < 20) {
        tile.capture_points = Some(20);
        events.push(Event::CaptureChanged {
            position: origin,
            from: before,
            to: 20,
        });
    }
}

pub(crate) fn execute_move_concealment(
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    path: Vec<Pos>,
    hide: bool,
) -> Result<Execution, ExecuteError> {
    let turn = ActiveTurn::open(state, player)?;
    let plan = turn.plan_move(unit_id, path)?;
    let original = &state.units[plan.unit_index()];
    let supported = ruleset::profile(original.kind).concealment.is_some();
    let target = if hide {
        Concealment::Hidden
    } else {
        Concealment::Exposed
    };
    if !supported || original.concealment == target {
        return Err(violation(Violation::ActionNotSupported {
            action: if hide {
                Action::MoveHide
            } else {
                Action::MoveReveal
            },
        }));
    }

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
    let unit = &mut outcome.state.units[plan.unit_index()];
    let from = unit.concealment;
    unit.concealment = target;
    outcome.events.push(Event::ConcealmentChanged {
        unit: unit_id,
        from,
        to: target,
    });
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

pub(crate) fn execute_move_wait(
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    path: Vec<Pos>,
) -> Result<Execution, ExecuteError> {
    let turn = ActiveTurn::open(state, player)?;
    let plan = turn.plan_move(unit_id, path)?;
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
    let outcome = execute_planned_movement(state, unit_id, &plan);
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}
