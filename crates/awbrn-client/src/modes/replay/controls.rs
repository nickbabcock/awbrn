use crate::loading::LoadedReplay;
use crate::modes::replay::navigation::action_requires_path_animation;
use crate::modes::replay::presentation::{
    ReplayAdvanceLock, ReplayRewindCommand, ReplayTurnCommand,
};
use crate::modes::replay::state::ReplayControlState;
use awbrn_game::replay::ReplayState;
use bevy::input::{ButtonState, keyboard::KeyboardInput};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReplayAdvanceResult {
    Advanced,
    AdvancedWithLock,
    Exhausted,
}

pub(crate) fn advance_replay_action(
    commands: &mut Commands,
    replay_state: &mut ReplayState,
    loaded_replay: &LoadedReplay,
) -> ReplayAdvanceResult {
    let Some(action) = loaded_replay
        .0
        .turns
        .get(replay_state.next_action_index as usize)
        .cloned()
    else {
        return ReplayAdvanceResult::Exhausted;
    };

    commands.queue(ReplayTurnCommand { action });
    replay_state.next_action_index += 1;

    if action_requires_path_animation(
        loaded_replay
            .0
            .turns
            .get((replay_state.next_action_index - 1) as usize)
            .expect("queued replay action should still exist"),
    ) {
        ReplayAdvanceResult::AdvancedWithLock
    } else {
        ReplayAdvanceResult::Advanced
    }
}

