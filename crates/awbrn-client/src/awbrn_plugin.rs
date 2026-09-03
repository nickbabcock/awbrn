//! Bevy plugin for AWBRN with support for multiple game modes.
//!
//! ```mermaid
//! stateDiagram-v2
//!     [*] --> Menu
//!
//!     state AppState {
//!         Menu --> Loading : ReplayToLoad resource<br/>or PendingGameStart resource
//!         Loading --> InGame : LoadingState Complete
//!         InGame --> Menu : User action
//!
//!         state Loading {
//!             [*] --> LoadingReplay : Replay mode
//!             [*] --> LoadingAssets : Game mode or<br/>after replay parsed
//!             LoadingReplay --> LoadingAssets : Replay parsed<br/>map loading starts
//!             LoadingAssets --> Complete : Map loaded
//!             Complete --> [*] : Transition to InGame
//!         }
//!     }
//!
//!     state GameMode {
//!         None --> Replay : ReplayToLoad resource
//!         None --> Game : PendingGameStart resource
//!         Replay --> None : Reset
//!         Game --> None : Reset
//!     }
//!
//!     note right of GameMode : Independent state<br/>determines active systems<br/>in InGame
//! ```

use crate::core::{GameMode, LoadingState};
use crate::features::event_bus;
use crate::loading::{
    DefaultStaticAssetPathResolver, LoadingPlugin, MapAssetPathResolver, StaticAssetPathResolver,
};
use bevy::prelude::*;
use std::sync::Arc;

pub struct AwbrnPlugin {
    map_resolver: Arc<dyn MapAssetPathResolver>,
    static_asset_resolver: Arc<dyn StaticAssetPathResolver>,
}

impl std::fmt::Debug for AwbrnPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwbrnPlugin").finish_non_exhaustive()
    }
}

impl AwbrnPlugin {
    pub fn new(map_resolver: Arc<dyn MapAssetPathResolver>) -> Self {
        Self {
            map_resolver,
            static_asset_resolver: Arc::new(DefaultStaticAssetPathResolver),
        }
    }

    pub fn with_static_asset_resolver(
        mut self,
        static_asset_resolver: Arc<dyn StaticAssetPathResolver>,
    ) -> Self {
        self.static_asset_resolver = static_asset_resolver;
        self
    }
}

impl Default for AwbrnPlugin {
    fn default() -> Self {
        Self {
            map_resolver: Arc::new(crate::loading::DefaultMapAssetPathResolver),
            static_asset_resolver: Arc::new(DefaultStaticAssetPathResolver),
        }
    }
}

impl Plugin for AwbrnPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::core::CorePlugin,
            LoadingPlugin::new(
                Arc::clone(&self.map_resolver),
                Arc::clone(&self.static_asset_resolver),
            ),
            crate::features::FeaturesPlugin,
            crate::projection::ClientProjectionPlugin,
            crate::render::RenderPlugin,
            crate::modes::replay::ReplayPlugin,
            crate::modes::play::PlayPlugin,
            // After the play mode, whose selection a reading follows.
            crate::modes::play::inspect::InspectionPlugin,
        ));

        // Cross-plugin OnEnter(Complete) scheduling
        app.add_systems(
            OnEnter(LoadingState::Complete),
            event_bus::emit_map_dimensions
                .run_if(resource_exists::<event_bus::EventSink<event_bus::MapDimensions>>),
        );
        app.add_systems(
            OnEnter(LoadingState::Complete),
            crate::features::input::spawn_tile_cursor.after(crate::loading::setup_ui_atlas),
        );
        app.add_systems(
            OnEnter(LoadingState::Complete),
            crate::modes::replay::bootstrap::initialize_replay_semantic_world_for_client
                .run_if(in_state(GameMode::Replay)),
        );
        app.add_systems(
            OnEnter(LoadingState::Complete),
            crate::modes::play::initialize_live_semantic_world.run_if(in_state(GameMode::Game)),
        );
        app.add_systems(
            OnEnter(LoadingState::Complete),
            crate::render::fog_overlay::spawn_fog_overlay_tiles,
        );
    }
}
