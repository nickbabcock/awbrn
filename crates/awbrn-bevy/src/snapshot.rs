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

use crate::core::map::TerrainTile;
use crate::core::{Capturing, Faction, GraphicalHp, HasCargo, MapPosition, Unit, UnitActive};
use crate::modes::replay::AwbwUnitId;
use crate::modes::replay::state::ReplayState;

#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[reflect(Component)]
pub struct ReplaySnapshotEntity;

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

pub struct ReplaySemanticSnapshot {
    pub next_action_index: u32,
    pub day: u32,
    pub scene: DynamicScene,
}

#[derive(Debug)]
pub enum ReplaySemanticSnapshotError {
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

impl From<SceneSpawnError> for ReplaySemanticSnapshotError {
    fn from(value: SceneSpawnError) -> Self {
        Self::SceneSpawn(value)
    }
}

impl fmt::Display for ReplaySemanticSnapshotError {
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
            Self::SceneSpawn(error) => write!(f, "failed to restore semantic snapshot: {error}"),
            Self::Serialization(error) => write!(f, "failed to serialize snapshot: {error}"),
        }
    }
}

impl Error for ReplaySemanticSnapshotError {}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CanonicalReplaySnapshot {
    pub next_action_index: u32,
    pub day: u32,
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

pub struct ReplaySnapshotPlugin;

impl Plugin for ReplaySnapshotPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ReplaySnapshotEntity>()
            .register_type::<MapPosition>()
            .register_type_data::<MapPosition, ReplaySemanticComponentType>()
            .register_type::<TerrainTile>()
            .register_type_data::<TerrainTile, ReplaySemanticComponentType>()
            .register_type::<Unit>()
            .register_type_data::<Unit, ReplaySemanticComponentType>()
            .register_type::<Faction>()
            .register_type_data::<Faction, ReplaySemanticComponentType>()
            .register_type::<AwbwUnitId>()
            .register_type_data::<AwbwUnitId, ReplaySemanticComponentType>()
            .register_type::<UnitActive>()
            .register_type_data::<UnitActive, ReplaySemanticComponentType>()
            .register_type::<Capturing>()
            .register_type_data::<Capturing, ReplaySemanticComponentType>()
            .register_type::<GraphicalHp>()
            .register_type_data::<GraphicalHp, ReplaySemanticComponentType>()
            .register_type::<crate::core::CarriedBy>()
            .register_type_data::<crate::core::CarriedBy, ReplaySemanticComponentType>()
            .register_type::<HasCargo>()
            .register_type_data::<HasCargo, ReplaySemanticComponentType>()
            .register_type::<ReplayState>()
            .register_type_data::<ReplayState, ReplaySemanticResourceType>();
    }
}

pub fn capture_replay_semantic_snapshot(
    world: &mut World,
) -> Result<ReplaySemanticSnapshot, ReplaySemanticSnapshotError> {
    let (next_action_index, day) = {
        let replay_state = world
            .get_resource::<ReplayState>()
            .ok_or(ReplaySemanticSnapshotError::MissingReplayState)?;
        (replay_state.next_action_index, replay_state.day)
    };
    let entities: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<ReplaySnapshotEntity>>();
        query.iter(world).collect()
    };

    let component_filter = replay_semantic_component_filter(world)?;
    let resource_filter = replay_semantic_resource_filter(world)?;
    let scene = DynamicSceneBuilder::from_world(world)
        .with_component_filter(component_filter)
        .with_resource_filter(resource_filter)
        .extract_entities(entities.into_iter())
        .extract_resources()
        .remove_empty_entities()
        .build();

    Ok(ReplaySemanticSnapshot {
        next_action_index,
        day,
        scene,
    })
}

pub fn restore_replay_semantic_snapshot(
    world: &mut World,
    snapshot: &ReplaySemanticSnapshot,
) -> Result<(), ReplaySemanticSnapshotError> {
    let entities: Vec<Entity> = {
        let mut query = world.query_filtered::<Entity, With<ReplaySnapshotEntity>>();
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

    // `ReplaySnapshotEntity` is intentionally excluded from `ReplaySemanticComponentType`
    // extraction, so restore must reapply it.
    let restored_entities: Vec<Entity> = entity_map.values().copied().collect();
    for entity in restored_entities {
        world.entity_mut(entity).insert(ReplaySnapshotEntity);
    }

    Ok(())
}

pub fn canonicalize_replay_semantic_snapshot(
    snapshot: &ReplaySemanticSnapshot,
    type_registry: &TypeRegistry,
) -> Result<CanonicalReplaySnapshot, ReplaySemanticSnapshotError> {
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
        resources,
        entities,
    })
}

