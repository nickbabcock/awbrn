//! Movement: path validation, the move itself, and what departing a tile costs.
//!
//! Normative source:
//! * `spec/semantics/movement.md`
//! * `spec/semantics/capture-reset.md`
//! * `spec/semantics/concealment.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::commander::{self};
use crate::ruleset::{self, TerrainTrait};
use crate::semantic::{AwbwVisibility, UnitAction, Visibility};
use crate::violation::Action;

/// Move there and end the unit's turn.
pub(super) struct Wait;

/// Move there and change concealment to the named state.
pub(super) struct Conceal(pub(super) Concealment);

#[derive(Debug)]
pub(super) struct WaitProof(AvailableDestination);

#[derive(Debug)]
pub(super) struct ConcealProof {
    target: Concealment,
    destination: AvailableDestination,
}

/// Proof that no disclosed unit occupies a movement's destination.
#[derive(Clone, Copy, Debug)]
pub(super) struct AvailableDestination;

pub(super) fn available_destination(
    movement: &PreparedMovement<'_>,
    view: &AwbwView<'_>,
) -> Result<AvailableDestination, Violation> {
    let destination = movement.plan().destination();
    if view
        .blocking_occupant(destination, movement.unit())
        .is_some()
    {
        return Err(Violation::DestinationOccupied {
            position: destination,
        });
    }
    Ok(AvailableDestination)
}

/// A movement that has been validated, and the numbers that validating it
/// produced.
///
/// The fields are private to this module. [`plan`] checks an arbitrary path,
/// and `from_field` accepts a path from a state-bound field. Holding this value
/// is proof that one of those checks produced the path. `execute_move_capture`
/// and `execute_move_join` each carried a copy of the arbitrary-path check;
/// nothing stopped another reducer from carrying a different one.
#[derive(Clone, Debug)]
pub(crate) struct MovedUnit<'a> {
    unit_index: usize,
    origin: Pos,
    route: Route,
    /// Borrowed from the state this movement was validated against. Cloning it
    /// here allocated a team name for every candidate destination an
    /// enumeration considered.
    actor_team: &'a crate::semantic::TeamId,
}

#[derive(Clone, Debug)]
enum Route {
    Materialized {
        path: Vec<Pos>,
        entry_costs: Vec<u64>,
    },
    Summary {
        destination: Pos,
        length: u16,
    },
}

