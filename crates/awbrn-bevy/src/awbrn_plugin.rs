//! Bevy plugin for AWBRN with support for multiple game modes.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Menu
//!
//!     state AppState {
//!         Menu --> Loading : ReplayToLoad resource<br/>or PendingGameStart resource
//!         Loading --> InGame : LoadingState Complete
//!         InGame --> Menu : User action
//!
//!         state Loading {
//!             [*] --> LoadingReplay : Replay mode
//!             [*] --> LoadingAssets : Game mode or<br/>after replay parsed
//!             LoadingReplay --> LoadingAssets : Replay parsed<br/>map loading starts
//!             LoadingAssets --> Complete : Map loaded
//!             Complete --> [*] : Transition to InGame
//!         }
//!     }
//!
//!     state GameMode {
//!         None --> Replay : ReplayToLoad resource
//!         None --> Game : PendingGameStart resource
//!         Replay --> None : Reset
//!         Game --> None : Reset
//!     }
//!
//!     note right of GameMode : Independent state<br/>determines active systems<br/>in InGame
//! ```

use crate::core::{GameMode, LoadingState};
use crate::features::event_bus::{EventBus, EventBusPlugin, GameEvent};
use crate::loading::{LoadingPlugin, MapAssetPathResolver};
use bevy::prelude::*;
use std::sync::Arc;

pub struct AwbrnPlugin {
    map_resolver: Arc<dyn MapAssetPathResolver>,
    event_bus: Option<Arc<dyn EventBus<GameEvent>>>,
}

