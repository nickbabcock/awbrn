use std::collections::BTreeMap;

use awbrn_game::MapPosition;
use awbrn_game::world::{
    Ammo, BoardIndex, CaptureProgress, Faction, FriendlyFactions, Fuel, GameMap, GraphicalHp,
    StrongIdMap, TerrainTile, Unit, UnitActive,
};
use awbrn_map::Position;
use awbrn_types::{GraphicalTerrain, PlayerFaction};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use serde::Deserialize;
use serde_json::Value;

use crate::core::{AppState, GameMode};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable, on_add = on_server_unit_id_add, on_remove = on_server_unit_id_remove)]
pub struct ServerUnitId(pub u64);

fn on_server_unit_id_add(mut world: DeferredWorld, context: HookContext) {
    let unit_id = *world
        .get::<ServerUnitId>(context.entity)
        .expect("server unit id should exist on add");
    world
        .resource_mut::<StrongIdMap<ServerUnitId>>()
        .insert(unit_id, context.entity);
}

fn on_server_unit_id_remove(mut world: DeferredWorld, context: HookContext) {
    let unit_id = *world
        .get::<ServerUnitId>(context.entity)
        .expect("server unit id should exist on remove");
    world
        .resource_mut::<StrongIdMap<ServerUnitId>>()
        .remove(unit_id);
}

#[derive(Component, Debug, Clone, PartialEq, Eq, Default)]
pub struct VisibleCargo(pub Vec<awbrn_types::Unit>);

#[derive(Debug, Clone, Default, Resource)]
pub struct MatchViewState {
    pub viewer_slot_index: Option<u8>,
    pub active_player_slot: u8,
    pub day: u32,
    pub slot_factions: BTreeMap<u8, PlayerFaction>,
}

impl MatchViewState {
    pub fn viewer_faction(&self) -> Option<PlayerFaction> {
        self.viewer_slot_index
            .and_then(|slot| self.slot_factions.get(&slot).copied())
    }

