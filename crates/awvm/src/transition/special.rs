//! Single-use unit actions that consume the acting unit or a site.
//!
//! Normative source:
//! * `spec/semantics/launch.md`
//! * `spec/semantics/explode.md`
//! * `spec/semantics/delete.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::commander::AreaStrikePolicy;
use crate::event::Event;
use crate::ruleset::UnitKind;
use crate::semantic::{AwbwVisibility, PlayerId, Pos, ReasonId, Silo, State, UnitId};
use crate::violation::{Action, Violation};

pub(crate) fn execute_move_launch(
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    path: Vec<Pos>,
    target: Pos,
) -> Result<Execution, ExecuteError> {
    let turn = ActiveTurn::open(state, player)?;
    let plan = turn.plan_move(unit_id, path)?;
    if target.x >= state.board.width() || target.y >= state.board.height() {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target.into()),
        }));
    }

    let unit = &state.units[plan.unit_index()];
    if !matches!(unit.kind.as_str(), "infantry" | "mech") {
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
    let visibility = AwbwVisibility;
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(silo_position)
            && occupancy_is_disclosed(&visibility, state, plan.actor_team(), other)
    }) {
        return Err(violation(Violation::DestinationOccupied {
            position: silo_position,
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

    // AWBW's silo missile is three visual bars (30 exact HP), nonlethal, and
    // affects every board unit, including allies. Derive the list after the
    // move and sort it so event order is independent of state-vector order.
    outcome.events.push(Event::AreaStrikeResolved {
        strike: 0,
        policy: AreaStrikePolicy::UnitHp,
        center: target,
        radius: 3,
        damage: 30,
    });
    let mut affected: Vec<UnitId> = outcome
        .state
        .units
        .iter()
        .filter(|unit| {
            board_position(unit).is_some_and(|position| {
                position.x.abs_diff(target.x) + position.y.abs_diff(target.y) <= 3
            })
        })
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
        let to_hp = from_hp.saturating_sub(30).max(1);
        if to_hp != from_hp {
            unit.hp = to_hp;
            outcome.events.push(Event::UnitDamaged {
                unit: id,
                from_hp,
                to_hp,
                reason: ReasonId::from("missile-silo"),
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

pub(crate) fn execute_move_explode(
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
    path: Vec<Pos>,
) -> Result<Execution, ExecuteError> {
    let turn = ActiveTurn::open(state, player)?;
    let plan = turn.plan_move(unit_id, path)?;
    let unit = &state.units[plan.unit_index()];
    if unit.kind != UnitKind::BlackBomb {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::MoveExplode,
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

    outcome.events.push(Event::AreaStrikeResolved {
        strike: 0,
        policy: AreaStrikePolicy::UnitHp,
        center: destination,
        radius: 3,
        damage: 50,
    });
    let mut affected: Vec<UnitId> = outcome
        .state
        .units
        .iter()
        .filter(|unit| unit.id != unit_id)
        .filter(|unit| {
            board_position(unit).is_some_and(|position| {
                position.x.abs_diff(destination.x) + position.y.abs_diff(destination.y) <= 3
            })
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
        let to_hp = from_hp.saturating_sub(50).max(1);
        if to_hp == from_hp {
            continue;
        }
        unit.hp = to_hp;
        outcome.events.push(Event::UnitDamaged {
            unit: id,
            from_hp,
            to_hp,
            reason: ReasonId::from("explode"),
        });
    }

    let exploding_owner = outcome.state.units[plan.unit_index()].owner.clone();
    outcome.state.units.remove(plan.unit_index());
    outcome.events.push(Event::UnitRemoved {
        unit: unit_id,
        reason: ReasonId::from("explode"),
    });
    if !outcome
        .state
        .units
        .iter()
        .any(|unit| unit.owner == exploding_owner)
    {
        eliminate_player(
            &mut outcome.state,
            &exploding_owner,
            &ReasonId::from("rout"),
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

pub(crate) fn execute_delete_unit(
    state: &State,
    player: &PlayerId,
    unit_id: UnitId,
) -> Result<Execution, ExecuteError> {
    let _turn = ActiveTurn::open(state, player)?;
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
    let position = board_position(unit)
        .ok_or_else(|| violation(Violation::UnitNotOnBoard { unit: unit_id }))?;

    let mut next = state.clone();
    let mut events = Vec::new();
    if let Some(before) = next
        .board
        .tile(position)
        .capture_points
        .filter(|points| *points < 20)
    {
        next.board.tile_mut(position).capture_points = Some(20);
        events.push(Event::CaptureChanged {
            position,
            from: before,
            to: 20,
        });
    }
    next.units.remove(unit_index);
    events.push(Event::UnitRemoved {
        unit: unit_id,
        reason: ReasonId::from("delete"),
    });
    if !next.units.iter().any(|unit| unit.owner == player) {
        eliminate_player(
            &mut next,
            player,
            &ReasonId::from("rout"),
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
