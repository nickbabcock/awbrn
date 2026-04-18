use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::reflect::AppTypeRegistry;
use bevy::prelude::*;
use bevy::reflect::serde::{ReflectSerializerProcessor, TypedReflectSerializer};
use bevy::reflect::{PartialReflect, TypeRegistry};
use bevy::scene::{DynamicEntity, DynamicScene, DynamicSceneBuilder, SceneFilter, SceneSpawnError};
use serde::Serialize;
use serde_json::Value;

use crate::MapPosition;
use crate::replay::{AwbwUnitId, ReplayFogDirty, ReplayState};
use crate::world::{
    Ammo, CaptureProgress, CarriedBy, Faction, FogActive, FogOfWarMap, FriendlyFactions, Fuel,
    GraphicalHp, HasCargo, TerrainHp, TerrainTile, Unit, UnitActive, VisionRange,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct ReplaySemanticComponentType;

#[derive(Clone, Copy, Debug, Default)]
pub struct ReplaySemanticResourceType;

impl<T> bevy::reflect::FromType<T> for ReplaySemanticComponentType {
    fn from_type() -> Self {
        Self
    }
}

impl<T> bevy::reflect::FromType<T> for ReplaySemanticResourceType {
    fn from_type() -> Self {
        Self
    }
}

pub struct GameSnapshot {
    pub next_action_index: u32,
    pub day: u32,
    pub active_player_id: Option<awbrn_types::AwbwGamePlayerId>,
    pub scene: DynamicScene,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CanonicalReplaySnapshot {
    pub next_action_index: u32,
    pub day: u32,
    pub active_player_id: Option<awbrn_types::AwbwGamePlayerId>,
    pub resources: Vec<CanonicalSceneEntry>,
    pub entities: Vec<CanonicalReplayEntity>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CanonicalReplayEntity {
    pub id: String,
    pub components: Vec<CanonicalSceneEntry>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CanonicalSceneEntry {
    pub type_path: String,
    pub value: Value,
}

#[derive(Debug)]
pub enum GameSnapshotError {
    MissingReplayState,
    MissingSemanticIdentity(Entity),
    MissingEntityMapping(Entity),
    DuplicateSemanticId {
        id: String,
        existing_entity: Entity,
        new_entity: Entity,
    },
    SceneSpawn(SceneSpawnError),
    Serialization(String),
}

impl From<SceneSpawnError> for GameSnapshotError {
    fn from(value: SceneSpawnError) -> Self {
        Self::SceneSpawn(value)
    }
}

impl fmt::Display for GameSnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingReplayState => f.write_str("missing ReplayState resource"),
            Self::MissingSemanticIdentity(entity) => {
                write!(f, "missing semantic identity for entity {entity:?}")
            }
            Self::MissingEntityMapping(entity) => {
                write!(f, "missing semantic entity mapping for {entity:?}")
            }
            Self::DuplicateSemanticId {
                id,
                existing_entity,
                new_entity,
            } => write!(
                f,
                "duplicate semantic id {id} for entities {existing_entity:?} and {new_entity:?}"
            ),
            Self::SceneSpawn(error) => write!(f, "failed to restore game snapshot: {error}"),
            Self::Serialization(error) => write!(f, "failed to serialize snapshot: {error}"),
        }
    }
}

impl Error for GameSnapshotError {}

pub struct GameSnapshotPlugin;

impl Plugin for GameSnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<MapPosition>()
            .register_type_data::<MapPosition, ReplaySemanticComponentType>()
            .register_type::<TerrainTile>()
            .register_type_data::<TerrainTile, ReplaySemanticComponentType>()
            .register_type::<TerrainHp>()
            .register_type_data::<TerrainHp, ReplaySemanticComponentType>()
            .register_type::<Unit>()
            .register_type_data::<Unit, ReplaySemanticComponentType>()
            .register_type::<Faction>()
            .register_type_data::<Faction, ReplaySemanticComponentType>()
            .register_type::<AwbwUnitId>()
            .register_type_data::<AwbwUnitId, ReplaySemanticComponentType>()
            .register_type::<UnitActive>()
            .register_type_data::<UnitActive, ReplaySemanticComponentType>()
            .register_type::<CaptureProgress>()
            .register_type_data::<CaptureProgress, ReplaySemanticComponentType>()
            .register_type::<GraphicalHp>()
            .register_type_data::<GraphicalHp, ReplaySemanticComponentType>()
            .register_type::<Fuel>()
            .register_type_data::<Fuel, ReplaySemanticComponentType>()
            .register_type::<Ammo>()
            .register_type_data::<Ammo, ReplaySemanticComponentType>()
            .register_type::<VisionRange>()
            .register_type_data::<VisionRange, ReplaySemanticComponentType>()
            .register_type::<CarriedBy>()
            .register_type_data::<CarriedBy, ReplaySemanticComponentType>()
            .register_type::<HasCargo>()
            .register_type_data::<HasCargo, ReplaySemanticComponentType>()
            .register_type::<ReplayState>()
            .register_type_data::<ReplayState, ReplaySemanticResourceType>();
    }
}

