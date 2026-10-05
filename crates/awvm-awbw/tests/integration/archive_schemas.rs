use awbrn_map::AwbwMapData;
use awbw_replay::{
    Hidden, Masked, ReplayParser,
    turn_models::{
        Action, EndInfo, GameOverAction, MoveAction, RepairAction, RepairedUnit, TargetedPlayer,
        UnitProperty,
    },
};
use awvm::semantic::{Match, Outcome, UnitId, VictoryReason};
use awvm_awbw::RecordedAdapter;
use indexmap::IndexMap;

use crate::common::{map_path, replay_path};

fn before_first_build() -> (RecordedAdapter, Action, UnitProperty) {
    let replay = ReplayParser::new()
        .parse(&std::fs::read(replay_path("1362397.zip")).unwrap())
        .unwrap();
    let map_id = replay.games[0].maps_id.as_u32();
    let map: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_path(&format!("{map_id}.json"))).unwrap())
            .unwrap();
    let mut adapter = RecordedAdapter::new(&replay, &map).unwrap();
    for action in replay.turns {
        if let Action::Build { new_unit, .. } = &action {
            let unit = new_unit
                .values()
                .find_map(Hidden::get_value)
                .unwrap()
                .clone();
            return (adapter, action, unit);
        }
        adapter.advance(&action).unwrap();
    }
    panic!("expected a build");
}

#[test]
fn build_uses_the_owner_hit_points_when_the_global_value_is_masked() {
    let (mut adapter, mut action, unit) = before_first_build();
    let id = UnitId::new(unit.units_id.as_u32());
    let mut masked = unit.clone();
    masked.units_hit_points = Masked::Masked;
    let Action::Build { new_unit, .. } = &mut action else {
        panic!("expected a build");
    };
    *new_unit = IndexMap::from([
        (TargetedPlayer::Global, Hidden::Visible(masked)),
        (
            TargetedPlayer::Player(unit.units_players_id),
            Hidden::Visible(unit),
        ),
    ]);
    adapter.advance(&action).unwrap();
    assert_eq!(adapter.state().units.get(id).unwrap().hp, 100);
}

#[test]
fn moves_and_repairs_preserve_known_hit_points_when_values_are_masked() {
    let (mut adapter, build, mut unit) = before_first_build();
    adapter.advance(&build).unwrap();
    let id = UnitId::new(unit.units_id.as_u32());
    let owner = TargetedPlayer::Player(unit.units_players_id);
    unit.units_hit_points = serde_json::from_value(serde_json::json!(4)).unwrap();
    let mut masked = unit.clone();
    masked.units_hit_points = Masked::Masked;
    let mut movement = MoveAction {
        unit: IndexMap::from([
            (TargetedPlayer::Global, Hidden::Visible(masked.clone())),
            (owner, Hidden::Visible(unit)),
        ]),
        paths: IndexMap::new(),
        dist: 0,
        trapped: false,
        discovered: None,
    };
    adapter.advance(&Action::Move(movement.clone())).unwrap();
    assert_eq!(adapter.state().units.get(id).unwrap().hp, 40);
    movement.unit = IndexMap::from([(TargetedPlayer::Global, Hidden::Visible(masked))]);
    adapter.advance(&Action::Move(movement)).unwrap();
    assert_eq!(adapter.state().units.get(id).unwrap().hp, 40);

    let mut repair = RepairAction {
        unit: IndexMap::from([(TargetedPlayer::Global, Hidden::Visible(id.get()))]),
        repaired: IndexMap::from([(
            TargetedPlayer::Global,
            RepairedUnit {
                units_id: awbrn_types::AwbwUnitId::new(id.get()),
                units_hit_points: Masked::Masked,
            },
        )]),
        funds: IndexMap::new(),
    };
    adapter
        .advance(&Action::Repair {
            move_action: None,
            repair_action: repair.clone(),
        })
        .unwrap();
    assert_eq!(adapter.state().units.get(id).unwrap().hp, 40);
    repair.repaired.insert(
        owner,
        RepairedUnit {
            units_id: awbrn_types::AwbwUnitId::new(id.get()),
            units_hit_points: serde_json::from_value(serde_json::json!(7)).unwrap(),
        },
    );
    adapter
        .advance(&Action::Repair {
            move_action: None,
            repair_action: repair,
        })
        .unwrap();
    assert_eq!(adapter.state().units.get(id).unwrap().hp, 70);
}

#[test]
fn legacy_game_over_uses_elimination_flags_without_winner_metadata() {
    let (mut adapter, _, _) = before_first_build();
    let winner_id = adapter.state().players[1].id();
    let winner = adapter.state().players[1].team.clone();
    let eliminated = adapter
        .state()
        .players
        .iter()
        .map(|player| {
            (
                player.id().to_string(),
                serde_json::json!(if player.id() == winner_id { "N" } else { "Y" }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let over: GameOverAction = serde_json::from_value(serde_json::json!({
        "message": "The game is over!",
        "playersElim": eliminated,
    }))
    .unwrap();
    adapter
        .advance(&Action::End {
            updated_info: EndInfo::GameOver(over),
        })
        .unwrap();
    assert_eq!(
        adapter.state().match_state,
        Match::Finished {
            outcome: Outcome::Victory {
                winners: vec![winner],
                reason: VictoryReason::DayLimit,
            },
        }
    );
}
