pub mod animation;
pub mod fog_overlay;
pub mod map;
pub mod units;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

pub use units::{OverlayBlink, OverlayKind, OverlayVisual, UnitOverlayRegistry};

/// Resource to store loaded UI atlas for reuse
#[derive(Resource)]
pub struct UiAtlasResource {
    pub handle: Handle<crate::UiAtlasAsset>,
    pub texture: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
}

/// Resource to store the unit sprite atlas for reuse.
#[derive(Resource)]
pub struct UnitAtlasResource {
    pub texture: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
}

/// Resource to store the terrain sprite atlas for reuse.
#[derive(Resource)]
pub struct TerrainAtlasResource {
    pub texture: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
}

/// System parameter that bundles UI atlas resource and assets for convenient access.
#[derive(SystemParam)]
pub(crate) struct UiAtlas<'w> {
    atlas_res: Res<'w, UiAtlasResource>,
    atlas_assets: Res<'w, Assets<crate::UiAtlasAsset>>,
}

impl<'w> UiAtlas<'w> {
    /// Creates a sprite from the UI atlas for the given sprite name.
    ///
    /// # Panics
    ///
    /// Panics if the UI atlas is not loaded or if the sprite name is not found.
    pub(crate) fn sprite_for(&self, sprite_name: &str) -> Sprite {
        let ui_atlas = self
            .atlas_assets
            .get(&self.atlas_res.handle)
            .expect("UI atlas should be loaded");

        let index_map = ui_atlas.index_map();
        let sprite_index = *index_map
            .get(sprite_name)
            .unwrap_or_else(|| panic!("{} not found in UI atlas", sprite_name));

        Sprite::from_atlas_image(
            self.atlas_res.texture.clone(),
            TextureAtlas {
                layout: self.atlas_res.layout.clone(),
                index: sprite_index,
            },
        )
    }
}

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            units::UnitRenderingPlugin,
            map::MapVisualsPlugin,
            animation::AnimationPlugin,
            fog_overlay::FogOverlayPlugin,
        ));
    }
}