    fn friendly_factions(&self) -> FriendlyFactions {
        let mut factions = FriendlyFactions::default();
        if let Some(faction) = self.viewer_faction() {
            factions.0.insert(faction);
        }
        factions
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchParticipantWire {
    pub slot_index: u8,
    pub faction_id: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchGameStateWire {
    pub viewer_slot_index: Option<u8>,
    pub day: u32,
    pub active_player_slot: u8,
    pub units: Vec<WireVisibleUnit>,
    pub terrain: Vec<WireVisibleTerrain>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchPlayerUpdateWire {
    pub day: u32,
    pub active_player_slot: u8,
    pub units_revealed: Vec<WireVisibleUnit>,
    pub units_moved: Vec<WireUnitMoved>,
    pub units_removed: Vec<u64>,
    pub terrain_revealed: Vec<WireVisibleTerrain>,
    pub terrain_changed: Vec<WireVisibleTerrain>,
    pub turn_change: Option<WireTurnChange>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireVisibleUnit {
    pub id: u64,
    pub unit_type: Value,
    pub faction: Value,
    pub position: WirePosition,
    pub hp: Option<u8>,
    pub active: Option<bool>,
    pub fuel: Option<u32>,
    pub ammo: Option<u32>,
    pub cargo_units: Option<Vec<Value>>,
    pub capture_progress: Option<u8>,
    pub hiding: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireVisibleTerrain {
    pub position: WirePosition,
    pub terrain: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePosition {
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireUnitMoved {
    pub id: u64,
    pub to: WirePosition,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireTurnChange {
    pub new_active_player_slot: u8,
    pub new_day: Option<u32>,
}

#[derive(Debug, Clone, Resource)]
pub struct PendingMatchState {
    pub state: MatchGameStateWire,
    pub participants: Vec<MatchParticipantWire>,
}

#[derive(Debug, Clone, Default, Resource)]
pub struct PendingMatchUpdates(pub Vec<MatchPlayerUpdateWire>);

#[derive(Debug, Clone)]
struct VisibleUnitData {
    id: ServerUnitId,
    unit_type: awbrn_types::Unit,
    faction: PlayerFaction,
    position: Position,
    hp: u8,
    active: bool,
    fuel: Option<u32>,
    ammo: Option<u32>,
    cargo_units: Vec<awbrn_types::Unit>,
    capture_progress: Option<u8>,
    hiding: bool,
}

#[derive(Debug, Clone)]
struct VisibleTerrainData {
    position: Position,
    terrain: GraphicalTerrain,
}

pub(crate) fn apply_pending_match_state(world: &mut World) {
    let Some(pending) = world.remove_resource::<PendingMatchState>() else {
        return;
    };

    let slot_factions = pending
        .participants
        .into_iter()
        .filter_map(|participant| {
            PlayerFaction::from_id(participant.faction_id)
                .map(|faction| (participant.slot_index, faction))
        })
        .collect::<BTreeMap<_, _>>();

    let view_state = MatchViewState {
        viewer_slot_index: pending.state.viewer_slot_index,
        active_player_slot: pending.state.active_player_slot,
        day: pending.state.day,
        slot_factions,
    };

    let unit_entities: Vec<_> = world
        .query_filtered::<Entity, With<ServerUnitId>>()
        .iter(world)
        .collect();
    for entity in unit_entities {
        let _ = world.despawn(entity);
    }

    world.insert_resource(view_state.clone());
    *world.resource_mut::<FriendlyFactions>() = view_state.friendly_factions();

    for terrain in pending
        .state
        .terrain
        .iter()
        .filter_map(parse_visible_terrain)
    {
        apply_visible_terrain(world, &terrain);
    }

    for unit in pending.state.units.iter().filter_map(parse_visible_unit) {
        upsert_visible_unit(world, &unit);
    }
}

pub(crate) fn apply_pending_match_updates(world: &mut World) {
    let Some(mut pending) = world.remove_resource::<PendingMatchUpdates>() else {
        return;
    };

    for update in pending.0.drain(..) {
        apply_match_update(world, update);
    }
}

fn apply_match_update(world: &mut World, update: MatchPlayerUpdateWire) {
    let mut view_state = world.resource::<MatchViewState>().clone();
    view_state.day = update.day;
    view_state.active_player_slot = update.active_player_slot;

    for terrain in update
        .terrain_revealed
        .iter()
        .chain(update.terrain_changed.iter())
        .filter_map(parse_visible_terrain)
    {
        apply_visible_terrain(world, &terrain);
    }

    for unit in update.units_revealed.iter().filter_map(parse_visible_unit) {
        upsert_visible_unit(world, &unit);
    }

    let viewer_faction = view_state.viewer_faction();
    let viewer_turn_active = view_state.viewer_slot_index == Some(view_state.active_player_slot);
    for moved in &update.units_moved {
        let unit_id = ServerUnitId(moved.id);
        let Some(entity) = world.resource::<StrongIdMap<ServerUnitId>>().get(&unit_id) else {
            continue;
        };

        let destination = Position::new(moved.to.x, moved.to.y);
        let mut entity_mut = world.entity_mut(entity);
        entity_mut.insert(MapPosition::from(destination));

        let is_viewer_unit = viewer_faction
            .and_then(|faction| {
                entity_mut
                    .get::<Faction>()
                    .map(|entity_faction| entity_faction.0 == faction)
            })
            .unwrap_or(false);

        if viewer_turn_active && is_viewer_unit {
            entity_mut.remove::<UnitActive>();
        }
    }

    for unit_id in &update.units_removed {
        let unit_id = ServerUnitId(*unit_id);
        let Some(entity) = world.resource::<StrongIdMap<ServerUnitId>>().get(&unit_id) else {
            continue;
        };
        let _ = world.despawn(entity);
    }

    if let Some(turn_change) = update.turn_change {
        view_state.active_player_slot = turn_change.new_active_player_slot;
        if let Some(day) = turn_change.new_day {
            view_state.day = day;
        }

        let viewer_faction = view_state.viewer_faction();
        let viewer_turn_active =
            view_state.viewer_slot_index == Some(view_state.active_player_slot);
        let entities: Vec<_> = world
            .query::<(Entity, &Faction)>()
            .iter(world)
            .filter_map(|(entity, faction)| {
                viewer_faction
                    .filter(|viewer_faction| *viewer_faction == faction.0)
                    .map(|_| entity)
            })
            .collect();

        for entity in entities {
            let mut entity_mut = world.entity_mut(entity);
            if viewer_turn_active {
                entity_mut.insert(UnitActive);
            } else {
                entity_mut.remove::<UnitActive>();
            }
        }
    }

    *world.resource_mut::<FriendlyFactions>() = view_state.friendly_factions();
    world.insert_resource(view_state);
}

fn apply_visible_terrain(world: &mut World, terrain: &VisibleTerrainData) {
    if world
        .resource_mut::<GameMap>()
        .set_terrain(terrain.position, terrain.terrain)
        .is_none()
    {
        return;
    }

    if let Ok(entity) = world
        .resource::<BoardIndex>()
        .terrain_entity(terrain.position)
        && let Ok(mut entity_mut) = world.get_entity_mut(entity)
    {
        entity_mut.insert(TerrainTile {
            terrain: terrain.terrain,
        });
    }
}

fn upsert_visible_unit(world: &mut World, unit: &VisibleUnitData) {
    let entity = world
        .resource::<StrongIdMap<ServerUnitId>>()
        .get(&unit.id)
        .unwrap_or_else(|| world.spawn_empty().id());

    let mut entity_mut = world.entity_mut(entity);
    entity_mut.insert((
        MapPosition::from(unit.position),
        Unit(unit.unit_type),
        Faction(unit.faction),
        ServerUnitId(unit.id.0),
        GraphicalHp(unit.hp),
    ));

    if unit.active {
        entity_mut.insert(UnitActive);
    } else {
        entity_mut.remove::<UnitActive>();
    }

    if let Some(fuel) = unit.fuel {
        entity_mut.insert(Fuel(fuel));
    } else {
        entity_mut.remove::<Fuel>();
    }

    if let Some(ammo) = unit.ammo {
        entity_mut.insert(Ammo(ammo));
    } else {
        entity_mut.remove::<Ammo>();
    }

    if let Some(progress) = unit.capture_progress.and_then(CaptureProgress::new) {
        entity_mut.insert(progress);
    } else {
        entity_mut.remove::<CaptureProgress>();
    }

    if unit.hiding {
        entity_mut.insert(awbrn_game::world::Hiding);
    } else {
        entity_mut.remove::<awbrn_game::world::Hiding>();
    }

    if unit.cargo_units.is_empty() {
        entity_mut.remove::<VisibleCargo>();
    } else {
        entity_mut.insert(VisibleCargo(unit.cargo_units.clone()));
    }
}

fn parse_visible_unit(unit: &WireVisibleUnit) -> Option<VisibleUnitData> {
    let unit_type = serde_json::from_value::<awbrn_types::Unit>(unit.unit_type.clone()).ok()?;
    let faction = serde_json::from_value::<PlayerFaction>(unit.faction.clone()).ok()?;
    let cargo_units = unit
        .cargo_units
        .as_ref()
        .map(|cargo| {
            cargo
                .iter()
                .filter_map(|value| serde_json::from_value::<awbrn_types::Unit>(value.clone()).ok())
                .collect()
        })
        .unwrap_or_default();

    Some(VisibleUnitData {
        id: ServerUnitId(unit.id),
        unit_type,
        faction,
        position: Position::new(unit.position.x, unit.position.y),
        hp: unit.hp.unwrap_or(10),
        active: unit.active.unwrap_or(false),
        fuel: unit.fuel,
        ammo: unit.ammo,
        cargo_units,
        capture_progress: unit.capture_progress,
        hiding: unit.hiding,
    })
}

fn parse_visible_terrain(terrain: &WireVisibleTerrain) -> Option<VisibleTerrainData> {
    Some(VisibleTerrainData {
        position: Position::new(terrain.position.x, terrain.position.y),
        terrain: serde_json::from_value::<GraphicalTerrain>(terrain.terrain.clone()).ok()?,
    })
}

pub(crate) fn apply_match_sync_system(world: &mut World) {
    if !matches!(
        world.get_resource::<State<GameMode>>().map(State::get),
        Some(GameMode::Game)
    ) || !matches!(
        world.get_resource::<State<AppState>>().map(State::get),
        Some(AppState::InGame)
    ) {
        return;
    }

    apply_pending_match_state(world);
    apply_pending_match_updates(world);
}
