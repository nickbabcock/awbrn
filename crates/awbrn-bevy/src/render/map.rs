use crate::core::map::{GameMap, TerrainTile};
use crate::core::{MapPosition, RenderLayer, SpriteSize};
use crate::features::weather::CurrentWeather;
use crate::render::TerrainAtlasResource;
use crate::render::animation::TerrainAnimation;
use awbrn_core::GraphicalTerrain;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use std::time::Duration;

#[derive(Component)]
#[require(SpriteSize { width: 16.0, height: 32.0, z_index: RenderLayer::BACKDROP })]
pub struct MapBackdrop;

#[derive(Component)]
pub(crate) struct AnimatedTerrain;

pub(crate) fn setup_terrain_atlas(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_server.load("textures/tiles.png");
    let layout = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(16, 32),
        awbrn_core::TILESHEET_COLUMNS,
        awbrn_core::TILESHEET_ROWS,
        None,
        None,
    ));

    commands.insert_resource(TerrainAtlasResource { texture, layout });
}

pub(crate) fn setup_map_backdrops(
    mut commands: Commands,
    terrain_atlas: Res<TerrainAtlasResource>,
    current_weather: Res<crate::features::weather::CurrentWeather>,
    game_map: Res<GameMap>,
    backdrops: Query<Entity, With<MapBackdrop>>,
) {
    for entity in &backdrops {
        commands.entity(entity).despawn();
    }

    let plain_index =
        awbrn_core::spritesheet_index(current_weather.weather(), GraphicalTerrain::Plain);

    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            commands.spawn((
                Sprite::from_atlas_image(
                    terrain_atlas.texture.clone(),
                    TextureAtlas {
                        layout: terrain_atlas.layout.clone(),
                        index: plain_index.index() as usize,
                    },
                ),
                Anchor::default(),
                MapPosition::new(x, y),
                MapBackdrop,
            ));
        }
    }
}

pub(crate) fn on_terrain_tile_insert(
    trigger: On<Insert, TerrainTile>,
    mut commands: Commands,
    terrain_tiles: Query<&TerrainTile>,
    current_weather: Option<Res<CurrentWeather>>,
    terrain_atlas: Res<TerrainAtlasResource>,
) {
    let entity = trigger.entity;
    let Ok(terrain_tile) = terrain_tiles.get(entity) else {
        warn!("TerrainTile entity {:?} not found in query", entity);
        return;
    };

    let weather = current_weather
        .as_ref()
        .map_or(awbrn_core::Weather::Clear, |weather| weather.weather());

    insert_terrain_visual(
        commands.entity(entity),
        &terrain_atlas,
        weather,
        terrain_tile.terrain,
    );
}

fn insert_terrain_visual(
    mut entity_commands: EntityCommands,
    terrain_atlas: &TerrainAtlasResource,
    weather: awbrn_core::Weather,
    terrain: GraphicalTerrain,
) {
    let sprite_index = awbrn_core::spritesheet_index(weather, terrain);

    entity_commands.insert((
        Sprite::from_atlas_image(
            terrain_atlas.texture.clone(),
            TextureAtlas {
                layout: terrain_atlas.layout.clone(),
                index: sprite_index.index() as usize,
            },
        ),
        Anchor::default(),
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
    } else {
        entity_commands.remove::<(TerrainAnimation, AnimatedTerrain)>();
    }
}

pub struct MapVisualsPlugin;

impl Plugin for MapVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_terrain_atlas)
            .add_observer(on_terrain_tile_insert);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_core::{Faction, PlayerFaction, Property, SeaDirection, Weather};

    #[test]
    fn terrain_insert_sets_sprite_state_for_static_terrain() {
        let mut app = App::new();
        app.insert_resource(CurrentWeather::default());
        app.insert_resource(TerrainAtlasResource {
            texture: Handle::default(),
            layout: Handle::default(),
        });
        app.add_observer(on_terrain_tile_insert);

        let entity = app
            .world_mut()
            .spawn(TerrainTile {
                terrain: GraphicalTerrain::Plain,
            })
            .id();

        let sprite = app.world().entity(entity).get::<Sprite>().unwrap();
        let atlas = sprite.texture_atlas.as_ref().unwrap();

        assert_eq!(
            atlas.index,
            awbrn_core::spritesheet_index(Weather::Clear, GraphicalTerrain::Plain).index() as usize
        );
        assert!(
            app.world()
                .entity(entity)
                .get::<TerrainAnimation>()
                .is_none()
        );
        assert!(
            app.world()
                .entity(entity)
                .get::<AnimatedTerrain>()
                .is_none()
        );
    }

    #[test]
    fn terrain_replace_refreshes_sprite_and_animation_components() {
        let mut app = App::new();
        app.insert_resource(CurrentWeather::default());
        app.insert_resource(TerrainAtlasResource {
            texture: Handle::default(),
            layout: Handle::default(),
        });
        app.add_observer(on_terrain_tile_insert);

        let entity = app
            .world_mut()
            .spawn(TerrainTile {
                terrain: GraphicalTerrain::Property(Property::City(Faction::Player(
                    PlayerFaction::OrangeStar,
                ))),
            })
            .id();

        assert!(app.world().entity(entity).contains::<TerrainAnimation>());
        assert!(app.world().entity(entity).contains::<AnimatedTerrain>());

        app.world_mut().entity_mut(entity).insert(TerrainTile {
            terrain: GraphicalTerrain::Sea(SeaDirection::N_E_S_W),
        });

        let sprite = app.world().entity(entity).get::<Sprite>().unwrap();
        let atlas = sprite.texture_atlas.as_ref().unwrap();

        assert_eq!(
            atlas.index,
            awbrn_core::spritesheet_index(
                Weather::Clear,
                GraphicalTerrain::Sea(SeaDirection::N_E_S_W)
            )
            .index() as usize
        );
        assert!(
            app.world()
                .entity(entity)
                .get::<TerrainAnimation>()
                .is_none()
        );
        assert!(
            app.world()
                .entity(entity)
                .get::<AnimatedTerrain>()
                .is_none()
        );

        app.world_mut().entity_mut(entity).insert(TerrainTile {
            terrain: GraphicalTerrain::Property(Property::City(Faction::Player(
                PlayerFaction::OrangeStar,
            ))),
        });

        let sprite = app.world().entity(entity).get::<Sprite>().unwrap();
        let atlas = sprite.texture_atlas.as_ref().unwrap();

        assert_eq!(
            atlas.index,
            awbrn_core::spritesheet_index(
                Weather::Clear,
                GraphicalTerrain::Property(Property::City(Faction::Player(
                    PlayerFaction::OrangeStar,
                )))
            )
            .index() as usize
        );
        assert!(app.world().entity(entity).contains::<TerrainAnimation>());
        assert!(app.world().entity(entity).contains::<AnimatedTerrain>());
    }
}