pub fn capture_game_snapshot(world: &mut World) -> Result<GameSnapshot, GameSnapshotError> {
    let (next_action_index, day, active_player_id) = {
        let replay_state = world
            .get_resource::<ReplayState>()
            .ok_or(GameSnapshotError::MissingReplayState)?;
        (
            replay_state.next_action_index,
            replay_state.day,
            replay_state.active_player_id,
        )
    };

    let entities: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, Or<(With<Unit>, With<TerrainTile>)>>();
        query.iter(world).collect()
    };

    let component_filter = game_semantic_component_filter(world);
    let resource_filter = game_semantic_resource_filter(world);
    let scene = DynamicSceneBuilder::from_world(world)
        .with_component_filter(component_filter)
        .with_resource_filter(resource_filter)
        .extract_entities(entities.into_iter())
        .extract_resources()
        .remove_empty_entities()
        .build();

    Ok(GameSnapshot {
        next_action_index,
        day,
        active_player_id,
        scene,
    })
}

pub fn restore_game_snapshot(
    world: &mut World,
    snapshot: &GameSnapshot,
) -> Result<(), GameSnapshotError> {
    let entities: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, Or<(With<Unit>, With<TerrainTile>)>>();
        query.iter(world).collect()
    };
    for entity in entities {
        let _ = world.despawn(entity);
    }

    let type_registry = world.resource::<AppTypeRegistry>().clone();
    let mut entity_map = EntityHashMap::default();
    snapshot
        .scene
        .write_to_world_with(world, &mut entity_map, &type_registry)?;

    if let Some(mut replay_state) = world.get_resource_mut::<ReplayState>() {
        replay_state.next_action_index = snapshot.next_action_index;
        replay_state.day = snapshot.day;
        replay_state.active_player_id = snapshot.active_player_id;
    } else {
        world.insert_resource(ReplayState {
            next_action_index: snapshot.next_action_index,
            day: snapshot.day,
            active_player_id: snapshot.active_player_id,
        });
    }

    if world.contains_resource::<FogActive>()
        && world.contains_resource::<FogOfWarMap>()
        && world.contains_resource::<FriendlyFactions>()
    {
        world.trigger(ReplayFogDirty);
    }

    Ok(())
}

pub fn canonicalize_replay_semantic_snapshot(
    snapshot: &GameSnapshot,
    type_registry: &TypeRegistry,
) -> Result<CanonicalReplaySnapshot, GameSnapshotError> {
    let semantic_ids = semantic_id_map(&snapshot.scene.entities)?;
    let processor = SemanticEntityProcessor {
        semantic_ids: &semantic_ids,
    };

    let mut resources = snapshot
        .scene
        .resources
        .iter()
        .map(|resource| canonical_scene_entry(resource.as_ref(), &processor, type_registry))
        .collect::<Result<Vec<_>, _>>()?;
    resources.sort_by(|left, right| left.type_path.cmp(&right.type_path));

    let mut entities = snapshot
        .scene
        .entities
        .iter()
        .map(|entity| canonical_entity(entity, &processor, type_registry))
        .collect::<Result<Vec<_>, _>>()?;
    entities.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(CanonicalReplaySnapshot {
        next_action_index: snapshot.next_action_index,
        day: snapshot.day,
        active_player_id: snapshot.active_player_id,
        resources,
        entities,
    })
}

fn game_semantic_component_filter(world: &World) -> SceneFilter {
    let type_registry = world.resource::<AppTypeRegistry>();
    let type_registry = type_registry.read();
    let mut filter = SceneFilter::deny_all();
    for (registration, _) in type_registry.iter_with_data::<ReplaySemanticComponentType>() {
        filter = filter.allow_by_id(registration.type_id());
    }
    filter
}