impl<'a> MovedUnit<'a> {
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
        match &self.route {
            Route::Materialized { path, .. } => path,
            Route::Summary { .. } => {
                panic!("a summarized route cannot be executed")
            }
        }
    }

    /// Movement points to enter each step of [`MovedUnit::path`]; the first is
    /// always zero.
    pub(crate) fn entry_costs(&self) -> &[u64] {
        match &self.route {
            Route::Materialized { entry_costs, .. } => entry_costs,
            Route::Summary { .. } => {
                panic!("a summarized route cannot be executed")
            }
        }
    }

    /// The team the mover belongs to, which decides what it can see.
    pub(crate) const fn actor_team(&self) -> &'a crate::semantic::TeamId {
        self.actor_team
    }

    /// The destination the mover asked for, which is not where it ends up if a
    /// hidden unit traps it.
    pub(crate) fn destination(&self) -> Pos {
        match &self.route {
            Route::Materialized { path, .. } => {
                *path.last().expect("a validated path has an origin")
            }
            Route::Summary { destination, .. } => *destination,
        }
    }

    pub(crate) fn path_len(&self) -> usize {
        match &self.route {
            Route::Materialized { path, .. } => path.len(),
            Route::Summary { length, .. } => usize::from(*length),
        }
    }

    pub(crate) const fn is_summary(&self) -> bool {
        matches!(self.route, Route::Summary { .. })
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
pub(crate) fn plan<'a>(
    active: &PreparedActiveUnit<'a>,
    path: Vec<Pos>,
) -> Result<MovedUnit<'a>, ExecuteError> {
    let state = active.state();
    let unit_id = active.unit();
    let unit_index = active.unit_index();
    let unit = &state.units[unit_index];
    let origin = active.origin();
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
    let movement = commander::effective_move(state, unit, profile.movement, profile.domain);
    let weather = commander::effective_weather(state, unit);
    let mut entry_costs = vec![0];
    if path.len() == 1
        && ruleset::terrain_has(state.board.tile(origin).terrain, TerrainTrait::Teleporter)
    {
        return Err(violation(Violation::TerrainImpassable {
            index: Some(0),
            position: origin,
        }));
    }
    for (index, position) in path.iter().copied().enumerate().skip(1) {
        let terrain = state.board.tile(position).terrain;
        let teleporter = ruleset::terrain_has(terrain, TerrainTrait::Teleporter);
        if index + 1 == path.len() && teleporter {
            return Err(violation(Violation::TerrainImpassable {
                index: Some(index),
                position,
            }));
        }
        let base_cost = ruleset::movement_cost(terrain, weather, profile.movement_class);
        // Teleporter zero-cost traversal is terrain behavior, not a finite
        // ordinary cost for commander cost-set operators to replace.
        let cost = if teleporter {
            base_cost
        } else {
            commander::effective_movement_cost(state, unit, base_cost)
        }
        .ok_or_else(|| {
            violation(Violation::TerrainImpassable {
                index: Some(index),
                position,
            })
        })?;
        entry_costs.push(cost);
    }
    let actor_team = state
        .players
        .get(unit.owner.get())
        .map(|candidate| &candidate.team)
        .ok_or_else(|| {
            ExecuteError::InvalidState(
                format!("unknown active player at seat {}", unit.owner.get()).into(),
            )
        })?;
    // A scan, not the view's occupancy index: an arbitrary path names two or
    // three tiles, and indexing the whole board to answer that loses.
    let view = AwbwVisibility.view(state, actor_team);
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
                && state
                    .players
                    .get(other.owner.get())
                    .is_some_and(|owner| owner.team != *actor_team)
                && occupancy_is_disclosed(&view, other)
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
        route: Route::Materialized { path, entry_costs },
        actor_team,
    })
}

/// Build a movement from a field that is bound to the same active-unit proof.
///
/// `PreparedMoveField` owns the field and the proof. It supplies a path and
/// costs that the field search produced for that proof.
pub(super) fn from_field<'a>(
    active: &PreparedActiveUnit<'a>,
    path: Vec<Pos>,
    entry_costs: Vec<u64>,
) -> MovedUnit<'a> {
    let state = active.state();
    let unit = &state.units[active.unit_index()];
    let actor_team = &state
        .players
        .get(unit.owner.get())
        .expect("an active unit has a player")
        .team;
    MovedUnit {
        unit_index: active.unit_index(),
        origin: active.origin(),
        route: Route::Materialized { path, entry_costs },
        actor_team,
    }
}

pub(super) fn summarized<'a>(
    active: &PreparedActiveUnit<'a>,
    destination: Pos,
    length: u16,
) -> MovedUnit<'a> {
    let state = active.state();
    let unit = &state.units[active.unit_index()];
    let actor_team = &state
        .players
        .get(unit.owner.get())
        .expect("an active unit has a player")
        .team;
    MovedUnit {
        unit_index: active.unit_index(),
        origin: active.origin(),
        route: Route::Summary {
            destination,
            length,
        },
        actor_team,
    }
}

