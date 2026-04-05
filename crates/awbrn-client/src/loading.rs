use crate::UiAtlasAsset;
use crate::core::{AppState, GameMode, LoadingState};
use crate::features::event_bus::{EventSink, ReplayLoaded, ReplayLoadedPlayer};
use crate::render::UiAtlasResource;
use awbrn_content::co_portrait_by_awbw_id;
use awbrn_game::world::GameMap;
use awbrn_map::{AwbrnMap, AwbwMap, AwbwMapData, Position};
use awbw_replay::game_models::AwbwPlayer;
use awbw_replay::{AwbwReplay, ReplayParser, game_models::AwbwBuilding};
use bevy::ecs::system::SystemParam;
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

/// Trait for resolving static asset paths from logical asset keys.
pub trait StaticAssetPathResolver: Send + Sync + 'static {
    fn resolve_path(&self, logical_path: &str) -> String;
}

/// Default implementation of StaticAssetPathResolver.
pub struct DefaultStaticAssetPathResolver;

impl StaticAssetPathResolver for DefaultStaticAssetPathResolver {
    fn resolve_path(&self, logical_path: &str) -> String {
        logical_path.to_string()
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
pub struct PendingGameStart(pub u32);

#[derive(Resource, Clone)]
pub(crate) struct MapPathResolver(pub(crate) Arc<dyn MapAssetPathResolver>);

#[derive(Resource, Clone)]
pub(crate) struct StaticPathResolver(pub(crate) Arc<dyn StaticAssetPathResolver>);

#[derive(Resource)]
pub(crate) struct MapAssetHandle(Handle<AwbwMapAsset>);

#[derive(Resource)]
pub(crate) struct PendingUiAtlas {
    pub(crate) atlas: Handle<UiAtlasAsset>,
    pub(crate) texture: Handle<Image>,
}

#[derive(Resource)]
struct PendingReplayLoadedEvent(ReplayLoaded);

#[derive(SystemParam)]
pub(crate) struct LoadingTransitions<'w> {
    app_state: ResMut<'w, NextState<AppState>>,
    game_mode_state: ResMut<'w, NextState<GameMode>>,
    loading_state: ResMut<'w, NextState<LoadingState>>,
}

impl LoadingTransitions<'_> {
    fn begin_loading(&mut self, game_mode: GameMode) {
        self.game_mode_state.set(game_mode);
        self.app_state.set(AppState::Loading);
        self.loading_state.set(LoadingState::LoadingAssets);
    }
}

#[derive(SystemParam)]
pub(crate) struct ClientAssetLoader<'w> {
    map_resolver: Res<'w, MapPathResolver>,
    static_resolver: Res<'w, StaticPathResolver>,
    asset_server: Res<'w, AssetServer>,
}

impl ClientAssetLoader<'_> {
    pub fn load_map(&self, map_id: u32) -> Handle<AwbwMapAsset> {
        let asset_path = self.map_resolver.0.resolve_path(map_id);
        self.asset_server.load(asset_path)
    }

    fn load_static<A: Asset>(&self, logical_path: &str) -> Handle<A> {
        let asset_path = self.static_resolver.0.resolve_path(logical_path);
        self.asset_server.load(asset_path)
    }

    pub fn load_ui_texture(&self) -> Handle<Image> {
        self.load_static("textures/ui.png")
    }

    pub fn load_ui_atlas(&self) -> Handle<UiAtlasAsset> {
        self.load_static("data/ui_atlas.json")
    }

    pub fn load_unit_texture(&self) -> Handle<Image> {
        self.load_static("textures/units.png")
    }

    pub fn load_terrain_texture(&self) -> Handle<Image> {
        self.load_static("textures/tiles.png")
    }

    fn load_pending_ui_atlas(&self) -> PendingUiAtlas {
        let atlas = self.load_ui_atlas();
        let texture = self.load_ui_texture();
        PendingUiAtlas { atlas, texture }
    }
}

