//! Properties: capturing them and building from them.
//!
//! Normative source:
//! * `spec/semantics/capture.md`
//! * `spec/semantics/production.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::commander::{self};
use crate::event::Event;
use crate::ruleset::{self, TerrainTrait, UnitKind};
use crate::semantic::{
    AwbwVisibility, Concealment, KnownReason, Location, Outcome, PlayerId, Pos, State, TerrainId,
    TileOwner, Unit, UnitAction, UnitId, VictoryReason,
};
use crate::violation::{Action, Violation};

pub(crate) fn execute_produce_unit(
    turn: &ActiveTurn<'_>,
    position: Pos,
    kind: UnitKind,
) -> Result<Execution, ExecuteError> {
    let state = turn.state();
    let player = turn.player();
    let player_index = state.player_index(player).ok_or_else(|| {
        ExecuteError::InvalidState(format!("unknown active player {player}").into())
    })?;
    let profile = ruleset::profile(kind);

    // Site validation precedes requested-kind validation: whether the player
    // owns a facility here does not depend on what they asked it to build.
    let tile = state.board.get(position);
    let site_valid = tile.is_some_and(|tile| {
        let terrain = ruleset::terrain(tile.terrain);
        let commander_facility =
            commander::commander_production_site(state, player, tile.terrain, profile.domain);
        tile.owner.is_owned_by(player)
            && (terrain.has(profile.domain.produces()) || commander_facility)
    });
    if !site_valid {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    }
    if state.settings.unit_bans.contains(&kind) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(kind.into()),
        }));
    }
    if state.settings.lab_units.contains(&kind) && !player_owns_lab(state, player) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(kind.into()),
        }));
    }
    if state
        .units
        .iter()
        .any(|unit| board_position(unit) == Some(position))
    {
        return Err(violation(Violation::DestinationOccupied { position }));
    }
    let current = owned_unit_count(state, player)?;
    if let Some(limit) = state.settings.unit_limit
        && current >= limit
    {
        return Err(violation(Violation::UnitLimitReached { current, limit }));
    }
    let cost = commander::effective_build_cost(state, player, profile.cost)
        .ok_or_else(|| ExecuteError::InvalidState("commander build cost overflow".into()))?;
    let funds = state.player(player_index).funds;
    if cost > funds {
        return Err(violation(Violation::InsufficientFunds {
            required: cost,
            available: funds,
        }));
    }
    let next_id = state
        .next_unit_id
        .ok_or_else(|| ExecuteError::InvalidState("production requires next_unit_id".into()))?;
    let allocated_id = UnitId::new(next_id);
    if state.units.contains(allocated_id) {
        return Err(ExecuteError::InvalidState(
            format!("next_unit_id {next_id} is not fresh").into(),
        ));
    }
    let max_fuel = profile.max_fuel;
    let max_ammo = profile.max_ammo;
    let incremented_id = next_id
        .checked_add(1)
        .ok_or_else(|| ExecuteError::InvalidState("next_unit_id overflow".into()))?;

    let mut next = state.clone();
    next.player_mut(player_index).funds -= cost;
    next.next_unit_id = Some(incremented_id);
    next.units.push(Unit {
        id: allocated_id,
        kind,
        owner: player.clone(),
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
            Event::FundsChanged {
                player: player.clone(),
                from: funds,
                to: funds - cost,
                reason: KnownReason::UnitProduction.into(),
            },
            Event::UnitCreated {
                unit: allocated_id,
                kind,
                owner: player.clone(),
                position,
            },
        ],
        random_consumed: 0,
    })
}

pub(crate) fn player_owns_lab(state: &State, player: &PlayerId) -> bool {
    state
        .board
        .tiles()
        .any(|tile| tile.terrain == TerrainId::Lab && tile.owner.is_owned_by(player))
}