pub(crate) fn execute_planned_movement(
    state: &State,
    unit_id: UnitId,
    plan: &MovedUnit<'_>,
    trap: Option<(usize, Pos, UnitId)>,
) -> MovementOutcome {
    let mut actual_length = trap
        .as_ref()
        .map_or(plan.path().len(), |(index, _, _)| *index);
    // A trap immediately beyond a zero-cost corridor must not strand the mover
    // on transit-only terrain. Roll the actual prefix back across that corridor.
    while actual_length > 1 {
        let candidate = plan.path()[actual_length - 1];
        if !ruleset::terrain_has(
            state.board.tile(candidate).terrain,
            TerrainTrait::Teleporter,
        ) {
            break;
        }
        actual_length -= 1;
    }
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

pub(crate) fn planned_movement_trap_with_view(
    plan: &MovedUnit<'_>,
    unit_id: UnitId,
    view: &AwbwView<'_>,
) -> Option<(usize, Pos, UnitId)> {
    plan.path()
        .iter()
        .copied()
        .enumerate()
        .skip(1)
        .find_map(|(index, position)| {
            view.hidden_occupant(position, unit_id)
                .map(|blocker| (index, position, blocker))
        })
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
    reset_capture_on_removal(state, origin, events);
}

pub(crate) fn reset_capture_on_removal(state: &mut State, position: Pos, events: &mut Vec<Event>) {
    let tile = &mut state.board.tile_mut(position);
    if let Some(before) = tile
        .capture_points
        .filter(|points| *points < crate::semantic::CAPTURE_REQUIRED_POINTS)
    {
        tile.capture_points = Some(crate::semantic::CAPTURE_REQUIRED_POINTS);
        events.push(Event::CaptureChanged {
            position,
            from: before,
            to: 20,
        });
    }
}

impl<'a> DestinationAction<'a> for Conceal {
    type Proof = ConcealProof;

    fn validate<M>(&self, at: &PreparedDestination<'a, M>) -> Result<Self::Proof, ExecuteError>
    where
        M: std::borrow::Borrow<crate::query::TurnMaps<'a>>,
    {
        let target = self.0;
        let hide = target == Concealment::Hidden;
        let movement = at.movement();
        let state = movement.state();
        let plan = movement.plan();
        let original = &state.units[plan.unit_index()];
        let supported = ruleset::profile(original.kind).concealment.is_some();
        if !supported || original.concealment == target {
            return Err(violation(Violation::ActionNotSupported {
                action: if hide {
                    Action::MoveHide
                } else {
                    Action::MoveReveal
                },
            }));
        }
        let available = at.available_destination()?;
        Ok(ConcealProof {
            target,
            destination: available,
        })
    }

    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a> {
        PreparedCommandKind::Concealment(bound)
    }
}

pub(super) fn execute_prepared_concealment(
    prepared: MovementAction<'_, ConcealProof>,
) -> Execution {
    let MovementAction {
        movement,
        trap,
        action: ConcealProof {
            target,
            destination: _destination,
        },
    } = prepared;
    let unit_id = movement.unit();
    let plan = movement.plan();
    let mut outcome = execute_planned_movement(movement.state(), unit_id, plan, trap);
    if outcome.trapped {
        return Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        };
    }
    let unit = &mut outcome.state.units[plan.unit_index()];
    let from = unit.concealment;
    unit.concealment = target;
    outcome.events.push(Event::ConcealmentChanged {
        unit: unit_id,
        from,
        to: target,
    });
    Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    }
}

impl<'a> DestinationAction<'a> for Wait {
    type Proof = WaitProof;

    fn validate<M>(&self, at: &PreparedDestination<'a, M>) -> Result<Self::Proof, ExecuteError>
    where
        M: std::borrow::Borrow<crate::query::TurnMaps<'a>>,
    {
        Ok(WaitProof(at.available_destination()?))
    }

    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a> {
        PreparedCommandKind::Wait(bound)
    }
}

pub(super) fn execute_prepared_wait(prepared: MovementAction<'_, WaitProof>) -> Execution {
    let MovementAction {
        movement,
        trap,
        action: WaitProof(_destination),
    } = prepared;
    let outcome =
        execute_planned_movement(movement.state(), movement.unit(), movement.plan(), trap);
    Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    }
}