fn replay_semantic_component_filter(
    world: &World,
) -> Result<SceneFilter, ReplaySemanticSnapshotError> {
    let type_registry = world.resource::<AppTypeRegistry>();
    let type_registry = type_registry.read();
    let mut filter = SceneFilter::deny_all();
    for (registration, _) in type_registry.iter_with_data::<ReplaySemanticComponentType>() {
        filter = filter.allow_by_id(registration.type_id());
    }
    Ok(filter)
}

fn replay_semantic_resource_filter(
    world: &World,
) -> Result<SceneFilter, ReplaySemanticSnapshotError> {
    let type_registry = world.resource::<AppTypeRegistry>();
    let type_registry = type_registry.read();
    let mut filter = SceneFilter::deny_all();
    for (registration, _) in type_registry.iter_with_data::<ReplaySemanticResourceType>() {
        filter = filter.allow_by_id(registration.type_id());
    }
    Ok(filter)
}

fn semantic_id_map(
    entities: &[DynamicEntity],
) -> Result<EntityHashMap<String>, ReplaySemanticSnapshotError> {
    let mut semantic_ids = EntityHashMap::default();
    let mut ids_to_entities = HashMap::with_capacity(entities.len());

    for entity in entities {
        let semantic_id = semantic_id_for_entity(entity)?;
        if let Some(existing_entity) = ids_to_entities.insert(semantic_id.clone(), entity.entity) {
            return Err(ReplaySemanticSnapshotError::DuplicateSemanticId {
                id: semantic_id,
                existing_entity,
                new_entity: entity.entity,
            });
        }
        semantic_ids.insert(entity.entity, semantic_id);
    }

    Ok(semantic_ids)
}

fn semantic_id_for_entity(entity: &DynamicEntity) -> Result<String, ReplaySemanticSnapshotError> {
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

    Err(ReplaySemanticSnapshotError::MissingSemanticIdentity(
        entity.entity,
    ))
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
) -> Result<CanonicalReplayEntity, ReplaySemanticSnapshotError> {
    let mut components = entity
        .components
        .iter()
        .map(|component| canonical_scene_entry(component.as_ref(), processor, type_registry))
        .collect::<Result<Vec<_>, _>>()?;
    components.sort_by(|left, right| left.type_path.cmp(&right.type_path));

    Ok(CanonicalReplayEntity {
        id: processor.semantic_ids.get(&entity.entity).cloned().ok_or(
            ReplaySemanticSnapshotError::MissingEntityMapping(entity.entity),
        )?,
        components,
    })
}