pub(crate) fn detect_replay_to_load(
    mut commands: Commands,
    replay_to_load: Res<ReplayToLoad>,
    mut transitions: LoadingTransitions,
    asset_loader: ClientAssetLoader,
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

    if let Some(replay_loaded) = replay_loaded_event(&replay) {
        commands.insert_resource(PendingReplayLoadedEvent(replay_loaded));
    }

    if let Some(first_game) = replay.games.first() {
        let map_id = first_game.maps_id;
        info!("Found map ID: {:?} in replay", map_id);

        let map_handle = asset_loader.load_map(map_id.as_u32());
        commands.insert_resource(MapAssetHandle(map_handle));
    } else {
        error!("No games found in replay");
        let map_handle = asset_loader.load_map(162795);
        commands.insert_resource(MapAssetHandle(map_handle));
    }

    commands.insert_resource(asset_loader.load_pending_ui_atlas());

    commands.insert_resource(LoadedReplay(replay));
    transitions.begin_loading(GameMode::Replay);
    info!("Started loading replay mode");
}

fn emit_pending_replay_loaded_event(
    mut commands: Commands,
    pending_event: Res<PendingReplayLoadedEvent>,
    sink: Option<Res<EventSink<ReplayLoaded>>>,
) {
    if let Some(sink) = sink {
        sink.emit(pending_event.0.clone());
    }
    commands.remove_resource::<PendingReplayLoadedEvent>();
}

