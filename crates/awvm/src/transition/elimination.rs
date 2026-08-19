//! Removing a player from play, and what that does to the match.
//!
//! Normative source:
//! * `spec/semantics/elimination.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::event::Event;
use crate::ruleset::{self, KnownReason, VictoryReason};
use crate::semantic::{
    Outcome, PlayerId, PlayerIdx, PlayerStatus, Pos, State, TeamStatus, TileOwner,
};

pub(crate) fn eliminate_player(
    state: &mut State,
    defeated_player: &PlayerId,
    cause: VictoryReason,
    beneficiary: Option<&PlayerId>,
    trigger_hq: Option<Pos>,
    events: &mut Vec<Event>,
) -> Result<bool, ExecuteError> {
    let player_index = state
        .player_index(defeated_player)
        .ok_or(ExecuteError::UnsupportedRuleset)?;
    let defeated_team = state.player_mut(player_index).team.clone();
    let previous_status = state.player_mut(player_index).status;
    state.player_mut(player_index).status = if cause == VictoryReason::Resignation {
        PlayerStatus::Resigned
    } else {
        PlayerStatus::Eliminated
    };
    events.push(Event::PlayerStatusChanged {
        player: defeated_player.clone(),
        from: previous_status,
        to: state.player_mut(player_index).status,
    });
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
        events.push(Event::TeamEliminated {
            team: defeated_team,
            reason: KnownReason::from(cause).into(),
        });
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
            reason: cause,
        };
        complete_match(state, outcome, events);
        return Ok(true);
    }

    let defeated_seat = state.player_index(defeated_player);
    let mut unit_ids: Vec<_> = state
        .units
        .iter()
        .filter(|unit| Some(unit.owner) == defeated_seat)
        .map(|unit| unit.id)
        .collect();
    unit_ids.sort();
    for unit_id in unit_ids {
        let unit_index = state
            .units
            .index_of(unit_id)
            .expect("elimination unit remains present until its pass");
        if let Some(position) = board_position(&state.units[unit_index]) {
            let tile = &mut state.board.tile_mut(position);
            if let Some(before) = tile.capture_points.filter(|points| *points < 20) {
                tile.capture_points = Some(20);
                events.push(Event::CaptureChanged {
                    position,
                    from: before,
                    to: 20,
                });
            }
        }
        events.push(Event::UnitRemoved {
            unit: unit_id,
            reason: KnownReason::Elimination.into(),
        });
        state.units.remove(unit_index);
    }

    let mut properties = Vec::new();
    let defeated_seat = state.player_index(defeated_player);
    let beneficiary_seat = beneficiary.and_then(|player| state.player_index(player));
    // Tiles name a seat and the event names a player, so the roster is copied
    // out here: the loop below holds the board mutably and cannot read it.
    let roster: Vec<PlayerId> = state
        .players
        .iter()
        .map(|player| player.id().clone())
        .collect();
    let name = |seat: Option<PlayerIdx>| seat.map(|seat| roster[seat.get()].clone());
    for (position, tile) in state.board.iter() {
        let owned = defeated_seat.is_some_and(|seat| tile.owner.is_owned_by(seat));
        if owned || trigger_hq == Some(position) {
            properties.push(position);
        }
    }
    for position in properties {
        let tile = state.board.tile_mut(position);
        if let Some(before) = tile.capture_points.filter(|points| *points < 20) {
            tile.capture_points = Some(20);
            events.push(Event::CaptureChanged {
                position,
                from: before,
                to: 20,
            });
        }
        if let Some(replacement) = ruleset::terrain(tile.terrain).elimination_replacement {
            let from = tile.terrain;
            tile.terrain = replacement;
            events.push(Event::TileTerrainChanged {
                position,
                from,
                to: replacement,
                reason: KnownReason::Elimination.into(),
            });
        }
        let previous_owner = tile.owner.player();
        let next_owner = beneficiary_seat;
        if previous_owner != next_owner {
            tile.owner = TileOwner::ownable(next_owner);
            events.push(Event::TileOwnerChanged {
                position,
                from: name(previous_owner),
                to: name(next_owner),
            });
        }
    }
    Ok(false)
}
