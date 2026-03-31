//! Pure game-logic functions for processing AWBW replay actions.
//!
//! These functions mutate ECS world state (units, terrain, resources) without
//! any rendering or animation concerns. The client layer calls these functions
//! and adds visual follow-up where needed.

use awbrn_map::Position;
use awbrn_types::{AwbwTerrain, GraphicalTerrain, PlayerFaction, Property};
use awbw_replay::turn_models::{
    Action, AttackSeamAction, AttackSeamCombat, CaptureAction, CombatUnit, FireAction, HpEffect,
    JoinAction, LoadAction, MoveAction, NewUnit, PowerAction, RepairAction, RepairedUnit,
    SupplyAction, TargetedPlayer, UnitAddGroup, UnitChange, UnitMap, UnitProperty, UpdatedInfo,
};
use bevy::{log, prelude::*};

use crate::MapPosition;
use crate::replay::{AwbwUnitId, PowerVisionBoosts, ReplayPlayerRegistry, ReplayState};
use crate::world::{
    Ammo, BoardIndex, Capturing, CarriedBy, Faction, Fuel, GameMap, GraphicalHp, Hiding,
    StrongIdMap, TerrainHp, TerrainTile, Unit, UnitActive, UnitDestroyed, VisionRange,
};

/// Event triggered when a new day begins during replay playback.
#[derive(Event, Debug, Clone)]
pub struct NewDay {
    pub day: u32,
}

/// The outcome of applying a move state update.
pub struct MoveOutcome {
    pub entity: Entity,
    pub new_position: MapPosition,
    pub position_changed: bool,
}

/// Returns the first visible unit property from a move action, along with
/// which targeted-player key produced it.
pub fn replay_move_view(move_action: &MoveAction) -> Option<(TargetedPlayer, &UnitProperty)> {
    move_action
        .unit
        .get(&TargetedPlayer::Global)
        .and_then(awbw_replay::Hidden::get_value)
        .map(|unit| (TargetedPlayer::Global, unit))
        .or_else(|| {
            move_action.unit.iter().find_map(|(targeted_player, unit)| {
                unit.get_value().map(|unit| (*targeted_player, unit))
            })
        })
}

/// Apply pure game-state changes for a move action. Returns `Some(MoveOutcome)`
/// on success so the caller can set up animation. Does NOT insert `MapPosition`
/// at the destination (the caller decides when/whether to do that).
pub fn apply_move_state(move_action: &MoveAction, world: &mut World) -> Option<MoveOutcome> {
    let (_, unit) = replay_move_view(move_action)?;

    let x = unit.units_x?;
    let y = unit.units_y?;

    let entity = {
        let units = world.resource::<StrongIdMap<AwbwUnitId>>();
        units.get(&AwbwUnitId(unit.units_id))
    };

    let entity = entity.or_else(|| {
        log::warn!(
            "Unit with ID {} not found in unit storage",
            unit.units_id.as_u32()
        );
        None
    })?;

    update_unit_resources_from_property(world, entity, unit);

    let new_position = MapPosition::new(x as usize, y as usize);
    let position_changed = world
        .entity(entity)
        .get::<MapPosition>()
        .map(|position| *position != new_position)
        .unwrap_or(true);

    if position_changed {
        world.entity_mut(entity).remove::<Capturing>();
    }

    world.entity_mut(entity).remove::<UnitActive>();

    Some(MoveOutcome {
        entity,
        new_position,
        position_changed,
    })
}

/// Apply a non-move action to the world, mutating game state directly.
pub fn apply_non_move_action(action: &Action, world: &mut World) {
    match action {
        Action::AttackSeam {
            attack_seam_action, ..
        } => apply_attack_seam(attack_seam_action, world),
        Action::Build { new_unit, .. } => apply_build(new_unit, world),
        Action::Capt { capture_action, .. } => apply_capture(capture_action, world),
        Action::Load { load_action, .. } => apply_load(load_action, world),
        Action::Unload {
            unit, transport_id, ..
        } => apply_unload(unit, *transport_id, world),
        Action::End { updated_info } => apply_end(updated_info, world),
        Action::Fire { fire_action, .. } => apply_fire(fire_action, world),
        Action::Power(power_action) => apply_power(power_action, world),
        Action::Repair { repair_action, .. } => apply_repair(repair_action, world),
        Action::Resign {
            next_turn_action: Some(next_turn_action),
            ..
        } => apply_end(&UpdatedInfo::from(next_turn_action.clone()), world),
        Action::Resign { .. } => {}
        Action::Supply { supply_action, .. } => apply_supply(supply_action, world),
        Action::Tag { updated_info } => apply_end(updated_info, world),
        Action::Join { join_action, .. } => apply_join(join_action, world),
        Action::Hide { move_action } => apply_hide(move_action.as_ref(), world),
        Action::Unhide { move_action } => apply_unhide(move_action.as_ref(), world),
        Action::Move(_) => {}
        _ => log::warn!("Unhandled action: {:?}", action),
    }
}

