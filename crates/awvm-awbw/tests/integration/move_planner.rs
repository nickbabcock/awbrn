use awbrn_map::AwbwMapData;
use awbrn_types::{AwbwCoId, AwbwMapId, PlayerFaction};
use awbw_replay::game_models::CoPower;
use awbw_replay::move_planner::{MovePlanner, PlannerSeat, PlannerSetup};
use awvm::commander::{AreaStrikePolicy, PowerLevel};
use awvm::query::area_strikes;
use awvm::semantic::{PlayerId, Pos, PowerState};
use awvm_awbw::initial_state;

use crate::common::{map_path, workspace_path};

const RACHEL: AwbwCoId = AwbwCoId::new(28);

/// Game 1710706 on map 186800: Blue Moon's Rachel with Grey Sky and Cobalt
/// Ice, against Orange Star, Red Fire and Amber Blossom. The planner records
/// the teams.
fn cowboy() -> (MovePlanner, PlannerSetup) {
    let planner = MovePlanner::from_slice(
        &std::fs::read(workspace_path("assets/move_planners/cowboy.json")).unwrap(),
    )
    .unwrap();
    let seats = planner
        .players()
        .into_iter()
        .filter(|player| player.faction == PlayerFaction::BlueMoon)
        .map(|player| PlannerSeat {
            player: player.id,
            co_id: RACHEL,
            team: None,
            funds: 0,
            co_power: 0,
            co_power_on: CoPower::None,
        })
        .collect();
    let setup = PlannerSetup {
        map_id: AwbwMapId::new(186800),
        seats,
        first_player: None,
    };
    (planner, setup)
}

#[test]
fn a_move_planner_reads_every_unit_and_building() {
    let (planner, _) = cowboy();
    assert_eq!(planner.name, "cowboy");
    assert_eq!(planner.units.len(), 84);
    assert_eq!(planner.units.iter().filter(|unit| unit.carried).count(), 2);
    let teams = planner
        .players()
        .into_iter()
        .map(|player| (player.faction, player.team.unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        teams,
        [
            (PlayerFaction::OrangeStar, "A".into()),
            (PlayerFaction::BlueMoon, "B".into()),
            (PlayerFaction::RedFire, "A".into()),
            (PlayerFaction::GreySky, "B".into()),
            (PlayerFaction::AmberBlossom, "A".into()),
            (PlayerFaction::CobaltIce, "B".into()),
        ]
    );
    assert!(
        planner
            .buildings
            .iter()
            .any(|building| (building.x, building.y, building.capture) == (15, 15, 1))
    );
}

#[test]
fn rachel_covering_fire_targets_from_a_move_planner() {
    let (planner, setup) = cowboy();
    let map: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_path("186800.json")).unwrap()).unwrap();
    let state = initial_state(&planner.to_replay(&setup), &map).unwrap();
    let rachel = planner
        .players()
        .into_iter()
        .find(|player| player.faction == PlayerFaction::BlueMoon)
        .unwrap();
    let strikes = area_strikes(
        &state,
        &PlayerId::from(rachel.id.as_u32().to_string()),
        PowerLevel::Scop,
    )
    .unwrap();
    let centers = strikes
        .iter()
        .map(|strike| (strike.policy, strike.center))
        .collect::<Vec<_>>();
    assert_eq!(
        centers,
        [
            (AreaStrikePolicy::InfantryHp, Pos::new(11, 12)),
            (AreaStrikePolicy::UnitValue, Pos::new(12, 14)),
            (AreaStrikePolicy::UnitHp, Pos::new(12, 13)),
        ]
    );
}

#[test]
fn a_planner_seat_sets_the_turn_order_funds_and_power() {
    let (planner, mut setup) = cowboy();
    let map: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_path("186800.json")).unwrap()).unwrap();
    let players = planner.players();
    let rachel = players
        .iter()
        .find(|player| player.faction == PlayerFaction::BlueMoon)
        .unwrap()
        .id;
    setup.seats[0].funds = 12_000;
    // AWBW records ten times the charge that AWVM counts.
    setup.seats[0].co_power = 90_000;
    // Rachel's normal power is on from her last turn.
    setup.seats[0].co_power_on = CoPower::Power;
    setup.first_player = Some(rachel);

    let state = initial_state(&planner.to_replay(&setup), &map).unwrap();

    let rachel = PlayerId::from(rachel.as_u32().to_string());
    assert_eq!(state.turn.active_player, rachel);
    // The turn order keeps the faction order, from Rachel.
    let order = state
        .turn
        .order
        .iter()
        .map(|player| player.as_str().parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = players
        .iter()
        .map(|player| player.id.as_u32())
        .collect::<Vec<_>>();
    expected.rotate_left(1);
    assert_eq!(order, expected);
    let seat = state.player_index(&rachel).unwrap();
    let player = state.player(seat);
    assert_eq!(player.funds, 12_000);
    assert_eq!(player.commanders[0].power_charge, 9_000);
    assert_eq!(player.power_state, PowerState::Cop { commander_slot: 0 });
}

#[test]
fn a_unit_that_moved_before_the_planner_saved_is_spent() {
    // Saved in Orange Star's turn, after eight of its units moved.
    let planner = MovePlanner::from_slice(
        &std::fs::read(workspace_path("assets/move_planners/cowboy_mid_turn.json")).unwrap(),
    )
    .unwrap();
    let orange_star = planner
        .players()
        .into_iter()
        .find(|player| player.faction == PlayerFaction::OrangeStar)
        .unwrap()
        .id;
    let setup = PlannerSetup {
        map_id: AwbwMapId::new(186800),
        seats: Vec::new(),
        first_player: Some(orange_star),
    };
    let map: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_path("186800.json")).unwrap()).unwrap();
    let state = initial_state(&planner.to_replay(&setup), &map).unwrap();

    let moved = planner
        .units
        .iter()
        .filter(|unit| unit.moved > 0)
        .map(|unit| awvm::semantic::UnitId::new(unit.id.as_u32()))
        .collect::<Vec<_>>();
    assert_eq!(moved.len(), 8);
    for unit in &state.units {
        let expected = if moved.contains(&unit.id) {
            awvm::semantic::UnitAction::Spent
        } else {
            awvm::semantic::UnitAction::Ready
        };
        assert_eq!(unit.action, expected, "unit {}", unit.id);
    }
}
