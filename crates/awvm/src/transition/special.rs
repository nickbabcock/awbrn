//! Single-use unit actions that consume the acting unit or a site.
//!
//! Normative source:
//! * `spec/semantics/launch.md`
//! * `spec/semantics/explode.md`
//! * `spec/semantics/delete.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::commander::AreaStrikePolicy;
use crate::ruleset::{MISSILE_SILO_STRIKE, UNIT_EXPLOSION};
use crate::semantic::{CAPTURE_REQUIRED_POINTS, Silo, VictoryReason};
use crate::violation::Action;

#[derive(Debug)]
pub(super) struct Launch(pub(super) Pos);

#[derive(Debug)]
pub(super) struct LaunchProof {
    target: Pos,
    destination: AvailableDestination,
}

#[derive(Debug)]
pub(super) struct Explode;

#[derive(Debug)]
pub(super) struct ExplodeProof(AvailableDestination);

#[derive(Debug)]
pub(super) struct PreparedDelete<'a> {
    unit: PreparedActiveUnit<'a>,
}

/// Everything a launch needs that the target does not decide.
///
/// A missile reaches any tile of the board, so of the four things a launch
/// validates only the target's bounds are about the target at all. Who is
/// firing, whether the silo underfoot is loaded, and whether the mover may
/// stop there are the same answer for every tile. A caller asking tile after
/// tile — [`crate::query`] enumerating launch targets is the one that does —
/// asks this once and then asks only about bounds.
///
/// This is the whole of [`Launch::validate`] except the bounds check, rather
/// than a second statement of it, so a rule added here cannot be missed by a
/// caller that skipped the loop on its answer.
pub(crate) fn launch_preflight<'a, M>(
    destination: &PreparedDestination<'a, M>,
) -> Result<AvailableDestination, ExecuteError>
where
    M: std::borrow::Borrow<crate::query::TurnMaps<'a>>,
{
    let movement = destination.movement();
    let state = movement.state();
    let plan = movement.plan();

    let unit = &state.units[plan.unit_index()];
    if !matches!(unit.kind, UnitKind::Infantry | UnitKind::Mech) {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::MoveLaunch,
        }));
    }
    let silo_position = plan.destination();
    let silo = &state.board.tile(silo_position).silo;
    if silo != &Some(Silo::Ready) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(silo_position.into()),
        }));
    }
    destination.available_destination()
}

impl<'a> DestinationAction<'a> for Launch {
    type Proof = LaunchProof;

    fn validate<M>(&self, at: &PreparedDestination<'a, M>) -> Result<Self::Proof, ExecuteError>
    where
        M: std::borrow::Borrow<crate::query::TurnMaps<'a>>,
    {
        let target = self.0;
        let state = at.movement().state();
        if target.x >= state.board.width() || target.y >= state.board.height() {
            return Err(violation(Violation::InvalidTarget {
                target: Some(target.into()),
            }));
        }
        Ok(LaunchProof {
            target,
            destination: launch_preflight(at)?,
        })
    }

    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a> {
        PreparedCommandKind::Launch(bound)
    }
}

pub(super) fn execute_prepared_launch(
    prepared: MovementAction<'_, LaunchProof>,
) -> Result<Execution, ExecuteError> {
    let MovementAction {
        movement,
        trap,
        action: LaunchProof {
            target,
            destination: _destination,
        },
    } = prepared;
    let state = movement.state();
    let unit_id = movement.unit();
    let plan = movement.plan();
    let silo_position = plan.destination();
    let mut outcome = execute_planned_movement(state, unit_id, plan, trap);
    if outcome.trapped {
        return Ok(Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        });
    }

    // AWBW's silo missile is nonlethal and affects every board unit in range,
    // including allies. Derive the list after the move and sort it so event
    // order is independent of state-vector order.
    let strike = MISSILE_SILO_STRIKE;
    outcome.events.push(Event::AreaStrikeResolved {
        strike: 0,
        policy: AreaStrikePolicy::UnitHp,
        center: target,
        radius: strike.radius,
        damage: strike.damage,
    });
    let mut affected: Vec<UnitId> = outcome
        .state
        .units
        .iter()
        .filter(|unit| board_position(unit).is_some_and(|position| strike.covers(target, position)))
        .map(|unit| unit.id)
        .collect();
    affected.sort();
    for id in affected {
        let unit = outcome
            .state
            .units
            .get_mut(id)
            .expect("launch target remains present");
        let from_hp = unit.hp;
        let to_hp = from_hp.saturating_sub(strike.damage).max(1);
        if to_hp != from_hp {
            unit.hp = to_hp;
            outcome.events.push(Event::UnitDamaged {
                unit: id,
                from_hp,
                to_hp,
                reason: KnownReason::MissileSilo.into(),
            });
        }
    }
    outcome.state.board.tile_mut(silo_position).silo = Some(Silo::Spent);
    outcome.events.push(Event::SiloChanged {
        position: silo_position,
        from: Silo::Ready,
        to: Silo::Spent,
    });
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

impl<'a> DestinationAction<'a> for Explode {
    type Proof = ExplodeProof;

