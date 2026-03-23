//! Replay command system for processing AWBW replay actions.
//!
//! Uses a custom Bevy Command to get direct `&mut World` access, enabling
//! immediate mutations that are visible to subsequent queries within the same
//! command execution.

use awbrn_core::{AwbwTerrain, GraphicalTerrain, PlayerFaction, Property};
use awbrn_map::Position;
use awbw_replay::turn_models::{
    Action, AttackSeamAction, AttackSeamCombat, CaptureAction, CombatUnit, FireAction, LoadAction,
    MoveAction, TargetedPlayer, UnitMap, UpdatedInfo,
};
use bevy::{log, prelude::*};

use crate::core::map::{TerrainHp, TerrainTile};
use crate::core::{
    BoardIndex, Capturing, CarriedBy, Faction, GraphicalHp, MapPosition, StrongIdMap, Unit,
    UnitActive, UnitDestroyed,
};
use crate::features::event_bus::{ExternalGameEvent, GameEvent, NewDay};
use crate::features::navigation::{PendingCourseArrows, global_path_tiles, path_positions};
use crate::loading::LoadedReplay;
use crate::modes::replay::AwbwUnitId;
use crate::modes::replay::state::ReplayState;
use crate::render::animation::UnitPathAnimation;

#[derive(Resource, Debug, Default)]
pub struct ReplayAdvanceLock {
    active_entity: Option<Entity>,
    deferred_action: Option<Action>,
}

impl ReplayAdvanceLock {
    pub fn is_active(&self) -> bool {
        self.active_entity.is_some()
    }

    pub fn activate(&mut self, entity: Entity, deferred_action: Option<Action>) {
        self.active_entity = Some(entity);
        self.deferred_action = deferred_action;
    }

    pub fn active_entity(&self) -> Option<Entity> {
        self.active_entity
    }

    pub fn release_for(&mut self, entity: Entity) -> Option<Action> {
        if self.active_entity != Some(entity) {
            return None;
        }

        self.active_entity = None;
        self.deferred_action.take()
    }
}

pub struct ReplayFollowupCommand {
    pub action: Action,
}

impl Command for ReplayFollowupCommand {
    fn apply(self, world: &mut World) {
        ReplayTurnCommand::apply_non_move_action(&self.action, world);
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
            return;
        }

        Self::apply_non_move_action(&self.action, world);
    }
}

impl ReplayTurnCommand {
    fn apply_move(move_action: &MoveAction, action: &Action, world: &mut World) -> bool {
        let Some(unit_data) = move_action.unit.get(&TargetedPlayer::Global) else {
            log::warn!("Move action missing global targeted player unit data");
            return false;
        };

        let Some(unit) = unit_data.get_value() else {
            log::warn!("Move action global unit data is hidden");
            return false;
        };

        let Some(x) = unit.units_x else {
            return false;
        };
        let Some(y) = unit.units_y else {
            return false;
        };

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

        let new_position = MapPosition::new(x as usize, y as usize);
        let position_changed = world
            .entity(entity)
            .get::<MapPosition>()
            .map(|position| *position != new_position)
            .unwrap_or(true);
        let idle_flip_x = world
            .entity(entity)
            .get::<Sprite>()
            .map(|sprite| sprite.flip_x)
            .unwrap_or(false);
        let path_tiles = global_path_tiles(move_action);
        let animated_path = path_tiles
            .as_ref()
            .and_then(|path| UnitPathAnimation::new(path_positions(path), idle_flip_x));

        let mut entity_mut = world.entity_mut(entity);
        if position_changed {
            entity_mut.remove::<Capturing>();
        }

        if let Some(path_animation) = animated_path {
            entity_mut.insert((path_animation, new_position));
            if let Some(path) = path_tiles {
                entity_mut.insert(PendingCourseArrows { path });
            }
            entity_mut.remove::<UnitActive>();

            let deferred_action = match action {
                Action::Move(_) => None,
                _ => Some(action.clone()),
            };

            world
                .resource_mut::<ReplayAdvanceLock>()
                .activate(entity, deferred_action);

            log::info!(
                "Started path animation for unit {} across {} tiles",
                unit.units_id.as_u32(),
                move_action
                    .paths
                    .get(&TargetedPlayer::Global)
                    .map_or(0, Vec::len)
            );
            return true;
        }

        entity_mut.insert(new_position);
        entity_mut.remove::<UnitActive>();

        false
    }

