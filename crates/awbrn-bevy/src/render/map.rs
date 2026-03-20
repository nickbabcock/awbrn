use crate::core::map::{GameMap, TerrainTile};
use crate::core::{AppState, GridSystem, RenderLayer};
use crate::features::weather::CurrentWeather;
use crate::render::TerrainAtlasResource;
use crate::render::animation::TerrainAnimation;
use awbrn_core::GraphicalTerrain;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor, TextureFormatPixelInfo};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension};
use bevy::sprite::Anchor;
use bevy::sprite_render::AlphaMode2d;
use std::time::Duration;

#[derive(Component)]
pub struct MapBackdrop;

#[derive(Component)]
pub(crate) struct AnimatedTerrain;

#[derive(Resource, Clone)]
pub(crate) struct BackdropTexturesResource {
    clear: Handle<Image>,
    snow: Handle<Image>,
    rain: Handle<Image>,
}

impl BackdropTexturesResource {
    fn texture_for(&self, weather: awbrn_core::Weather) -> Handle<Image> {
        match weather {
            awbrn_core::Weather::Clear => self.clear.clone(),
            awbrn_core::Weather::Snow => self.snow.clone(),
            awbrn_core::Weather::Rain => self.rain.clone(),
        }
    }
}

const TERRAIN_TILE_WIDTH: u32 = 16;
const TERRAIN_TILE_HEIGHT: u32 = 32;
const BACKDROP_TILE_SIZE: u32 = 16;

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

pub(crate) fn initialize_backdrop_textures(
    mut commands: Commands,
    terrain_atlas: Res<TerrainAtlasResource>,
    mut images: ResMut<Assets<Image>>,
) {
    let (clear_image, snow_image, rain_image) = {
        let Some(atlas_image) = images.get(&terrain_atlas.texture) else {
            return;
        };

        (
            extract_plain_backdrop_image(atlas_image, awbrn_core::Weather::Clear),
            extract_plain_backdrop_image(atlas_image, awbrn_core::Weather::Snow),
            extract_plain_backdrop_image(atlas_image, awbrn_core::Weather::Rain),
        )
    };

    let clear = images.add(clear_image);
    let snow = images.add(snow_image);
    let rain = images.add(rain_image);

    commands.insert_resource(BackdropTexturesResource { clear, snow, rain });
}

pub(crate) fn setup_map_backdrops(
    mut commands: Commands,
    backdrop_textures: Res<BackdropTexturesResource>,
    current_weather: Res<CurrentWeather>,
    game_map: Res<GameMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    backdrops: Query<Entity, With<MapBackdrop>>,
) {
    if !backdrops.is_empty() {
        return;
    }

    let map_width = game_map.width() as f32 * GridSystem::TILE_SIZE;
    let map_height = game_map.height() as f32 * GridSystem::TILE_SIZE;

    let mesh = meshes.add(Rectangle::new(map_width, map_height));
    let material = materials.add(ColorMaterial {
        texture: Some(backdrop_textures.texture_for(current_weather.weather())),
        uv_transform: Affine2::from_scale(Vec2::new(
            game_map.width() as f32,
            game_map.height() as f32,
        )),
        alpha_mode: AlphaMode2d::Opaque,
        ..default()
    });

    commands.spawn((
        Mesh2d(mesh),
        MeshMaterial2d(material),
        Transform::from_xyz(
            0.0,
            -GridSystem::TILE_SIZE / 2.0,
            RenderLayer::BACKDROP as f32,
        ),
        MapBackdrop,
    ));
}

