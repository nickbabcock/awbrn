use crate::core::map::{GameMap, TerrainTile};
use crate::core::{MapPosition, SpriteSize};
use crate::render::animation::TerrainAnimation;
use awbrn_core::GraphicalTerrain;
use awbrn_map::Position;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use std::time::Duration;

#[derive(Component)]
#[require(SpriteSize { width: 16.0, height: 32.0, z_index: -1 })]
pub struct MapBackdrop;

#[derive(Component)]
pub(crate) struct AnimatedTerrain;

pub(crate) fn setup_map_visuals(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    current_weather: Res<crate::features::weather::CurrentWeather>,
    game_map: Res<GameMap>,
) {
    let texture = asset_server.load("textures/tiles.png");
    let layout = TextureAtlasLayout::from_grid(
        UVec2::new(16, 32),
        awbrn_core::TILESHEET_COLUMNS,
        awbrn_core::TILESHEET_ROWS,
        None,
        None,
    );
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    let plain_index =
        awbrn_core::spritesheet_index(current_weather.weather(), GraphicalTerrain::Plain);

    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            let position = Position::new(x, y);
            if let Some(terrain) = game_map.terrain_at(position) {
                commands.spawn((
                    Sprite::from_atlas_image(
                        texture.clone(),
                        TextureAtlas {
                            layout: texture_atlas_layout.clone(),
                            index: plain_index.index() as usize,
                        },
                    ),
                    Anchor::default(),
                    MapPosition::new(x, y),
                    MapBackdrop,
                ));

                let sprite_index =
                    awbrn_core::spritesheet_index(current_weather.weather(), terrain);

                let mut entity_commands = commands.spawn((
                    Sprite::from_atlas_image(
                        texture.clone(),
                        TextureAtlas {
                            layout: texture_atlas_layout.clone(),
                            index: sprite_index.index() as usize,
                        },
                    ),
                    Anchor::default(),
                    MapPosition::new(x, y),
                    TerrainTile {
                        terrain,
                        position: Position::new(x, y),
                    },
                ));

                if sprite_index.animation_frames() > 1 {
                    let frame_durations = awbrn_core::get_terrain_animation_frames(terrain);
                    let initial_duration = frame_durations
                        .as_ref()
                        .map(|f| f.get_duration(0))
                        .unwrap_or(300);
                    entity_commands.insert((
                        TerrainAnimation {
                            start_index: sprite_index.index(),
                            frame_count: sprite_index.animation_frames(),
                            current_frame: 0,
                            frame_timer: Timer::new(
                                Duration::from_millis(initial_duration as u64),
                                TimerMode::Once,
                            ),
                            frame_durations,
                        },
                        AnimatedTerrain,
                    ));
                }
            }
        }
    }
}

pub struct MapVisualsPlugin;

impl Plugin for MapVisualsPlugin {
    fn build(&self, _app: &mut App) {
        // Map visuals setup is triggered by the orchestrator on LoadingState::Complete
    }
}