pub(crate) fn detect_pending_game_start(
    mut commands: Commands,
    pending_game: Res<PendingGameStart>,
    mut transitions: LoadingTransitions,
    asset_loader: ClientAssetLoader,
) {
    let map_handle = asset_loader.load_map(pending_game.0);
    commands.insert_resource(MapAssetHandle(map_handle));
    commands.remove_resource::<PendingGameStart>();

    commands.insert_resource(asset_loader.load_pending_ui_atlas());

    transitions.begin_loading(GameMode::Game);
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

fn replay_loaded_event(replay: &AwbwReplay) -> Option<ReplayLoaded> {
    let first_game = replay.games.first()?;
    let mut players = first_game.players.clone();
    players.sort_by_key(|player| player.order);

    Some(ReplayLoaded {
        game_id: first_game.id.as_u32(),
        map_id: first_game.maps_id.as_u32(),
        day: first_game.day,
        fog: first_game.fog,
        team_game: first_game.team,
        players: players
            .iter()
            .map(|player| replay_loaded_player(player, first_game.team))
            .collect(),
    })
}

fn replay_loaded_player(player: &AwbwPlayer, team_game: bool) -> ReplayLoadedPlayer {
    let co = co_portrait_by_awbw_id(player.co_id);
    if co.is_none() {
        warn!(
            "Unknown active CO id {} for replay player {}",
            player.co_id,
            player.id.as_u32()
        );
    }

    let tag_co = player.tags_co_id.and_then(|co_id| {
        let portrait = co_portrait_by_awbw_id(co_id);
        if portrait.is_none() {
            warn!(
                "Unknown tag CO id {} for replay player {}",
                co_id,
                player.id.as_u32()
            );
        }
        portrait
    });

    ReplayLoadedPlayer {
        player_id: player.id.as_u32(),
        user_id: player.users_id.as_u32(),
        order: player.order,
        team: team_game.then(|| player.team.clone()),
        eliminated: player.eliminated,
        faction_code: player.faction.country_code().to_string(),
        faction_name: player.faction.name().to_string(),
        co_key: co.map(|portrait| portrait.key().to_string()),
        co_name: co.map(|portrait| portrait.display_name().to_string()),
        tag_co_key: tag_co.map(|portrait| portrait.key().to_string()),
        tag_co_name: tag_co.map(|portrait| portrait.display_name().to_string()),
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
    static_asset_resolver: Arc<dyn StaticAssetPathResolver>,
}

impl LoadingPlugin {
    pub fn new(
        map_resolver: Arc<dyn MapAssetPathResolver>,
        static_asset_resolver: Arc<dyn StaticAssetPathResolver>,
    ) -> Self {
        Self {
            map_resolver,
            static_asset_resolver,
        }
    }
}

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::JsonAssetPlugin::<AwbwMapAsset>::new())
            .add_plugins(crate::JsonAssetPlugin::<UiAtlasAsset>::new())
            .insert_resource(MapPathResolver(self.map_resolver.clone()))
            .insert_resource(StaticPathResolver(self.static_asset_resolver.clone()))
            .add_systems(
                Update,
                check_assets_loaded.run_if(in_state(LoadingState::LoadingAssets)),
            )
            .add_systems(
                Update,
                (
                    detect_replay_to_load.run_if(resource_exists::<ReplayToLoad>),
                    detect_pending_game_start.run_if(resource_exists::<PendingGameStart>),
                    emit_pending_replay_loaded_event
                        .run_if(resource_exists::<PendingReplayLoadedEvent>),
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
    use awbw_replay::AwbwReplay;
    use awbw_replay::game_models::{AwbwGame, AwbwPlayer, CoPower, MatchType};
    use std::collections::HashMap;

    struct MappingStaticAssetPathResolver {
        entries: HashMap<String, String>,
    }

    impl StaticAssetPathResolver for MappingStaticAssetPathResolver {
        fn resolve_path(&self, logical_path: &str) -> String {
            self.entries
                .get(logical_path)
                .cloned()
                .unwrap_or_else(|| logical_path.to_string())
        }
    }

    #[test]
    fn default_static_asset_resolver_returns_logical_path() {
        let resolver = DefaultStaticAssetPathResolver;

        assert_eq!(resolver.resolve_path("textures/ui.png"), "textures/ui.png");
    }

    #[test]
    fn mapping_static_asset_resolver_returns_override_when_present() {
        let resolver = MappingStaticAssetPathResolver {
            entries: HashMap::from([(
                "textures/ui.png".to_string(),
                "https://cdn.example.com/ui-123.png".to_string(),
            )]),
        };

        assert_eq!(
            resolver.resolve_path("textures/ui.png"),
            "https://cdn.example.com/ui-123.png"
        );
    }

    #[test]
    fn mapping_static_asset_resolver_falls_back_to_logical_path_when_missing() {
        let resolver = MappingStaticAssetPathResolver {
            entries: HashMap::new(),
        };

        assert_eq!(resolver.resolve_path("textures/ui.png"), "textures/ui.png");
    }

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

    #[test]
    fn replay_loaded_event_resolves_cos_and_sorts_players() {
        let replay = AwbwReplay {
            games: vec![AwbwGame {
                id: awbrn_types::AwbwGameId::new(7),
                name: "Test Replay".to_string(),
                password: None,
                creator: awbrn_types::AwbwPlayerId::new(99),
                start_date: "2026-03-28".to_string(),
                end_date: None,
                activity_date: "2026-03-28".to_string(),
                maps_id: awbrn_types::AwbwMapId::new(162795),
                weather_type: "Clear".to_string(),
                weather_start: None,
                weather_code: "C".to_string(),
                win_condition: None,
                turn: 1,
                day: 4,
                active: true,
                funds: 1000,
                capture_win: 0,
                fog: true,
                comment: None,
                game_type: MatchType::Tag,
                boot_interval: 0,
                starting_funds: 1000,
                official: false,
                min_rating: 0,
                max_rating: None,
                league: None,
                team: true,
                aet_interval: 0,
                aet_date: "2026-03-28".to_string(),
                use_powers: true,
                players: vec![
                    test_player(2, 2, PlayerFaction::BlueMoon, 30, Some(31), "B"),
                    test_player(1, 1, PlayerFaction::OrangeStar, 11, None, "A"),
                ],
                buildings: Vec::new(),
                units: Vec::new(),
                timers_initial: None,
                timers_increment: 0,
                timers_max_turn: 0,
            }],
            turns: Vec::new(),
        };

        let replay_loaded = replay_loaded_event(&replay).expect("replay should emit roster event");
        assert_eq!(replay_loaded.game_id, 7);
        assert_eq!(replay_loaded.map_id, 162795);
        assert_eq!(replay_loaded.day, 4);
        assert_eq!(replay_loaded.players.len(), 2);
        assert_eq!(replay_loaded.players[0].player_id, 1);
        assert_eq!(replay_loaded.players[0].user_id, 101);
        assert_eq!(replay_loaded.players[0].co_key.as_deref(), Some("adder"));
        assert_eq!(replay_loaded.players[0].co_name.as_deref(), Some("Adder"));
        assert_eq!(replay_loaded.players[0].team.as_deref(), Some("A"));
        assert_eq!(replay_loaded.players[1].co_key.as_deref(), Some("von-bolt"));
        assert_eq!(
            replay_loaded.players[1].tag_co_key.as_deref(),
            Some("no-co")
        );
    }

    #[test]
    fn replay_loaded_event_uses_null_for_unknown_cos() {
        let replay = AwbwReplay {
            games: vec![AwbwGame {
                id: awbrn_types::AwbwGameId::new(9),
                name: "Unknown CO Replay".to_string(),
                password: None,
                creator: awbrn_types::AwbwPlayerId::new(99),
                start_date: "2026-03-28".to_string(),
                end_date: None,
                activity_date: "2026-03-28".to_string(),
                maps_id: awbrn_types::AwbwMapId::new(162795),
                weather_type: "Clear".to_string(),
                weather_start: None,
                weather_code: "C".to_string(),
                win_condition: None,
                turn: 1,
                day: 1,
                active: true,
                funds: 1000,
                capture_win: 0,
                fog: false,
                comment: None,
                game_type: MatchType::Normal,
                boot_interval: 0,
                starting_funds: 1000,
                official: false,
                min_rating: 0,
                max_rating: None,
                league: None,
                team: false,
                aet_interval: 0,
                aet_date: "2026-03-28".to_string(),
                use_powers: true,
                players: vec![test_player(
                    1,
                    1,
                    PlayerFaction::OrangeStar,
                    999,
                    Some(888),
                    "A",
                )],
                buildings: Vec::new(),
                units: Vec::new(),
                timers_initial: None,
                timers_increment: 0,
                timers_max_turn: 0,
            }],
            turns: Vec::new(),
        };

        let replay_loaded = replay_loaded_event(&replay).expect("replay should emit roster event");
        assert_eq!(replay_loaded.players.len(), 1);
        assert_eq!(replay_loaded.players[0].co_key, None);
        assert_eq!(replay_loaded.players[0].co_name, None);
        assert_eq!(replay_loaded.players[0].tag_co_key, None);
        assert_eq!(replay_loaded.players[0].tag_co_name, None);
        assert_eq!(replay_loaded.players[0].team, None);
    }

    fn test_player(
        player_id: u32,
        order: u32,
        faction: PlayerFaction,
        co_id: u32,
        tag_co_id: Option<u32>,
        team: &str,
    ) -> AwbwPlayer {
        AwbwPlayer {
            id: awbrn_types::AwbwGamePlayerId::new(player_id),
            users_id: awbrn_types::AwbwPlayerId::new(100 + player_id),
            games_id: awbrn_types::AwbwGameId::new(1),
            faction,
            co_id,
            funds: 0,
            turn: None,
            email: None,
            uniq_id: None,
            eliminated: false,
            last_read: "2026-03-28".to_string(),
            last_read_broadcasts: None,
            emailpress: None,
            signature: None,
            co_power: 0,
            co_power_on: CoPower::None,
            order,
            accept_draw: false,
            co_max_power: 0,
            co_max_spower: 0,
            co_image: None,
            team: team.to_string(),
            aet_count: 0,
            turn_start: "2026-03-28".to_string(),
            turn_clock: 0,
            tags_co_id: tag_co_id,
            tags_co_power: None,
            tags_co_max_power: None,
            tags_co_max_spower: None,
            interface: false,
        }
    }
}
