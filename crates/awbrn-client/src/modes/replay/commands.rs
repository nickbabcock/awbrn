//! Replay command system for processing AWBW replay actions.
//!
//! Uses a custom Bevy Command to get direct `&mut World` access, enabling
//! immediate mutations that are visible to subsequent queries within the same
//! command execution.

use awbw_replay::turn_models::{Action, MoveAction};
use bevy::{log, prelude::*};

use crate::features::event_bus::{EventSink, NewDay as ExternalNewDay};
use crate::features::player_roster::{
    PlayerFunds, PlayerRosterConfig, PlayerUnitCosts, emit_player_roster_updated,
    player_ids_for_team,
};
use crate::modes::replay::navigation::{
    PendingCourseArrows, path_positions, replay_move_view, replay_path_tiles,
};
use crate::render::animation::UnitPathAnimation;
use awbrn_game::replay::{
    AwbwUnitId, NewDay, ReplayState, apply_move_state,
    apply_non_move_action as game_apply_non_move_action,
};
use awbrn_game::world::{CarriedBy, Faction, StrongIdMap, Unit};

#[derive(Resource, Debug, Default)]
pub struct ReplayAdvanceLock {
    active_entity: Option<Entity>,
    deferred_action: Option<Action>,
    recompute_fog: bool,
}

impl ReplayAdvanceLock {
    pub fn is_active(&self) -> bool {
        self.active_entity.is_some()
    }

    pub fn activate(
        &mut self,
        entity: Entity,
        deferred_action: Option<Action>,
        recompute_fog: bool,
    ) {
        self.active_entity = Some(entity);
        self.deferred_action = deferred_action;
        self.recompute_fog = recompute_fog;
    }

    pub fn active_entity(&self) -> Option<Entity> {
        self.active_entity
    }

    pub fn release_for(&mut self, entity: Entity) -> Option<ReplayAnimationFollowup> {
        if self.active_entity != Some(entity) {
            return None;
        }

        self.active_entity = None;
        Some(ReplayAnimationFollowup {
            action: self.deferred_action.take(),
            recompute_fog: std::mem::take(&mut self.recompute_fog),
        })
    }
}

#[derive(Debug)]
pub struct ReplayAnimationFollowup {
    pub action: Option<Action>,
    pub recompute_fog: bool,
}

pub struct ReplayFollowupCommand {
    pub action: Option<Action>,
    pub recompute_fog: bool,
}

impl Command for ReplayFollowupCommand {
    fn apply(self, world: &mut World) {
        if let Some(action) = &self.action {
            apply_non_move_action(action, world);
            update_player_roster_funds(action, world);
            update_player_roster_unit_costs(action, world);
        }
        if self.recompute_fog {
            world.trigger(super::fog::ReplayFogDirty);
        }
        emit_player_roster_updated(world);
    }
}

/// A custom Command for processing replay turn actions.
pub struct ReplayTurnCommand {
    pub action: Action,
}

impl Command for ReplayTurnCommand {
    fn apply(self, world: &mut World) {
        if let Some(mov) = self.action.move_action()
            && Self::apply_move(mov, &self.action, world)
        {
            // Move started a path animation — fog recompute happens in
            // ReplayFollowupCommand after animation completes.
            return;
        }

        apply_non_move_action(&self.action, world);
        update_player_roster_funds(&self.action, world);
        update_player_roster_unit_costs(&self.action, world);
        world.trigger(super::fog::ReplayFogDirty);
        emit_player_roster_updated(world);
    }
}

pub(crate) fn apply_non_move_action(action: &Action, world: &mut World) {
    game_apply_non_move_action(action, world);
}

fn update_player_roster_funds(action: &Action, world: &mut World) {
    let updates = collect_player_roster_fund_updates(action, world);
    let Some(mut funds) = world.get_resource_mut::<PlayerFunds>() else {
        return;
    };

    for (player_id, value) in updates {
        match value {
            FundUpdate::Set(funds_value) => funds.set(player_id, funds_value),
            FundUpdate::Subtract(amount) => funds.subtract(player_id, amount),
        }
    }
}

fn update_player_roster_unit_costs(action: &Action, world: &mut World) {
    let updates = collect_player_roster_unit_cost_updates(action);
    let Some(mut unit_costs) = world.get_resource_mut::<PlayerUnitCosts>() else {
        return;
    };

    for (unit_id, cost) in updates {
        unit_costs.set(unit_id, cost);
    }
}

