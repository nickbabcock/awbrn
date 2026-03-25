//! Replay command system for processing AWBW replay actions.
//!
//! Uses a custom Bevy Command to get direct `&mut World` access, enabling
//! immediate mutations that are visible to subsequent queries within the same
//! command execution.

use awbrn_core::{AwbwTerrain, GraphicalTerrain, PlayerFaction, Property};
use awbrn_map::Position;
use awbw_replay::turn_models::{
    Action, AttackSeamAction, AttackSeamCombat, CaptureAction, CombatUnit, FireAction, LoadAction,
    MoveAction, PowerAction, RepairAction, RepairedUnit, SupplyAction, TargetedPlayer, UnitMap,
    UnitProperty, UpdatedInfo,
};
use bevy::{log, prelude::*};

use crate::core::map::{TerrainHp, TerrainTile};
use crate::core::units::VisionRange;
use crate::core::{
    Ammo, BoardIndex, Capturing, CarriedBy, Faction, Fuel, GraphicalHp, MapPosition, StrongIdMap,
    Unit, UnitActive, UnitDestroyed,
};
use crate::features::event_bus::{ExternalGameEvent, GameEvent, NewDay};
use crate::features::navigation::{
    PendingCourseArrows, path_positions, replay_move_view, replay_path_tiles,
};
use crate::loading::LoadedReplay;
use crate::modes::replay::AwbwUnitId;
use crate::modes::replay::PowerVisionBoosts;
use crate::modes::replay::state::ReplayState;
use crate::render::animation::UnitPathAnimation;
use crate::render::map::TerrainVisualOverride;

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
            ReplayTurnCommand::apply_non_move_action(action, world);
        }
        if self.recompute_fog {
            world.trigger(super::fog::ReplayFogDirty);
        }
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

        Self::apply_non_move_action(&self.action, world);
        world.trigger(super::fog::ReplayFogDirty);
    }
}

impl ReplayTurnCommand {
    fn apply_move(move_action: &MoveAction, action: &Action, world: &mut World) -> bool {
        let Some((targeted_player, unit)) = replay_move_view(move_action) else {
            log::warn!("Move action missing visible targeted player unit data");
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

        Self::update_unit_resources_from_property(world, entity, unit);

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
        let path_tiles = replay_path_tiles(move_action, targeted_player);
        let unit_faction = world.entity(entity).get::<Faction>().unwrap().0;
        let unit_is_air =
            world.entity(entity).get::<Unit>().unwrap().0.domain() == awbrn_core::UnitDomain::Air;
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

        // Load actions will remove the unit from the board entirely, so skip
        // inserting MapPosition at the destination to avoid evicting the
        // transport from the BoardIndex.
        let is_load = matches!(action, Action::Load { .. });

        let mut entity_mut = world.entity_mut(entity);
        if position_changed {
            entity_mut.remove::<Capturing>();
        }

        if should_animate_for_viewer && let Some(path_animation) = animated_path {
            if is_load {
                entity_mut.insert(path_animation);
            } else {
                entity_mut.insert((path_animation, new_position));
            }
            if let Some(path) = current_view_path {
                entity_mut.insert(PendingCourseArrows { path });
            }
            entity_mut.remove::<UnitActive>();

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

        if !is_load {
            entity_mut.insert(new_position);
        }
        entity_mut.remove::<UnitActive>();

        false
    }

    fn path_tiles_for_current_view(
        path: &[crate::features::navigation::ReplayPathTile],
        world: &World,
        unit_faction: awbrn_core::PlayerFaction,
        unit_is_air: bool,
    ) -> Vec<crate::features::navigation::ReplayPathTile> {
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
            .map(|tile| crate::features::navigation::ReplayPathTile {
                position: tile.position,
                unit_visible: tile.unit_visible
                    && fog_map.is_unit_visible(tile.position, unit_is_air),
            })
            .collect()
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
            Action::Power(power_action) => Self::apply_power(power_action, world),
            Action::Repair { repair_action, .. } => Self::apply_repair(repair_action, world),
            Action::Supply { supply_action, .. } => Self::apply_supply(supply_action, world),
            Action::Move(_) => {}
            _ => log::warn!("Unhandled action: {:?}", action),
        }
    }

    fn apply_attack_seam(attack_seam_action: &AttackSeamAction, world: &mut World) {
        let attacker_entity = attack_seam_action
            .unit
            .values()
            .find_map(Self::visible_attack_seam_combat)
            .and_then(|combat_unit| Self::update_combat_unit_state(world, combat_unit));

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
                Fuel(unit.units_fuel.unwrap_or(unit.units_name.max_fuel())),
                Ammo(unit.units_ammo.unwrap_or(unit.units_name.max_ammo())),
                VisionRange(unit.units_vision.unwrap_or(1)),
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

        // Track active player for fog viewpoint
        let next_player_id = awbrn_core::AwbwGamePlayerId::new(updated_info.next_player_id);
        world.resource_mut::<ReplayState>().active_player_id = Some(next_player_id);

        if let Some(mut power_vision_boosts) = world.get_resource_mut::<PowerVisionBoosts>() {
            power_vision_boosts.0.clear();
        }

        Self::apply_end_resource_updates(updated_info, world);

        // If viewpoint is ActivePlayer, update friendly factions for the new player
        let viewpoint = world
            .resource::<crate::modes::replay::fog::ReplayViewpoint>()
            .clone();
        if matches!(
            viewpoint,
            crate::modes::replay::fog::ReplayViewpoint::ActivePlayer
        ) {
            super::fog::sync_viewpoint(world);
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
                let entity = Self::update_combat_unit_state(world, attacker_unit);
                if attacker_entity.is_none() {
                    attacker_entity = entity;
                }
            }

            if let Some(defender_unit) = combat_info.defender.get_value() {
                Self::update_combat_unit_state(world, defender_unit);
            }
        }

        if let Some(entity) = attacker_entity {
            world.entity_mut(entity).remove::<UnitActive>();
        }
    }