fn game_semantic_resource_filter(world: &World) -> SceneFilter {
    let type_registry = world.resource::<AppTypeRegistry>();
    let type_registry = type_registry.read();
    let mut filter = SceneFilter::deny_all();
    for (registration, _) in type_registry.iter_with_data::<ReplaySemanticResourceType>() {
        filter = filter.allow_by_id(registration.type_id());
    }
    filter
}

fn semantic_id_map(entities: &[DynamicEntity]) -> Result<EntityHashMap<String>, GameSnapshotError> {
    let mut semantic_ids = EntityHashMap::default();
    let mut ids_to_entities = HashMap::with_capacity(entities.len());

    for entity in entities {
        let semantic_id = semantic_id_for_entity(entity)?;
        if let Some(existing_entity) = ids_to_entities.insert(semantic_id.clone(), entity.entity) {
            return Err(GameSnapshotError::DuplicateSemanticId {
                id: semantic_id,
                existing_entity,
                new_entity: entity.entity,
            });
        }
        semantic_ids.insert(entity.entity, semantic_id);
    }

    Ok(semantic_ids)
}

fn semantic_id_for_entity(entity: &DynamicEntity) -> Result<String, GameSnapshotError> {
    let mut terrain_entity = false;
    let mut map_position = None;

    for component in &entity.components {
        if let Some(unit_id) = component.try_downcast_ref::<AwbwUnitId>() {
            return Ok(format!("unit:{}", unit_id.0.as_u32()));
        }
        if component.try_downcast_ref::<TerrainTile>().is_some() {
            terrain_entity = true;
        }
        if let Some(position) = component.try_downcast_ref::<MapPosition>() {
            map_position = Some(position.position());
        }
    }

    if terrain_entity && let Some(position) = map_position {
        return Ok(format!("terrain:{},{}", position.x, position.y));
    }

    Err(GameSnapshotError::MissingSemanticIdentity(entity.entity))
}

struct SemanticEntityProcessor<'a> {
    semantic_ids: &'a EntityHashMap<String>,
}

impl ReflectSerializerProcessor for SemanticEntityProcessor<'_> {
    fn try_serialize<S>(
        &self,
        value: &dyn PartialReflect,
        _registry: &TypeRegistry,
        serializer: S,
    ) -> Result<Result<S::Ok, S>, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(entity) = value.try_downcast_ref::<Entity>() {
            let id = self.semantic_ids.get(entity).ok_or_else(|| {
                serde::ser::Error::custom(format!("missing semantic mapping for {entity:?}"))
            })?;
            Ok(Ok(serde::Serializer::serialize_str(serializer, id)?))
        } else {
            Ok(Err(serializer))
        }
    }
}

fn canonical_entity(
    entity: &DynamicEntity,
    processor: &SemanticEntityProcessor,
    type_registry: &TypeRegistry,
) -> Result<CanonicalReplayEntity, GameSnapshotError> {
    let mut components = entity
        .components
        .iter()
        .map(|component| canonical_scene_entry(component.as_ref(), processor, type_registry))
        .collect::<Result<Vec<_>, _>>()?;
    components.sort_by(|left, right| left.type_path.cmp(&right.type_path));

    Ok(CanonicalReplayEntity {
        id: processor
            .semantic_ids
            .get(&entity.entity)
            .cloned()
            .ok_or(GameSnapshotError::MissingEntityMapping(entity.entity))?,
        components,
    })
}

