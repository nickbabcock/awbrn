use crate::common::{map_path, replay_path};
use awbrn_map::AwbwMapData;
use awbw_replay::{
    Hidden, Masked, ReplayParser,
    turn_models::{Action, MoveAction, RepairAction, RepairedUnit, TargetedPlayer, UnitProperty},
};
use awvm::semantic::{Location, Pos, UnitId};
use awvm_awbw::RecordedAdapter;
use indexmap::IndexMap;

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

fn before_sonja_combat(index: usize) -> (RecordedAdapter, Action) {
    let replay = ReplayParser::new()
        .parse(&std::fs::read(replay_path("replay_1356213_sonja-masked-hp.zip")).unwrap())
        .unwrap();
    let map: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_path("163726.json")).unwrap()).unwrap();
    let mut adapter = RecordedAdapter::new(&replay, &map).unwrap();
    for action in &replay.turns[..index] {
        adapter.advance(action).unwrap();
    }
    (adapter, replay.turns[index].clone())
}

#[test]
fn sonja_combat_uses_the_owner_hit_points_after_damage() {
    for (index, expected_hp) in [(110, 60), (132, 20)] {
        let (mut adapter, action) = before_sonja_combat(index);
        adapter.advance(&action).unwrap();
        assert_eq!(
            adapter
                .state()
                .units
                .get(UnitId::new(172015972))
                .unwrap()
                .hp,
            expected_hp,
            "Sonja combat action {index} must use the visible owner's HP"
        );
    }
}

#[test]
fn combat_resolves_only_matching_hit_points_and_preserves_selected_properties() {
    let (adapter, action) = before_sonja_combat(110);
    for (selected_hp, recipient_hp, mismatched_id, expected_hp) in [
        (None, Some(6), false, 60),
        (Some(7), Some(6), false, 70),
        (None, None, false, 100),
        (None, Some(6), true, 100),
        (None, Some(0), false, 0),
    ] {
        let mut adapter = adapter.clone();
        let mut action = action.clone();
        let Action::Fire { fire_action, .. } = &mut action else {
            panic!("expected combat");
        };
        let mut rows = fire_action.combat_info_vision.values_mut();
        let selected = rows
            .next()
            .unwrap()
            .combat_info
            .attacker
            .get_value()
            .unwrap();
        let id = UnitId::new(selected.units_id.as_u32());
        let selected_ammo = selected.units_ammo;
        let hp = |value: Option<u8>| {
            Some(value.map_or(Masked::Masked, |hp| {
                serde_json::from_value(serde_json::json!(hp)).unwrap()
            }))
        };
        for (index, view) in fire_action.combat_info_vision.values_mut().enumerate() {
            let Masked::Visible(attacker) = &mut view.combat_info.attacker else {
                panic!("expected a visible combat unit");
            };
            attacker.units_hit_points = hp(if index == 0 { selected_hp } else { None });
            if index == 1 {
                attacker.units_hit_points = hp(recipient_hp);
                attacker.units_ammo = selected_ammo + 1;
                if mismatched_id {
                    attacker.units_id = awbrn_types::AwbwUnitId::new(id.get() + 1);
                }
            }
        }
        adapter.advance(&action).unwrap();
        if expected_hp == 0 {
            assert!(!adapter.state().units.contains(id));
        } else {
            let unit = adapter.state().units.get(id).unwrap();
            assert_eq!(unit.hp, expected_hp);
            assert_eq!(unit.ammo, u64::from(selected_ammo));
        }
    }
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
fn moves_keep_global_properties_when_recipient_hit_points_are_visible() {
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
    global.units_fuel = Some(80);
    unit.units_fuel = Some(20);
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
    assert_eq!(moving.fuel, 80);
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
fn owner_hit_points_do_not_require_recipient_coordinates() {
    let (mut adapter, build, mut owner_unit) = before_first_build();
    adapter.advance(&build).unwrap();
    let id = UnitId::new(owner_unit.units_id.as_u32());
    let mut global = owner_unit.clone();
    global.units_hit_points = Masked::Masked;
    owner_unit.units_x = None;
    owner_unit.units_y = None;
    owner_unit.units_hit_points = serde_json::from_value(serde_json::json!(6)).unwrap();
    let owner = TargetedPlayer::Player(owner_unit.units_players_id);
    let mut movement = MoveAction {
        unit: IndexMap::from([
            (TargetedPlayer::Global, Hidden::Visible(global)),
            (owner, Hidden::Visible(owner_unit.clone())),
        ]),
        paths: IndexMap::new(),
        dist: 0,
        trapped: false,
        discovered: None,
    };
    adapter.advance(&Action::Move(movement.clone())).unwrap();
    assert_eq!(adapter.state().units.get(id).unwrap().hp, 60);

    // A row for a different unit cannot disclose this unit's HP.
    owner_unit.units_id = awbrn_types::AwbwUnitId::new(id.get() + 1);
    owner_unit.units_hit_points = serde_json::from_value(serde_json::json!(3)).unwrap();
    movement.unit.insert(owner, Hidden::Visible(owner_unit));
    adapter.advance(&Action::Move(movement)).unwrap();
    assert_eq!(adapter.state().units.get(id).unwrap().hp, 60);
}

#[test]
fn building_with_only_masked_hit_points_fails_without_changing_state() {
    let (mut adapter, mut build, _) = before_first_build();
    let prior = adapter.state().clone();
    let Action::Build { new_unit, .. } = &mut build else {
        panic!("expected a build");
    };
    for row in new_unit.values_mut() {
        if let Hidden::Visible(unit) = row {
            unit.units_hit_points = Masked::Masked;
        }
    }
    let error = adapter.advance(&build).unwrap_err();
    assert!(error.to_string().contains("built unit hit points"));
    assert_eq!(adapter.state(), &prior);
}