    fn apply_repair(repair_action: &RepairAction, world: &mut World) {
        let Some(repairing_id) =
            Self::targeted_hidden_value(&repair_action.unit).map(awbrn_core::AwbwUnitId::new)
        else {
            log::warn!("Repair action missing repairing unit ID");
            return;
        };

        let Some(repaired) = Self::targeted_value(&repair_action.repaired).cloned() else {
            log::warn!("Repair action missing repaired unit payload");
            return;
        };

        Self::apply_repaired_unit(world, &repaired);
        Self::mark_unit_inactive(world, repairing_id);
    }

    fn apply_supply(supply_action: &SupplyAction, world: &mut World) {
        let Some(supplying_id) =
            Self::targeted_hidden_value(&supply_action.unit).map(awbrn_core::AwbwUnitId::new)
        else {
            log::warn!("Supply action missing supplying unit ID");
            return;
        };

        for supplied_id in Self::targeted_vec_union(&supply_action.supplied) {
            Self::refill_unit_resources_by_id(world, supplied_id);
        }

        Self::mark_unit_inactive(world, supplying_id);
    }

    fn update_combat_unit_state(world: &mut World, combat_unit: &CombatUnit) -> Option<Entity> {
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

        world
            .entity_mut(entity)
            .insert(Ammo(combat_unit.units_ammo));

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

    fn apply_end_resource_updates(updated_info: &UpdatedInfo, world: &mut World) {
        world
            .resource_mut::<crate::features::weather::CurrentWeather>()
            .set(updated_info.next_weather.into());

        if let Some(supplied) = &updated_info.supplied {
            for supplied_id in Self::targeted_vec_union(supplied) {
                Self::refill_unit_resources_by_id(world, supplied_id);
            }
        }

        if let Some(repaired) = &updated_info.repaired {
            for repaired_unit in Self::targeted_vec_union(repaired) {
                Self::apply_repaired_unit(world, &repaired_unit);
            }
        }
    }

    fn apply_repaired_unit(world: &mut World, repaired: &RepairedUnit) {
        let unit_id = AwbwUnitId(repaired.units_id);
        let entity = {
            let units = world.resource::<StrongIdMap<AwbwUnitId>>();
            units.get(&unit_id)
        };

        let Some(entity) = entity else {
            log::warn!(
                "Repaired unit entity not found for ID: {}",
                repaired.units_id.as_u32()
            );
            return;
        };

        let hp_value = repaired.units_hit_points.value();
        world.entity_mut(entity).insert(GraphicalHp(hp_value));

        if hp_value == 0 {
            world
                .entity_mut(entity)
                .trigger(|entity| UnitDestroyed { entity });
            return;
        }

        Self::refill_unit_resources(world, entity);
    }

    fn apply_power(power_action: &PowerAction, world: &mut World) {
        if let Some(weather) = &power_action.weather {
            world
                .resource_mut::<crate::features::weather::CurrentWeather>()
                .set(weather.weather_code.into());
        }

        if let Some(global) = &power_action.global
            && global.units_vision != 0
        {
            let maybe_faction = world
                .resource::<crate::modes::replay::fog::ReplayPlayerRegistry>()
                .faction_for_player(power_action.player_id);

            let Some(faction) = maybe_faction else {
                log::warn!(
                    "Power action player {:?} missing from replay registry",
                    power_action.player_id
                );
                return;
            };

            let mut power_vision_boosts = world
                .get_resource_mut::<PowerVisionBoosts>()
                .expect("ReplayPlugin should initialize PowerVisionBoosts");
            *power_vision_boosts.0.entry(faction).or_insert(0) += global.units_vision;
        }
    }

    fn update_unit_resources_from_property(world: &mut World, entity: Entity, unit: &UnitProperty) {
        let mut entity_mut = world.entity_mut(entity);
        if let Some(fuel) = unit.units_fuel {
            entity_mut.insert(Fuel(fuel));
        }
        if let Some(ammo) = unit.units_ammo {
            entity_mut.insert(Ammo(ammo));
        }
        if let Some(vision) = unit.units_vision {
            entity_mut.insert(VisionRange(vision.max(1)));
        }
    }

    fn refill_unit_resources_by_id(world: &mut World, unit_id: awbrn_core::AwbwUnitId) {
        let entity = {
            let units = world.resource::<StrongIdMap<AwbwUnitId>>();
            units.get(&AwbwUnitId(unit_id))
        };

        let Some(entity) = entity else {
            log::warn!("Unit entity not found for ID: {}", unit_id.as_u32());
            return;
        };

        Self::refill_unit_resources(world, entity);
    }

    fn refill_unit_resources(world: &mut World, entity: Entity) {
        let Some(unit) = world.get::<Unit>(entity).copied() else {
            log::warn!(
                "Cannot refill resources for entity {:?} without Unit",
                entity
            );
            return;
        };

        world
            .entity_mut(entity)
            .insert((Fuel(unit.0.max_fuel()), Ammo(unit.0.max_ammo())));
    }

    fn mark_unit_inactive(world: &mut World, unit_id: awbrn_core::AwbwUnitId) {
        let entity = {
            let units = world.resource::<StrongIdMap<AwbwUnitId>>();
            units.get(&AwbwUnitId(unit_id))
        };

        let Some(entity) = entity else {
            log::warn!("Unit entity not found for ID: {}", unit_id.as_u32());
            return;
        };

        world.entity_mut(entity).remove::<UnitActive>();
    }

    fn targeted_hidden_value<T: Copy>(
        values: &indexmap::IndexMap<TargetedPlayer, awbw_replay::Hidden<T>>,
    ) -> Option<T> {
        values
            .get(&TargetedPlayer::Global)
            .and_then(|value: &awbw_replay::Hidden<T>| value.get_value().copied())
            .or_else(|| {
                values
                    .values()
                    .find_map(|value: &awbw_replay::Hidden<T>| value.get_value().copied())
            })
    }

    fn targeted_value<T>(values: &indexmap::IndexMap<TargetedPlayer, T>) -> Option<&T> {
        values
            .get(&TargetedPlayer::Global)
            .or_else(|| values.values().next())
    }

    fn targeted_vec_union<T: Clone + PartialEq>(
        values: &indexmap::IndexMap<TargetedPlayer, Vec<T>>,
    ) -> Vec<T> {
        let mut combined: Vec<T> = Vec::new();
        for value_list in values.values() {
            for value in value_list {
                if !combined.contains(value) {
                    combined.push(value.clone());
                }
            }
        }
        combined
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

        let (displayed_terrain, tile_hidden) = {
            let entity_ref = world.entity(terrain_entity);
            let current_terrain = entity_ref
                .get::<TerrainTile>()
                .map(|tile| tile.terrain)
                .unwrap_or(new_terrain);
            let displayed_terrain = entity_ref
                .get::<TerrainVisualOverride>()
                .and_then(|override_terrain| override_terrain.0)
                .unwrap_or(current_terrain);
            let tile_hidden = world
                .get_resource::<crate::features::FogActive>()
                .is_some_and(|fog_active| fog_active.0)
                && world
                    .get_resource::<crate::features::FogOfWarMap>()
                    .is_some_and(|fog_map| fog_map.is_fogged(pos));
            (displayed_terrain, tile_hidden)
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
        entity.insert(TerrainVisualOverride(
            (tile_hidden && displayed_terrain != new_terrain).then_some(displayed_terrain),
        ));
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

        Self::set_terrain_at(world, pos, new_terrain, None);

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
    use crate::core::{Ammo, Fuel};
    use crate::features::weather::CurrentWeather;
    use crate::loading::LoadedReplay;
    use crate::render::TerrainAtlasResource;
    use crate::render::map::TerrainVisualOverride;
    use crate::render::map::{AnimatedTerrain, on_terrain_tile_insert};
    use awbrn_core::{
        AwbwUnitId as CoreUnitId, Faction as TerrainFaction, GraphicalTerrain, PipeRubbleType,
        PipeSeamType, PlayerFaction, Property,
    };
    use awbrn_map::AwbrnMap;
    use awbw_replay::AwbwReplay;
    use awbw_replay::game_models::{AwbwPlayer, CoPower};
    use awbw_replay::turn_models::{
        Action, AttackSeamAction, AttackSeamCombat, BuildingInfo, CaptureAction, CombatInfo,
        CombatInfoVision, CombatUnit, CopValueInfo, CopValues, FireAction, GlobalStatBoost,
        MoveAction, PathTile, PowerAction, RepairAction, RepairedUnit, SupplyAction,
        TargetedPlayer, UnitProperty, UpdatedInfo, WeatherChange, WeatherCode,
    };
    use awbw_replay::{Hidden, Masked};

    #[test]
    fn one_step_paths_use_expected_single_segment_duration() {
        use crate::features::navigation::{scaled_animation_duration, unit_path_segment_durations};
        let durations = unit_path_segment_durations(2).expect("two-tile path should animate");
        assert_eq!(durations, vec![scaled_animation_duration(400)]);
    }

    #[test]
    fn multi_step_paths_use_expected_edge_and_interior_durations() {
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
            action: deferred_action.action,
            recompute_fog: deferred_action.recompute_fog,
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
    fn moving_unit_updates_resource_components_from_replay_payload() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_core::Unit::Tank,
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
                        awbrn_core::Unit::Tank,
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
        assert!(crate::features::navigation::action_requires_path_animation(
            &Action::Move(test_player_targeted_move_action(
                CoreUnitId::new(1),
                2,
                2,
                &[(1, 2), (2, 2)],
                1,
            ))
        ));
    }

    #[test]
    fn move_uses_path_tiles_from_the_same_targeted_view_as_unit_state() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(1, 2), CoreUnitId::new(1));

        ReplayTurnCommand {
            action: Action::Move(MoveAction {
                unit: [
                    (
                        TargetedPlayer::Player(awbrn_core::AwbwGamePlayerId::new(11)),
                        Hidden::Hidden,
                    ),
                    (
                        TargetedPlayer::Player(awbrn_core::AwbwGamePlayerId::new(10)),
                        Hidden::Visible(test_unit_property(CoreUnitId::new(1), 2, 2)),
                    ),
                ]
                .into(),
                paths: [
                    (
                        TargetedPlayer::Player(awbrn_core::AwbwGamePlayerId::new(11)),
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
                        TargetedPlayer::Player(awbrn_core::AwbwGamePlayerId::new(10)),
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
            awbrn_core::Unit::Infantry,
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
    fn stationary_supply_refills_supplied_units_and_inactivates_supplier() {
        let mut app = replay_turn_test_app();
        let supplier = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_core::Unit::APC,
            PlayerFaction::OrangeStar,
        );
        let target = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 1),
            CoreUnitId::new(2),
            awbrn_core::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        app.world_mut()
            .entity_mut(target)
            .insert((Fuel(10), Ammo(1)));

        ReplayTurnCommand {
            action: Action::Supply {
                move_action: None,
                supply_action: SupplyAction {
                    unit: [(TargetedPlayer::Global, Hidden::Visible(1))].into(),
                    rows: vec!["2".to_string()],
                    supplied: [
                        (
                            TargetedPlayer::Player(awbrn_core::AwbwGamePlayerId::new(10)),
                            vec![CoreUnitId::new(2)],
                        ),
                        (
                            TargetedPlayer::Player(awbrn_core::AwbwGamePlayerId::new(11)),
                            vec![],
                        ),
                    ]
                    .into(),
                },
            },
        }
        .apply(app.world_mut());

        assert_eq!(app.world().entity(target).get::<Fuel>(), Some(&Fuel(70)));
        assert_eq!(app.world().entity(target).get::<Ammo>(), Some(&Ammo(9)));
        assert!(!app.world().entity(supplier).contains::<UnitActive>());
    }

    #[test]
    fn stationary_supply_merges_global_and_player_specific_targets() {
        let mut app = replay_turn_test_app();
        let supplier = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_core::Unit::APC,
            PlayerFaction::OrangeStar,
        );
        let global_target = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 1),
            CoreUnitId::new(2),
            awbrn_core::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        let player_target = spawn_test_unit_kind(
            &mut app,
            Position::new(3, 1),
            CoreUnitId::new(3),
            awbrn_core::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        app.world_mut()
            .entity_mut(global_target)
            .insert((Fuel(10), Ammo(1)));
        app.world_mut()
            .entity_mut(player_target)
            .insert((Fuel(9), Ammo(2)));

        ReplayTurnCommand {
            action: Action::Supply {
                move_action: None,
                supply_action: SupplyAction {
                    unit: [(TargetedPlayer::Global, Hidden::Visible(1))].into(),
                    rows: vec!["2".to_string()],
                    supplied: [
                        (TargetedPlayer::Global, vec![CoreUnitId::new(2)]),
                        (
                            TargetedPlayer::Player(awbrn_core::AwbwGamePlayerId::new(10)),
                            vec![CoreUnitId::new(3)],
                        ),
                    ]
                    .into(),
                },
            },
        }
        .apply(app.world_mut());

        assert_eq!(
            app.world().entity(global_target).get::<Fuel>(),
            Some(&Fuel(70))
        );
        assert_eq!(
            app.world().entity(global_target).get::<Ammo>(),
            Some(&Ammo(9))
        );
        assert_eq!(
            app.world().entity(player_target).get::<Fuel>(),
            Some(&Fuel(70))
        );
        assert_eq!(
            app.world().entity(player_target).get::<Ammo>(),
            Some(&Ammo(9))
        );
        assert!(!app.world().entity(supplier).contains::<UnitActive>());
    }

    #[test]
    fn move_then_supply_uses_player_targeted_move_payloads() {
        let mut app = replay_turn_test_app();
        let supplier = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 3),
            CoreUnitId::new(1),
            awbrn_core::Unit::APC,
            PlayerFaction::OrangeStar,
        );
        let target = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 1),
            CoreUnitId::new(2),
            awbrn_core::Unit::Tank,
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
                        awbrn_core::Unit::APC,
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
            awbrn_core::Unit::APC,
            PlayerFaction::OrangeStar,
        );
        let target = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 1),
            CoreUnitId::new(2),
            awbrn_core::Unit::Tank,
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
                            awbrn_core::Unit::APC,
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
                                units_ammo: 4,
                                units_hit_points: Some(test_hp(8)),
                                units_id: CoreUnitId::new(1),
                                units_x: 2,
                                units_y: 2,
                            }),
                            defender: Masked::Visible(CombatUnit {
                                units_ammo: 2,
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
        assert_eq!(app.world().entity(attacker).get::<Ammo>(), Some(&Ammo(4)));
        assert!(
            !app.world().entity(attacker).contains::<UnitActive>(),
            "attacker should be marked inactive"
        );
    }

    #[test]
    fn repair_refills_resources_and_sets_repaired_hp() {
        let mut app = replay_turn_test_app();
        let repairer = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_core::Unit::APC,
            PlayerFaction::OrangeStar,
        );
        let repaired = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 1),
            CoreUnitId::new(2),
            awbrn_core::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        app.world_mut()
            .entity_mut(repaired)
            .insert((Fuel(5), Ammo(1), GraphicalHp(3)));

        ReplayTurnCommand {
            action: Action::Repair {
                move_action: None,
                repair_action: RepairAction {
                    unit: [(TargetedPlayer::Global, Hidden::Visible(1))].into(),
                    repaired: [(
                        TargetedPlayer::Player(awbrn_core::AwbwGamePlayerId::new(10)),
                        RepairedUnit {
                            units_id: CoreUnitId::new(2),
                            units_hit_points: test_hp(7),
                        },
                    )]
                    .into(),
                    funds: [(TargetedPlayer::Global, Hidden::Visible(0))].into(),
                },
            },
        }
        .apply(app.world_mut());

        assert_eq!(app.world().entity(repaired).get::<Fuel>(), Some(&Fuel(70)));
        assert_eq!(app.world().entity(repaired).get::<Ammo>(), Some(&Ammo(9)));
        assert_eq!(
            app.world().entity(repaired).get::<GraphicalHp>(),
            Some(&GraphicalHp(7))
        );
        assert!(!app.world().entity(repairer).contains::<UnitActive>());
    }

    #[test]
    fn build_spawns_units_with_vision_range() {
        let mut app = replay_turn_test_app();
        app.insert_resource(LoadedReplay(AwbwReplay {
            games: Vec::new(),
            turns: Vec::new(),
        }));

        ReplayTurnCommand {
            action: Action::Build {
                new_unit: [(
                    TargetedPlayer::Global,
                    Hidden::Visible(test_unit_property(CoreUnitId::new(7), 4, 5)),
                )]
                .into(),
                discovered: Default::default(),
            },
        }
        .apply(app.world_mut());

        let mut query = app.world_mut().query::<(&AwbwUnitId, &VisionRange)>();
        let (_, vision_range) = query
            .iter(app.world())
            .find(|(unit_id, _)| unit_id.0 == CoreUnitId::new(7))
            .expect("built unit should exist");

        assert_eq!(vision_range.0, 2);
    }

    #[test]
    fn power_action_updates_weather_and_active_player_vision() {
        let mut app = replay_turn_test_app();
        let boosted = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_core::Unit::Infantry,
            PlayerFaction::OrangeStar,
        );
        let unaffected = spawn_test_unit_kind(
            &mut app,
            Position::new(3, 2),
            CoreUnitId::new(2),
            awbrn_core::Unit::Infantry,
            PlayerFaction::BlueMoon,
        );
        app.world_mut().insert_resource(
            crate::modes::replay::fog::ReplayPlayerRegistry::from_players(
                &[AwbwPlayer {
                    id: awbrn_core::AwbwGamePlayerId::new(1),
                    users_id: awbrn_core::AwbwPlayerId::new(100),
                    games_id: awbrn_core::AwbwGameId::new(1),
                    faction: PlayerFaction::OrangeStar,
                    co_id: 0,
                    funds: 0,
                    turn: None,
                    email: None,
                    uniq_id: None,
                    eliminated: false,
                    last_read: String::new(),
                    last_read_broadcasts: None,
                    emailpress: None,
                    signature: None,
                    co_power: 0,
                    co_power_on: CoPower::None,
                    order: 1,
                    accept_draw: false,
                    co_max_power: 0,
                    co_max_spower: 0,
                    co_image: None,
                    team: "1".to_string(),
                    aet_count: 0,
                    turn_start: String::new(),
                    turn_clock: 0,
                    tags_co_id: None,
                    tags_co_power: None,
                    tags_co_max_power: None,
                    tags_co_max_spower: None,
                    interface: false,
                }],
                false,
            ),
        );
        app.world_mut().entity_mut(boosted).insert(VisionRange(2));
        app.world_mut()
            .entity_mut(unaffected)
            .insert(VisionRange(4));

        ReplayTurnCommand {
            action: Action::Power(PowerAction {
                player_id: awbrn_core::AwbwGamePlayerId::new(1),
                co_name: "Drake".to_string(),
                co_power: "Power".to_string(),
                power_name: "Typhoon".to_string(),
                players_cop: 0,
                global: Some(GlobalStatBoost {
                    units_movement_points: 0,
                    units_vision: 1,
                }),
                hp_change: None,
                unit_replace: None,
                unit_add: None,
                player_replace: None,
                missile_coords: None,
                weather: Some(WeatherChange {
                    weather_code: WeatherCode::Rain,
                    weather_name: "Rain".to_string(),
                }),
            }),
        }
        .apply(app.world_mut());

        assert_eq!(
            app.world().resource::<CurrentWeather>().weather(),
            awbrn_core::Weather::Rain
        );
        assert_eq!(
            app.world().entity(boosted).get::<VisionRange>(),
            Some(&VisionRange(2))
        );
        assert_eq!(
            app.world().entity(unaffected).get::<VisionRange>(),
            Some(&VisionRange(4))
        );
        assert_eq!(
            app.world()
                .resource::<PowerVisionBoosts>()
                .0
                .get(&PlayerFaction::OrangeStar),
            Some(&1)
        );
    }

    #[test]
    fn end_clears_power_vision_boosts_and_applies_next_weather() {
        let mut app = replay_turn_test_app();
        app.world_mut()
            .resource_mut::<PowerVisionBoosts>()
            .0
            .insert(PlayerFaction::OrangeStar, 2);
        app.world_mut()
            .resource_mut::<CurrentWeather>()
            .set(awbrn_core::Weather::Rain);

        ReplayTurnCommand::apply_end(
            &UpdatedInfo {
                event: "NextTurn".to_string(),
                next_player_id: 2,
                next_funds: [(TargetedPlayer::Global, Hidden::Visible(0))].into(),
                next_timer: 0,
                next_weather: WeatherCode::Clear,
                supplied: None,
                repaired: None,
                day: 1,
                next_turn_start: String::new(),
            },
            app.world_mut(),
        );

        assert!(
            app.world().resource::<PowerVisionBoosts>().0.is_empty(),
            "temporary power vision boosts should end with the turn"
        );
        assert_eq!(
            app.world().resource::<CurrentWeather>().weather(),
            awbrn_core::Weather::Clear
        );
    }

    #[test]
    fn end_updates_supplied_and_repaired_units_before_reactivation() {
        let mut app = replay_turn_test_app();
        app.insert_resource(ReplayState {
            next_action_index: 0,
            day: 1,
            active_player_id: None,
        });
        let supplied = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_core::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        let repaired = spawn_test_unit_kind(
            &mut app,
            Position::new(3, 2),
            CoreUnitId::new(2),
            awbrn_core::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        app.world_mut()
            .entity_mut(supplied)
            .insert((Fuel(9), Ammo(1)));
        app.world_mut()
            .entity_mut(repaired)
            .insert((Fuel(6), Ammo(2), GraphicalHp(4)));
        app.world_mut().entity_mut(supplied).remove::<UnitActive>();
        app.world_mut().entity_mut(repaired).remove::<UnitActive>();

        ReplayTurnCommand::apply_end(
            &UpdatedInfo {
                event: "NextTurn".to_string(),
                next_player_id: 1,
                next_funds: [(TargetedPlayer::Global, Hidden::Visible(0))].into(),
                next_timer: 0,
                next_weather: WeatherCode::Clear,
                supplied: Some(
                    [(
                        TargetedPlayer::Player(awbrn_core::AwbwGamePlayerId::new(10)),
                        vec![CoreUnitId::new(1)],
                    )]
                    .into(),
                ),
                repaired: Some(
                    [(
                        TargetedPlayer::Global,
                        vec![RepairedUnit {
                            units_id: CoreUnitId::new(2),
                            units_hit_points: test_hp(6),
                        }],
                    )]
                    .into(),
                ),
                day: 2,
                next_turn_start: "2025-01-01 00:00:00".to_string(),
            },
            app.world_mut(),
        );

        assert_eq!(app.world().entity(supplied).get::<Fuel>(), Some(&Fuel(70)));
        assert_eq!(app.world().entity(supplied).get::<Ammo>(), Some(&Ammo(9)));
        assert_eq!(app.world().entity(repaired).get::<Fuel>(), Some(&Fuel(70)));
        assert_eq!(app.world().entity(repaired).get::<Ammo>(), Some(&Ammo(9)));
        assert_eq!(
            app.world().entity(repaired).get::<GraphicalHp>(),
            Some(&GraphicalHp(6))
        );
        assert!(app.world().entity(supplied).contains::<UnitActive>());
        assert!(app.world().entity(repaired).contains::<UnitActive>());
        assert_eq!(app.world().resource::<ReplayState>().day, 2);
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
        app.init_resource::<crate::features::fog::FogOfWarMap>();
        app.init_resource::<crate::features::fog::FogActive>();
        app.init_resource::<crate::features::fog::FriendlyFactions>();
        app.init_resource::<crate::modes::replay::fog::ReplayFogEnabled>();
        app.init_resource::<crate::modes::replay::fog::ReplayTerrainKnowledge>();
        app.init_resource::<crate::modes::replay::fog::ReplayViewpoint>();
        app.init_resource::<crate::modes::replay::fog::ReplayPlayerRegistry>();
        app.init_resource::<PowerVisionBoosts>();
        app.insert_resource(ReplayState::default());
        app.add_observer(on_terrain_tile_insert);
        app.add_observer(crate::modes::replay::fog::on_replay_fog_dirty);
        app
    }

    fn spawn_test_unit(app: &mut App, position: Position, unit_id: CoreUnitId) -> Entity {
        spawn_test_unit_kind(
            app,
            position,
            unit_id,
            awbrn_core::Unit::Infantry,
            PlayerFaction::OrangeStar,
        )
    }

    fn spawn_test_unit_kind(
        app: &mut App,
        position: Position,
        unit_id: CoreUnitId,
        unit: awbrn_core::Unit,
        faction: PlayerFaction,
    ) -> Entity {
        app.world_mut()
            .spawn((
                MapPosition::from(position),
                Unit(unit),
                Faction(faction),
                AwbwUnitId(unit_id),
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
                awbrn_core::Unit::Infantry,
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
        let player = TargetedPlayer::Player(awbrn_core::AwbwGamePlayerId::new(10));
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
        test_unit_property_with_resources(unit_id, x, y, awbrn_core::Unit::Infantry, 99, 0)
    }

    fn test_unit_property_with_resources(
        unit_id: CoreUnitId,
        x: u32,
        y: u32,
        unit_name: awbrn_core::Unit,
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
    fn hidden_capture_preserves_last_known_building_visual_same_frame() {
        let mut app = replay_turn_test_app();
        let property_entity = spawn_test_terrain(
            &mut app,
            Position::new(2, 2),
            GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
            None,
        );
        spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_core::Unit::Infantry,
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

        let terrain_tile = app
            .world()
            .entity(property_entity)
            .get::<TerrainTile>()
            .unwrap();
        let visual_override = app
            .world()
            .entity(property_entity)
            .get::<TerrainVisualOverride>()
            .unwrap();

        assert_eq!(
            terrain_tile.terrain,
            GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
                PlayerFaction::BlueMoon,
            )))
        );
        assert_eq!(
            *visual_override,
            TerrainVisualOverride(Some(GraphicalTerrain::Property(Property::City(
                TerrainFaction::Neutral,
            ))))
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
    fn move_then_load_preserves_transport_in_board_index() {
        use awbw_replay::turn_models::LoadAction;

        let mut app = replay_turn_test_app();
        let transport = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_core::Unit::APC,
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
