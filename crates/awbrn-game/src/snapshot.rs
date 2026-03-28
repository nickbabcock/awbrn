use std::error::Error;
use std::fmt;

use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::reflect::AppTypeRegistry;
use bevy::prelude::*;
use bevy::scene::{DynamicScene, DynamicSceneBuilder, SceneFilter, SceneSpawnError};

use crate::MapPosition;
use crate::replay::{AwbwUnitId, ReplayFogDirty, ReplayState};
use crate::world::{
    Ammo, Capturing, CarriedBy, Faction, FogActive, FogOfWarMap, FriendlyFactions, Fuel,
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
            .register_type::<Capturing>()
            .register_type_data::<Capturing, ReplaySemanticComponentType>()
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