    pub(crate) fn apply_non_move_action(action: &Action, world: &mut World) {
        match action {
            Action::AttackSeam {
                attack_seam_action, ..
            } => Self::apply_attack_seam(attack_seam_action, world),
            Action::Build { new_unit, .. } => Self::apply_build(new_unit, world),
            Action::Capt { capture_action, .. } => Self::apply_capture(capture_action, world),
            Action::Load { load_action, .. } => Self::apply_load(load_action, world),
            Action::Unload {
                unit, transport_id, ..
            } => Self::apply_unload(unit, *transport_id, world),
            Action::End { updated_info } => Self::apply_end(updated_info, world),
            Action::Fire { fire_action, .. } => Self::apply_fire(fire_action, world),
            Action::Move(_) => {}
            _ => log::warn!("Unhandled action: {:?}", action),
        }
    }

    fn apply_attack_seam(attack_seam_action: &AttackSeamAction, world: &mut World) {
        let attacker_entity = attack_seam_action
            .unit
            .values()
            .find_map(Self::visible_attack_seam_combat)
            .and_then(|combat_unit| Self::update_unit_hp(world, combat_unit));

        let Some(new_terrain) =
            Self::pipe_terrain_from_replay(attack_seam_action.buildings_terrain_id)
        else {
            log::warn!(
                "Unsupported AttackSeam terrain ID {} at ({}, {})",
                attack_seam_action.buildings_terrain_id,
                attack_seam_action.seam_x,
                attack_seam_action.seam_y
            );
            if let Some(entity) = attacker_entity {
                world.entity_mut(entity).remove::<UnitActive>();
            }
            return;
        };

        let terrain_hp = match new_terrain {
            GraphicalTerrain::PipeSeam(_) => u8::try_from(attack_seam_action.buildings_hit_points)
                .ok()
                .map(TerrainHp),
            GraphicalTerrain::PipeRubble(_) => None,
            _ => None,
        };

        if matches!(new_terrain, GraphicalTerrain::PipeSeam(_)) && terrain_hp.is_none() {
            log::warn!(
                "AttackSeam left seam terrain with invalid HP {} at ({}, {})",
                attack_seam_action.buildings_hit_points,
                attack_seam_action.seam_x,
                attack_seam_action.seam_y
            );
        }

        let seam_position = Position::new(
            attack_seam_action.seam_x as usize,
            attack_seam_action.seam_y as usize,
        );
        Self::set_terrain_at(world, seam_position, new_terrain, terrain_hp);

        if let Some(entity) = attacker_entity {
            world.entity_mut(entity).remove::<UnitActive>();
        }
    }

    fn apply_build(new_unit: &UnitMap, world: &mut World) {
        let players = {
            let loaded_replay = world.resource::<LoadedReplay>();
            loaded_replay
                .0
                .games
                .first()
                .map(|g| g.players.clone())
                .unwrap_or_default()
        };

        for (_player, unit_data) in new_unit.iter() {
            let Some(unit) = unit_data.get_value() else {
                continue;
            };

            let Some(x) = unit.units_x else { continue };
            let Some(y) = unit.units_y else { continue };

            let faction = players
                .iter()
                .find(|p| p.id.as_u32() == unit.units_players_id)
                .map(|p| p.faction)
                .unwrap_or(PlayerFaction::OrangeStar);

            let unit_name = format!(
                "{} - {} - {}",
                faction.country_code(),
                unit.units_name.name(),
                unit.units_id.as_u32()
            );

            world.spawn((
                Name::new(unit_name),
                MapPosition::new(x as usize, y as usize),
                Faction(faction),
                AwbwUnitId(unit.units_id),
                Unit(unit.units_name),
            ));
        }
    }

