use crate::UiAtlasAsset;
use crate::core::{AppState, GameMode, LoadingState};
use crate::render::UiAtlasResource;
use awbrn_game::world::GameMap;
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData, Position};
use awbw_replay::{AwbwReplay, ReplayParser, game_models::AwbwBuilding};
use bevy::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

/// Trait for resolving map asset paths from map IDs
pub trait MapAssetPathResolver: Send + Sync + 'static {
    fn resolve_path(&self, map_id: u32) -> String;
}

/// Default implementation of MapAssetPathResolver
pub struct DefaultMapAssetPathResolver;

impl MapAssetPathResolver for DefaultMapAssetPathResolver {
    fn resolve_path(&self, map_id: u32) -> String {
        format!("maps/{}.json", map_id)
    }
}

#[derive(Asset, TypePath, Deserialize)]
#[serde(transparent)]
pub struct AwbwMapAsset(AwbwMapData);

impl AwbwMapAsset {
    fn to_awbw_map(&self) -> AwbwMap {
        AwbwMap::try_from(&self.0).unwrap()
    }
}

/// Resource containing the raw replay data to parse and load
#[derive(Resource)]
pub struct ReplayToLoad(pub Vec<u8>);

/// Resource containing the loaded replay data
#[derive(Resource)]
pub struct LoadedReplay(pub AwbwReplay);

/// Resource to mark that a new game should be started
#[derive(Resource)]
pub struct PendingGameStart(pub Handle<AwbwMapAsset>);

#[derive(Resource, Clone)]
pub(crate) struct MapPathResolver(pub(crate) Arc<dyn MapAssetPathResolver>);

#[derive(Resource)]
pub(crate) struct MapAssetHandle(Handle<AwbwMapAsset>);

#[derive(Resource)]
pub(crate) struct PendingUiAtlas {
    pub(crate) atlas: Handle<UiAtlasAsset>,
    pub(crate) texture: Handle<Image>,
}

pub(crate) fn detect_replay_to_load(
    mut commands: Commands,
    replay_to_load: Res<ReplayToLoad>,
    mut app_state: ResMut<NextState<AppState>>,
    mut game_mode_state: ResMut<NextState<GameMode>>,
    mut loading_state: ResMut<NextState<LoadingState>>,
    map_resolver: Res<MapPathResolver>,
    asset_server: Res<AssetServer>,
) {
    commands.remove_resource::<ReplayToLoad>();

    let parser = ReplayParser::new();
    let replay = match parser.parse(&replay_to_load.0) {
        Ok(replay) => replay,
        Err(e) => {
            error!("Failed to parse replay: {:?}", e);
            return;
        }
    };

    if let Some(first_game) = replay.games.first() {
        let map_id = first_game.maps_id;
        info!("Found map ID: {:?} in replay", map_id);

        let asset_path = map_resolver.0.resolve_path(map_id.as_u32());
        let map_handle: Handle<AwbwMapAsset> = asset_server.load(asset_path);
        commands.insert_resource(MapAssetHandle(map_handle));
    } else {
        error!("No games found in replay");
        let asset_path = map_resolver.0.resolve_path(162795);
        let map_handle: Handle<AwbwMapAsset> = asset_server.load(asset_path);
        commands.insert_resource(MapAssetHandle(map_handle));
    }

    let ui_atlas_handle = asset_server.load("data/ui_atlas.json");
    let ui_texture_handle = asset_server.load("textures/ui.png");
    commands.insert_resource(PendingUiAtlas {
        atlas: ui_atlas_handle,
        texture: ui_texture_handle,
    });

    commands.insert_resource(LoadedReplay(replay));
    game_mode_state.set(GameMode::Replay);
    app_state.set(AppState::Loading);
    loading_state.set(LoadingState::LoadingAssets);
    info!("Started loading replay mode");
}

pub(crate) fn detect_pending_game_start(
    mut commands: Commands,
    pending_game: Res<PendingGameStart>,
    mut app_state: ResMut<NextState<AppState>>,
    mut game_mode_state: ResMut<NextState<GameMode>>,
    mut loading_state: ResMut<NextState<LoadingState>>,
    asset_server: Res<AssetServer>,
) {
    commands.insert_resource(MapAssetHandle(pending_game.0.clone()));
    commands.remove_resource::<PendingGameStart>();

    let ui_atlas_handle = asset_server.load("data/ui_atlas.json");
    let ui_texture_handle = asset_server.load("textures/ui.png");
    commands.insert_resource(PendingUiAtlas {
        atlas: ui_atlas_handle,
        texture: ui_texture_handle,
    });

    game_mode_state.set(GameMode::Game);
    app_state.set(AppState::Loading);
    loading_state.set(LoadingState::LoadingAssets);
    info!("Started game mode");
}