impl AwbrnPlugin {
    pub fn new(map_resolver: Arc<dyn MapAssetPathResolver>) -> Self {
        Self {
            map_resolver,
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus<GameEvent>>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }
}

impl Default for AwbrnPlugin {
    fn default() -> Self {
        Self {
            map_resolver: Arc::new(crate::loading::DefaultMapAssetPathResolver),
            event_bus: None,
        }
    }
}

impl Plugin for AwbrnPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::core::CorePlugin,
            LoadingPlugin::new(self.map_resolver.clone()),
            crate::render::RenderPlugin,
            crate::features::FeaturesPlugin,
            crate::modes::replay::ReplayPlugin,
            crate::modes::play::PlayPlugin,
        ));

        if let Some(ref bus) = self.event_bus {
            app.add_plugins(EventBusPlugin::new(bus.clone()));
        }

        // Cross-plugin OnEnter(Complete) scheduling
        app.add_systems(
            OnEnter(LoadingState::Complete),
            crate::features::event_bus::emit_map_dimensions,
        );
        app.add_systems(
            OnEnter(LoadingState::Complete),
            crate::features::input::spawn_tile_cursor.after(crate::loading::setup_ui_atlas),
        );
        app.add_systems(
            OnEnter(LoadingState::Complete),
            crate::modes::replay::bootstrap::initialize_replay_semantic_world
                .run_if(in_state(GameMode::Replay)),
        );
        app.add_systems(
            OnEnter(LoadingState::Complete),
            crate::core::map::initialize_terrain_semantic_world.run_if(in_state(GameMode::Game)),
        );
    }
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use crate::core::map::GameMap;
    use crate::core::*;
    use crate::features::navigation::{
        CourseArrowPiece, animate_course_arrows, animate_unit_paths, spawn_pending_course_arrows,
    };
    use crate::features::weather::CurrentWeather;
    use crate::modes::replay::AwbwUnitId;
    use crate::modes::replay::commands::ReplayAdvanceLock;
    use crate::render::units::on_unit_active_remove;
    use crate::render::{TerrainAtlasResource, UiAtlasResource};
    use awbrn_core::{AwbwUnitId as CoreUnitId, GraphicalTerrain, PlayerFaction, Property};
    use awbrn_map::{AwbrnMap, Position};
    use awbw_replay::turn_models::{
        Action, AwbwHpDisplay, BuildingInfo, CaptureAction, CombatInfo, CombatInfoVision,
        CombatUnit, CopValueInfo, CopValues, FireAction, MoveAction, TargetedPlayer, UnitProperty,
    };
    use awbw_replay::{Hidden, Masked};
    use bevy::prelude::*;
    use indexmap::IndexMap;

    pub(crate) fn replay_animation_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.init_resource::<GameMap>();
        app.init_resource::<StrongIdMap<AwbwUnitId>>();
        app.init_resource::<Assets<crate::UiAtlasAsset>>();
        app.init_resource::<Assets<TextureAtlasLayout>>();
        app.insert_resource(ReplayAdvanceLock::default());
        app.insert_resource(CurrentWeather::default());
        app.insert_resource(TerrainAtlasResource {
            texture: Handle::default(),
            layout: Handle::default(),
        });
        app.add_observer(on_map_position_insert);
        app.add_observer(crate::render::map::on_terrain_tile_insert);
        app.add_observer(on_unit_active_remove);
        app.add_observer(spawn_pending_course_arrows);
        app.add_systems(Update, (animate_course_arrows, animate_unit_paths));

        app.world_mut().resource_mut::<GameMap>().set(AwbrnMap::new(
            40,
            40,
            GraphicalTerrain::Plain,
        ));
        insert_test_ui_atlas(&mut app);

        app
    }

    pub(crate) fn insert_test_ui_atlas(app: &mut App) {
        let atlas_handle = {
            let mut assets = app
                .world_mut()
                .resource_mut::<Assets<crate::UiAtlasAsset>>();
            assets.add(crate::UiAtlasAsset {
                size: crate::UiAtlasSize {
                    width: 48,
                    height: 16,
                },
                sprites: vec![
                    crate::UiAtlasSprite {
                        name: "Arrow_Body.png".to_string(),
                        x: 0,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                    crate::UiAtlasSprite {
                        name: "Arrow_Curved.png".to_string(),
                        x: 16,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                    crate::UiAtlasSprite {
                        name: "Arrow_Tip.png".to_string(),
                        x: 32,
                        y: 0,
                        width: 16,
                        height: 16,
                    },
                ],
            })
        };
        let layout_handle = {
            let mut layouts = app.world_mut().resource_mut::<Assets<TextureAtlasLayout>>();
            layouts.add(TextureAtlasLayout::from_grid(
                UVec2::new(16, 16),
                3,
                1,
                None,
                None,
            ))
        };

        app.world_mut().insert_resource(UiAtlasResource {
            handle: atlas_handle,
            texture: Handle::default(),
            layout: layout_handle,
        });
    }

    pub(crate) fn spawn_test_unit(
        app: &mut App,
        position: Position,
        unit_id: CoreUnitId,
        faction: PlayerFaction,
    ) -> Entity {
        app.world_mut()
            .spawn((
                MapPosition::from(position),
                Transform::default(),
                Sprite::from_atlas_image(
                    Handle::default(),
                    TextureAtlas {
                        layout: Handle::default(),
                        index: 0,
                    },
                ),
                Unit(awbrn_core::Unit::Infantry),
                Faction(faction),
                AwbwUnitId(unit_id),
                UnitActive,
            ))
            .id()
    }

    pub(crate) fn spawn_test_property(app: &mut App, position: Position) {
        app.world_mut().spawn((
            MapPosition::from(position),
            Transform::default(),
            Sprite::from_atlas_image(
                Handle::default(),
                TextureAtlas {
                    layout: Handle::default(),
                    index: 0,
                },
            ),
            crate::core::map::TerrainTile {
                terrain: GraphicalTerrain::Property(Property::City(awbrn_core::Faction::Neutral)),
            },
        ));
    }

    pub(crate) fn course_arrows(app: &mut App) -> Vec<(CourseArrowPiece, Visibility, Transform)> {
        let mut query = app
            .world_mut()
            .query::<(&CourseArrowPiece, &Visibility, &Transform)>();
        query
            .iter(app.world())
            .map(|(piece, visibility, transform)| (*piece, *visibility, *transform))
            .collect()
    }

    pub(crate) fn test_move_action() -> Action {
        test_move_action_for(
            CoreUnitId::new(173623341),
            3276855,
            7,
            32,
            &[(8, 33), (7, 33), (7, 32)],
        )
    }

    pub(crate) fn test_move_action_for(
        unit_id: CoreUnitId,
        player_id: u32,
        final_x: u32,
        final_y: u32,
        path: &[(u32, u32)],
    ) -> Action {
        Action::Move(MoveAction {
            unit: IndexMap::from([(
                TargetedPlayer::Global,
                Hidden::Visible(test_unit_property(
                    unit_id.as_u32(),
                    player_id,
                    awbrn_core::Unit::Infantry,
                    final_x,
                    final_y,
                )),
            )]),
            paths: IndexMap::from([(
                TargetedPlayer::Global,
                path.iter()
                    .map(|&(x, y)| awbw_replay::turn_models::PathTile {
                        unit_visible: true,
                        x,
                        y,
                    })
                    .collect(),
            )]),
            dist: 3,
            trapped: false,
            discovered: None,
        })
    }

    pub(crate) fn test_capture_action() -> Action {
        Action::Capt {
            move_action: Some(MoveAction {
                unit: IndexMap::from([(
                    TargetedPlayer::Global,
                    Hidden::Visible(test_unit_property(1, 1, awbrn_core::Unit::Infantry, 2, 1)),
                )]),
                paths: IndexMap::from([(
                    TargetedPlayer::Global,
                    vec![
                        awbw_replay::turn_models::PathTile {
                            unit_visible: true,
                            x: 2,
                            y: 2,
                        },
                        awbw_replay::turn_models::PathTile {
                            unit_visible: true,
                            x: 2,
                            y: 1,
                        },
                    ],
                )]),
                dist: 1,
                trapped: false,
                discovered: None,
            }),
            capture_action: CaptureAction {
                building_info: BuildingInfo {
                    buildings_capture: 10,
                    buildings_id: 99,
                    buildings_x: 2,
                    buildings_y: 1,
                    buildings_team: None,
                },
                vision: IndexMap::new(),
                income: None,
            },
        }
    }

    pub(crate) fn test_fire_action() -> Action {
        Action::Fire {
            move_action: Some(MoveAction {
                unit: IndexMap::from([(
                    TargetedPlayer::Global,
                    Hidden::Visible(test_unit_property(10, 1, awbrn_core::Unit::Infantry, 5, 4)),
                )]),
                paths: IndexMap::from([(
                    TargetedPlayer::Global,
                    vec![
                        awbw_replay::turn_models::PathTile {
                            unit_visible: true,
                            x: 4,
                            y: 4,
                        },
                        awbw_replay::turn_models::PathTile {
                            unit_visible: true,
                            x: 5,
                            y: 4,
                        },
                    ],
                )]),
                dist: 1,
                trapped: false,
                discovered: None,
            }),
            fire_action: FireAction {
                combat_info_vision: IndexMap::from([(
                    TargetedPlayer::Global,
                    CombatInfoVision {
                        has_vision: true,
                        combat_info: CombatInfo {
                            attacker: Masked::Visible(CombatUnit {
                                units_ammo: 0,
                                units_hit_points: Some(test_hp(8)),
                                units_id: CoreUnitId::new(10),
                                units_x: 5,
                                units_y: 4,
                            }),
                            defender: Masked::Visible(CombatUnit {
                                units_ammo: 0,
                                units_hit_points: Some(test_hp(5)),
                                units_id: CoreUnitId::new(11),
                                units_x: 5,
                                units_y: 4,
                            }),
                        },
                    },
                )]),
                cop_values: CopValues {
                    attacker: CopValueInfo {
                        player_id: awbrn_core::AwbwGamePlayerId::new(1),
                        cop_value: 0,
                        tag_value: None,
                    },
                    defender: CopValueInfo {
                        player_id: awbrn_core::AwbwGamePlayerId::new(2),
                        cop_value: 0,
                        tag_value: None,
                    },
                },
            },
        }
    }

    fn test_unit_property(
        unit_id: u32,
        player_id: u32,
        unit_name: awbrn_core::Unit,
        x: u32,
        y: u32,
    ) -> UnitProperty {
        UnitProperty {
            units_id: CoreUnitId::new(unit_id),
            units_games_id: Some(1403019),
            units_players_id: player_id,
            units_name: unit_name,
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
            units_x: Some(x),
            units_y: Some(y),
            units_moved: Some(1),
            units_capture: Some(0),
            units_fired: Some(0),
            units_hit_points: test_hp(10),
            units_cargo1_units_id: Default::default(),
            units_cargo2_units_id: Default::default(),
            units_carried: Some("N".to_string()),
            countries_code: PlayerFaction::OrangeStar,
        }
    }

    pub(crate) fn test_hp(value: u8) -> AwbwHpDisplay {
        serde_json::from_value(serde_json::json!(value)).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use crate::core::*;
    use crate::features::navigation::{COURSE_ARROW_BASE_SCALE, CourseArrowSpriteKind};
    use crate::modes::replay::commands::{ReplayAdvanceLock, ReplayTurnCommand};
    use awbrn_core::{
        AwbwUnitId as CoreUnitId, GraphicalMovement, PlayerFaction, get_unit_animation_frames,
    };
    use awbrn_map::Position;
    use bevy::prelude::*;
    use std::time::Duration;

    #[test]
    fn animated_move_visits_intermediate_tiles_and_releases_lock() {
        let mut app = replay_animation_test_app();
        let unit_entity = spawn_test_unit(
            &mut app,
            Position::new(8, 33),
            CoreUnitId::new(173623341),
            PlayerFaction::GreenEarth,
        );
        app.update();

        let start_translation = app
            .world()
            .entity(unit_entity)
            .get::<Transform>()
            .unwrap()
            .translation;

        ReplayTurnCommand {
            action: test_move_action(),
        }
        .apply(app.world_mut());

        assert!(
            app.world()
                .entity(unit_entity)
                .contains::<crate::render::animation::UnitPathAnimation>()
        );
        assert_eq!(
            app.world()
                .entity(unit_entity)
                .get::<MapPosition>()
                .unwrap()
                .position(),
            Position::new(7, 32)
        );
        assert_eq!(
            app.world().resource::<ReplayAdvanceLock>().active_entity(),
            Some(unit_entity)
        );
        assert_eq!(
            app.world()
                .entity(unit_entity)
                .get::<Transform>()
                .unwrap()
                .translation,
            start_translation
        );

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(50));
        app.update();

        let mid_translation = app
            .world()
            .entity(unit_entity)
            .get::<Transform>()
            .unwrap()
            .translation;
        assert_ne!(mid_translation, start_translation);

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(650));
        app.update();

        let expected_final = position_to_world_translation(
            app.world().entity(unit_entity).get::<SpriteSize>().unwrap(),
            Position::new(7, 32),
            app.world().resource::<crate::core::map::GameMap>(),
        );
        let final_translation = app
            .world()
            .entity(unit_entity)
            .get::<Transform>()
            .unwrap()
            .translation;
        assert!(
            final_translation.abs_diff_eq(expected_final, 0.05),
            "unexpected final translation: {final_translation:?}"
        );
        let final_sprite = app.world().entity(unit_entity).get::<Sprite>().unwrap();
        assert!(!final_sprite.flip_x);
        assert_eq!(
            final_sprite.texture_atlas.as_ref().unwrap().index,
            get_unit_animation_frames(
                GraphicalMovement::Idle,
                awbrn_core::Unit::Infantry,
                PlayerFaction::GreenEarth
            )
            .start_index() as usize
        );
        assert!(
            !app.world()
                .entity(unit_entity)
                .contains::<crate::render::animation::UnitPathAnimation>()
        );
        assert!(!app.world().resource::<ReplayAdvanceLock>().is_active());
    }

    #[test]
    fn move_action_spawns_and_expires_course_arrows_in_world_space() {
        let mut app = replay_animation_test_app();
        let unit_entity = spawn_test_unit(
            &mut app,
            Position::new(8, 33),
            CoreUnitId::new(173623341),
            PlayerFaction::GreenEarth,
        );
        app.update();

        ReplayTurnCommand {
            action: test_move_action(),
        }
        .apply(app.world_mut());
        app.update();

        let arrows = course_arrows(&mut app);
        assert_eq!(arrows.len(), 2);

        let curved = arrows
            .iter()
            .find(|(piece, _, _)| piece.kind == CourseArrowSpriteKind::Curved)
            .expect("curve tile should spawn");
        assert!(matches!(curved.1, Visibility::Visible));
        assert!((curved.2.scale.x - COURSE_ARROW_BASE_SCALE).abs() < 0.001);

        let tip = arrows
            .iter()
            .find(|(piece, _, _)| piece.kind == CourseArrowSpriteKind::Tip)
            .expect("tip tile should spawn");
        assert!(matches!(tip.1, Visibility::Hidden));

        let unit_z = app
            .world()
            .entity(unit_entity)
            .get::<Transform>()
            .unwrap()
            .translation
            .z;
        assert!(curved.2.translation.z > 0.0);
        assert!(curved.2.translation.z > unit_z);

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(25));
        app.update();

        for (_, visibility, _) in course_arrows(&mut app) {
            assert!(matches!(visibility, Visibility::Visible));
        }

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(300));
        app.update();

        assert!(course_arrows(&mut app).is_empty());
    }

    #[test]
    fn capture_followup_waits_for_move_completion() {
        let mut app = replay_animation_test_app();
        let unit_entity = spawn_test_unit(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            PlayerFaction::OrangeStar,
        );
        spawn_test_property(&mut app, Position::new(2, 1));
        app.update();

        ReplayTurnCommand {
            action: test_capture_action(),
        }
        .apply(app.world_mut());

        assert!(!app.world().entity(unit_entity).contains::<Capturing>());

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(400));
        app.update();

        assert!(app.world().entity(unit_entity).contains::<Capturing>());
    }

    #[test]
    fn fire_followup_waits_for_move_completion() {
        let mut app = replay_animation_test_app();
        let attacker = spawn_test_unit(
            &mut app,
            Position::new(4, 4),
            CoreUnitId::new(10),
            PlayerFaction::OrangeStar,
        );
        let defender = spawn_test_unit(
            &mut app,
            Position::new(5, 4),
            CoreUnitId::new(11),
            PlayerFaction::BlueMoon,
        );
        app.update();

        ReplayTurnCommand {
            action: test_fire_action(),
        }
        .apply(app.world_mut());

        assert!(app.world().entity(attacker).get::<GraphicalHp>().is_none());
        assert!(app.world().entity(defender).get::<GraphicalHp>().is_none());

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(400));
        app.update();

        assert_eq!(
            app.world()
                .entity(attacker)
                .get::<GraphicalHp>()
                .unwrap()
                .value(),
            8
        );
        assert_eq!(
            app.world()
                .entity(defender)
                .get::<GraphicalHp>()
                .unwrap()
                .value(),
            5
        );
    }

    #[test]
    fn lateral_animation_uses_faction_facing_and_restores_idle_pose() {
        let mut app = replay_animation_test_app();
        let unit_entity = spawn_test_unit(
            &mut app,
            Position::new(4, 4),
            CoreUnitId::new(42),
            PlayerFaction::BlueMoon,
        );
        app.update();

        ReplayTurnCommand {
            action: test_move_action_for(CoreUnitId::new(42), 1, 5, 4, &[(4, 4), (5, 4)]),
        }
        .apply(app.world_mut());

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(200));
        app.update();

        let moving_sprite = app.world().entity(unit_entity).get::<Sprite>().unwrap();
        assert!(!moving_sprite.flip_x);

        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_millis(200));
        app.update();

        let final_sprite = app.world().entity(unit_entity).get::<Sprite>().unwrap();
        assert!(!final_sprite.flip_x);
        assert_eq!(
            final_sprite.texture_atlas.as_ref().unwrap().index,
            get_unit_animation_frames(
                GraphicalMovement::Idle,
                awbrn_core::Unit::Infantry,
                PlayerFaction::BlueMoon
            )
            .start_index() as usize
        );
    }
}