fn collect_player_roster_fund_updates(
    action: &Action,
    world: &World,
) -> Vec<(awbrn_types::AwbwGamePlayerId, FundUpdate)> {
    match action {
        Action::Build { new_unit, .. } => {
            if let Some(unit) = new_unit.values().find_map(|unit| unit.get_value()) {
                vec![(
                    awbrn_types::AwbwGamePlayerId::new(unit.units_players_id),
                    FundUpdate::Subtract(unit.units_cost.unwrap_or(unit.units_name.base_cost())),
                )]
            } else {
                Vec::new()
            }
        }
        Action::Join { join_action, .. } => {
            if let Some(new_funds) =
                awbrn_game::replay::commands::targeted_value(&join_action.new_funds)
            {
                vec![(
                    awbrn_types::AwbwGamePlayerId::new(join_action.player_id),
                    FundUpdate::Set(*new_funds),
                )]
            } else {
                Vec::new()
            }
        }
        Action::Repair { repair_action, .. } => {
            if let Some(new_funds) =
                awbrn_game::replay::commands::targeted_hidden_value(&repair_action.funds)
                && let Some(active_player_id) = world
                    .get_resource::<ReplayState>()
                    .and_then(|state| state.active_player_id)
            {
                vec![(active_player_id, FundUpdate::Set(new_funds))]
            } else {
                Vec::new()
            }
        }
        Action::End { updated_info } | Action::Tag { updated_info } => {
            if let Some(next_funds) =
                awbrn_game::replay::commands::targeted_hidden_value(&updated_info.next_funds)
            {
                vec![(
                    awbrn_types::AwbwGamePlayerId::new(updated_info.next_player_id),
                    FundUpdate::Set(next_funds),
                )]
            } else {
                Vec::new()
            }
        }
        Action::Resign {
            next_turn_action: Some(next_turn_action),
            ..
        } => {
            if let Some(next_funds) =
                awbrn_game::replay::commands::targeted_hidden_value(&next_turn_action.next_funds)
            {
                vec![(
                    awbrn_types::AwbwGamePlayerId::new(next_turn_action.next_player_id),
                    FundUpdate::Set(next_funds),
                )]
            } else {
                Vec::new()
            }
        }
        Action::Power(power_action) => {
            if let Some(player_replace) = &power_action.player_replace
                && let Some(config) = world.get_resource::<PlayerRosterConfig>()
            {
                let mut updates = Vec::new();
                for (_audience, changes) in player_replace {
                    for (subject, change) in changes {
                        let Some(players) = targeted_players(config, *subject) else {
                            continue;
                        };
                        if let Some(player_funds) = change.players_funds {
                            for player_id in players {
                                updates.push((player_id, FundUpdate::Set(player_funds)));
                            }
                        }
                    }
                }
                updates
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

enum FundUpdate {
    Set(u32),
    Subtract(u32),
}

fn collect_player_roster_unit_cost_updates(
    action: &Action,
) -> Vec<(awbrn_game::replay::AwbwUnitId, u32)> {
    match action {
        Action::Build { new_unit, .. } => new_unit
            .values()
            .find_map(|unit| unit.get_value())
            .map(|unit| {
                vec![(
                    awbrn_game::replay::AwbwUnitId(unit.units_id),
                    unit.units_cost.unwrap_or(unit.units_name.base_cost()),
                )]
            })
            .unwrap_or_default(),
        Action::Power(power_action) => power_action
            .unit_add
            .as_ref()
            .into_iter()
            .flat_map(|groups| groups.values())
            .flat_map(|group| {
                group.units.iter().map(|unit| {
                    (
                        awbrn_game::replay::AwbwUnitId(unit.units_id),
                        group.unit_name.base_cost(),
                    )
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn targeted_players(
    config: &PlayerRosterConfig,
    subject: awbw_replay::turn_models::TargetedPlayer,
) -> Option<Vec<awbrn_types::AwbwGamePlayerId>> {
    match subject {
        awbw_replay::turn_models::TargetedPlayer::Player(player_id) => Some(vec![player_id]),
        awbw_replay::turn_models::TargetedPlayer::Team(team) => {
            Some(player_ids_for_team(config, team).collect())
        }
        awbw_replay::turn_models::TargetedPlayer::Global => None,
    }
}

impl ReplayTurnCommand {
    fn apply_move(move_action: &MoveAction, action: &Action, world: &mut World) -> bool {
        let Some((targeted_player, unit)) = replay_move_view(move_action) else {
            log::warn!("Move action missing visible targeted player unit data");
            return false;
        };

        // Resolve entity before calling apply_move_state so we can use it for
        // path animation setup even if units_x/units_y are absent (apply_move_state
        // will return None for those cases).
        let entity = {
            let units = world.resource::<StrongIdMap<AwbwUnitId>>();
            units.get(&AwbwUnitId(unit.units_id))
        };

        let Some(entity) = entity else {
            log::warn!(
                "Unit with ID {} not found in unit storage",
                unit.units_id.as_u32()
            );
            return false;
        };

        let Some(outcome) = apply_move_state(move_action, world) else {
            return false;
        };

        let new_position = outcome.new_position;

        let idle_flip_x = world
            .entity(entity)
            .get::<Sprite>()
            .map(|sprite| sprite.flip_x)
            .unwrap_or(false);
        let path_tiles = replay_path_tiles(move_action, targeted_player);
        let unit_faction = world.entity(entity).get::<Faction>().unwrap().0;
        let unit_is_air =
            world.entity(entity).get::<Unit>().unwrap().0.domain() == awbrn_types::UnitDomain::Air;
        let current_view_path = path_tiles
            .as_deref()
            .map(|path| Self::path_tiles_for_current_view(path, world, unit_faction, unit_is_air));
        let animated_path = path_tiles
            .as_ref()
            .and_then(|path| UnitPathAnimation::new(path_positions(path), idle_flip_x));
        let path_tile_count = path_tiles.as_ref().map_or(0, Vec::len);
        let should_animate_for_viewer = current_view_path
            .as_ref()
            .is_none_or(|path| path.iter().any(|tile| tile.unit_visible));

        // Load actions remove the unit from the board entirely, and joins
        // resolve by despawning the moving unit into the survivor. In both
        // cases, skip inserting MapPosition at the destination so BoardIndex
        // keeps pointing at the entity that should remain there.
        let skip_destination_map_position =
            matches!(action, Action::Load { .. } | Action::Join { .. });

        if should_animate_for_viewer && let Some(path_animation) = animated_path {
            let mut entity_mut = world.entity_mut(entity);
            if skip_destination_map_position {
                entity_mut.insert(path_animation);
            } else {
                entity_mut.insert((path_animation, new_position));
            }
            if let Some(path) = current_view_path {
                entity_mut.insert(PendingCourseArrows { path });
            }

            let deferred_action = match action {
                Action::Move(_) => None,
                _ => Some(action.clone()),
            };

            world
                .resource_mut::<ReplayAdvanceLock>()
                .activate(entity, deferred_action, true);

            log::info!(
                "Started path animation for unit {} across {} tiles",
                unit.units_id.as_u32(),
                path_tile_count
            );
            return true;
        }

        if !skip_destination_map_position {
            world.entity_mut(entity).insert(new_position);
        }

        false
    }

    fn path_tiles_for_current_view(
        path: &[crate::modes::replay::navigation::ReplayPathTile],
        world: &World,
        unit_faction: awbrn_types::PlayerFaction,
        unit_is_air: bool,
    ) -> Vec<crate::modes::replay::navigation::ReplayPathTile> {
        use crate::features::fog::{FogActive, FogOfWarMap, FriendlyFactions};

        let fog_active = world.resource::<FogActive>();
        if !fog_active.0 {
            return path.to_vec();
        }

        let friendly = world.resource::<FriendlyFactions>();
        if friendly.0.contains(&unit_faction) {
            return path.to_vec();
        }

        let fog_map = world.resource::<FogOfWarMap>();
        path.iter()
            .map(|tile| crate::modes::replay::navigation::ReplayPathTile {
                position: tile.position,
                unit_visible: tile.unit_visible
                    && fog_map.is_unit_visible(tile.position, unit_is_air),
            })
            .collect()
    }
}

/// Observer: when `CarriedBy` is added to an entity, hide it visually.
pub(crate) fn on_carried_by_add(trigger: On<Insert, CarriedBy>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(Visibility::Hidden);
}

/// Observer: when `CarriedBy` is removed from an entity, keep it hidden until
/// the projection pass decides whether the unloaded unit is actually visible.
pub(crate) fn on_carried_by_remove(trigger: On<Remove, CarriedBy>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(Visibility::Hidden);
}

/// Observer: forward `NewDay` game events to registered sinks.
pub(crate) fn on_new_day(trigger: On<NewDay>, sink: If<Res<EventSink<ExternalNewDay>>>) {
    sink.emit(ExternalNewDay { day: trigger.day });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::{ProjectedTerrainRenderState, project_terrain_render_state};
    use crate::render::TerrainAtlasResource;
    use crate::render::map::{
        AnimatedTerrain, sync_all_terrain_visuals_on_weather_change, sync_changed_terrain_visuals,
    };
    use awbrn_game::MapPosition;
    use awbrn_game::replay::{PowerVisionBoosts, ReplayState};
    use awbrn_game::world::{
        Ammo, BoardIndex, CaptureProgress, CurrentWeather, Fuel, GraphicalHp, StrongIdMap,
        TerrainHp, TerrainTile, UnitActive,
    };
    use awbrn_map::{AwbrnMap, Position};
    use awbrn_types::{
        AwbwGamePlayerId, AwbwUnitId as CoreUnitId, Faction as TerrainFaction, GraphicalTerrain,
        PlayerFaction, Property,
    };
    use awbw_replay::Hidden;
    use awbw_replay::Masked;
    use awbw_replay::turn_models::{
        Action, BuildingInfo, CaptureAction, JoinAction, MoveAction, PathTile, SupplyAction,
        TargetedPlayer, UnitProperty,
    };

    #[test]
    fn one_step_paths_use_expected_single_segment_duration() {
        use crate::modes::replay::navigation::{
            scaled_animation_duration, unit_path_segment_durations,
        };
        let durations = unit_path_segment_durations(2).expect("two-tile path should animate");
        assert_eq!(durations, vec![scaled_animation_duration(400)]);
    }

    #[test]
    fn multi_step_paths_use_expected_edge_and_interior_durations() {
        use crate::modes::replay::navigation::{
            scaled_animation_duration, unit_path_segment_durations,
        };
        let durations = unit_path_segment_durations(4).expect("four-tile path should animate");
        assert_eq!(
            durations,
            vec![
                scaled_animation_duration(280),
                scaled_animation_duration(280),
                scaled_animation_duration(280),
            ]
        );
    }

    #[test]
    fn capture_action_reapplies_capture_progress_after_move_completion() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        spawn_test_terrain(
            &mut app,
            Position::new(3, 2),
            GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
            None,
        );
        app.world_mut()
            .entity_mut(unit_entity)
            .insert(CaptureProgress::new(10).unwrap());

        ReplayTurnCommand {
            action: test_capture_action(CoreUnitId::new(1), Position::new(3, 2)),
        }
        .apply(app.world_mut());

        assert_eq!(
            app.world()
                .entity(unit_entity)
                .get::<MapPosition>()
                .unwrap()
                .position(),
            Position::new(3, 2)
        );
        assert!(
            !app.world()
                .entity(unit_entity)
                .contains::<CaptureProgress>()
        );

        let deferred_action = app
            .world_mut()
            .resource_mut::<ReplayAdvanceLock>()
            .release_for(unit_entity)
            .expect("capture action should be deferred while the move animates");
        ReplayFollowupCommand {
            action: deferred_action.action,
            recompute_fog: deferred_action.recompute_fog,
        }
        .apply(app.world_mut());

        assert_eq!(
            app.world()
                .entity(unit_entity)
                .get::<CaptureProgress>()
                .map(|progress| progress.value()),
            Some(10)
        );
    }

    #[test]
    fn moving_unit_requests_course_arrows_with_visibility_data() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));

        ReplayTurnCommand {
            action: Action::Move(MoveAction {
                unit: [(
                    TargetedPlayer::Global,
                    Hidden::Visible(test_unit_property(CoreUnitId::new(1), 4, 2)),
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
                            unit_visible: false,
                            x: 3,
                            y: 2,
                        },
                        PathTile {
                            unit_visible: true,
                            x: 4,
                            y: 2,
                        },
                    ],
                )]
                .into(),
                dist: 2,
                trapped: false,
                discovered: None,
            }),
        }
        .apply(app.world_mut());

        let pending = app
            .world()
            .entity(unit_entity)
            .get::<PendingCourseArrows>()
            .expect("move should request course arrows");

        assert_eq!(pending.path.len(), 3);
        assert_eq!(pending.path[1].position, Position::new(3, 2));
        assert!(!pending.path[1].unit_visible);
    }

    #[test]
    fn moving_unit_updates_resource_components_from_replay_payload() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
        );

        ReplayTurnCommand {
            action: Action::Move(MoveAction {
                unit: [(
                    TargetedPlayer::Global,
                    Hidden::Visible(test_unit_property_with_resources(
                        CoreUnitId::new(1),
                        2,
                        2,
                        awbrn_types::Unit::Tank,
                        37,
                        5,
                    )),
                )]
                .into(),
                paths: [(
                    TargetedPlayer::Global,
                    vec![PathTile {
                        unit_visible: true,
                        x: 2,
                        y: 2,
                    }],
                )]
                .into(),
                dist: 0,
                trapped: false,
                discovered: None,
            }),
        }
        .apply(app.world_mut());

        assert_eq!(
            app.world().entity(unit_entity).get::<Fuel>(),
            Some(&Fuel(37))
        );
        assert_eq!(
            app.world().entity(unit_entity).get::<Ammo>(),
            Some(&Ammo(5))
        );
    }

    #[test]
    fn player_targeted_paths_still_request_animation_lock() {
        assert!(
            crate::modes::replay::navigation::action_requires_path_animation(&Action::Move(
                test_player_targeted_move_action(CoreUnitId::new(1), 2, 2, &[(1, 2), (2, 2)], 1,)
            ))
        );
    }

    #[test]
    fn move_uses_path_tiles_from_the_same_targeted_view_as_unit_state() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(1, 2), CoreUnitId::new(1));

        ReplayTurnCommand {
            action: Action::Move(MoveAction {
                unit: [
                    (
                        TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(11)),
                        Hidden::Hidden,
                    ),
                    (
                        TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(10)),
                        Hidden::Visible(test_unit_property(CoreUnitId::new(1), 2, 2)),
                    ),
                ]
                .into(),
                paths: [
                    (
                        TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(11)),
                        vec![
                            PathTile {
                                unit_visible: false,
                                x: 1,
                                y: 2,
                            },
                            PathTile {
                                unit_visible: false,
                                x: 4,
                                y: 2,
                            },
                        ],
                    ),
                    (
                        TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(10)),
                        vec![
                            PathTile {
                                unit_visible: true,
                                x: 1,
                                y: 2,
                            },
                            PathTile {
                                unit_visible: true,
                                x: 2,
                                y: 2,
                            },
                        ],
                    ),
                ]
                .into(),
                dist: 1,
                trapped: false,
                discovered: None,
            }),
        }
        .apply(app.world_mut());

        assert_eq!(
            app.world().entity(unit_entity).get::<MapPosition>(),
            Some(&MapPosition::new(2, 2))
        );
        let pending = app
            .world()
            .entity(unit_entity)
            .get::<PendingCourseArrows>()
            .expect("move should request course arrows from the selected view");
        assert_eq!(pending.path.len(), 2);
        assert_eq!(pending.path[1].position, Position::new(2, 2));
        assert!(pending.path[1].unit_visible);
    }

    #[test]
    fn replay_hidden_enemy_paths_do_not_spawn_viewer_animation() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit_kind(
            &mut app,
            Position::new(1, 2),
            CoreUnitId::new(1),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
        );
        app.world_mut()
            .resource_mut::<crate::features::FogActive>()
            .0 = true;
        app.world_mut()
            .resource_mut::<crate::features::FriendlyFactions>()
            .0 = std::collections::HashSet::from([PlayerFaction::OrangeStar]);
        app.world_mut()
            .resource_mut::<crate::features::FogOfWarMap>()
            .reset(40, 40);
        app.world_mut()
            .resource_mut::<crate::features::FogOfWarMap>()
            .reveal(Position::new(2, 2));

        ReplayTurnCommand {
            action: Action::Move(MoveAction {
                unit: [(
                    TargetedPlayer::Global,
                    Hidden::Visible(test_unit_property(CoreUnitId::new(1), 2, 2)),
                )]
                .into(),
                paths: [(
                    TargetedPlayer::Global,
                    vec![
                        PathTile {
                            unit_visible: false,
                            x: 1,
                            y: 2,
                        },
                        PathTile {
                            unit_visible: false,
                            x: 2,
                            y: 2,
                        },
                    ],
                )]
                .into(),
                dist: 1,
                trapped: false,
                discovered: None,
            }),
        }
        .apply(app.world_mut());

        assert!(
            !app.world()
                .entity(unit_entity)
                .contains::<PendingCourseArrows>(),
            "all-hidden replay path masks should suppress course arrows"
        );
        assert!(
            !app.world()
                .entity(unit_entity)
                .contains::<crate::render::animation::UnitPathAnimation>(),
            "all-hidden replay path masks should suppress unit path animation"
        );
    }

    #[test]
    fn move_then_supply_uses_player_targeted_move_payloads() {
        let mut app = replay_turn_test_app();
        let supplier = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 3),
            CoreUnitId::new(1),
            awbrn_types::Unit::APC,
            PlayerFaction::OrangeStar,
        );
        let target = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 1),
            CoreUnitId::new(2),
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        app.world_mut()
            .entity_mut(target)
            .insert((Fuel(10), Ammo(1)));

        ReplayTurnCommand {
            action: Action::Supply {
                move_action: Some(test_player_targeted_move_action_with_resources(
                    test_unit_property_with_resources(
                        CoreUnitId::new(1),
                        2,
                        2,
                        awbrn_types::Unit::APC,
                        55,
                        0,
                    ),
                    &[(2, 3), (2, 2)],
                    1,
                )),
                supply_action: SupplyAction {
                    unit: [(TargetedPlayer::Global, Hidden::Visible(1))].into(),
                    rows: vec!["2".to_string()],
                    supplied: [(TargetedPlayer::Global, vec![CoreUnitId::new(2)])].into(),
                },
            },
        }
        .apply(app.world_mut());

        assert_eq!(
            app.world().entity(supplier).get::<MapPosition>(),
            Some(&MapPosition::new(2, 2))
        );
        assert!(
            app.world()
                .entity(supplier)
                .contains::<PendingCourseArrows>()
        );
        assert_eq!(app.world().entity(target).get::<Fuel>(), Some(&Fuel(10)));

        let deferred_action = app
            .world_mut()
            .resource_mut::<ReplayAdvanceLock>()
            .release_for(supplier)
            .expect("move + supply should defer the non-move action");

        ReplayFollowupCommand {
            action: deferred_action.action,
            recompute_fog: deferred_action.recompute_fog,
        }
        .apply(app.world_mut());

        assert_eq!(app.world().entity(target).get::<Fuel>(), Some(&Fuel(70)));
        assert_eq!(app.world().entity(target).get::<Ammo>(), Some(&Ammo(9)));
        assert!(!app.world().entity(supplier).contains::<UnitActive>());
    }

    #[test]
    fn move_then_supply_refills_on_followup() {
        let mut app = replay_turn_test_app();
        let supplier = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 3),
            CoreUnitId::new(1),
            awbrn_types::Unit::APC,
            PlayerFaction::OrangeStar,
        );
        let target = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 1),
            CoreUnitId::new(2),
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        app.world_mut()
            .entity_mut(target)
            .insert((Fuel(10), Ammo(1)));

        ReplayTurnCommand {
            action: Action::Supply {
                move_action: Some(MoveAction {
                    unit: [(
                        TargetedPlayer::Global,
                        Hidden::Visible(test_unit_property_with_resources(
                            CoreUnitId::new(1),
                            2,
                            2,
                            awbrn_types::Unit::APC,
                            55,
                            0,
                        )),
                    )]
                    .into(),
                    paths: [(
                        TargetedPlayer::Global,
                        vec![
                            PathTile {
                                unit_visible: true,
                                x: 2,
                                y: 3,
                            },
                            PathTile {
                                unit_visible: true,
                                x: 2,
                                y: 2,
                            },
                        ],
                    )]
                    .into(),
                    dist: 1,
                    trapped: false,
                    discovered: None,
                }),
                supply_action: SupplyAction {
                    unit: [(TargetedPlayer::Global, Hidden::Visible(1))].into(),
                    rows: vec!["2".to_string()],
                    supplied: [(TargetedPlayer::Global, vec![CoreUnitId::new(2)])].into(),
                },
            },
        }
        .apply(app.world_mut());

        assert_eq!(app.world().entity(target).get::<Fuel>(), Some(&Fuel(10)));
        let deferred_action = app
            .world_mut()
            .resource_mut::<ReplayAdvanceLock>()
            .release_for(supplier)
            .expect("move + supply should defer the non-move action");

        ReplayFollowupCommand {
            action: deferred_action.action,
            recompute_fog: deferred_action.recompute_fog,
        }
        .apply(app.world_mut());

        assert_eq!(app.world().entity(target).get::<Fuel>(), Some(&Fuel(70)));
        assert_eq!(app.world().entity(target).get::<Ammo>(), Some(&Ammo(9)));
        assert!(!app.world().entity(supplier).contains::<UnitActive>());
    }

    #[test]
    fn move_then_join_preserves_survivor_in_board_index() {
        let mut app = replay_turn_test_app();
        let destination = Position::new(2, 2);
        let source = Position::new(2, 3);
        let surviving = spawn_test_unit(&mut app, destination, CoreUnitId::new(1));
        let joining = spawn_test_unit(&mut app, source, CoreUnitId::new(2));

        let board = app.world().resource::<BoardIndex>();
        assert_eq!(board.unit_entity(destination).unwrap(), Some(surviving));
        assert_eq!(board.unit_entity(source).unwrap(), Some(joining));

        ReplayTurnCommand {
            action: Action::Join {
                move_action: Some(MoveAction {
                    unit: [(
                        TargetedPlayer::Global,
                        Hidden::Visible(test_unit_property(CoreUnitId::new(2), 2, 2)),
                    )]
                    .into(),
                    paths: [(
                        TargetedPlayer::Global,
                        vec![
                            PathTile {
                                unit_visible: true,
                                x: 2,
                                y: 3,
                            },
                            PathTile {
                                unit_visible: true,
                                x: 2,
                                y: 2,
                            },
                        ],
                    )]
                    .into(),
                    dist: 1,
                    trapped: false,
                    discovered: None,
                }),
                join_action: JoinAction {
                    player_id: 1,
                    new_funds: [(TargetedPlayer::Global, 5000)].into(),
                    unit: [(
                        TargetedPlayer::Global,
                        Hidden::Visible(test_unit_property_with_resources(
                            CoreUnitId::new(1),
                            2,
                            2,
                            awbrn_types::Unit::Infantry,
                            90,
                            0,
                        )),
                    )]
                    .into(),
                    join_id: [(TargetedPlayer::Global, Hidden::Visible(2))].into(),
                },
            },
        }
        .apply(app.world_mut());

        let board = app.world().resource::<BoardIndex>();
        assert_eq!(
            board.unit_entity(destination).unwrap(),
            Some(surviving),
            "survivor should remain indexed at the occupied join destination"
        );
        assert_eq!(
            board.unit_entity(source).unwrap(),
            Some(joining),
            "joining unit should remain indexed at its source tile until despawn"
        );

        let deferred = app
            .world_mut()
            .resource_mut::<ReplayAdvanceLock>()
            .release_for(joining)
            .expect("join action should be deferred while the move animates");
        ReplayFollowupCommand {
            action: deferred.action,
            recompute_fog: deferred.recompute_fog,
        }
        .apply(app.world_mut());

        assert!(
            app.world().get_entity(joining).is_err(),
            "joining unit should be despawned after the join resolves"
        );
        assert_eq!(
            app.world().entity(surviving).get::<MapPosition>(),
            Some(&MapPosition::from(destination))
        );

        let board = app.world().resource::<BoardIndex>();
        assert_eq!(board.unit_entity(destination).unwrap(), Some(surviving));
        assert_eq!(
            board.unit_entity(source).unwrap(),
            None,
            "source tile should be cleared once the joining unit is despawned"
        );
    }

    #[test]
    fn move_then_load_preserves_transport_in_board_index() {
        use awbw_replay::turn_models::LoadAction;

        let mut app = replay_turn_test_app();
        let transport = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_types::Unit::APC,
            PlayerFaction::OrangeStar,
        );
        let cargo = spawn_test_unit(&mut app, Position::new(2, 3), CoreUnitId::new(2));

        // Verify initial board index state
        let board = app.world().resource::<BoardIndex>();
        assert_eq!(
            board.unit_entity(Position::new(2, 2)).unwrap(),
            Some(transport)
        );
        assert_eq!(board.unit_entity(Position::new(2, 3)).unwrap(), Some(cargo));

        // Execute move-then-load: infantry at (2,3) moves to (2,2) and loads into APC
        ReplayTurnCommand {
            action: Action::Load {
                move_action: Some(MoveAction {
                    unit: [(
                        TargetedPlayer::Global,
                        Hidden::Visible(test_unit_property(CoreUnitId::new(2), 2, 2)),
                    )]
                    .into(),
                    paths: [(
                        TargetedPlayer::Global,
                        vec![
                            PathTile {
                                unit_visible: true,
                                x: 2,
                                y: 3,
                            },
                            PathTile {
                                unit_visible: true,
                                x: 2,
                                y: 2,
                            },
                        ],
                    )]
                    .into(),
                    dist: 1,
                    trapped: false,
                    discovered: None,
                }),
                load_action: LoadAction {
                    loaded: [(TargetedPlayer::Global, Hidden::Visible(CoreUnitId::new(2)))].into(),
                    transport: [(TargetedPlayer::Global, Hidden::Visible(CoreUnitId::new(1)))]
                        .into(),
                },
            },
        }
        .apply(app.world_mut());

        // Release the deferred load action (move animation was started)
        let deferred = app
            .world_mut()
            .resource_mut::<ReplayAdvanceLock>()
            .release_for(cargo)
            .expect("load action should be deferred while the move animates");
        ReplayFollowupCommand {
            action: deferred.action,
            recompute_fog: deferred.recompute_fog,
        }
        .apply(app.world_mut());

        // Verify cargo is loaded and removed from board
        assert!(app.world().entity(cargo).get::<MapPosition>().is_none());
        assert_eq!(
            app.world().entity(cargo).get::<CarriedBy>(),
            Some(&CarriedBy(transport))
        );

        // The transport must still be registered in the board index at its position
        let board = app.world().resource::<BoardIndex>();
        assert_eq!(
            board.unit_entity(Position::new(2, 2)).unwrap(),
            Some(transport),
            "Transport should still be in board index after loading cargo"
        );
        assert_eq!(
            board.unit_entity(Position::new(2, 3)).unwrap(),
            None,
            "Cargo's original position should be cleared from board index"
        );
    }

    #[test]
    fn capture_completion_replaces_terrain_tile_and_refreshes_visuals() {
        let mut app = replay_turn_test_app();
        let property_entity = spawn_test_terrain(
            &mut app,
            Position::new(2, 2),
            GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
            None,
        );
        spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));

        ReplayTurnCommand {
            action: test_stationary_capture_action(Position::new(2, 2), 20),
        }
        .apply(app.world_mut());
        app.update();

        let terrain_tile = app
            .world()
            .entity(property_entity)
            .get::<TerrainTile>()
            .unwrap();
        let sprite = app.world().entity(property_entity).get::<Sprite>().unwrap();
        let atlas = sprite.texture_atlas.as_ref().unwrap();

        assert_eq!(
            terrain_tile.terrain,
            GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
                PlayerFaction::OrangeStar,
            )))
        );
        assert_eq!(
            atlas.index,
            awbrn_content::spritesheet_index(
                app.world().resource::<CurrentWeather>().weather(),
                terrain_tile.terrain,
            )
            .index() as usize
        );
        assert!(
            app.world()
                .entity(property_entity)
                .contains::<AnimatedTerrain>()
        );
    }

    #[test]
    fn hidden_capture_preserves_last_known_building_visual_same_frame() {
        let mut app = replay_turn_test_app();
        let player_id = AwbwGamePlayerId::new(1);
        let property_entity = spawn_test_terrain(
            &mut app,
            Position::new(2, 2),
            GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
            None,
        );
        let mut registry = crate::modes::replay::fog::ReplayPlayerRegistry::default();
        registry.add_player(player_id, PlayerFaction::OrangeStar, 0);
        let terrain_knowledge = {
            let game_map = app.world().resource::<awbrn_game::world::GameMap>();
            crate::modes::replay::fog::ReplayTerrainKnowledge::from_map_and_registry(
                game_map, &registry,
            )
        };
        app.world_mut().insert_resource(registry);
        app.world_mut()
            .insert_resource(crate::modes::replay::fog::ReplayViewpoint::Player(
                player_id,
            ));
        app.world_mut().insert_resource(ReplayState {
            active_player_id: Some(player_id),
            ..ReplayState::default()
        });
        app.world_mut().insert_resource(terrain_knowledge);
        spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
        );
        app.world_mut()
            .resource_mut::<crate::features::FogActive>()
            .0 = true;
        app.world_mut()
            .resource_mut::<crate::features::FriendlyFactions>()
            .0 = std::collections::HashSet::from([PlayerFaction::OrangeStar]);

        ReplayTurnCommand {
            action: test_stationary_capture_action(Position::new(2, 2), 20),
        }
        .apply(app.world_mut());
        app.world_mut().flush();
        app.update();

        let terrain_tile = app
            .world()
            .entity(property_entity)
            .get::<TerrainTile>()
            .unwrap();
        let visual_override = app
            .world()
            .entity(property_entity)
            .get::<ProjectedTerrainRenderState>()
            .unwrap();

        assert_eq!(
            terrain_tile.terrain,
            GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
                PlayerFaction::BlueMoon,
            )))
        );
        assert_eq!(
            *visual_override,
            ProjectedTerrainRenderState(GraphicalTerrain::Property(Property::City(
                TerrainFaction::Neutral,
            )))
        );
    }

    fn replay_turn_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(BoardIndex::new(40, 40));
        app.insert_resource(StrongIdMap::<AwbwUnitId>::default());
        app.insert_resource(awbrn_game::world::GameMap::default());
        app.insert_resource(CurrentWeather::default());
        app.insert_resource(ReplayAdvanceLock::default());
        app.insert_resource(TerrainAtlasResource {
            texture: Handle::default(),
            layout: Handle::default(),
        });
        app.init_resource::<crate::features::fog::FogOfWarMap>();
        app.init_resource::<crate::features::fog::FogActive>();
        app.init_resource::<crate::features::fog::FriendlyFactions>();
        app.init_resource::<crate::modes::replay::fog::ReplayFogEnabled>();
        app.init_resource::<crate::modes::replay::fog::ReplayTerrainKnowledge>();
        app.init_resource::<crate::modes::replay::fog::ReplayViewpoint>();
        app.init_resource::<crate::modes::replay::fog::ReplayPlayerRegistry>();
        app.init_resource::<PowerVisionBoosts>();
        app.insert_resource(ReplayState::default());
        app.add_observer(crate::modes::replay::fog::on_replay_fog_dirty);
        app.add_systems(
            Update,
            (
                project_terrain_render_state,
                sync_changed_terrain_visuals,
                sync_all_terrain_visuals_on_weather_change
                    .run_if(resource_changed::<CurrentWeather>),
            )
                .chain(),
        );
        app
    }

    fn spawn_test_unit(app: &mut App, position: Position, unit_id: CoreUnitId) -> Entity {
        spawn_test_unit_kind(
            app,
            position,
            unit_id,
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
        )
    }

    fn spawn_test_unit_kind(
        app: &mut App,
        position: Position,
        unit_id: CoreUnitId,
        unit: awbrn_types::Unit,
        faction: PlayerFaction,
    ) -> Entity {
        app.world_mut()
            .spawn((
                MapPosition::from(position),
                Unit(unit),
                Faction(faction),
                AwbwUnitId(unit_id),
                GraphicalHp(10),
                Fuel(unit.max_fuel()),
                Ammo(unit.max_ammo()),
                UnitActive,
            ))
            .id()
    }

    fn spawn_test_terrain(
        app: &mut App,
        position: Position,
        terrain: GraphicalTerrain,
        terrain_hp: Option<TerrainHp>,
    ) -> Entity {
        let width = position.x + 1;
        let height = position.y + 1;
        let mut map = AwbrnMap::new(width, height, GraphicalTerrain::Plain);
        map.set_terrain(position, terrain);
        app.world_mut()
            .resource_mut::<awbrn_game::world::GameMap>()
            .set(map);

        let mut entity = app
            .world_mut()
            .spawn((MapPosition::from(position), TerrainTile { terrain }));
        if let Some(terrain_hp) = terrain_hp {
            entity.insert(terrain_hp);
        }
        entity.id()
    }

    fn test_unit_property(unit_id: CoreUnitId, x: u32, y: u32) -> UnitProperty {
        test_unit_property_with_resources(unit_id, x, y, awbrn_types::Unit::Infantry, 99, 0)
    }

    fn test_unit_property_with_resources(
        unit_id: CoreUnitId,
        x: u32,
        y: u32,
        unit_name: awbrn_types::Unit,
        fuel: u32,
        ammo: u32,
    ) -> UnitProperty {
        UnitProperty {
            units_id: unit_id,
            units_games_id: Some(1403019),
            units_players_id: 1,
            units_name: unit_name,
            units_movement_points: Some(3),
            units_vision: Some(2),
            units_fuel: Some(fuel),
            units_fuel_per_turn: Some(0),
            units_sub_dive: "N".to_string(),
            units_ammo: Some(ammo),
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
            units_cargo1_units_id: Masked::Masked,
            units_cargo2_units_id: Masked::Masked,
            units_carried: Some("N".to_string()),
            countries_code: PlayerFaction::OrangeStar,
        }
    }

    fn test_hp(value: u8) -> awbw_replay::turn_models::AwbwHpDisplay {
        serde_json::from_value(serde_json::json!(value)).unwrap()
    }

    fn test_player_targeted_move_action(
        unit_id: CoreUnitId,
        final_x: u32,
        final_y: u32,
        path: &[(u32, u32)],
        dist: u32,
    ) -> MoveAction {
        test_player_targeted_move_action_with_resources(
            test_unit_property_with_resources(
                unit_id,
                final_x,
                final_y,
                awbrn_types::Unit::Infantry,
                99,
                0,
            ),
            path,
            dist,
        )
    }

    fn test_player_targeted_move_action_with_resources(
        unit: UnitProperty,
        path: &[(u32, u32)],
        dist: u32,
    ) -> MoveAction {
        let player = TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(10));
        MoveAction {
            unit: [(player, Hidden::Visible(unit))].into(),
            paths: [(
                player,
                path.iter()
                    .map(|&(x, y)| PathTile {
                        unit_visible: true,
                        x,
                        y,
                    })
                    .collect::<Vec<_>>(),
            )]
            .into(),
            dist,
            trapped: false,
            discovered: None,
        }
    }

    fn test_capture_action(unit_id: CoreUnitId, building_position: Position) -> Action {
        Action::Capt {
            move_action: Some(MoveAction {
                unit: [(
                    TargetedPlayer::Global,
                    Hidden::Visible(test_unit_property(
                        unit_id,
                        building_position.x as u32,
                        building_position.y as u32,
                    )),
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
                            x: building_position.x as u32,
                            y: building_position.y as u32,
                        },
                    ],
                )]
                .into(),
                dist: 1,
                trapped: false,
                discovered: None,
            }),
            capture_action: CaptureAction {
                building_info: BuildingInfo {
                    buildings_capture: 10,
                    buildings_id: 99,
                    buildings_x: building_position.x as u32,
                    buildings_y: building_position.y as u32,
                    buildings_team: None,
                },
                vision: Default::default(),
                income: None,
            },
        }
    }

    fn test_stationary_capture_action(building_position: Position, capture_amount: i32) -> Action {
        Action::Capt {
            move_action: None,
            capture_action: CaptureAction {
                building_info: BuildingInfo {
                    buildings_capture: capture_amount,
                    buildings_id: 99,
                    buildings_x: building_position.x as u32,
                    buildings_y: building_position.y as u32,
                    buildings_team: None,
                },
                vision: Default::default(),
                income: None,
            },
        }
    }
}
