use crate::core::map::TerrainTile;
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

/// Re-insert each TerrainTile to trigger the on_terrain_tile_insert observer,
/// which re-derives sprite and animation state from the current weather.
pub(crate) fn refresh_terrain_on_weather_change(
    terrain_query: Query<(Entity, &TerrainTile)>,
    mut commands: Commands,
) {
    for (entity, terrain_tile) in &terrain_query {
        commands.entity(entity).insert(*terrain_tile);
    }
}

pub(crate) fn refresh_backdrops_on_weather_change(
    current_weather: Res<CurrentWeather>,
    mut backdrop_query: Query<&mut Sprite, With<MapBackdrop>>,
) {
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

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentWeather>().add_systems(
            Update,
            (
                handle_weather_toggle,
                (
                    refresh_terrain_on_weather_change,
                    refresh_backdrops_on_weather_change,
                )
                    .run_if(resource_changed::<CurrentWeather>)
                    .after(handle_weather_toggle),
            )
                .run_if(in_state(crate::core::AppState::InGame)),
        );
    }
}
