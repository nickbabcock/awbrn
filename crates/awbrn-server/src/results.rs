//! Authoritative results for a finished match, with one entry per seat.

use std::collections::HashMap;

use awvm::event::Event;
use awvm::ruleset::{DrawReason, KnownReason, VictoryReason};
use awvm::semantic::{Match, Outcome, PlayerStatus, Reason, State, TeamId};
use serde::Serialize;
use tsify::Tsify;

const PLACEMENT_FIRST: u32 = 1;

/// Why a seat received its result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Tsify)]
#[serde(untagged)]
pub enum SeatResultReason {
    Victory(VictoryReason),
    Draw(DrawReason),
}

/// Result for one seat's team.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Tsify)]
#[serde(rename_all = "kebab-case")]
pub enum SeatOutcome {
    Win,
    Loss,
    Draw,
}

/// Result for one seat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct SeatResult {
    pub slot_index: u8,
    #[tsify(type = "string")]
    pub team_id: TeamId,
    pub outcome: SeatOutcome,
    pub placement: u32,
    /// Exit cause or match-ending reason; absent for a standing winner.
    #[tsify(optional)]
    pub reason: Option<SeatResultReason>,
    pub status: PlayerStatus,
}

/// Results for all seats in slot order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MatchResults {
    pub seats: Vec<SeatResult>,
}

#[derive(Clone, Copy, Debug)]
struct SeatExit {
    cause: VictoryReason,
    order: u32,
}

/// Exit causes and order keyed by slot.
#[derive(Debug, Default)]
pub(crate) struct SeatExits {
    exits: HashMap<u8, SeatExit>,
    next_order: u32,
}

impl SeatExits {
    pub(crate) fn observe(&mut self, state: &State, events: &[Event]) {
        for event in events {
            let Event::PlayerStatusChanged { player, reason, .. } = event else {
                continue;
            };
            let (Some(seat), Some(cause)) = (state.player_index(player), victory_reason(reason))
            else {
                continue;
            };
            // Keep the first exit order.
            self.exits.entry(seat.get() as u8).or_insert_with(|| {
                let order = self.next_order;
                self.next_order += 1;
                SeatExit { cause, order }
            });
        }
    }

    fn cause(&self, slot_index: u8) -> Option<VictoryReason> {
        self.exits.get(&slot_index).map(|exit| exit.cause)
    }

    fn order(&self, slot_index: u8) -> Option<u32> {
        self.exits.get(&slot_index).map(|exit| exit.order)
    }
}

/// Build results for a finished non-cancelled match.
pub(crate) fn match_results(state: &State, exits: &SeatExits) -> Option<MatchResults> {
    let Match::Finished { outcome } = &state.match_state else {
        return None;
    };
    let (first_place_teams, ending) = match outcome {
        Outcome::Victory { winners, reason } => {
            (winners.as_slice(), SeatResultReason::Victory(*reason))
        }
        Outcome::Draw { teams, reason } => (teams.as_slice(), SeatResultReason::Draw(*reason)),
        Outcome::Cancelled { .. } => return None,
    };
    let drawn = matches!(outcome, Outcome::Draw { .. });
    let placements = team_placements(state, exits, first_place_teams);

    let seats = state
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            let slot_index = index as u8;
            let first_place = first_place_teams.contains(&player.team);
            let outcome = match (first_place, drawn) {
                (true, true) => SeatOutcome::Draw,
                (true, false) => SeatOutcome::Win,
                (false, _) => SeatOutcome::Loss,
            };
            let reason = match exits.cause(slot_index) {
                Some(cause) => Some(SeatResultReason::Victory(cause)),
                // A standing winner has no exit cause.
                None if outcome == SeatOutcome::Win => None,
                None => Some(ending),
            };
            SeatResult {
                slot_index,
                team_id: player.team.clone(),
                outcome,
                placement: placement_of(&placements, &player.team),
                reason,
                status: player.status,
            }
        })
        .collect();

    Some(MatchResults { seats })
}

/// Rank winning or drawn teams first; rank losers by elimination order.
fn team_placements(state: &State, exits: &SeatExits, first_place: &[TeamId]) -> Vec<(TeamId, u32)> {
    let mut losers: Vec<(TeamId, Option<u32>)> = state
        .teams
        .iter()
        .map(|team| &team.id)
        .filter(|team| !first_place.contains(team))
        .map(|team| (team.clone(), team_eliminated_at(state, exits, team)))
        .collect();
    // Standing teams rank above eliminated teams; later elimination ranks higher.
    losers.sort_by_key(|(_, eliminated_at)| std::cmp::Reverse(eliminated_at.unwrap_or(u32::MAX)));

    first_place
        .iter()
        .map(|team| (team.clone(), PLACEMENT_FIRST))
        .chain(
            losers
                .into_iter()
                .enumerate()
                .map(|(rank, (team, _))| (team, PLACEMENT_FIRST + 1 + rank as u32)),
        )
        .collect()
}

/// Return a team's last exit order, or `None` if it still has a seat.
fn team_eliminated_at(state: &State, exits: &SeatExits, team: &TeamId) -> Option<u32> {
    state
        .players
        .iter()
        .enumerate()
        .filter(|(_, player)| player.team == team)
        .try_fold(None, |latest: Option<u32>, (index, _)| {
            let order = exits.order(index as u8)?;
            Some(Some(latest.map_or(order, |latest| latest.max(order))))
        })
        .flatten()
}

fn placement_of(placements: &[(TeamId, u32)], team: &TeamId) -> u32 {
    placements
        .iter()
        .find(|(id, _)| id == team)
        .map(|(_, placement)| *placement)
        // A finished match ranks every team.
        .unwrap_or(PLACEMENT_FIRST + 1)
}

fn victory_reason(reason: &Reason) -> Option<VictoryReason> {
    let Reason::Known(known) = reason else {
        return None;
    };
    match known {
        KnownReason::Rout => Some(VictoryReason::Rout),
        KnownReason::HqCapture => Some(VictoryReason::HqCapture),
        KnownReason::LabCapture => Some(VictoryReason::LabCapture),
        KnownReason::CaptureLimit => Some(VictoryReason::CaptureLimit),
        KnownReason::DayLimit => Some(VictoryReason::DayLimit),
        KnownReason::Resignation => Some(VictoryReason::Resignation),
        KnownReason::Timeout => Some(VictoryReason::Timeout),
        _ => None,
    }
}