pub(crate) fn handle_replay_controls(
    mut commands: Commands,
    mut keyboard_input: MessageReader<KeyboardInput>,
    mut replay_control: Local<ReplayControlState>,
    mut replay_state: ResMut<ReplayState>,
    loaded_replay: Res<LoadedReplay>,
    replay_lock: Res<ReplayAdvanceLock>,
    viewpoint_params: (
        ResMut<awbrn_game::replay::ReplayViewpoint>,
        Res<awbrn_game::replay::ReplayPlayerRegistry>,
    ),
) {
    let (mut viewpoint, registry) = viewpoint_params;
    let mut replay_blocked = replay_lock.is_active();

    for event in keyboard_input.read() {
        if event.state != ButtonState::Pressed {
            if event.key_code == KeyCode::ArrowRight {
                replay_control.suppress_exhausted_repeat = false;
            }
            continue;
        }

        match event.key_code {
            KeyCode::ArrowLeft => {
                if replay_blocked || replay_state.next_action_index == 0 {
                    continue;
                }

                commands.queue(ReplayRewindCommand {
                    target_index: replay_state.next_action_index - 1,
                });
                replay_control.suppress_exhausted_repeat = false;
            }
            KeyCode::ArrowRight => {
                if replay_blocked {
                    continue;
                }

                if event.repeat && replay_control.suppress_exhausted_repeat {
                    continue;
                }

                match advance_replay_action(&mut commands, &mut replay_state, &loaded_replay) {
                    ReplayAdvanceResult::Advanced => {
                        replay_control.suppress_exhausted_repeat = false;
                    }
                    ReplayAdvanceResult::AdvancedWithLock => {
                        replay_control.suppress_exhausted_repeat = false;
                        replay_blocked = true;
                    }
                    ReplayAdvanceResult::Exhausted => {
                        info!("Reached the end of the replay turns");
                        replay_control.suppress_exhausted_repeat = true;
                    }
                }
            }
            KeyCode::Digit0 | KeyCode::Numpad0 => {
                viewpoint.set_if_neq(awbrn_game::replay::ReplayViewpoint::Spectator);
            }
            KeyCode::Tab => {
                viewpoint.set_if_neq(awbrn_game::replay::ReplayViewpoint::ActivePlayer);
            }
            key @ (KeyCode::Digit1
            | KeyCode::Digit2
            | KeyCode::Digit3
            | KeyCode::Digit4
            | KeyCode::Digit5
            | KeyCode::Digit6
            | KeyCode::Digit7
            | KeyCode::Digit8
            | KeyCode::Numpad1
            | KeyCode::Numpad2
            | KeyCode::Numpad3
            | KeyCode::Numpad4
            | KeyCode::Numpad5
            | KeyCode::Numpad6
            | KeyCode::Numpad7
            | KeyCode::Numpad8) => {
                let index = match key {
                    KeyCode::Digit1 | KeyCode::Numpad1 => 0,
                    KeyCode::Digit2 | KeyCode::Numpad2 => 1,
                    KeyCode::Digit3 | KeyCode::Numpad3 => 2,
                    KeyCode::Digit4 | KeyCode::Numpad4 => 3,
                    KeyCode::Digit5 | KeyCode::Numpad5 => 4,
                    KeyCode::Digit6 | KeyCode::Numpad6 => 5,
                    KeyCode::Digit7 | KeyCode::Numpad7 => 6,
                    KeyCode::Digit8 | KeyCode::Numpad8 => 7,
                    _ => unreachable!(),
                };
                if let Some(player_id) = registry.player_id_at_index(index) {
                    viewpoint.set_if_neq(awbrn_game::replay::ReplayViewpoint::Player(player_id));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loading::apply_replay_building_overrides;
    use crate::modes::replay::bootstrap::initialize_replay_semantic_world_for_client;
    use awbrn_game::GameWorldPlugin;
    use awbrn_game::replay::AwbwUnitId;
    use awbrn_game::world::{GameMap, StrongIdMap};
    use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData};
    use awbrn_types::AwbwGamePlayerId;
    use awbw_replay::AwbwReplay;
    use awbw_replay::ReplayParser;
    use awbw_replay::turn_models::{Action, PowerAction};
    use awvm_awbw::RecordedAdapter;
    use bevy::input::keyboard::{Key, NativeKey};
    use std::path::Path;

    #[test]
    fn replay_press_advances_immediately() {
        let mut app = replay_controls_test_app(2);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 1);
    }

    #[test]
    fn replay_repeat_presses_advance_one_action_each() {
        let mut app = replay_controls_test_app(3);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 3);
    }

    #[test]
    fn replay_ignores_unrelated_and_release_events() {
        let mut app = replay_controls_test_app(2);

        send_key_event(&mut app, KeyCode::Space, ButtonState::Pressed, false);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Released, false);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 0);
    }

    #[test]
    fn replay_repeat_events_stop_at_end_until_release() {
        let mut app = replay_controls_test_app(1);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        app.update();
        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 1);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 1);
    }

    #[test]
    fn replay_release_clears_end_suppression() {
        let mut app = replay_controls_test_app(1);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        app.update();
        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 1);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        app.update();
        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 1);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Released, false);
        app.update();

        app.world_mut()
            .resource_mut::<ReplayState>()
            .next_action_index = 0;
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 1);
    }

    #[test]
    fn replay_move_action_blocks_additional_presses_in_same_frame() {
        let mut app = replay_controls_test_app_with_actions(vec![
            test_move_action(),
            test_replay_action(),
            test_replay_action(),
        ]);

        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, false);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        send_key_event(&mut app, KeyCode::ArrowRight, ButtonState::Pressed, true);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 1);
    }

    #[test]
    fn replay_left_rewinds_one_action_boundary() {
        let (mut app, _) = replay_rewind_controls_test_app();
        app.world_mut()
            .resource_mut::<ReplayState>()
            .next_action_index = 2;

        send_key_event(&mut app, KeyCode::ArrowLeft, ButtonState::Pressed, false);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 1);
    }

    #[test]
    fn replay_left_stops_at_the_initial_boundary() {
        let (mut app, _) = replay_rewind_controls_test_app();

        send_key_event(&mut app, KeyCode::ArrowLeft, ButtonState::Pressed, false);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 0);
    }

    #[test]
    fn replay_left_repeat_rewinds_one_action_boundary() {
        let (mut app, _) = replay_rewind_controls_test_app();
        app.world_mut()
            .resource_mut::<ReplayState>()
            .next_action_index = 2;

        send_key_event(&mut app, KeyCode::ArrowLeft, ButtonState::Pressed, true);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 1);
    }

    #[test]
    fn replay_left_repeat_is_ignored_while_advance_is_locked() {
        let (mut app, replay) = replay_rewind_controls_test_app();
        for action in replay.turns.iter().cloned() {
            ReplayTurnCommand { action }.apply(app.world_mut());
            if app.world().resource::<ReplayAdvanceLock>().is_active() {
                break;
            }
        }
        assert!(app.world().resource::<ReplayAdvanceLock>().is_active());
        app.world_mut()
            .resource_mut::<ReplayState>()
            .next_action_index = 2;

        send_key_event(&mut app, KeyCode::ArrowLeft, ButtonState::Pressed, true);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 2);
    }

    #[test]
    fn replay_left_keeps_index_when_rewind_source_is_missing() {
        let mut app = replay_controls_test_app(3);
        app.world_mut()
            .resource_mut::<ReplayState>()
            .next_action_index = 2;

        send_key_event(&mut app, KeyCode::ArrowLeft, ButtonState::Pressed, false);
        app.update();

        assert_eq!(app.world().resource::<ReplayState>().next_action_index, 2);
    }

    fn replay_controls_test_app(action_count: usize) -> App {
        replay_controls_test_app_with_actions(vec![test_replay_action(); action_count])
    }

    fn replay_controls_test_app_with_actions(actions: Vec<Action>) -> App {
        let mut app = App::new();
        app.add_message::<KeyboardInput>();
        app.add_systems(Update, handle_replay_controls);
        app.insert_resource(ReplayState::default());
        app.insert_resource(ReplayAdvanceLock::default());
        app.insert_resource(StrongIdMap::<AwbwUnitId>::default());
        app.insert_resource(LoadedReplay(AwbwReplay {
            games: Vec::new(),
            turns: actions,
        }));
        app.init_resource::<crate::features::visibility::ViewerVisibility>();
        app.init_resource::<crate::features::visibility::FriendlyFactions>();
        app.init_resource::<awbrn_game::replay::ReplayTerrainKnowledge>();
        app.init_resource::<awbrn_game::replay::RecipientObservations>();
        app.init_resource::<awbrn_game::replay::ReplayViewpoint>();
        app.init_resource::<awbrn_game::replay::ReplayPlayerRegistry>();
        app
    }

    fn replay_rewind_controls_test_app() -> (App, AwbwReplay) {
        let replay_bytes = std::fs::read(replay_fixture_path("1362397.zip")).unwrap();
        let replay = ReplayParser::new().parse(&replay_bytes).unwrap();
        let map_data: AwbwMapData =
            serde_json::from_slice(&std::fs::read(map_fixture_path("162795.json")).unwrap())
                .unwrap();
        let adapter = RecordedAdapter::new(&replay, &map_data).unwrap();
        let mut graphical_map = AwbwMap::try_from(&map_data).unwrap();
        apply_replay_building_overrides(
            &mut graphical_map,
            &replay.games.first().unwrap().buildings,
        );

        let mut app = App::new();
        app.add_plugins(GameWorldPlugin);
        app.add_message::<KeyboardInput>();
        app.add_systems(Update, handle_replay_controls);
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(AwbrnMap::from_map(&graphical_map));
        app.insert_resource(LoadedReplay(replay.clone()));
        app.insert_resource(
            crate::modes::replay::presentation::ReplayTransitionSource::new(adapter),
        );
        initialize_replay_semantic_world_for_client(app.world_mut());
        (app, replay)
    }

    fn replay_fixture_path(file_name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/replays")
            .join(file_name)
    }

    fn map_fixture_path(file_name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps")
            .join(file_name)
    }

    fn send_key_event(app: &mut App, key_code: KeyCode, state: ButtonState, repeat: bool) {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state,
            text: None,
            repeat,
            window: Entity::PLACEHOLDER,
        });
    }

    fn test_replay_action() -> Action {
        Action::Power(PowerAction {
            player_id: AwbwGamePlayerId::new(1),
            co_name: "Test CO".to_string(),
            co_power: "N".to_string(),
            power_name: "Test Power".to_string(),
            players_cop: 0,
            global: None,
            hp_change: None,
            unit_replace: None,
            unit_add: None,
            player_replace: None,
            missile_coords: None,
            weather: None,
        })
    }

    fn test_move_action() -> Action {
        use awbw_replay::turn_models::{MoveAction, PathTile, TargetedPlayer, UnitProperty};
        use awbw_replay::{Hidden, Masked};
        Action::Move(MoveAction {
            unit: [(
                TargetedPlayer::Global,
                Hidden::Visible(UnitProperty {
                    units_id: awbrn_types::AwbwUnitId::new(1),
                    units_games_id: Some(1),
                    units_players_id: AwbwGamePlayerId::new(1),
                    units_name: awbrn_types::Unit::Infantry,
                    units_movement_points: Some(3),
                    units_vision: Some(2),
                    units_fuel: Some(99),
                    units_fuel_per_turn: Some(0),
                    units_sub_dive: "N".to_string(),
                    units_ammo: Some(0),
                    units_short_range: Some(0),
                    units_long_range: Some(0),
                    units_second_weapon: Some("N".to_string()),
                    units_symbol: Some("G".to_string()),
                    units_cost: Some(1000),
                    units_movement_type: "F".to_string(),
                    units_x: Some(3),
                    units_y: Some(2),
                    units_moved: Some(1),
                    units_capture: Some(0),
                    units_fired: Some(0),
                    units_hit_points: serde_json::from_value(serde_json::json!(10)).unwrap(),
                    units_cargo1_units_id: Masked::Masked,
                    units_cargo2_units_id: Masked::Masked,
                    units_carried: Some("N".to_string()),
                    countries_code: awbrn_types::PlayerFaction::OrangeStar,
                }),
            )]
            .into(),
            paths: [(
                TargetedPlayer::Global,
                vec![
                    PathTile {
                        unit_visible: true,
                        x: 2,
                        y: 2,
                    },
                    PathTile {
                        unit_visible: true,
                        x: 3,
                        y: 2,
                    },
                ],
            )]
            .into(),
            dist: 1,
            trapped: false,
            discovered: None,
        })
    }
}