pub(crate) fn refresh_map_backdrop_on_weather_change(
    backdrop_textures: Res<BackdropTexturesResource>,
    current_weather: Res<CurrentWeather>,
    backdrop: Query<&MeshMaterial2d<ColorMaterial>, With<MapBackdrop>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let Some(material_handle) = backdrop.iter().next() else {
        return;
    };

    let Some(material) = materials.get_mut(&material_handle.0) else {
        return;
    };

    material.texture = Some(backdrop_textures.texture_for(current_weather.weather()));
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

fn extract_plain_backdrop_image(source: &Image, weather: awbrn_core::Weather) -> Image {
    let sprite_index = awbrn_core::spritesheet_index(weather, GraphicalTerrain::Plain).index();
    let columns = awbrn_core::TILESHEET_COLUMNS;
    let col = u32::from(sprite_index) % columns;
    let row = u32::from(sprite_index) / columns;
    let base_x = col * TERRAIN_TILE_WIDTH;
    let base_y = row * TERRAIN_TILE_HEIGHT + (TERRAIN_TILE_HEIGHT - BACKDROP_TILE_SIZE);
    let source_width = source.texture_descriptor.size.width as usize;
    let pixel_size = source
        .texture_descriptor
        .format
        .pixel_size()
        .expect("terrain atlas image must use a byte-addressable texture format");
    let source_data = source
        .data
        .as_ref()
        .expect("terrain atlas image should have CPU-side pixel data");
    let row_stride = source_width * pixel_size;
    let copy_width = BACKDROP_TILE_SIZE as usize * pixel_size;

    let mut data =
        Vec::with_capacity((BACKDROP_TILE_SIZE * BACKDROP_TILE_SIZE) as usize * pixel_size);

    for y in 0..BACKDROP_TILE_SIZE {
        let row_start = (base_y as usize + y as usize) * row_stride + base_x as usize * pixel_size;
        let row_end = row_start + copy_width;
        data.extend_from_slice(&source_data[row_start..row_end]);
    }

    let mut image = Image::new(
        Extent3d {
            width: BACKDROP_TILE_SIZE,
            height: BACKDROP_TILE_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        source.texture_descriptor.format,
        RenderAssetUsages::default(),
    );
    let mut sampler = ImageSamplerDescriptor::nearest();
    sampler.set_address_mode(ImageAddressMode::Repeat);
    image.sampler = ImageSampler::Descriptor(sampler);
    image
}

pub struct MapVisualsPlugin;

impl Plugin for MapVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_terrain_atlas)
            .add_systems(
                Update,
                initialize_backdrop_textures
                    .run_if(resource_exists::<TerrainAtlasResource>)
                    .run_if(not(resource_exists::<BackdropTexturesResource>)),
            )
            .add_systems(
                Update,
                setup_map_backdrops
                    .run_if(resource_exists::<BackdropTexturesResource>)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                refresh_map_backdrop_on_weather_change
                    .after(crate::features::weather::refresh_terrain_on_weather_change)
                    .run_if(resource_exists::<BackdropTexturesResource>)
                    .run_if(resource_changed::<CurrentWeather>)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_observer(on_terrain_tile_insert);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_core::{Faction, PlayerFaction, Property, SeaDirection, Weather};
    use bevy::ecs::system::RunSystemOnce;
    use bevy::mesh::VertexAttributeValues;

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

    #[test]
    fn extracts_plain_backdrop_tile_from_bottom_of_atlas_cell() {
        let mut atlas = Image::new(
            Extent3d {
                width: TERRAIN_TILE_WIDTH * awbrn_core::TILESHEET_COLUMNS,
                height: TERRAIN_TILE_HEIGHT,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![
                0;
                (TERRAIN_TILE_WIDTH * awbrn_core::TILESHEET_COLUMNS * TERRAIN_TILE_HEIGHT * 4)
                    as usize
            ],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );

        let plain_index =
            awbrn_core::spritesheet_index(Weather::Clear, GraphicalTerrain::Plain).index();
        let col = u32::from(plain_index) % awbrn_core::TILESHEET_COLUMNS;
        let x = col * TERRAIN_TILE_WIDTH;

        for y in 0..TERRAIN_TILE_HEIGHT {
            for dx in 0..TERRAIN_TILE_WIDTH {
                let pixel = atlas
                    .pixel_bytes_mut(UVec3::new(x + dx, y, 0))
                    .expect("pixel should be writable");
                pixel.copy_from_slice(&[10, y as u8, 20, 255]);
            }
        }

        let image = extract_plain_backdrop_image(&atlas, Weather::Clear);

        assert_eq!(image.texture_descriptor.size.width, BACKDROP_TILE_SIZE);
        assert_eq!(image.texture_descriptor.size.height, BACKDROP_TILE_SIZE);
        assert_eq!(
            image
                .pixel_bytes(UVec3::new(0, 0, 0))
                .expect("cropped pixel should exist"),
            &[10, 16, 20, 255]
        );
        assert_eq!(
            image
                .pixel_bytes(UVec3::new(0, BACKDROP_TILE_SIZE - 1, 0))
                .expect("cropped pixel should exist"),
            &[10, 31, 20, 255]
        );
        let ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("backdrop image should use a custom repeat sampler");
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::Repeat);
    }

    #[test]
    fn setup_map_backdrops_spawns_one_repeated_mesh_for_current_weather() {
        let mut app = App::new();
        app.init_resource::<GameMap>();
        app.init_resource::<Assets<Image>>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<ColorMaterial>>();
        app.insert_resource(CurrentWeather::default());
        let clear = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        let snow = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        let rain = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        app.insert_resource(BackdropTexturesResource {
            clear: clear.clone(),
            snow,
            rain,
        });
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(awbrn_map::AwbrnMap::new(3, 2, GraphicalTerrain::Plain));

        app.world_mut()
            .run_system_once(setup_map_backdrops)
            .unwrap();

        let mut query = app.world_mut().query::<(
            &Mesh2d,
            &MeshMaterial2d<ColorMaterial>,
            &Transform,
            &MapBackdrop,
        )>();
        let items: Vec<_> = query.iter(app.world()).collect();
        assert_eq!(items.len(), 1);

        let (mesh_handle, material_handle, transform, _) = items[0];
        assert_eq!(
            transform.translation,
            Vec3::new(
                0.0,
                -GridSystem::TILE_SIZE / 2.0,
                RenderLayer::BACKDROP as f32
            )
        );

        let materials = app.world().resource::<Assets<ColorMaterial>>();
        let material = materials
            .get(&material_handle.0)
            .expect("backdrop material should exist");
        assert_eq!(material.texture.as_ref(), Some(&clear));
        assert_eq!(
            material.uv_transform,
            Affine2::from_scale(Vec2::new(3.0, 2.0))
        );
        assert_eq!(material.alpha_mode, AlphaMode2d::Opaque);

        let meshes = app.world().resource::<Assets<Mesh>>();
        let mesh = meshes
            .get(&mesh_handle.0)
            .expect("backdrop mesh should exist");
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("rectangle mesh should have positions");
        let VertexAttributeValues::Float32x3(positions) = positions else {
            panic!("rectangle mesh positions should be Float32x3");
        };
        let xs: Vec<f32> = positions.iter().map(|pos| pos[0]).collect();
        let ys: Vec<f32> = positions.iter().map(|pos| pos[1]).collect();
        let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min);
        let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert_eq!(max_x - min_x, 48.0);
        assert_eq!(max_y - min_y, 32.0);
    }

    #[test]
    fn refresh_map_backdrop_on_weather_change_updates_existing_material() {
        let mut app = App::new();
        app.init_resource::<GameMap>();
        app.init_resource::<Assets<Image>>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<ColorMaterial>>();
        app.insert_resource(CurrentWeather::default());
        let clear = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        let snow = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        let rain = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());
        app.insert_resource(BackdropTexturesResource {
            clear,
            snow: snow.clone(),
            rain,
        });
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(awbrn_map::AwbrnMap::new(2, 2, GraphicalTerrain::Plain));

        app.world_mut()
            .run_system_once(setup_map_backdrops)
            .unwrap();
        app.world_mut()
            .resource_mut::<CurrentWeather>()
            .set(Weather::Snow);
        app.world_mut()
            .run_system_once(refresh_map_backdrop_on_weather_change)
            .unwrap();

        let mut query = app
            .world_mut()
            .query::<(&MeshMaterial2d<ColorMaterial>, &MapBackdrop)>();
        let items: Vec<_> = query.iter(app.world()).collect();
        assert_eq!(items.len(), 1);

        let materials = app.world().resource::<Assets<ColorMaterial>>();
        let material = materials
            .get(&items[0].0.0)
            .expect("backdrop material should exist");
        assert_eq!(material.texture.as_ref(), Some(&snow));
    }
}