    fn validate<M>(&self, at: &PreparedDestination<'a, M>) -> Result<Self::Proof, ExecuteError>
    where
        M: std::borrow::Borrow<crate::query::TurnMaps<'a>>,
    {
        let movement = at.movement();
        let state = movement.state();
        let plan = movement.plan();
        let unit = &state.units[plan.unit_index()];
        if unit.kind != UnitKind::BlackBomb {
            return Err(violation(Violation::ActionNotSupported {
                action: Action::MoveExplode,
            }));
        }
        Ok(ExplodeProof(at.available_destination()?))
    }

    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a> {
        PreparedCommandKind::Explode(bound)
    }
}

pub(super) fn execute_prepared_explode(
    prepared: MovementAction<'_, ExplodeProof>,
) -> Result<Execution, ExecuteError> {
    let MovementAction {
        movement,
        trap,
        action: ExplodeProof(_destination),
    } = prepared;
    let state = movement.state();
    let unit_id = movement.unit();
    let plan = movement.plan();
    let destination = plan.destination();
    let mut outcome = execute_planned_movement(state, unit_id, plan, trap);
    if outcome.trapped {
        return Ok(Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        });
    }

    let strike = UNIT_EXPLOSION;
    outcome.events.push(Event::AreaStrikeResolved {
        strike: 0,
        policy: AreaStrikePolicy::UnitHp,
        center: destination,
        radius: strike.radius,
        damage: strike.damage,
    });
    let mut affected: Vec<UnitId> = outcome
        .state
        .units
        .iter()
        .filter(|unit| unit.id != unit_id)
        .filter(|unit| {
            board_position(unit).is_some_and(|position| strike.covers(destination, position))
        })
        .map(|unit| unit.id)
        .collect();
    affected.sort();
    for id in affected {
        let unit = outcome
            .state
            .units
            .get_mut(id)
            .expect("explosion target remains present");
        let from_hp = unit.hp;
        let to_hp = from_hp.saturating_sub(strike.damage).max(1);
        if to_hp == from_hp {
            continue;
        }
        unit.hp = to_hp;
        outcome.events.push(Event::UnitDamaged {
            unit: id,
            from_hp,
            to_hp,
            reason: KnownReason::Explode.into(),
        });
    }

    let exploding_seat = outcome.state.units[plan.unit_index()].owner;
    let exploding_owner = outcome.state.player_id(exploding_seat).clone();
    outcome.state.units.remove(plan.unit_index());
    outcome.events.push(Event::UnitRemoved {
        unit: unit_id,
        reason: KnownReason::Explode.into(),
    });
    if !outcome
        .state
        .units
        .iter()
        .any(|unit| unit.owner == exploding_seat)
    {
        eliminate_player(
            &mut outcome.state,
            &exploding_owner,
            VictoryReason::Rout,
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

pub(super) fn prepare_delete(
    unit: PreparedActiveUnit<'_>,
) -> Result<PreparedDelete<'_>, ExecuteError> {
    Ok(PreparedDelete { unit })
}

pub(super) fn execute_prepared_delete(
    prepared: PreparedDelete<'_>,
) -> Result<Execution, ExecuteError> {
    let PreparedDelete { unit } = prepared;
    let state = unit.state();
    let player = &state.turn.active_player;
    let player_seat = state.player_index(player);
    let unit_id = unit.unit();
    let position = unit.origin();

    let mut next = state.clone();
    let mut events = Vec::new();
    if let Some(before) = next
        .board
        .tile(position)
        .capture_points
        .filter(|points| *points < CAPTURE_REQUIRED_POINTS)
    {
        next.board.tile_mut(position).capture_points = Some(CAPTURE_REQUIRED_POINTS);
        events.push(Event::CaptureChanged {
            position,
            from: before,
            to: 20,
        });
    }
    remove_unit_and_cargo(&mut next, unit_id, KnownReason::Delete, &mut events);
    if !next
        .units
        .iter()
        .any(|unit| Some(unit.owner) == player_seat)
    {
        eliminate_player(
            &mut next,
            player,
            VictoryReason::Rout,
            None,
            None,
            &mut events,
        )?;
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: 0,
    })
}