pub(crate) fn execute_move_capture(
    turn: &ActiveTurn<'_>,
    unit_id: UnitId,
    path: Vec<Pos>,
) -> Result<Execution, ExecuteError> {
    let state = turn.state();
    let player = turn.player();
    let plan = turn.plan_move(unit_id, path)?;
    let unit = &state.units[plan.unit_index()];
    let origin = plan.origin();
    let path = plan.path();
    let profile = ruleset::profile(unit.kind);
    let actor_team = plan.actor_team();
    let unit_index = plan.unit_index();
    let entry_costs = plan.entry_costs();
    let visibility = AwbwVisibility;

    if !profile.can_capture {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::Capture,
        }));
    }
    let destination = *path.last().expect("origin was checked");
    let destination_tile = &state.board.tile(destination);
    let capturable = ruleset::terrain_has(destination_tile.terrain, TerrainTrait::Capturable);
    let owner = destination_tile.owner.player();
    let owner_is_hostile = owner.is_none_or(|owner| {
        state
            .find_player(owner)
            .is_some_and(|candidate| candidate.team != actor_team)
    });
    if !capturable || !owner_is_hostile {
        return Err(violation(Violation::InvalidTarget {
            target: Some(destination.into()),
        }));
    }
    if state.units.iter().any(|other| {
        other.id != unit_id
            && board_position(other) == Some(destination)
            && occupancy_is_disclosed(&visibility, state, actor_team, other)
    }) {
        return Err(violation(Violation::DestinationOccupied {
            position: destination,
        }));
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

    let tile = &mut next.board.tile_mut(destination);
    let before = tile
        .capture_points
        .ok_or(ExecuteError::UnsupportedRuleset)?;
    let capture_strength =
        commander::effective_capture_points(state, unit, u64::from(unit.hp.div_ceil(10)));
    if u64::from(before) > capture_strength {
        let after = u8::try_from(u64::from(before) - capture_strength)
            .map_err(|_| ExecuteError::InvalidState("capture result overflow".into()))?;
        tile.capture_points = Some(after);
        events.push(Event::CaptureChanged {
            position: destination,
            from: before,
            to: after,
        });
    } else {
        let previous_owner = tile.owner.to_optional();
        events.push(Event::CaptureChanged {
            position: destination,
            from: before,
            to: 0,
        });
        tile.owner = TileOwner::Owned(player.clone());
        events.push(Event::TileOwnerChanged {
            position: destination,
            from: previous_owner.clone(),
            to: Some(player.clone()),
        });
        tile.capture_points = Some(20);
        events.push(Event::CaptureChanged {
            position: destination,
            from: 0,
            to: 20,
        });
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
                .find_player(player)
                .map(|candidate| candidate.team.clone())
                .ok_or_else(|| ExecuteError::InvalidState("capturing player is absent".into()))?;
            complete_match(
                &mut next,
                Outcome::Victory {
                    winners: vec![winning_team],
                    reason: VictoryReason::CaptureLimit,
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
            .tiles()
            .any(|candidate| candidate.terrain == TerrainId::Hq);
        let is_lab = captured_profile.has(TerrainTrait::LabVictory);
        let last_owned_lab_lost = previous_owner.as_deref().is_some_and(|owner| {
            !next.board.tiles().any(|candidate| {
                candidate.terrain == TerrainId::Lab
                    && candidate
                        .owner
                        .player()
                        .is_some_and(|candidate_owner| candidate_owner == owner)
            })
        });
        if (defeats_owner || (no_hq_on_map && is_lab && last_owned_lab_lost))
            && let Some(previous_owner) = previous_owner
        {
            let cause = if defeats_owner {
                VictoryReason::HqCapture
            } else {
                VictoryReason::LabCapture
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

pub(crate) fn owned_unit_count(state: &State, player: &PlayerId) -> Result<u64, ExecuteError> {
    u64::try_from(
        state
            .units
            .iter()
            .filter(|unit| unit.owner == player)
            .count(),
    )
    .map_err(|_| ExecuteError::InvalidState("owned unit count exceeds u64".into()))
}

pub(crate) fn capture_limit_count(state: &State, player: &PlayerId) -> u64 {
    state
        .board
        .tiles()
        .filter(|tile| {
            tile.owner.player().is_some_and(|owner| owner == player)
                && ruleset::terrain_has(tile.terrain, TerrainTrait::CountsTowardCaptureLimit)
        })
        .count() as u64
}
