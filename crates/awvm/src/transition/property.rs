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
    Concealment, KnownReason, Location, Outcome, PlayerIdx, Pos, State, TerrainId, TileOwner, Unit,
    UnitAction, UnitId, VictoryReason,
};
use crate::violation::{Action, Violation};

#[derive(Debug)]
pub(super) struct Capture;

#[derive(Debug)]
pub(super) struct CaptureProof {
    strength: u64,
    destination: AvailableDestination,
}

#[derive(Debug)]
pub(super) struct PreparedProduction<'a> {
    site: PreparedProductionSite<'a>,
    kind: UnitKind,
    cost: u64,
    funds: u64,
    allocated_id: UnitId,
    incremented_id: u32,
    max_fuel: u64,
    max_ammo: u64,
}

pub(super) fn prepare_production_site<'a>(
    turn: &ActiveTurn<'a>,
    position: Pos,
) -> Result<PreparedProductionSite<'a>, ExecuteError> {
    let state = turn.state();
    let player = turn.player();
    let player_index = state.player_index(player).ok_or_else(|| {
        ExecuteError::InvalidState(format!("unknown active player {player}").into())
    })?;
    let occupied = state
        .units
        .iter()
        .any(|unit| board_position(unit) == Some(position));
    let owned_units = owned_unit_count(state, player_index)?;
    let owns_lab = player_owns_lab(state, player_index);
    Ok(PreparedProductionSite {
        state,
        position,
        player_index,
        occupied,
        owned_units,
        owns_lab,
    })
}

/// Every rule a build request must pass, and what the site would charge.
///
/// This is the whole of what makes a request legal. All
/// [`prepare_production`] adds after it is bookkeeping for carrying the
/// request out, namely allocating the new unit's identifier, and a caller only
/// deciding whether to offer the build has no reason to fault on a state that
/// cannot allocate one. Splitting the two lets a build menu read the reducer's
/// own answer instead of restating these checks.
///
/// The order of the checks matters. The price is tested last, so an
/// [`Violation::InsufficientFunds`] means everything else was accepted.
pub(super) fn production_cost(
    site: &PreparedProductionSite<'_>,
    kind: UnitKind,
) -> Result<u64, ExecuteError> {
    let state = site.state;
    let profile = ruleset::profile(kind);

    // Site validation precedes requested-kind validation: whether the player
    // owns a facility here does not depend on what they asked it to build.
    let position = site.position;
    let tile = state.board.get(position);
    let seat = site.player_index;
    let site_valid = tile.is_some_and(|tile| {
        tile.owner.is_owned_by(seat)
            && commander::production_site(state, seat, tile.terrain, profile.domain)
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
    if state.settings.lab_units.contains(&kind) && !site.owns_lab {
        return Err(violation(Violation::InvalidTarget {
            target: Some(kind.into()),
        }));
    }
    if site.occupied {
        return Err(violation(Violation::DestinationOccupied { position }));
    }
    let current = site.owned_units;
    if let Some(limit) = state.settings.unit_limit
        && current >= limit
    {
        return Err(violation(Violation::UnitLimitReached { current, limit }));
    }
    let cost = commander::effective_build_cost(state, Some(seat), profile.cost)
        .ok_or_else(|| ExecuteError::InvalidState("commander build cost overflow".into()))?;
    let funds = state.player(site.player_index).funds;
    if cost > funds {
        return Err(violation(Violation::InsufficientFunds {
            required: cost,
            available: funds,
        }));
    }
    Ok(cost)
}

pub(super) fn prepare_production(
    site: PreparedProductionSite<'_>,
    kind: UnitKind,
) -> Result<PreparedProduction<'_>, ExecuteError> {
    let state = site.state;
    let profile = ruleset::profile(kind);
    let cost = production_cost(&site, kind)?;
    let funds = state.player(site.player_index).funds;
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

    Ok(PreparedProduction {
        site,
        kind,
        cost,
        funds,
        allocated_id,
        incremented_id,
        max_fuel,
        max_ammo,
    })
}

pub(super) fn execute_prepared_production(prepared: PreparedProduction<'_>) -> Execution {
    let PreparedProduction {
        site,
        kind,
        cost,
        funds,
        allocated_id,
        incremented_id,
        max_fuel,
        max_ammo,
    } = prepared;
    let state = site.state;
    let player = &state.turn.active_player;
    let mut next = state.clone();
    next.player_mut(site.player_index).funds -= cost;
    next.next_unit_id = Some(incremented_id);
    next.units.push(Unit {
        id: allocated_id,
        kind,
        owner: site.player_index,
        hp: 100,
        fuel: max_fuel,
        ammo: max_ammo,
        action: UnitAction::Spent,
        concealment: Concealment::Exposed,
        location: Location::Board {
            position: site.position,
        },
    });
    Execution {
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
                position: site.position,
            },
        ],
        random_consumed: 0,
    }
}

pub(crate) fn player_owns_lab(state: &State, seat: PlayerIdx) -> bool {
    state
        .board
        .tiles()
        .any(|tile| tile.terrain == TerrainId::Lab && tile.owner.is_owned_by(seat))
}