fn canonical_scene_entry(
    reflect_value: &dyn PartialReflect,
    processor: &SemanticEntityProcessor,
    type_registry: &TypeRegistry,
) -> Result<CanonicalSceneEntry, GameSnapshotError> {
    let type_path = reflect_value
        .get_represented_type_info()
        .map(|info| info.type_path().to_string())
        .unwrap_or_else(|| reflect_value.reflect_type_path().to_string());
    let ser = TypedReflectSerializer::with_processor(reflect_value, type_registry, processor);
    let value =
        serde_json::to_value(&ser).map_err(|e| GameSnapshotError::Serialization(e.to_string()))?;
    Ok(CanonicalSceneEntry { type_path, value })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::ReplayState;
    use crate::world::{Ammo, Faction, Fuel, GameMap, TerrainHp, Unit, UnitActive, VisionRange};
    use crate::{GameWorldPlugin, MapPosition};
    use awbrn_map::AwbrnMap;
    use awbrn_types::{AwbwGamePlayerId, GraphicalTerrain, PlayerFaction};
    use bevy::app::App;
    use bevy::ecs::reflect::AppTypeRegistry;
    use bevy::{ecs::entity::MapEntities, ecs::reflect::ReflectMapEntities};

    #[derive(Component, Reflect, MapEntities)]
    #[reflect(Component, MapEntities)]
    struct TestEntityRef(#[entities] Entity);

    #[test]
    fn snapshot_round_trip_preserves_canonical_form() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState {
            next_action_index: 7,
            day: 3,
            active_player_id: None,
        });

        app.world_mut().spawn((
            MapPosition::new(0, 0),
            TerrainTile {
                terrain: GraphicalTerrain::Plain,
            },
            TerrainHp(55),
        ));
        app.world_mut().spawn((
            MapPosition::new(1, 0),
            Faction(PlayerFaction::OrangeStar),
            AwbwUnitId(awbrn_types::AwbwUnitId::new(1)),
            Unit(awbrn_types::Unit::Infantry),
            UnitActive,
        ));

        let snapshot = capture_game_snapshot(app.world_mut()).unwrap();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let canonical = canonicalize_replay_semantic_snapshot(&snapshot, &type_registry).unwrap();
        drop(type_registry);

        let mut restored = snapshot_test_app();
        restore_game_snapshot(restored.world_mut(), &snapshot).unwrap();

        let restored_snapshot = capture_game_snapshot(restored.world_mut()).unwrap();
        let type_registry = restored.world().resource::<AppTypeRegistry>().read();
        let restored_canonical =
            canonicalize_replay_semantic_snapshot(&restored_snapshot, &type_registry).unwrap();

        assert_eq!(canonical, restored_canonical);
    }

    #[test]
    fn snapshot_restore_preserves_active_player_id() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState {
            next_action_index: 7,
            day: 3,
            active_player_id: Some(AwbwGamePlayerId::new(42)),
        });

        let snapshot = capture_game_snapshot(app.world_mut()).unwrap();
        assert_eq!(snapshot.active_player_id, Some(AwbwGamePlayerId::new(42)));

        let mut restored = snapshot_test_app();
        restore_game_snapshot(restored.world_mut(), &snapshot).unwrap();

        assert_eq!(
            restored.world().resource::<ReplayState>().active_player_id,
            Some(AwbwGamePlayerId::new(42))
        );
    }

    #[test]
    fn canonicalizer_rewrites_entity_refs_to_semantic_ids() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState {
            next_action_index: 1,
            day: 1,
            active_player_id: None,
        });

        let transport = app
            .world_mut()
            .spawn((
                MapPosition::new(0, 0),
                Faction(PlayerFaction::OrangeStar),
                AwbwUnitId(awbrn_types::AwbwUnitId::new(1)),
                Unit(awbrn_types::Unit::APC),
                UnitActive,
            ))
            .id();
        let cargo = app
            .world_mut()
            .spawn((
                MapPosition::new(0, 0),
                Faction(PlayerFaction::OrangeStar),
                AwbwUnitId(awbrn_types::AwbwUnitId::new(2)),
                Unit(awbrn_types::Unit::Infantry),
                TestEntityRef(transport),
            ))
            .id();
        let _ = cargo;

        let snapshot = capture_game_snapshot(app.world_mut()).unwrap();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let canonical = canonicalize_replay_semantic_snapshot(&snapshot, &type_registry).unwrap();
        let cargo_entity = canonical
            .entities
            .iter()
            .find(|entity| entity.id == "unit:2")
            .unwrap();
        let carried_by = cargo_entity
            .components
            .iter()
            .find(|component| component.type_path.ends_with("TestEntityRef"))
            .unwrap();

        assert_eq!(carried_by.value, Value::String("unit:1".into()));
    }

    #[test]
    fn canonicalizer_terrain_only_includes_semantic_components() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState::default());

        app.world_mut().spawn((
            MapPosition::new(0, 0),
            TerrainTile {
                terrain: GraphicalTerrain::Plain,
            },
        ));

        let snapshot = capture_game_snapshot(app.world_mut()).unwrap();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let canonical = canonicalize_replay_semantic_snapshot(&snapshot, &type_registry).unwrap();
        let terrain = canonical
            .entities
            .iter()
            .find(|entity| entity.id == "terrain:0,0")
            .unwrap();

        let component_paths: Vec<_> = terrain
            .components
            .iter()
            .map(|c| c.type_path.as_str())
            .collect();
        assert!(
            component_paths
                .iter()
                .all(|path| path.ends_with("MapPosition") || path.ends_with("TerrainTile")),
            "unexpected components: {component_paths:?}"
        );
    }

    #[test]
    fn canonicalizer_includes_terrain_hp() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState::default());

        app.world_mut().spawn((
            MapPosition::new(0, 0),
            TerrainTile {
                terrain: GraphicalTerrain::Plain,
            },
            TerrainHp(55),
        ));

        let snapshot = capture_game_snapshot(app.world_mut()).unwrap();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let canonical = canonicalize_replay_semantic_snapshot(&snapshot, &type_registry).unwrap();
        let terrain = canonical
            .entities
            .iter()
            .find(|entity| entity.id == "terrain:0,0")
            .unwrap();

        let terrain_hp = terrain
            .components
            .iter()
            .find(|component| component.type_path.ends_with("TerrainHp"))
            .unwrap();

        assert_eq!(terrain_hp.value, Value::Number(55.into()));
    }

    #[test]
    fn canonicalizer_includes_unit_resources() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState::default());

        app.world_mut().spawn((
            MapPosition::new(1, 0),
            Faction(PlayerFaction::OrangeStar),
            AwbwUnitId(awbrn_types::AwbwUnitId::new(7)),
            Unit(awbrn_types::Unit::Tank),
            Fuel(37),
            Ammo(5),
            VisionRange(6),
        ));

        let snapshot = capture_game_snapshot(app.world_mut()).unwrap();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let canonical = canonicalize_replay_semantic_snapshot(&snapshot, &type_registry).unwrap();
        let unit = canonical
            .entities
            .iter()
            .find(|entity| entity.id == "unit:7")
            .unwrap();

        let fuel = unit
            .components
            .iter()
            .find(|component| component.type_path.ends_with("Fuel"))
            .unwrap();
        let ammo = unit
            .components
            .iter()
            .find(|component| component.type_path.ends_with("Ammo"))
            .unwrap();
        let vision_range = unit
            .components
            .iter()
            .find(|component| component.type_path.ends_with("VisionRange"))
            .unwrap();

        assert_eq!(fuel.value, Value::Number(37.into()));
        assert_eq!(ammo.value, Value::Number(5.into()));
        assert_eq!(vision_range.value, Value::Number(6.into()));
    }

    #[test]
    fn canonicalizer_rejects_duplicate_semantic_ids() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState::default());

        app.world_mut().spawn((
            MapPosition::new(0, 0),
            Faction(PlayerFaction::OrangeStar),
            AwbwUnitId(awbrn_types::AwbwUnitId::new(1)),
            Unit(awbrn_types::Unit::Infantry),
        ));
        app.world_mut().spawn((
            MapPosition::new(1, 0),
            Faction(PlayerFaction::BlueMoon),
            AwbwUnitId(awbrn_types::AwbwUnitId::new(1)),
            Unit(awbrn_types::Unit::Mech),
        ));

        let snapshot = capture_game_snapshot(app.world_mut()).unwrap();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let error = canonicalize_replay_semantic_snapshot(&snapshot, &type_registry).unwrap_err();

        assert!(matches!(
            error,
            GameSnapshotError::DuplicateSemanticId { ref id, .. } if id == "unit:1"
        ));
    }

    #[test]
    fn restore_does_not_leave_stale_entities() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState::default());
        app.world_mut().spawn((
            MapPosition::new(0, 0),
            TerrainTile {
                terrain: GraphicalTerrain::Plain,
            },
        ));

        let snapshot = capture_game_snapshot(app.world_mut()).unwrap();

        let mut restored = snapshot_test_app();
        restore_game_snapshot(restored.world_mut(), &snapshot).unwrap();

        let terrain_count = {
            let mut query = restored
                .world_mut()
                .query_filtered::<Entity, With<TerrainTile>>();
            query.iter(restored.world()).count()
        };

        assert_eq!(terrain_count, 1);
    }

    fn snapshot_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(GameWorldPlugin);
        app.register_type::<TestEntityRef>()
            .register_type_data::<TestEntityRef, ReplaySemanticComponentType>();
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(AwbrnMap::new(2, 2, GraphicalTerrain::Plain));
        app
    }
}
