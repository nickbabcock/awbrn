use awbrn_map::AwbwMapData;
use awbw_replay::{
    Hidden, Masked, ReplayParser,
    turn_models::{
        Action, EndInfo, GameOverAction, MoveAction, RepairAction, RepairedUnit, TargetedPlayer,
        UnitProperty,
    },
};
use awvm::semantic::{Location, Match, Outcome, Pos, UnitId, VictoryReason};
use awvm_awbw::RecordedAdapter;
use indexmap::IndexMap;

use crate::common::{map_path, replay_path};

#[test]
fn packed_turn_history_moves_units_before_reusing_their_tiles() {
    let replay = ReplayParser::new()
        .parse(&std::fs::read(replay_path("replay_1598747_null-min-rating.zip")).unwrap())
        .unwrap();
    assert_eq!(replay.games.len(), 27);
    assert_eq!(replay.turns.len(), 505);
    let map: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_path("153972.json")).unwrap()).unwrap();
    let mut adapter = RecordedAdapter::new(&replay, &map).unwrap();
    let mut checked_build = false;
    for action in &replay.turns {
        if let Action::Build { new_unit, .. } = action
            && new_unit
                .values()
                .filter_map(Hidden::get_value)
                .any(|unit| unit.units_id.as_u32() == 195603830)
        {
            let existing = adapter.state().units.get(UnitId::new(195450357)).unwrap();
            let Location::Board { position } = existing.location else {
                panic!("expected the existing unit on the board");
            };
            assert_eq!((position.x, position.y), (11, 6));
            checked_build = true;
        }
        adapter.advance(action).unwrap();
    }
    assert!(checked_build);
}

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
fn moves_keep_global_coordinates_when_recipient_hit_points_are_visible() {
    let (mut adapter, build, mut unit) = before_first_build();
    adapter.advance(&build).unwrap();
    let id = UnitId::new(unit.units_id.as_u32());
    let mut state = adapter.state().clone();
    let global_position = state.units.get(id).unwrap().location;
    let recipient_position = Location::Board {
        position: Pos::new(0, 0),
    };
    assert!(
        state
            .units
            .iter()
            .all(|unit| unit.location != recipient_position)
    );
    let mut occupant = *state.units.get(id).unwrap();
    let occupant_id = UnitId::new(state.next_unit_id.unwrap());
    occupant.id = occupant_id;
    occupant.location = recipient_position;
    state.units.push(occupant);
    state.next_unit_id = Some(occupant_id.get() + 1);
    let mut adapter = RecordedAdapter::from_state(state).unwrap();

    let owner = TargetedPlayer::Player(unit.units_players_id);
    let mut global = unit.clone();
    global.units_hit_points = Masked::Masked;
    unit.units_x = Some(0);
    unit.units_y = Some(0);
    unit.units_hit_points = serde_json::from_value(serde_json::json!(4)).unwrap();
    let mut movement = MoveAction {
        unit: IndexMap::from([
            (TargetedPlayer::Global, Hidden::Visible(global.clone())),
            (owner, Hidden::Visible(unit)),
        ]),
        paths: IndexMap::new(),
        dist: 0,
        trapped: false,
        discovered: None,
    };
    adapter.advance(&Action::Move(movement.clone())).unwrap();
    let moving = adapter.state().units.get(id).unwrap();
    assert_eq!(moving.location, global_position);
    assert_eq!(moving.hp, 40);
    assert_eq!(
        adapter.state().units.get(occupant_id).unwrap().location,
        recipient_position
    );

    global.units_hit_points = serde_json::from_value(serde_json::json!(8)).unwrap();
    movement
        .unit
        .insert(TargetedPlayer::Global, Hidden::Visible(global));
    adapter.advance(&Action::Move(movement)).unwrap();
    let moving = adapter.state().units.get(id).unwrap();
    assert_eq!(moving.location, global_position);
    assert_eq!(moving.hp, 80);
    assert!(adapter.state().units.contains(occupant_id));
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