    fn apply_capture(capture_action: &CaptureAction, world: &mut World) {
        let building_pos = Position::new(
            capture_action.building_info.buildings_x as usize,
            capture_action.building_info.buildings_y as usize,
        );

        let capturing_unit = {
            match world
                .resource::<BoardIndex>()
                .unit_entity(building_pos)
                .ok()
                .flatten()
            {
                Some(entity) => world
                    .get::<Faction>(entity)
                    .map(|faction| (entity, faction.0)),
                None => None,
            }
        };

        let Some((entity, faction)) = capturing_unit else {
            log::warn!("No unit found at capture position {:?}", building_pos);
            return;
        };

        world.entity_mut(entity).remove::<UnitActive>();

        if capture_action.building_info.buildings_capture >= 20 {
            world.entity_mut(entity).remove::<Capturing>();
            Self::flip_building(world, building_pos, faction);
        } else {
            world.entity_mut(entity).insert(Capturing);
        }
    }

    fn apply_end(updated_info: &UpdatedInfo, world: &mut World) {
        let current_day = {
            let replay_state = world.resource::<ReplayState>();
            replay_state.day
        };

        if updated_info.day != current_day {
            world.resource_mut::<ReplayState>().day = updated_info.day;
            world.write_message(ExternalGameEvent {
                payload: GameEvent::NewDay(NewDay {
                    day: updated_info.day,
                }),
            });
        }

        Self::activate_all_units(world);
    }

    fn apply_load(load_action: &LoadAction, world: &mut World) {
        let loaded_unit_id = load_action
            .loaded
            .values()
            .find_map(|hidden| hidden.get_value().copied());

        let transport_unit_id = load_action
            .transport
            .values()
            .find_map(|hidden| hidden.get_value().copied());

        let Some(loaded_id_core) = loaded_unit_id else {
            log::warn!("No loaded unit ID found in load action");
            return;
        };

        let Some(transport_id_core) = transport_unit_id else {
            log::warn!("No transport unit ID found in load action");
            return;
        };

        let loaded_id = AwbwUnitId(loaded_id_core);
        let transport_id = AwbwUnitId(transport_id_core);

        let (loaded_entity, transport_entity) = {
            let units = world.resource::<StrongIdMap<AwbwUnitId>>();
            (units.get(&loaded_id), units.get(&transport_id))
        };

        let Some(loaded_entity) = loaded_entity else {
            log::warn!(
                "Loaded unit entity not found for ID: {}",
                loaded_id_core.as_u32()
            );
            return;
        };

        let Some(transport_entity) = transport_entity else {
            log::warn!(
                "Transport unit entity not found for ID: {}",
                transport_id_core.as_u32()
            );
            return;
        };

        world
            .entity_mut(loaded_entity)
            .insert((Visibility::Hidden, CarriedBy(transport_entity)))
            .remove::<MapPosition>();

        log::info!(
            "Loaded unit {} into transport {}",
            loaded_id_core.as_u32(),
            transport_id_core.as_u32()
        );
    }

    fn apply_unload(
        unit_map: &UnitMap,
        transport_id_core: awbrn_core::AwbwUnitId,
        world: &mut World,
    ) {
        let unloaded_unit = unit_map.values().find_map(|hidden| hidden.get_value());

        let Some(unit) = unloaded_unit else {
            log::warn!("No unloaded unit found in unload action");
            return;
        };

        let Some(x) = unit.units_x else {
            log::warn!("Unloaded unit has no x coordinate");
            return;
        };

        let Some(y) = unit.units_y else {
            log::warn!("Unloaded unit has no y coordinate");
            return;
        };

        let unloaded_id = AwbwUnitId(unit.units_id);

        let unloaded_entity = {
            let units = world.resource::<StrongIdMap<AwbwUnitId>>();
            units.get(&unloaded_id)
        };

        let Some(unloaded_entity) = unloaded_entity else {
            log::warn!(
                "Unloaded unit entity not found for ID: {}",
                unit.units_id.as_u32()
            );
            return;
        };

        world
            .entity_mut(unloaded_entity)
            .insert(Visibility::Inherited)
            .insert(MapPosition::new(x as usize, y as usize))
            .remove::<CarriedBy>();

        log::info!(
            "Unloaded unit {} from transport {} at ({}, {})",
            unit.units_id.as_u32(),
            transport_id_core.as_u32(),
            x,
            y
        );
    }