pub fn apply_attack_seam(attack_seam_action: &AttackSeamAction, world: &mut World) {
    let attacker_entity = attack_seam_action
        .unit
        .values()
        .find_map(visible_attack_seam_combat)
        .and_then(|combat_unit| update_combat_unit_state(world, combat_unit));

    let Some(new_terrain) = pipe_terrain_from_replay(attack_seam_action.buildings_terrain_id)
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
    set_terrain_at(world, seam_position, new_terrain, terrain_hp);

    if let Some(entity) = attacker_entity {
        world.entity_mut(entity).remove::<UnitActive>();
    }
}

pub fn apply_build(new_unit: &UnitMap, world: &mut World) {
    // Collect units to spawn (avoids holding borrow on registry while mutating world)
    let units_to_spawn: Vec<_> = new_unit
        .iter()
        .filter_map(|(_player, unit_data)| {
            let unit = unit_data.get_value()?;
            let x = unit.units_x?;
            let y = unit.units_y?;
            let faction = world
                .get_resource::<ReplayPlayerRegistry>()
                .and_then(|r| {
                    r.faction_for_player(awbrn_types::AwbwGamePlayerId::new(unit.units_players_id))
                })
                .unwrap_or(PlayerFaction::OrangeStar);
            Some((unit.clone(), x, y, faction))
        })
        .collect();

    for (unit, x, y, faction) in units_to_spawn {
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
            GraphicalHp(unit.units_hit_points.value()),
            VisionRange(unit.units_vision.unwrap_or(unit.units_name.base_vision())),
        ));
    }
}

pub fn apply_capture(capture_action: &CaptureAction, world: &mut World) {
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
        flip_building(world, building_pos, faction);
    } else {
        world.entity_mut(entity).insert(Capturing);
    }
}

pub fn apply_end(updated_info: &UpdatedInfo, world: &mut World) {
    let current_day = {
        let replay_state = world.resource::<ReplayState>();
        replay_state.day
    };

    if updated_info.day != current_day {
        world.resource_mut::<ReplayState>().day = updated_info.day;
        world.trigger(NewDay {
            day: updated_info.day,
        });
    }

    // Track active player for fog viewpoint
    let next_player_id = awbrn_types::AwbwGamePlayerId::new(updated_info.next_player_id);
    world.resource_mut::<ReplayState>().active_player_id = Some(next_player_id);

    if let Some(mut power_vision_boosts) = world.get_resource_mut::<PowerVisionBoosts>() {
        power_vision_boosts.0.clear();
    }

    apply_end_resource_updates(updated_info, world);

    activate_all_units(world);
}

pub fn apply_load(load_action: &LoadAction, world: &mut World) {
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

    // Visibility is managed by observers on CarriedBy in the client layer.
    world
        .entity_mut(loaded_entity)
        .insert(CarriedBy(transport_entity))
        .remove::<MapPosition>();

    log::info!(
        "Loaded unit {} into transport {}",
        loaded_id_core.as_u32(),
        transport_id_core.as_u32()
    );
}