fn canonical_scene_entry(
    reflect_value: &dyn PartialReflect,
    processor: &SemanticEntityProcessor,
    type_registry: &TypeRegistry,
) -> Result<CanonicalSceneEntry, ReplaySemanticSnapshotError> {
    let type_path = reflect_value
        .get_represented_type_info()
        .map(|info| info.type_path().to_string())
        .unwrap_or_else(|| reflect_value.reflect_type_path().to_string());
    let ser = TypedReflectSerializer::with_processor(reflect_value, type_registry, processor);
    let value = serde_json::to_value(&ser)
        .map_err(|e| ReplaySemanticSnapshotError::Serialization(e.to_string()))?;
    Ok(CanonicalSceneEntry { type_path, value })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::map::GameMap;
    use crate::core::{CorePlugin, MapPosition};
    use crate::features::CurrentWeather;
    use crate::modes::replay::ReplayPlugin;
    use awbrn_core::{GraphicalTerrain, PlayerFaction};
    use awbrn_map::AwbrnMap;
    use bevy::state::app::StatesPlugin;
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
        });

        app.world_mut().spawn((
            ReplaySnapshotEntity,
            MapPosition::new(0, 0),
            TerrainTile {
                terrain: GraphicalTerrain::Plain,
            },
        ));
        app.world_mut().spawn((
            ReplaySnapshotEntity,
            MapPosition::new(1, 0),
            Faction(PlayerFaction::OrangeStar),
            AwbwUnitId(awbrn_core::AwbwUnitId::new(1)),
            Unit(awbrn_core::Unit::Infantry),
            UnitActive,
        ));

        let snapshot = capture_replay_semantic_snapshot(app.world_mut()).unwrap();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let canonical = canonicalize_replay_semantic_snapshot(&snapshot, &type_registry).unwrap();
        drop(type_registry);

        let mut restored = snapshot_test_app();
        restore_replay_semantic_snapshot(restored.world_mut(), &snapshot).unwrap();

        let restored_snapshot = capture_replay_semantic_snapshot(restored.world_mut()).unwrap();
        let type_registry = restored.world().resource::<AppTypeRegistry>().read();
        let restored_canonical =
            canonicalize_replay_semantic_snapshot(&restored_snapshot, &type_registry).unwrap();

        assert_eq!(canonical, restored_canonical);
    }

    #[test]
    fn canonicalizer_rewrites_entity_refs_to_semantic_ids() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState {
            next_action_index: 1,
            day: 1,
        });

        let transport = app
            .world_mut()
            .spawn((
                ReplaySnapshotEntity,
                MapPosition::new(0, 0),
                Faction(PlayerFaction::OrangeStar),
                AwbwUnitId(awbrn_core::AwbwUnitId::new(1)),
                Unit(awbrn_core::Unit::APC),
                UnitActive,
            ))
            .id();
        let cargo = app
            .world_mut()
            .spawn((
                ReplaySnapshotEntity,
                MapPosition::new(0, 0),
                Faction(PlayerFaction::OrangeStar),
                AwbwUnitId(awbrn_core::AwbwUnitId::new(2)),
                Unit(awbrn_core::Unit::Infantry),
                TestEntityRef(transport),
            ))
            .id();
        let _ = cargo;

        let snapshot = capture_replay_semantic_snapshot(app.world_mut()).unwrap();
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
    fn canonicalizer_excludes_filter_only_snapshot_marker() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState::default());

        app.world_mut().spawn((
            ReplaySnapshotEntity,
            MapPosition::new(0, 0),
            TerrainTile {
                terrain: GraphicalTerrain::Plain,
            },
        ));

        let snapshot = capture_replay_semantic_snapshot(app.world_mut()).unwrap();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let canonical = canonicalize_replay_semantic_snapshot(&snapshot, &type_registry).unwrap();
        let terrain = canonical
            .entities
            .iter()
            .find(|entity| entity.id == "terrain:0,0")
            .unwrap();

        assert!(
            terrain
                .components
                .iter()
                .all(|component| !component.type_path.ends_with("ReplaySnapshotEntity"))
        );
    }

    #[test]
    fn canonicalizer_rejects_duplicate_semantic_ids() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState::default());

        app.world_mut().spawn((
            ReplaySnapshotEntity,
            MapPosition::new(0, 0),
            Faction(PlayerFaction::OrangeStar),
            AwbwUnitId(awbrn_core::AwbwUnitId::new(1)),
            Unit(awbrn_core::Unit::Infantry),
        ));
        app.world_mut().spawn((
            ReplaySnapshotEntity,
            MapPosition::new(1, 0),
            Faction(PlayerFaction::BlueMoon),
            AwbwUnitId(awbrn_core::AwbwUnitId::new(1)),
            Unit(awbrn_core::Unit::Mech),
        ));

        let snapshot = capture_replay_semantic_snapshot(app.world_mut()).unwrap();
        let type_registry = app.world().resource::<AppTypeRegistry>().read();
        let error = canonicalize_replay_semantic_snapshot(&snapshot, &type_registry).unwrap_err();

        assert!(matches!(
            error,
            ReplaySemanticSnapshotError::DuplicateSemanticId { ref id, .. } if id == "unit:1"
        ));
    }

    #[test]
    fn restore_reapplies_snapshot_marker() {
        let mut app = snapshot_test_app();
        app.world_mut().insert_resource(ReplayState::default());
        app.world_mut().spawn((
            ReplaySnapshotEntity,
            MapPosition::new(0, 0),
            TerrainTile {
                terrain: GraphicalTerrain::Plain,
            },
        ));

        let snapshot = capture_replay_semantic_snapshot(app.world_mut()).unwrap();

        let mut restored = snapshot_test_app();
        restore_replay_semantic_snapshot(restored.world_mut(), &snapshot).unwrap();

        let marker_count = {
            let mut query = restored
                .world_mut()
                .query_filtered::<Entity, With<ReplaySnapshotEntity>>();
            query.iter(restored.world()).count()
        };

        assert_eq!(marker_count, 1);
    }

    fn snapshot_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((StatesPlugin, CorePlugin, ReplayPlugin));
        app.register_type::<TestEntityRef>()
            .register_type_data::<TestEntityRef, ReplaySemanticComponentType>();
        app.insert_resource(CurrentWeather::default());
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(AwbrnMap::new(2, 2, GraphicalTerrain::Plain));
        app
    }
}