    fn apply_fire(fire_action: &FireAction, world: &mut World) {
        let mut attacker_entity = None;

        for (_player, combat_vision) in fire_action.combat_info_vision.iter() {
            let combat_info = &combat_vision.combat_info;

            if let Some(attacker_unit) = combat_info.attacker.get_value() {
                let entity = Self::update_unit_hp(world, attacker_unit);
                if attacker_entity.is_none() {
                    attacker_entity = entity;
                }
            }

            if let Some(defender_unit) = combat_info.defender.get_value() {
                Self::update_unit_hp(world, defender_unit);
            }
        }

        if let Some(entity) = attacker_entity {
            world.entity_mut(entity).remove::<UnitActive>();
        }
    }

    fn update_unit_hp(world: &mut World, combat_unit: &CombatUnit) -> Option<Entity> {
        let unit_id = AwbwUnitId(combat_unit.units_id);

        let entity = {
            let units = world.resource::<StrongIdMap<AwbwUnitId>>();
            units.get(&unit_id)
        };

        let Some(entity) = entity else {
            log::warn!(
                "Unit entity not found for ID: {}",
                combat_unit.units_id.as_u32()
            );
            return None;
        };

        if let Some(hp_display) = combat_unit.units_hit_points {
            let hp_value = hp_display.value();
            world.entity_mut(entity).insert(GraphicalHp(hp_value));

            if hp_value == 0 {
                world
                    .entity_mut(entity)
                    .trigger(|entity| UnitDestroyed { entity });
                return None;
            }

            log::info!(
                "Updated unit {} HP to {}",
                combat_unit.units_id.as_u32(),
                hp_value
            );
        }

        Some(entity)
    }

    fn visible_attack_seam_combat(combat: &AttackSeamCombat) -> Option<&CombatUnit> {
        combat.combat_info.get_value()
    }

    fn pipe_terrain_from_replay(buildings_terrain_id: u32) -> Option<GraphicalTerrain> {
        let terrain_id = u8::try_from(buildings_terrain_id).ok()?;
        let terrain = AwbwTerrain::try_from(terrain_id).ok()?;
        match terrain {
            AwbwTerrain::PipeSeam(pipe_seam_type) => {
                Some(GraphicalTerrain::PipeSeam(pipe_seam_type))
            }
            AwbwTerrain::PipeRubble(pipe_rubble_type) => {
                Some(GraphicalTerrain::PipeRubble(pipe_rubble_type))
            }
            _ => None,
        }
    }

    fn set_terrain_at(
        world: &mut World,
        pos: Position,
        new_terrain: GraphicalTerrain,
        terrain_hp: Option<TerrainHp>,
    ) {
        let terrain_entity = world.resource::<BoardIndex>().terrain_entity(pos).ok();

        let Some(terrain_entity) = terrain_entity else {
            log::warn!("No terrain entity found at {:?}", pos);
            return;
        };

        let map_updated = world
            .resource_mut::<crate::core::map::GameMap>()
            .set_terrain(pos, new_terrain)
            .is_some();
        if !map_updated {
            log::warn!("No GameMap tile found at {:?}", pos);
            return;
        }

        let mut entity = world.entity_mut(terrain_entity);
        entity.insert(TerrainTile {
            terrain: new_terrain,
        });
        if let Some(terrain_hp) = terrain_hp {
            entity.insert(terrain_hp);
        } else {
            entity.remove::<TerrainHp>();
        }
    }

