//! The move planner's wire edge.
//!
//! [`move_planner_players`] is a free function: the page reads the players of
//! a planner file before it asks for a board, so that it can ask for each
//! player's CO, funds and power. [`crate::BevyApp::load_move_planner`] then
//! opens the board.

use awbrn_types::{AwbwCoId, AwbwGamePlayerId, AwbwMapId, Co, CoExt};
use awbw_replay::game_models::CoPower;
use awbw_replay::move_planner::{MovePlanner, PlannerSeat, PlannerSetup};
use serde::{Deserialize, Serialize};
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

/// One player that holds a unit or a building in a planner file.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MovePlannerPlayer {
    pub player_id: u32,
    pub faction_id: u8,
    pub faction_code: String,
    pub faction_name: String,
    /// The team of the player's buildings. Players with the same team are
    /// allies.
    pub team: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MovePlannerSummary {
    pub name: String,
    /// The AWBW game that the planner was saved from, if it has a unit.
    pub game_id: Option<u32>,
    pub players: Vec<MovePlannerPlayer>,
    /// The AWBW identifiers of the COs whose super power fires area
    /// strikes, which the board can preview.
    pub area_strike_co_ids: Vec<u32>,
    /// The charge that one power star is worth, by the number of powers
    /// that the CO used before, from none to the last count that raises it.
    pub power_star_charges: Vec<u64>,
}

/// The game data that a planner file does not record, from the page.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MovePlannerSetup {
    pub map_id: u32,
    /// The player who acts first.
    pub first_player_id: u32,
    /// The player whose super power strikes the board previews, on every
    /// turn. Without one, the board previews no strikes.
    pub previewed_player_id: Option<u32>,
    pub seats: Vec<MovePlannerSeat>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MovePlannerSeat {
    pub player_id: u32,
    /// The AWBW identifier of the player's CO.
    pub co_id: u32,
    pub funds: u32,
    /// The charge on the power meter, in the units of
    /// [`MovePlannerSummary::power_star_charges`].
    pub power_charge: u64,
    /// How many powers the CO used before. Each use raises the cost of a
    /// star.
    pub power_uses: u64,
    pub power_on: MovePlannerPowerOn,
    /// Players with the same team are allies. Without a team, the player
    /// keeps the team that the planner records.
    pub team: Option<String>,
}

/// The power that is on for a player when the board opens.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Tsify)]
#[serde(rename_all = "kebab-case")]
pub enum MovePlannerPowerOn {
    None,
    Cop,
    Scop,
}

/// Read the players of a move planner file.
#[wasm_bindgen]
pub fn move_planner_players(data: &[u8]) -> Result<Ts<MovePlannerSummary>, JsError> {
    let planner = parse(data)?;
    let summary = MovePlannerSummary {
        name: planner.name.clone(),
        game_id: planner.units.first().map(|unit| unit.games_id.as_u32()),
        players: planner
            .players()
            .into_iter()
            .map(|player| MovePlannerPlayer {
                player_id: player.id.as_u32(),
                faction_id: player.faction.awbw_id().as_u8(),
                faction_code: player.faction.country_code().to_string(),
                faction_name: player.faction.name().to_string(),
                team: player.team,
            })
            .collect(),
        area_strike_co_ids: area_strike_co_ids(),
        power_star_charges: power_star_charges(),
    };
    Ok(summary.into_ts()?)
}

/// The COs of the portrait catalog, which the page offers, whose super power
/// fires area strikes.
fn area_strike_co_ids() -> Vec<u32> {
    awbrn_content::co_portraits()
        .iter()
        .map(|portrait| portrait.awbw_id())
        .filter(|id| {
            Co::from_awbw_id(*id).is_some_and(|co| {
                awvm::commander::power_fires_area_strikes(co, awvm::commander::PowerLevel::Scop)
            })
        })
        .map(|id| id.as_u32())
        .collect()
}

fn power_star_charges() -> Vec<u64> {
    let mut charges = Vec::<u64>::new();
    for uses in 0.. {
        let Ok(charge) = awvm::commander::power_star_charge(uses) else {
            break;
        };
        // The cost stops rising at the use cap.
        if charges.last() == Some(&charge) {
            break;
        }
        charges.push(charge);
    }
    charges
}

pub(crate) fn parse(data: &[u8]) -> Result<MovePlanner, JsError> {
    MovePlanner::from_slice(data)
        .map_err(|error| JsError::new(&format!("Invalid move planner file: {error}")))
}

/// The replay for a planner board with `setup`.
pub(crate) fn planner_replay(
    planner: &MovePlanner,
    setup: &MovePlannerSetup,
) -> Result<awbw_replay::AwbwReplay, JsError> {
    let players = planner.players();
    let known = |player_id: u32| {
        if players.iter().any(|player| player.id.as_u32() == player_id) {
            Ok(AwbwGamePlayerId::new(player_id))
        } else {
            Err(JsError::new(&format!(
                "Player {player_id} has no unit or building in the planner"
            )))
        }
    };
    let seats = setup
        .seats
        .iter()
        .map(|seat| {
            // AWBW records ten times the charge that AWVM counts.
            let co_power = seat
                .power_charge
                .checked_mul(10)
                .and_then(|charge| u32::try_from(charge).ok())
                .ok_or_else(|| {
                    JsError::new(&format!("Power charge {} is too large", seat.power_charge))
                })?;
            Ok(PlannerSeat {
                player: known(seat.player_id)?,
                co_id: AwbwCoId::new(seat.co_id),
                team: seat.team.clone(),
                funds: seat.funds,
                co_power,
                co_power_on: match seat.power_on {
                    MovePlannerPowerOn::None => CoPower::None,
                    MovePlannerPowerOn::Cop => CoPower::Power,
                    MovePlannerPowerOn::Scop => CoPower::SuperPower,
                },
            })
        })
        .collect::<Result<Vec<_>, JsError>>()?;
    Ok(planner.to_replay(&PlannerSetup {
        map_id: AwbwMapId::new(setup.map_id),
        seats,
        first_player: Some(known(setup.first_player_id)?),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rachel_is_among_the_cos_with_area_strikes() {
        let ids = area_strike_co_ids();
        assert!(ids.contains(&28));
        // Jess's super power fires no missiles.
        assert!(!ids.contains(&14));
    }

    #[test]
    fn a_star_costs_more_after_each_power_until_the_cap() {
        let charges = power_star_charges();
        assert_eq!(charges[0], awvm::commander::power_star_charge(0).unwrap());
        assert!(charges.len() > 2);
        assert!(charges.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