impl<'a> DestinationAction<'a> for Capture {
    type Proof = CaptureProof;

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
        let profile = ruleset::profile(unit.kind);
        let actor_team = plan.actor_team();
        if !profile.can_capture {
            return Err(violation(Violation::ActionNotSupported {
                action: Action::Capture,
            }));
        }
        let position = plan.destination();
        let destination_tile = &state.board.tile(position);
        let capturable = ruleset::terrain_has(destination_tile.terrain, TerrainTrait::Capturable);
        let owner = destination_tile.owner.player();
        let owner_is_hostile = owner.is_none_or(|seat| {
            state
                .players
                .get(seat.get())
                .is_some_and(|candidate| candidate.team != actor_team)
        });
        if !capturable || !owner_is_hostile {
            return Err(violation(Violation::InvalidTarget {
                target: Some(position.into()),
            }));
        }
        let available_destination = destination.available_destination()?;

        let strength =
            commander::effective_capture_points(state, unit, u64::from(unit.hp.div_ceil(10)));
        Ok(CaptureProof {
            strength,
            destination: available_destination,
        })
    }

    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a> {
        PreparedCommandKind::Capture(bound)
    }
}

pub(super) fn execute_prepared_capture(
    prepared: MovementAction<'_, CaptureProof>,
) -> Result<Execution, ExecuteError> {
    let MovementAction {
        movement,
        trap,
        action:
            CaptureProof {
                strength: capture_strength,
                destination: _destination,
            },
    } = prepared;
    let state = movement.state();
    let player = &state.turn.active_player;
    let destination = movement.plan().destination();
    let mut outcome = execute_planned_movement(state, movement.unit(), movement.plan(), trap);
    if outcome.trapped {
        return Ok(Execution {
            state: outcome.state,
            events: outcome.events,
            random_consumed: 0,
        });
    }

    let next = &mut outcome.state;
    let events = &mut outcome.events;
    // The tile names a seat and the event names a player, so both are resolved
    // before the board is borrowed mutably.
    let capturing_player = player.clone();
    let capturing_seat = next
        .player_index(&capturing_player)
        .ok_or_else(|| ExecuteError::InvalidState("capturing player is absent".into()))?;
    // The seat is kept beside the name: the elimination checks below ask what
    // else that seat still holds, and the tile has been overwritten by then.
    let previous_owner = next
        .board
        .tile(destination)
        .owner
        .player()
        .map(|seat| (seat, next.player_id(seat).clone()));
    let tile = &mut next.board.tile_mut(destination);
    let before = tile
        .capture_points
        .ok_or(ExecuteError::UnsupportedRuleset)?;
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
        events.push(Event::CaptureChanged {
            position: destination,
            from: before,
            to: 0,
        });
        tile.owner = TileOwner::Owned(capturing_seat);
        events.push(Event::TileOwnerChanged {
            position: destination,
            from: previous_owner.as_ref().map(|(_, owner)| owner.clone()),
            to: Some(capturing_player.clone()),
        });
        tile.capture_points = Some(crate::semantic::CAPTURE_REQUIRED_POINTS);
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
                .is_some_and(|limit| capture_limit_count(next, capturing_seat) >= limit)
        {
            let winning_team = next
                .find_player(player)
                .map(|candidate| candidate.team.clone())
                .ok_or_else(|| ExecuteError::InvalidState("capturing player is absent".into()))?;
            complete_match(
                next,
                Outcome::Victory {
                    winners: vec![winning_team],
                    reason: VictoryReason::CaptureLimit,
                },
                events,
            );
            return Ok(Execution {
                state: outcome.state,
                events: outcome.events,
                random_consumed: 0,
            });
        }
        let defeats_owner = captured_profile.has(TerrainTrait::CaptureDefeatsOwner);
        let no_hq_on_map = !next
            .board
            .tiles()
            .any(|candidate| candidate.terrain == TerrainId::Hq);
        let is_lab = captured_profile.has(TerrainTrait::LabVictory);
        let owner_seat = previous_owner.as_ref().map(|(seat, _)| *seat);
        let last_owned_lab_lost = owner_seat.is_some_and(|seat| {
            !next.board.tiles().any(|candidate| {
                candidate.terrain == TerrainId::Lab && candidate.owner.is_owned_by(seat)
            })
        });
        if (defeats_owner || (no_hq_on_map && is_lab && last_owned_lab_lost))
            && let Some((_, previous_owner)) = previous_owner
        {
            let cause = if defeats_owner {
                VictoryReason::HqCapture
            } else {
                VictoryReason::LabCapture
            };
            eliminate_player(
                next,
                &previous_owner,
                cause,
                Some(player),
                Some(destination),
                events,
            )?;
        }
    }
    Ok(Execution {
        state: outcome.state,
        events: outcome.events,
        random_consumed: 0,
    })
}

pub(crate) fn owned_unit_count(state: &State, seat: PlayerIdx) -> Result<u64, ExecuteError> {
    u64::try_from(state.units.iter().filter(|unit| unit.owner == seat).count())
        .map_err(|_| ExecuteError::InvalidState("owned unit count exceeds u64".into()))
}

pub(crate) fn capture_limit_count(state: &State, seat: PlayerIdx) -> u64 {
    state
        .board
        .tiles()
        .filter(|tile| {
            tile.owner.is_owned_by(seat)
                && ruleset::terrain_has(tile.terrain, TerrainTrait::CountsTowardCaptureLimit)
        })
        .count() as u64
}