    fn flip_building(world: &mut World, pos: Position, faction: PlayerFaction) {
        let terrain_entity = world.resource::<BoardIndex>().terrain_entity(pos).ok();

        let Some(terrain_entity) = terrain_entity else {
            return;
        };

        let entity_ref = world.entity(terrain_entity);
        let terrain_tile = entity_ref.get::<TerrainTile>().unwrap();

        let new_terrain = match terrain_tile.terrain {
            GraphicalTerrain::Property(property) => {
                let new_property = match property {
                    Property::City(_) => Property::City(awbrn_core::Faction::Player(faction)),
                    Property::Base(_) => Property::Base(awbrn_core::Faction::Player(faction)),
                    Property::Airport(_) => Property::Airport(awbrn_core::Faction::Player(faction)),
                    Property::Port(_) => Property::Port(awbrn_core::Faction::Player(faction)),
                    Property::ComTower(_) => {
                        Property::ComTower(awbrn_core::Faction::Player(faction))
                    }
                    Property::Lab(_) => Property::Lab(awbrn_core::Faction::Player(faction)),
                    Property::HQ(_) => Property::HQ(faction),
                };

                GraphicalTerrain::Property(new_property)
            }
            _ => return,
        };

        world.entity_mut(terrain_entity).insert(TerrainTile {
            terrain: new_terrain,
        });

        log::info!("Captured building at {:?} flipped to {:?}", pos, faction);
    }

    fn activate_all_units(world: &mut World) {
        let unit_entities: Vec<Entity> = {
            let mut query = world.query_filtered::<Entity, With<Unit>>();
            query.iter(world).collect()
        };

        for entity in &unit_entities {
            let Ok(mut entity_mut) = world.get_entity_mut(*entity) else {
                warn!(
                    "expected entity from query missing when setting UnitActive: {:?}",
                    entity
                );
                continue;
            };
            entity_mut.insert(UnitActive);
        }

        if !unit_entities.is_empty() {
            log::info!("Activated {} units for new turn", unit_entities.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::weather::CurrentWeather;
    use crate::render::TerrainAtlasResource;
    use crate::render::map::{AnimatedTerrain, on_terrain_tile_insert};
    use awbrn_core::{
        AwbwUnitId as CoreUnitId, Faction as TerrainFaction, GraphicalTerrain, PipeRubbleType,
        PipeSeamType, PlayerFaction, Property,
    };
    use awbrn_map::AwbrnMap;
    use awbw_replay::turn_models::{
        Action, AttackSeamAction, AttackSeamCombat, BuildingInfo, CaptureAction, CombatInfo,
        CombatInfoVision, CombatUnit, CopValueInfo, CopValues, FireAction, MoveAction, PathTile,
        TargetedPlayer, UnitProperty,
    };
    use awbw_replay::{Hidden, Masked};

    #[test]
    fn one_step_paths_use_reference_single_segment_duration() {
        use crate::features::navigation::{scaled_animation_duration, unit_path_segment_durations};
        let durations = unit_path_segment_durations(2).expect("two-tile path should animate");
        assert_eq!(durations, vec![scaled_animation_duration(400)]);
    }

    #[test]
    fn multi_step_paths_use_reference_edge_and_interior_durations() {
        use crate::features::navigation::{scaled_animation_duration, unit_path_segment_durations};
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
    fn moving_to_a_new_tile_clears_capturing() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        app.world_mut().entity_mut(unit_entity).insert(Capturing);

        ReplayTurnCommand {
            action: test_move_action(CoreUnitId::new(1), 3, 2, &[(2, 2), (3, 2)], 1),
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
        assert!(!app.world().entity(unit_entity).contains::<Capturing>());
    }

    #[test]
    fn stationary_move_preserves_capturing() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        app.world_mut().entity_mut(unit_entity).insert(Capturing);

        ReplayTurnCommand {
            action: test_move_action(CoreUnitId::new(1), 2, 2, &[(2, 2)], 0),
        }
        .apply(app.world_mut());

        assert!(app.world().entity(unit_entity).contains::<Capturing>());
    }

    #[test]
    fn capture_action_reapplies_capturing_after_move_completion() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        app.world_mut().entity_mut(unit_entity).insert(Capturing);

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
        assert!(!app.world().entity(unit_entity).contains::<Capturing>());

        let deferred_action = app
            .world_mut()
            .resource_mut::<ReplayAdvanceLock>()
            .release_for(unit_entity)
            .expect("capture action should be deferred while the move animates");
        ReplayFollowupCommand {
            action: deferred_action,
        }
        .apply(app.world_mut());

        assert!(app.world().entity(unit_entity).contains::<Capturing>());
    }

    #[test]
    fn stationary_capture_marks_unit_inactive() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));