pub(crate) fn check_assets_loaded(
    map_handle: Res<MapAssetHandle>,
    pending_ui: Res<PendingUiAtlas>,
    awbw_maps: Res<Assets<AwbwMapAsset>>,
    ui_atlas_assets: Res<Assets<UiAtlasAsset>>,
    loaded_replay: Option<Res<LoadedReplay>>,
    mut game_map: ResMut<GameMap>,
    mut next_state: ResMut<NextState<LoadingState>>,
) {
    let Some(awbw_map_asset) = awbw_maps.get(&map_handle.0) else {
        return;
    };

    if ui_atlas_assets.get(&pending_ui.atlas).is_none() {
        return;
    }

    let mut awbw_map = awbw_map_asset.to_awbw_map();
    if let Some(replay) = loaded_replay
        && let Some(first_game) = replay.0.games.first()
    {
        apply_replay_building_overrides(&mut awbw_map, &first_game.buildings);
    }
    let awbrn_map = AwbrnMap::from_map(&awbw_map);

    info!(
        "Map asset processed: {}x{}. UI atlas loaded. Transitioning to Complete state.",
        awbrn_map.width(),
        awbrn_map.height()
    );

    game_map.set(awbrn_map);
    next_state.set(LoadingState::Complete);
}

pub fn apply_replay_building_overrides(map: &mut AwbwMap, buildings: &[AwbwBuilding]) {
    for building in buildings {
        let position = Position::new(building.x as usize, building.y as usize);
        let Some(terrain) = map.terrain_at_mut(position) else {
            warn!(
                "Skipping replay building override at out-of-bounds position {:?}",
                position
            );
            continue;
        };

        *terrain = building.terrain_id;
    }
}

pub(crate) fn setup_ui_atlas(
    mut commands: Commands,
    pending_ui: Res<PendingUiAtlas>,
    ui_atlas_assets: Res<Assets<UiAtlasAsset>>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let ui_atlas = ui_atlas_assets
        .get(&pending_ui.atlas)
        .expect("UI atlas should be loaded before setup");

    let layout = ui_atlas.layout();
    let layout_handle = texture_atlas_layouts.add(layout);

    commands.insert_resource(UiAtlasResource {
        handle: pending_ui.atlas.clone(),
        texture: pending_ui.texture.clone(),
        layout: layout_handle,
    });

    info!("UI atlas resource initialized");
}

pub(crate) fn transition_to_in_game(mut next_app_state: ResMut<NextState<AppState>>) {
    next_app_state.set(AppState::InGame);
}

pub struct LoadingPlugin {
    map_resolver: Arc<dyn MapAssetPathResolver>,
}

impl LoadingPlugin {
    pub fn new(map_resolver: Arc<dyn MapAssetPathResolver>) -> Self {
        Self { map_resolver }
    }
}

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::JsonAssetPlugin::<AwbwMapAsset>::new())
            .add_plugins(crate::JsonAssetPlugin::<UiAtlasAsset>::new())
            .insert_resource(MapPathResolver(self.map_resolver.clone()))
            .add_systems(
                Update,
                check_assets_loaded.run_if(in_state(LoadingState::LoadingAssets)),
            )
            .add_systems(
                Update,
                (
                    detect_replay_to_load.run_if(resource_exists::<ReplayToLoad>),
                    detect_pending_game_start.run_if(resource_exists::<PendingGameStart>),
                ),
            )
            .add_systems(
                OnEnter(LoadingState::Complete),
                (setup_ui_atlas, transition_to_in_game),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_types::{AwbwTerrain, Faction as TerrainFaction, PlayerFaction, Property};

    #[test]
    fn test_apply_replay_building_overrides_updates_owned_property_terrain() {
        let mut map = AwbwMap::new(3, 3, AwbwTerrain::Plain);
        *map.terrain_at_mut(Position::new(1, 1)).unwrap() =
            AwbwTerrain::Property(Property::City(TerrainFaction::Neutral));

        apply_replay_building_overrides(
            &mut map,
            &[AwbwBuilding {
                id: 1,
                games_id: 7,
                terrain_id: AwbwTerrain::Property(Property::City(TerrainFaction::Player(
                    PlayerFaction::BlueMoon,
                ))),
                x: 1,
                y: 1,
                capture: 20,
                last_capture: 20,
                last_updated: "2026-03-14".to_string(),
            }],
        );

        assert_eq!(
            map.terrain_at(Position::new(1, 1)),
            Some(AwbwTerrain::Property(Property::City(
                TerrainFaction::Player(PlayerFaction::BlueMoon)
            )))
        );
    }

    #[test]
    fn test_apply_replay_building_overrides_ignores_out_of_bounds_positions() {
        let mut map = AwbwMap::new(2, 2, AwbwTerrain::Plain);

        apply_replay_building_overrides(
            &mut map,
            &[AwbwBuilding {
                id: 1,
                games_id: 7,
                terrain_id: AwbwTerrain::Property(Property::HQ(PlayerFaction::PurpleLightning)),
                x: 99,
                y: 99,
                capture: 20,
                last_capture: 20,
                last_updated: "2026-03-14".to_string(),
            }],
        );

        assert_eq!(
            map.terrain_at(Position::new(0, 0)),
            Some(AwbwTerrain::Plain)
        );
        assert_eq!(
            map.terrain_at(Position::new(1, 1)),
            Some(AwbwTerrain::Plain)
        );
    }
}