pub fn apply_unload(
    unit_map: &UnitMap,
    transport_id_core: awbrn_types::AwbwUnitId,
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

    // Visibility is managed by observers on CarriedBy removal in the client layer.
    world
        .entity_mut(unloaded_entity)
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

pub fn apply_fire(fire_action: &FireAction, world: &mut World) {
    let mut attacker_entity = None;

    for (_player, combat_vision) in fire_action.combat_info_vision.iter() {
        let combat_info = &combat_vision.combat_info;

        if let Some(attacker_unit) = combat_info.attacker.get_value() {
            let entity = update_combat_unit_state(world, attacker_unit);
            if attacker_entity.is_none() {
                attacker_entity = entity;
            }
        }

        if let Some(defender_unit) = combat_info.defender.get_value() {
            update_combat_unit_state(world, defender_unit);
        }
    }

    if let Some(entity) = attacker_entity {
        world.entity_mut(entity).remove::<UnitActive>();
    }
}

pub fn apply_power(power_action: &PowerAction, world: &mut World) {
    if let Some(weather) = &power_action.weather {
        world
            .resource_mut::<crate::world::CurrentWeather>()
            .set(weather.weather_code.into());
    }

    if let Some(global) = &power_action.global
        && global.units_vision != 0
    {
        let maybe_faction = world
            .resource::<ReplayPlayerRegistry>()
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
            .expect("GamePlugin should initialize PowerVisionBoosts");
        *power_vision_boosts.0.entry(faction).or_insert(0) += global.units_vision;
    }

    if let Some(hp_change) = &power_action.hp_change {
        if let Some(hp_gain) = &hp_change.hp_gain {
            apply_power_hp_effect(world, hp_gain);
        }
        if let Some(hp_loss) = &hp_change.hp_loss {
            apply_power_hp_effect(world, hp_loss);
        }
    }

    if let Some(unit_replace) = &power_action.unit_replace {
        for group in unit_replace.values() {
            if let Some(units) = &group.units {
                for change in units {
                    apply_power_unit_change(world, change);
                }
            }
        }
    }

    if let Some(unit_add) = &power_action.unit_add {
        let mut seen = std::collections::HashSet::new();
        for group in unit_add.values() {
            for unit in &group.units {
                if seen.insert(unit.units_id) {
                    apply_power_unit_add(world, group, unit);
                }
            }
        }
    }
}

pub fn apply_repair(repair_action: &RepairAction, world: &mut World) {
    let Some(repairing_id) =
        targeted_hidden_value(&repair_action.unit).map(awbrn_types::AwbwUnitId::new)
    else {
        log::warn!("Repair action missing repairing unit ID");
        return;
    };

    let Some(repaired) = targeted_value(&repair_action.repaired).cloned() else {
        log::warn!("Repair action missing repaired unit payload");
        return;
    };

    apply_repaired_unit(world, &repaired);
    mark_unit_inactive(world, repairing_id);
}

fn apply_power_hp_effect(world: &mut World, effect: &HpEffect) {
    if effect.hp == 0 && (effect.units_fuel - 1.0).abs() < f64::EPSILON || effect.players.is_empty()
    {
        return;
    }

    let faction_set = effect
        .players
        .iter()
        .filter_map(|player_id| {
            world
                .resource::<ReplayPlayerRegistry>()
                .faction_for_player(*player_id)
        })
        .collect::<std::collections::HashSet<_>>();
    let unit_entities: Vec<Entity> = {
        let mut query = world.query_filtered::<(Entity, &Faction), With<Unit>>();
        query
            .iter(world)
            .filter_map(|(entity, faction)| faction_set.contains(&faction.0).then_some(entity))
            .collect()
    };

    for entity in unit_entities {
        let current_hp = world
            .get::<GraphicalHp>(entity)
            .map(|hp| i32::from(hp.value()))
            .unwrap_or(10);

        let (max_fuel, current_fuel) = {
            let entity_ref = world.entity(entity);
            let unit = entity_ref
                .get::<Unit>()
                .expect("power effects should only target units")
                .0;
            let max_fuel = unit.max_fuel();
            let current_fuel = entity_ref.get::<Fuel>().map_or(max_fuel, Fuel::value);
            (max_fuel, current_fuel)
        };

        let next_hp = (current_hp + effect.hp).clamp(1, 10) as u8;
        let next_fuel = ((current_fuel as f64) * effect.units_fuel).ceil() as u32;
        world
            .entity_mut(entity)
            .insert((GraphicalHp(next_hp), Fuel(next_fuel.clamp(0, max_fuel))));
    }
}

fn apply_power_unit_change(world: &mut World, change: &UnitChange) {
    let entity = {
        let units = world.resource::<StrongIdMap<AwbwUnitId>>();
        units.get(&AwbwUnitId(change.units_id))
    };

    let Some(entity) = entity else {
        log::warn!(
            "Power unit replacement target not found for ID: {}",
            change.units_id.as_u32()
        );
        return;
    };

    let mut entity_mut = world.entity_mut(entity);
    if let Some(hp) = change.units_hit_points {
        let hp_value = hp.value().max(1);
        entity_mut.insert(GraphicalHp(hp_value));
    }
    if let Some(ammo) = change.units_ammo {
        entity_mut.insert(Ammo(ammo));
    }
    if let Some(fuel) = change.units_fuel {
        entity_mut.insert(Fuel(fuel));
    }
    if let Some(moved) = change.units_moved {
        if moved < 0 {
            entity_mut.remove::<UnitActive>();
        } else if moved == 0 {
            entity_mut.insert(UnitActive);
        }
    }
}

fn apply_power_unit_add(world: &mut World, group: &UnitAddGroup, unit: &NewUnit) {
    let faction = world
        .resource::<ReplayPlayerRegistry>()
        .faction_for_player(group.player_id)
        .unwrap_or(PlayerFaction::OrangeStar);
    let unit_name = format!(
        "{} - {} - {}",
        faction.country_code(),
        group.unit_name.name(),
        unit.units_id.as_u32()
    );
    world.spawn((
        Name::new(unit_name),
        MapPosition::new(unit.units_x as usize, unit.units_y as usize),
        Faction(faction),
        AwbwUnitId(unit.units_id),
        Unit(group.unit_name),
        Fuel(group.unit_name.max_fuel()),
        Ammo(group.unit_name.max_ammo()),
        GraphicalHp(10),
        VisionRange(group.unit_name.base_vision()),
    ));
}

pub fn apply_supply(supply_action: &SupplyAction, world: &mut World) {
    let Some(supplying_id) =
        targeted_hidden_value(&supply_action.unit).map(awbrn_types::AwbwUnitId::new)
    else {
        log::warn!("Supply action missing supplying unit ID");
        return;
    };

    for supplied_id in targeted_vec_union(&supply_action.supplied) {
        refill_unit_resources_by_id(world, supplied_id);
    }

    mark_unit_inactive(world, supplying_id);
}

pub fn apply_join(join_action: &JoinAction, world: &mut World) {
    let surviving_unit = join_action.unit.values().find_map(|h| h.get_value());

    let Some(unit) = surviving_unit else {
        log::warn!("Join action missing surviving unit data");
        return;
    };

    let surviving_id = AwbwUnitId(unit.units_id);
    let surviving_entity = {
        let units = world.resource::<StrongIdMap<AwbwUnitId>>();
        units.get(&surviving_id)
    };

    let Some(surviving_entity) = surviving_entity else {
        log::warn!(
            "Surviving unit entity not found for ID: {}",
            unit.units_id.as_u32()
        );
        return;
    };

    let hp_value = unit.units_hit_points.value();
    world
        .entity_mut(surviving_entity)
        .insert(GraphicalHp(hp_value));
    update_unit_resources_from_property(world, surviving_entity, unit);

    let Some(joining_id) =
        targeted_hidden_value(&join_action.join_id).map(awbrn_types::AwbwUnitId::new)
    else {
        log::warn!("Join action missing joining unit ID");
        world.entity_mut(surviving_entity).remove::<UnitActive>();
        return;
    };

    let joining_entity = {
        let units = world.resource::<StrongIdMap<AwbwUnitId>>();
        units.get(&AwbwUnitId(joining_id))
    };

    if let Some(joining_entity) = joining_entity {
        world.despawn(joining_entity);
        log::info!(
            "Unit {} joined into unit {} (HP: {})",
            joining_id.as_u32(),
            unit.units_id.as_u32(),
            hp_value,
        );
    } else {
        log::warn!(
            "Joining unit entity not found for ID: {}",
            joining_id.as_u32()
        );
    }

    world.entity_mut(surviving_entity).remove::<UnitActive>();
}

pub fn apply_hide(move_action: Option<&MoveAction>, world: &mut World) {
    let Some(mov) = move_action else {
        log::warn!("Hide action missing move data");
        return;
    };

    let Some((_, unit)) = replay_move_view(mov) else {
        log::warn!("Hide action missing visible unit data");
        return;
    };

    let entity = {
        let units = world.resource::<StrongIdMap<AwbwUnitId>>();
        units.get(&AwbwUnitId(unit.units_id))
    };

    if let Some(entity) = entity {
        world.entity_mut(entity).insert(Hiding);
        log::info!("Unit {} is now hiding", unit.units_id.as_u32());
    } else {
        log::warn!(
            "Hide unit entity not found for ID: {}",
            unit.units_id.as_u32()
        );
    }
}

pub fn apply_unhide(move_action: Option<&MoveAction>, world: &mut World) {
    let Some(mov) = move_action else {
        log::warn!("Unhide action missing move data");
        return;
    };

    let Some((_, unit)) = replay_move_view(mov) else {
        log::warn!("Unhide action missing visible unit data");
        return;
    };

    let entity = {
        let units = world.resource::<StrongIdMap<AwbwUnitId>>();
        units.get(&AwbwUnitId(unit.units_id))
    };

    if let Some(entity) = entity {
        world.entity_mut(entity).remove::<Hiding>();
        log::info!("Unit {} is no longer hiding", unit.units_id.as_u32());
    } else {
        log::warn!(
            "Unhide unit entity not found for ID: {}",
            unit.units_id.as_u32()
        );
    }
}

pub fn update_combat_unit_state(world: &mut World, combat_unit: &CombatUnit) -> Option<Entity> {
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

pub fn update_unit_resources_from_property(world: &mut World, entity: Entity, unit: &UnitProperty) {
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

pub fn refill_unit_resources(world: &mut World, entity: Entity) {
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

pub fn refill_unit_resources_by_id(world: &mut World, unit_id: awbrn_types::AwbwUnitId) {
    let entity = {
        let units = world.resource::<StrongIdMap<AwbwUnitId>>();
        units.get(&AwbwUnitId(unit_id))
    };

    let Some(entity) = entity else {
        log::warn!("Unit entity not found for ID: {}", unit_id.as_u32());
        return;
    };

    refill_unit_resources(world, entity);
}

pub fn mark_unit_inactive(world: &mut World, unit_id: awbrn_types::AwbwUnitId) {
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

pub fn apply_repaired_unit(world: &mut World, repaired: &RepairedUnit) {
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

    refill_unit_resources(world, entity);
}

pub fn activate_all_units(world: &mut World) {
    let unit_entities: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<Unit>>();
        query.iter(world).collect()
    };

    for entity in &unit_entities {
        let Ok(mut entity_mut) = world.get_entity_mut(*entity) else {
            log::warn!(
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

/// Update `TerrainTile`, `GameMap`, and `TerrainHp` for the entity at `pos`.
/// No visual override logic here — that lives in the client layer.
pub fn set_terrain_at(
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
        .resource_mut::<GameMap>()
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

pub fn flip_building(world: &mut World, pos: Position, faction: PlayerFaction) {
    let terrain_entity = world.resource::<BoardIndex>().terrain_entity(pos).ok();

    let Some(terrain_entity) = terrain_entity else {
        return;
    };

    let entity_ref = world.entity(terrain_entity);
    let terrain_tile = entity_ref.get::<TerrainTile>().unwrap();

    let new_terrain = match terrain_tile.terrain {
        GraphicalTerrain::Property(property) => {
            let new_property = match property {
                Property::City(_) => Property::City(awbrn_types::Faction::Player(faction)),
                Property::Base(_) => Property::Base(awbrn_types::Faction::Player(faction)),
                Property::Airport(_) => Property::Airport(awbrn_types::Faction::Player(faction)),
                Property::Port(_) => Property::Port(awbrn_types::Faction::Player(faction)),
                Property::ComTower(_) => Property::ComTower(awbrn_types::Faction::Player(faction)),
                Property::Lab(_) => Property::Lab(awbrn_types::Faction::Player(faction)),
                Property::HQ(_) => Property::HQ(faction),
            };

            GraphicalTerrain::Property(new_property)
        }
        _ => return,
    };

    set_terrain_at(world, pos, new_terrain, None);

    log::info!("Captured building at {:?} flipped to {:?}", pos, faction);
}

pub fn targeted_hidden_value<T: Copy>(
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

pub fn targeted_value<T>(values: &indexmap::IndexMap<TargetedPlayer, T>) -> Option<&T> {
    values
        .get(&TargetedPlayer::Global)
        .or_else(|| values.values().next())
}

pub fn targeted_vec_union<T: Clone + PartialEq>(
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

pub fn visible_attack_seam_combat(combat: &AttackSeamCombat) -> Option<&CombatUnit> {
    combat.combat_info.get_value()
}

pub fn pipe_terrain_from_replay(buildings_terrain_id: u32) -> Option<GraphicalTerrain> {
    let terrain_id = u8::try_from(buildings_terrain_id).ok()?;
    let terrain = AwbwTerrain::try_from(terrain_id).ok()?;
    match terrain {
        AwbwTerrain::PipeSeam(pipe_seam_type) => Some(GraphicalTerrain::PipeSeam(pipe_seam_type)),
        AwbwTerrain::PipeRubble(pipe_rubble_type) => {
            Some(GraphicalTerrain::PipeRubble(pipe_rubble_type))
        }
        _ => None,
    }
}

fn apply_end_resource_updates(updated_info: &UpdatedInfo, world: &mut World) {
    world
        .resource_mut::<crate::world::CurrentWeather>()
        .set(updated_info.next_weather.into());

    if let Some(supplied) = &updated_info.supplied {
        for supplied_id in targeted_vec_union(supplied) {
            refill_unit_resources_by_id(world, supplied_id);
        }
    }

    if let Some(repaired) = &updated_info.repaired {
        for repaired_unit in targeted_vec_union(repaired) {
            apply_repaired_unit(world, &repaired_unit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::AwbrnMap;
    use awbrn_types::{
        AwbwUnitId as CoreUnitId, GraphicalTerrain, PipeRubbleType, PipeSeamType, PlayerFaction,
    };
    use awbw_replay::turn_models::{
        AttackSeamAction, AttackSeamCombat, BuildingInfo, CaptureAction, CombatInfo,
        CombatInfoVision, CombatUnit, CopValueInfo, CopValues, FireAction, GlobalStatBoost,
        HpChange, HpEffect, JoinAction, LoadAction, PathTile, PowerAction, RepairAction,
        RepairedUnit, SupplyAction, TargetedPlayer, UnitProperty, UpdatedInfo, WeatherChange,
        WeatherCode,
    };
    use awbw_replay::{Hidden, Masked};

    use crate::world::CurrentWeather;

    fn replay_turn_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(BoardIndex::new(40, 40));
        app.insert_resource(StrongIdMap::<AwbwUnitId>::default());
        app.insert_resource(crate::world::GameMap::default());
        app.insert_resource(CurrentWeather::default());
        app.init_resource::<crate::world::FogOfWarMap>();
        app.init_resource::<crate::world::FogActive>();
        app.init_resource::<crate::world::FriendlyFactions>();
        app.init_resource::<crate::replay::ReplayFogEnabled>();
        app.init_resource::<crate::replay::ReplayTerrainKnowledge>();
        app.init_resource::<crate::replay::ReplayViewpoint>();
        app.init_resource::<crate::replay::ReplayPlayerRegistry>();
        app.init_resource::<PowerVisionBoosts>();
        app.insert_resource(ReplayState::default());
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
            .resource_mut::<crate::world::GameMap>()
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

    #[test]
    fn stationary_supply_refills_supplied_units_and_inactivates_supplier() {
        let mut app = replay_turn_test_app();
        let supplier = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
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

        apply_non_move_action(
            &Action::Supply {
                move_action: None,
                supply_action: SupplyAction {
                    unit: [(TargetedPlayer::Global, Hidden::Visible(1))].into(),
                    rows: vec!["2".to_string()],
                    supplied: [
                        (
                            TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(10)),
                            vec![CoreUnitId::new(2)],
                        ),
                        (
                            TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(11)),
                            vec![],
                        ),
                    ]
                    .into(),
                },
            },
            app.world_mut(),
        );

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
            awbrn_types::Unit::APC,
            PlayerFaction::OrangeStar,
        );
        let global_target = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 1),
            CoreUnitId::new(2),
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        let player_target = spawn_test_unit_kind(
            &mut app,
            Position::new(3, 1),
            CoreUnitId::new(3),
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        app.world_mut()
            .entity_mut(global_target)
            .insert((Fuel(10), Ammo(1)));
        app.world_mut()
            .entity_mut(player_target)
            .insert((Fuel(9), Ammo(2)));

        apply_non_move_action(
            &Action::Supply {
                move_action: None,
                supply_action: SupplyAction {
                    unit: [(TargetedPlayer::Global, Hidden::Visible(1))].into(),
                    rows: vec!["2".to_string()],
                    supplied: [
                        (TargetedPlayer::Global, vec![CoreUnitId::new(2)]),
                        (
                            TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(10)),
                            vec![CoreUnitId::new(3)],
                        ),
                    ]
                    .into(),
                },
            },
            app.world_mut(),
        );

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
    fn fire_action_despawns_unit_at_zero_hp() {
        let mut app = replay_turn_test_app();
        app.add_observer(crate::world::units::on_unit_destroyed);
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
                        player_id: awbrn_types::AwbwGamePlayerId::new(1),
                        cop_value: 0,
                        tag_value: None,
                    },
                    defender: CopValueInfo {
                        player_id: awbrn_types::AwbwGamePlayerId::new(2),
                        cop_value: 0,
                        tag_value: None,
                    },
                },
            },
        };

        apply_non_move_action(&fire, app.world_mut());

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
            awbrn_types::Unit::APC,
            PlayerFaction::OrangeStar,
        );
        let repaired = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 1),
            CoreUnitId::new(2),
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        app.world_mut()
            .entity_mut(repaired)
            .insert((Fuel(5), Ammo(1), GraphicalHp(3)));

        apply_non_move_action(
            &Action::Repair {
                move_action: None,
                repair_action: RepairAction {
                    unit: [(TargetedPlayer::Global, Hidden::Visible(1))].into(),
                    repaired: [(
                        TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(10)),
                        RepairedUnit {
                            units_id: CoreUnitId::new(2),
                            units_hit_points: test_hp(7),
                        },
                    )]
                    .into(),
                    funds: [(TargetedPlayer::Global, Hidden::Visible(0))].into(),
                },
            },
            app.world_mut(),
        );

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

        apply_non_move_action(
            &Action::Build {
                new_unit: [(
                    TargetedPlayer::Global,
                    Hidden::Visible(test_unit_property(CoreUnitId::new(7), 4, 5)),
                )]
                .into(),
                discovered: Default::default(),
            },
            app.world_mut(),
        );

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
            awbrn_types::Unit::Infantry,
            PlayerFaction::OrangeStar,
        );
        let unaffected = spawn_test_unit_kind(
            &mut app,
            Position::new(3, 2),
            CoreUnitId::new(2),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
        );
        {
            let mut registry = crate::replay::ReplayPlayerRegistry::default();
            registry.add_player(
                awbrn_types::AwbwGamePlayerId::new(1),
                PlayerFaction::OrangeStar,
                0,
            );
            app.world_mut().insert_resource(registry);
        }
        app.world_mut().entity_mut(boosted).insert(VisionRange(2));
        app.world_mut()
            .entity_mut(unaffected)
            .insert(VisionRange(4));

        apply_non_move_action(
            &Action::Power(PowerAction {
                player_id: awbrn_types::AwbwGamePlayerId::new(1),
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
            app.world_mut(),
        );

        assert_eq!(
            app.world().resource::<CurrentWeather>().weather(),
            awbrn_types::Weather::Rain
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
    fn power_hp_loss_floors_units_at_one_hp_and_updates_fuel() {
        let mut app = replay_turn_test_app();
        let victim = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_types::Unit::Tank,
            PlayerFaction::BlueMoon,
        );
        let mut registry = crate::replay::ReplayPlayerRegistry::default();
        registry.add_player(
            awbrn_types::AwbwGamePlayerId::new(2),
            PlayerFaction::BlueMoon,
            0,
        );
        app.world_mut().insert_resource(registry);
        app.world_mut()
            .entity_mut(victim)
            .insert((GraphicalHp(1), Fuel(10)));

        apply_non_move_action(
            &Action::Power(PowerAction {
                player_id: awbrn_types::AwbwGamePlayerId::new(1),
                co_name: "Hawke".to_string(),
                co_power: "Power".to_string(),
                power_name: "Black Wave".to_string(),
                players_cop: 0,
                global: None,
                hp_change: Some(HpChange {
                    hp_gain: None,
                    hp_loss: Some(HpEffect {
                        players: vec![awbrn_types::AwbwGamePlayerId::new(2)],
                        hp: -2,
                        units_fuel: 0.5,
                    }),
                }),
                unit_replace: None,
                unit_add: None,
                player_replace: None,
                missile_coords: None,
                weather: None,
            }),
            app.world_mut(),
        );

        assert!(
            app.world().get_entity(victim).is_ok(),
            "power damage should not destroy units"
        );
        assert_eq!(
            app.world().entity(victim).get::<GraphicalHp>(),
            Some(&GraphicalHp(1))
        );
        assert_eq!(app.world().entity(victim).get::<Fuel>(), Some(&Fuel(5)));
    }

    #[test]
    fn power_unit_replace_zero_hp_floors_units_at_one_hp() {
        let mut app = replay_turn_test_app();
        let victim = spawn_test_unit_kind(
            &mut app,
            Position::new(2, 2),
            CoreUnitId::new(1),
            awbrn_types::Unit::Infantry,
            PlayerFaction::BlueMoon,
        );

        apply_non_move_action(
            &Action::Power(PowerAction {
                player_id: awbrn_types::AwbwGamePlayerId::new(1),
                co_name: "Rachel".to_string(),
                co_power: "Super".to_string(),
                power_name: "Covering Fire".to_string(),
                players_cop: 0,
                global: None,
                hp_change: None,
                unit_replace: Some(
                    [(
                        TargetedPlayer::Global,
                        awbw_replay::turn_models::UnitReplaceGroup {
                            units: Some(vec![awbw_replay::turn_models::UnitChange {
                                units_id: CoreUnitId::new(1),
                                units_hit_points: Some(test_hp(0)),
                                units_ammo: None,
                                units_fuel: None,
                                units_movement_points: None,
                                units_long_range: None,
                                units_moved: None,
                            }]),
                        },
                    )]
                    .into(),
                ),
                unit_add: None,
                player_replace: None,
                missile_coords: None,
                weather: None,
            }),
            app.world_mut(),
        );

        assert!(
            app.world().get_entity(victim).is_ok(),
            "power unit replacement should not destroy units"
        );
        assert_eq!(
            app.world().entity(victim).get::<GraphicalHp>(),
            Some(&GraphicalHp(1))
        );
    }

    #[test]
    fn power_unit_add_deduplicates_units_and_uses_base_vision() {
        let mut app = replay_turn_test_app();
        let mut registry = crate::replay::ReplayPlayerRegistry::default();
        registry.add_player(
            awbrn_types::AwbwGamePlayerId::new(1),
            PlayerFaction::OrangeStar,
            0,
        );
        app.world_mut().insert_resource(registry);

        apply_non_move_action(
            &Action::Power(PowerAction {
                player_id: awbrn_types::AwbwGamePlayerId::new(1),
                co_name: "Sensei".to_string(),
                co_power: "Power".to_string(),
                power_name: "Copter Command".to_string(),
                players_cop: 0,
                global: None,
                hp_change: None,
                unit_replace: None,
                unit_add: Some(
                    [
                        (
                            TargetedPlayer::Global,
                            UnitAddGroup {
                                player_id: awbrn_types::AwbwGamePlayerId::new(1),
                                unit_name: awbrn_types::Unit::Recon,
                                units: vec![NewUnit {
                                    units_id: CoreUnitId::new(99),
                                    units_x: 3,
                                    units_y: 4,
                                }],
                            },
                        ),
                        (
                            TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(1)),
                            UnitAddGroup {
                                player_id: awbrn_types::AwbwGamePlayerId::new(1),
                                unit_name: awbrn_types::Unit::Recon,
                                units: vec![NewUnit {
                                    units_id: CoreUnitId::new(99),
                                    units_x: 3,
                                    units_y: 4,
                                }],
                            },
                        ),
                    ]
                    .into(),
                ),
                player_replace: None,
                missile_coords: None,
                weather: None,
            }),
            app.world_mut(),
        );

        let mut query = app
            .world_mut()
            .query::<(&AwbwUnitId, &VisionRange, &MapPosition)>();
        let matching = query
            .iter(app.world())
            .filter(|(unit_id, _, _)| unit_id.0 == CoreUnitId::new(99))
            .collect::<Vec<_>>();

        assert_eq!(
            matching.len(),
            1,
            "duplicate unit_add entries should spawn once"
        );
        assert_eq!(
            matching[0].1,
            &VisionRange(awbrn_types::Unit::Recon.base_vision())
        );
        assert_eq!(matching[0].2, &MapPosition::new(3, 4));
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
            .set(awbrn_types::Weather::Rain);

        apply_end(
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
            awbrn_types::Weather::Clear
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
            awbrn_types::Unit::Tank,
            PlayerFaction::OrangeStar,
        );
        let repaired = spawn_test_unit_kind(
            &mut app,
            Position::new(3, 2),
            CoreUnitId::new(2),
            awbrn_types::Unit::Tank,
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

        apply_end(
            &UpdatedInfo {
                event: "NextTurn".to_string(),
                next_player_id: 1,
                next_funds: [(TargetedPlayer::Global, Hidden::Visible(0))].into(),
                next_timer: 0,
                next_weather: WeatherCode::Clear,
                supplied: Some(
                    [(
                        TargetedPlayer::Player(awbrn_types::AwbwGamePlayerId::new(10)),
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
    fn join_action_despawns_joining_unit_and_updates_survivor() {
        let mut app = replay_turn_test_app();
        let surviving = spawn_test_unit(&mut app, Position::new(3, 3), CoreUnitId::new(1));
        let joining = spawn_test_unit(&mut app, Position::new(3, 3), CoreUnitId::new(2));

        apply_non_move_action(
            &Action::Join {
                move_action: None,
                join_action: JoinAction {
                    player_id: 1,
                    new_funds: [(TargetedPlayer::Global, 5000)].into(),
                    unit: [(
                        TargetedPlayer::Global,
                        Hidden::Visible(test_unit_property_with_resources(
                            CoreUnitId::new(1),
                            3,
                            3,
                            awbrn_types::Unit::Infantry,
                            90,
                            0,
                        )),
                    )]
                    .into(),
                    join_id: [(TargetedPlayer::Global, Hidden::Visible(2))].into(),
                },
            },
            app.world_mut(),
        );

        assert!(
            app.world().get_entity(joining).is_err(),
            "joining unit should be despawned"
        );
        assert_eq!(app.world().entity(surviving).get::<Fuel>(), Some(&Fuel(90)));
        assert!(!app.world().entity(surviving).contains::<UnitActive>());
    }

    #[test]
    fn hide_action_inserts_hiding_component() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));

        apply_non_move_action(
            &Action::Hide {
                move_action: Some(MoveAction {
                    unit: [(
                        TargetedPlayer::Global,
                        Hidden::Visible(test_unit_property(CoreUnitId::new(1), 2, 2)),
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
            },
            app.world_mut(),
        );

        assert!(app.world().entity(unit_entity).contains::<Hiding>());
    }

    #[test]
    fn unhide_action_removes_hiding_component() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        app.world_mut().entity_mut(unit_entity).insert(Hiding);

        apply_non_move_action(
            &Action::Unhide {
                move_action: Some(MoveAction {
                    unit: [(
                        TargetedPlayer::Global,
                        Hidden::Visible(test_unit_property(CoreUnitId::new(1), 2, 2)),
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
            },
            app.world_mut(),
        );

        assert!(!app.world().entity(unit_entity).contains::<Hiding>());
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

        apply_non_move_action(
            &test_attack_seam_action(
                CoreUnitId::new(1),
                Position::new(4, 2),
                55,
                GraphicalTerrain::PipeSeam(PipeSeamType::Vertical),
                8,
            ),
            app.world_mut(),
        );

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
                .resource::<crate::world::GameMap>()
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

        apply_non_move_action(
            &test_attack_seam_action(
                CoreUnitId::new(1),
                Position::new(4, 2),
                -5,
                GraphicalTerrain::PipeRubble(PipeRubbleType::Vertical),
                8,
            ),
            app.world_mut(),
        );

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
                .resource::<crate::world::GameMap>()
                .terrain_at(Position::new(4, 2)),
            Some(GraphicalTerrain::PipeRubble(PipeRubbleType::Vertical))
        );
    }

    #[test]
    fn moving_to_a_new_tile_clears_capturing() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        app.world_mut().entity_mut(unit_entity).insert(Capturing);

        let move_action = MoveAction {
            unit: [(
                TargetedPlayer::Global,
                Hidden::Visible(test_unit_property(CoreUnitId::new(1), 3, 2)),
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
        };

        let outcome = apply_move_state(&move_action, app.world_mut()).unwrap();
        // Caller inserts position
        app.world_mut()
            .entity_mut(outcome.entity)
            .insert(outcome.new_position);

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

        let move_action = MoveAction {
            unit: [(
                TargetedPlayer::Global,
                Hidden::Visible(test_unit_property(CoreUnitId::new(1), 2, 2)),
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
        };

        apply_move_state(&move_action, app.world_mut());

        assert!(app.world().entity(unit_entity).contains::<Capturing>());
    }

    #[test]
    fn stationary_capture_marks_unit_inactive() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));

        apply_non_move_action(
            &Action::Capt {
                move_action: None,
                capture_action: CaptureAction {
                    building_info: BuildingInfo {
                        buildings_capture: 10,
                        buildings_id: 99,
                        buildings_x: 2,
                        buildings_y: 2,
                        buildings_team: None,
                    },
                    vision: Default::default(),
                    income: None,
                },
            },
            app.world_mut(),
        );

        assert!(app.world().entity(unit_entity).contains::<Capturing>());
        assert!(!app.world().entity(unit_entity).contains::<UnitActive>());
    }

    #[test]
    fn stationary_capture_completion_marks_unit_inactive() {
        let mut app = replay_turn_test_app();
        let unit_entity = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        app.world_mut().entity_mut(unit_entity).insert(Capturing);

        apply_non_move_action(
            &Action::Capt {
                move_action: None,
                capture_action: CaptureAction {
                    building_info: BuildingInfo {
                        buildings_capture: 20,
                        buildings_id: 99,
                        buildings_x: 2,
                        buildings_y: 2,
                        buildings_team: None,
                    },
                    vision: Default::default(),
                    income: None,
                },
            },
            app.world_mut(),
        );

        assert!(!app.world().entity(unit_entity).contains::<Capturing>());
        assert!(!app.world().entity(unit_entity).contains::<UnitActive>());
    }

    #[test]
    fn load_action_removes_map_position_from_carried_units() {
        let mut app = replay_turn_test_app();
        let transport = spawn_test_unit(&mut app, Position::new(2, 2), CoreUnitId::new(1));
        let cargo = spawn_test_unit(&mut app, Position::new(2, 3), CoreUnitId::new(2));

        apply_load(
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
            .remove::<MapPosition>();

        apply_unload(
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
    }
}