        ReplayTurnCommand {
            action: test_stationary_capture_action(Position::new(2, 2), 10),
        }
        .apply(app.world_mut());

        assert!(app.world().entity(unit_entity).contains::<Capturing>());
        assert!(!app.world().entity(unit_entity).contains::<UnitActive>());
    }

    #[test]
    fn stationary_capture_completion_marks_unit_inactive() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        app.world_mut().entity_mut(unit_entity).insert(Capturing);

        ReplayTurnCommand {
            action: test_stationary_capture_action(Position::new(2, 2), 20),
        }
        .apply(app.world_mut());

        assert!(!app.world().entity(unit_entity).contains::<Capturing>());
        assert!(!app.world().entity(unit_entity).contains::<UnitActive>());
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
    fn fire_action_despawns_unit_at_zero_hp() {
        let mut app = replay_turn_test_app();
        app.add_observer(crate::core::units::on_unit_destroyed);
        let attacker = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        let defender = spawn_test_unit(&mut app, Position::new(3, 2), CoreUnitId::new(2));

        let fire = Action::Fire {
            move_action: None,
            fire_action: FireAction {
                combat_info_vision: [(
                    TargetedPlayer::Global,
                    CombatInfoVision {
                        has_vision: true,
                        combat_info: CombatInfo {
                            attacker: Masked::Visible(CombatUnit {
                                units_ammo: 0,
                                units_hit_points: Some(test_hp(8)),
                                units_id: CoreUnitId::new(1),
                                units_x: 2,
                                units_y: 2,
                            }),
                            defender: Masked::Visible(CombatUnit {
                                units_ammo: 0,
                                units_hit_points: Some(test_hp(0)),
                                units_id: CoreUnitId::new(2),
                                units_x: 3,
                                units_y: 2,
                            }),
                        },
                    },
                )]
                .into(),
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
        };

        ReplayTurnCommand { action: fire }.apply(app.world_mut());

        assert!(
            app.world().get_entity(defender).is_err(),
            "defender with 0 HP should be despawned"
        );
        assert_eq!(
            app.world()
                .entity(attacker)
                .get::<GraphicalHp>()
                .unwrap()
                .value(),
            8
        );
        assert!(
            !app.world().entity(attacker).contains::<UnitActive>(),
            "attacker should be marked inactive"
        );
    }

    #[test]
    fn attack_seam_updates_remaining_terrain_hp() {
        let mut app = replay_turn_test_app();
        let attacker = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        spawn_test_terrain(
            &mut app,
            Position::new(4, 2),
            GraphicalTerrain::PipeSeam(PipeSeamType::Vertical),
            Some(TerrainHp(99)),
        );

        ReplayTurnCommand {
            action: test_attack_seam_action(
                CoreUnitId::new(1),
                Position::new(4, 2),
                55,
                GraphicalTerrain::PipeSeam(PipeSeamType::Vertical),
                8,
            ),
        }
        .apply(app.world_mut());

        let terrain_entity = terrain_entity_at(&mut app, Position::new(4, 2));
        let terrain = app
            .world()
            .entity(terrain_entity)
            .get::<TerrainTile>()
            .unwrap();
        let terrain_hp = app
            .world()
            .entity(terrain_entity)
            .get::<TerrainHp>()
            .unwrap();

        assert_eq!(
            terrain.terrain,
            GraphicalTerrain::PipeSeam(PipeSeamType::Vertical)
        );
        assert_eq!(terrain_hp.value(), 55);
        assert_eq!(
            app.world()
                .resource::<crate::core::map::GameMap>()
                .terrain_at(Position::new(4, 2)),
            Some(GraphicalTerrain::PipeSeam(PipeSeamType::Vertical))
        );
        assert_eq!(
            app.world()
                .entity(attacker)
                .get::<GraphicalHp>()
                .unwrap()
                .value(),
            8
        );
        assert!(!app.world().entity(attacker).contains::<UnitActive>());
    }

    #[test]
    fn attack_seam_turns_destroyed_seam_into_rubble() {
        let mut app = replay_turn_test_app();
        spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        spawn_test_terrain(
            &mut app,
            Position::new(4, 2),
            GraphicalTerrain::PipeSeam(PipeSeamType::Vertical),
            Some(TerrainHp(3)),
        );

        ReplayTurnCommand {
            action: test_attack_seam_action(
                CoreUnitId::new(1),
                Position::new(4, 2),
                -5,
                GraphicalTerrain::PipeRubble(PipeRubbleType::Vertical),
                8,
            ),
        }
        .apply(app.world_mut());

        let terrain_entity = terrain_entity_at(&mut app, Position::new(4, 2));
        let terrain = app
            .world()
            .entity(terrain_entity)
            .get::<TerrainTile>()
            .unwrap();

        assert_eq!(
            terrain.terrain,
            GraphicalTerrain::PipeRubble(PipeRubbleType::Vertical)
        );
        assert!(
            app.world()
                .entity(terrain_entity)
                .get::<TerrainHp>()
                .is_none(),
            "destroyed seam should not retain terrain HP"
        );
        assert_eq!(
            app.world()
                .resource::<crate::core::map::GameMap>()
                .terrain_at(Position::new(4, 2)),
            Some(GraphicalTerrain::PipeRubble(PipeRubbleType::Vertical))
        );
    }

    fn replay_turn_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(BoardIndex::new(40, 40));
        app.insert_resource(StrongIdMap::<AwbwUnitId>::default());
        app.insert_resource(crate::core::map::GameMap::default());
        app.insert_resource(CurrentWeather::default());
        app.insert_resource(ReplayAdvanceLock::default());
        app.insert_resource(TerrainAtlasResource {
            texture: Handle::default(),
            layout: Handle::default(),
        });
        app.add_observer(on_terrain_tile_insert);
        app
    }

    fn spawn_test_unit(app: &mut App, position: Position, unit_id: CoreUnitId) -> Entity {
        app.world_mut()
            .spawn((
                MapPosition::from(position),
                Unit(awbrn_core::Unit::Infantry),
                Faction(PlayerFaction::OrangeStar),
                AwbwUnitId(unit_id),
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
            .resource_mut::<crate::core::map::GameMap>()
            .set(map);

        let mut entity = app
            .world_mut()
            .spawn((MapPosition::from(position), TerrainTile { terrain }));
        if let Some(terrain_hp) = terrain_hp {
            entity.insert(terrain_hp);
        }
        entity.id()
    }

    fn terrain_entity_at(app: &mut App, position: Position) -> Entity {
        let mut query = app.world_mut().query::<(Entity, &MapPosition)>();
        query
            .iter(app.world())
            .find(|(_, map_pos)| map_pos.position() == position)
            .map(|(entity, _)| entity)
            .unwrap()
    }

    fn test_move_action(
        unit_id: CoreUnitId,
        final_x: u32,
        final_y: u32,
        path: &[(u32, u32)],
        dist: u32,
    ) -> Action {
        Action::Move(MoveAction {
            unit: [(
                TargetedPlayer::Global,
                Hidden::Visible(test_unit_property(unit_id, final_x, final_y)),
            )]
            .into(),
            paths: [(
                TargetedPlayer::Global,
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
        })
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

    fn test_attack_seam_action(
        unit_id: CoreUnitId,
        seam_position: Position,
        buildings_hit_points: i32,
        terrain: GraphicalTerrain,
        unit_hp: u8,
    ) -> Action {
        let buildings_terrain_id = match terrain {
            GraphicalTerrain::PipeSeam(PipeSeamType::Horizontal) => 113,
            GraphicalTerrain::PipeSeam(PipeSeamType::Vertical) => 114,
            GraphicalTerrain::PipeRubble(PipeRubbleType::Horizontal) => 115,
            GraphicalTerrain::PipeRubble(PipeRubbleType::Vertical) => 116,
            _ => unreachable!("test only supports seam terrain variants"),
        };

        Action::AttackSeam {
            move_action: None,
            attack_seam_action: AttackSeamAction {
                unit: [(
                    TargetedPlayer::Global,
                    AttackSeamCombat {
                        has_vision: true,
                        combat_info: Masked::Visible(CombatUnit {
                            units_ammo: 0,
                            units_hit_points: Some(test_hp(unit_hp)),
                            units_id: unit_id,
                            units_x: seam_position.x as u32,
                            units_y: seam_position.y as u32,
                        }),
                    },
                )]
                .into(),
                buildings_hit_points,
                buildings_terrain_id,
                seam_x: seam_position.x as u32,
                seam_y: seam_position.y as u32,
            },
        }
    }

    fn test_unit_property(unit_id: CoreUnitId, x: u32, y: u32) -> UnitProperty {
        UnitProperty {
            units_id: unit_id,
            units_games_id: Some(1403019),
            units_players_id: 1,
            units_name: awbrn_core::Unit::Infantry,
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
            units_cargo1_units_id: Masked::Masked,
            units_cargo2_units_id: Masked::Masked,
            units_carried: Some("N".to_string()),
            countries_code: PlayerFaction::OrangeStar,
        }
    }

    fn test_hp(value: u8) -> awbw_replay::turn_models::AwbwHpDisplay {
        serde_json::from_value(serde_json::json!(value)).unwrap()
    }

    #[test]
    fn capture_completion_replaces_terrain_tile_and_refreshes_visuals() {
        let mut app = replay_turn_test_app();
        let property_entity = app
            .world_mut()
            .spawn((
                MapPosition::new(2, 2),
                TerrainTile {
                    terrain: GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
                },
            ))
            .id();
        spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));

        ReplayTurnCommand {
            action: test_stationary_capture_action(Position::new(2, 2), 20),
        }
        .apply(app.world_mut());

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
            awbrn_core::spritesheet_index(
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
    fn load_action_removes_map_position_from_carried_units() {
        use awbw_replay::turn_models::LoadAction;

        let mut app = replay_turn_test_app();
        let transport = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        let cargo = spawn_test_unit(&mut app, Position::new(2, 3), CoreUnitId::new(2));

        ReplayTurnCommand::apply_load(
            &LoadAction {
                loaded: [(TargetedPlayer::Global, Hidden::Visible(CoreUnitId::new(2)))].into(),
                transport: [(TargetedPlayer::Global, Hidden::Visible(CoreUnitId::new(1)))].into(),
            },
            app.world_mut(),
        );

        assert!(app.world().entity(cargo).get::<MapPosition>().is_none());
        assert_eq!(
            app.world().entity(cargo).get::<CarriedBy>(),
            Some(&CarriedBy(transport))
        );
    }

    #[test]
    fn unload_action_restores_map_position() {
        let mut app = replay_turn_test_app();
        let transport = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        let cargo = spawn_test_unit(&mut app, Position::new(2, 3), CoreUnitId::new(2));

        app.world_mut()
            .entity_mut(cargo)
            .insert(CarriedBy(transport))
            .insert(Visibility::Hidden)
            .remove::<MapPosition>();

        ReplayTurnCommand::apply_unload(
            &[(
                TargetedPlayer::Global,
                Hidden::Visible(test_unit_property(CoreUnitId::new(2), 4, 1)),
            )]
            .into(),
            CoreUnitId::new(1),
            app.world_mut(),
        );

        assert_eq!(
            app.world()
                .entity(cargo)
                .get::<MapPosition>()
                .unwrap()
                .position(),
            Position::new(4, 1)
        );
        assert!(app.world().entity(cargo).get::<CarriedBy>().is_none());
        assert_eq!(
            app.world().entity(cargo).get::<Visibility>(),
            Some(&Visibility::Inherited)
        );
    }
}
