//! Von Bolt's Ex Machina in a fog game. The power's record lists only the
//! units its player can see, and the stun lasts through the target's next turn.

use std::collections::HashSet;

use crate::common::{map_path, replay_path};
use awbrn_map::AwbwMapData;
use awbw_replay::{AwbwReplay, ReplayParser, turn_models::Action};
use awvm::semantic::{Location, PlayerId, Pos, UnitAction, UnitId};
use awvm_awbw::RecordedAdapter;

fn von_bolt_fog_game() -> (AwbwReplay, AwbwMapData) {
    let replay = ReplayParser::new()
        .parse(&std::fs::read(replay_path("replay_1442615_von-bolt-fog.zip")).unwrap())
        .unwrap();
    let map_id = replay.games[0].maps_id.as_u32();
    let map = serde_json::from_slice(&std::fs::read(map_path(&format!("{map_id}.json"))).unwrap())
        .unwrap();
    (replay, map)
}

#[test]
fn ex_machina_stuns_the_enemy_units_it_does_not_list() {
    let (replay, map) = von_bolt_fog_game();
    let mut adapter = RecordedAdapter::new(&replay, &map).unwrap();
    let (mut strikes, mut unlisted) = (0, 0);

    for action in &replay.turns {
        let Action::Power(power) = action else {
            adapter.advance(action).unwrap();
            continue;
        };
        if power.power_name != "Ex Machina" {
            adapter.advance(action).unwrap();
            continue;
        }

        let before = adapter.state().clone();
        adapter.advance(action).unwrap();
        let after = adapter.state();
        let actor = after
            .player_index(&PlayerId::from(power.player_id.as_u32().to_string()))
            .unwrap();
        let centers: Vec<_> = power
            .missile_coords
            .iter()
            .flatten()
            .map(|center| Pos::new(center.x as u8, center.y as u8))
            .collect();
        assert!(!centers.is_empty(), "Ex Machina records its center");
        let listed: HashSet<_> = power
            .unit_replace
            .iter()
            .flat_map(|groups| groups.values())
            .filter_map(|group| group.units.as_ref())
            .flatten()
            .map(|change| change.units_id.as_u32())
            .collect();

        for unit in &before.units {
            let Location::Board { position } = unit.location else {
                continue;
            };
            // As in Advance Wars, Ex Machina reaches two tiles from its center.
            if centers.iter().all(|center| center.distance(position) > 2) {
                continue;
            }
            let struck = after.units.get(unit.id).unwrap();
            if unit.owner == actor {
                assert_eq!((struck.hp, struck.action), (unit.hp, unit.action));
                continue;
            }
            assert_eq!(struck.action, UnitAction::Immobilized, "unit {}", unit.id);
            if !listed.contains(&unit.id.get()) {
                // It takes three HP bars and leaves at least one.
                let bars = unit.hp.div_ceil(10).saturating_sub(3).max(1);
                assert_eq!(struck.hp, bars * 10, "unit {}", unit.id);
                unlisted += 1;
            }
        }
        strikes += 1;
    }

    assert!(strikes > 0, "the game has no Ex Machina");
    assert!(unlisted > 0, "no Ex Machina hit a unit it did not list");
}

/// Each turn starts as AWBW recorded it: a stunned unit has spent its turn,
/// and a unit hit while out of sight shows the damage.
#[test]
fn every_turn_start_matches_the_recorded_snapshot() {
    let (replay, map) = von_bolt_fog_game();
    let mut adapter = RecordedAdapter::new(&replay, &map).unwrap();
    let mut turn = 0;

    for action in &replay.turns {
        adapter.advance(action).unwrap();
        if !matches!(action, Action::End { .. }) {
            continue;
        }
        turn += 1;
        let Some(snapshot) = replay.games.get(turn) else {
            break;
        };
        for recorded in &snapshot.units {
            let id = recorded.id.as_u32();
            let unit = adapter
                .state()
                .units
                .get(UnitId::new(id))
                .unwrap_or_else(|| panic!("turn {turn}: unit {id} is missing"));
            assert_eq!(
                unit.hp.div_ceil(10),
                recorded.hit_points.ceil() as u8,
                "turn {turn}: unit {id} HP"
            );
            if recorded.players_id.as_u32() == snapshot.turn {
                assert_eq!(
                    unit.action != UnitAction::Ready,
                    recorded.moved > 0,
                    "turn {turn}: unit {id} is {:?}",
                    unit.action
                );
            }
        }
    }

    assert!(turn > 1, "the game has no turn starts to compare");
}
