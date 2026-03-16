use crate::core::map::TerrainTile;
use crate::render::animation::TerrainAnimation;
use crate::render::map::MapBackdrop;
use awbrn_core::Weather;
use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentWeather(Weather);

impl Default for CurrentWeather {
    fn default() -> Self {
        CurrentWeather(Weather::Clear)
    }
}

impl CurrentWeather {
    pub fn set(&mut self, weather: Weather) {
        self.0 = weather;
    }

    pub fn weather(&self) -> Weather {
        self.0
    }
}

pub(crate) fn handle_weather_toggle(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut current_weather: ResMut<CurrentWeather>,
) {
    if keyboard_input.just_pressed(KeyCode::Space) {
        let new_weather = match current_weather.weather() {
            Weather::Clear => Weather::Snow,
            Weather::Snow => Weather::Rain,
            Weather::Rain => Weather::Clear,
        };

        current_weather.set(new_weather);
        info!("Weather changed to: {:?}", current_weather.weather());
    }
}

pub(crate) fn update_backdrop_on_weather_change(
    current_weather: Res<CurrentWeather>,
    mut backdrop_query: Query<&mut Sprite, (With<MapBackdrop>, Without<TerrainTile>)>,
) {
    if !current_weather.is_changed() {
        return;
    }

    let plain_index = awbrn_core::spritesheet_index(
        current_weather.weather(),
        awbrn_core::GraphicalTerrain::Plain,
    );

    for mut sprite in backdrop_query.iter_mut() {
        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = plain_index.index() as usize;
        }
    }
}

type StaticTerrainQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Sprite, &'static TerrainTile),
    (Without<TerrainAnimation>, Without<MapBackdrop>),
>;

pub(crate) fn update_static_terrain_on_weather_change(
    current_weather: Res<CurrentWeather>,
    mut static_query: StaticTerrainQuery,
) {
    if !current_weather.is_changed() {
        return;
    }

    for (mut sprite, terrain_tile) in static_query.iter_mut() {
        let sprite_index =
            awbrn_core::spritesheet_index(current_weather.weather(), terrain_tile.terrain);
        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = sprite_index.index() as usize;
        }
    }
}

type AnimatedTerrainQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Sprite,
        &'static TerrainTile,
        &'static mut TerrainAnimation,
    ),
    (With<TerrainAnimation>, Without<MapBackdrop>),
>;

pub(crate) fn update_animated_terrain_on_weather_change(
    current_weather: Res<CurrentWeather>,
    mut animated_query: AnimatedTerrainQuery,
) {
    if !current_weather.is_changed() {
        return;
    }

    for (mut sprite, terrain_tile, mut animation) in animated_query.iter_mut() {
        let sprite_index =
            awbrn_core::spritesheet_index(current_weather.weather(), terrain_tile.terrain);

        animation.start_index = sprite_index.index();
        animation.frame_count = sprite_index.animation_frames();
        animation.current_frame = 0;
        let initial_duration = animation
            .frame_durations
            .as_ref()
            .map(|f| f.get_duration(0))
            .unwrap_or(300);
        animation.frame_timer = Timer::new(
            std::time::Duration::from_millis(initial_duration as u64),
            TimerMode::Once,
        );

        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = sprite_index.index() as usize;
        }
    }
}

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentWeather>().add_systems(
            Update,
            (
                handle_weather_toggle,
                update_backdrop_on_weather_change,
                update_static_terrain_on_weather_change,
                update_animated_terrain_on_weather_change,
            )
                .run_if(in_state(crate::core::AppState::InGame)),
        );
    }
}
