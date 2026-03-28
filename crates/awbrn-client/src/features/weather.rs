pub use awbrn_game::world::CurrentWeather;

use awbrn_types::Weather;
use bevy::prelude::*;

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

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentWeather>().add_systems(
            Update,
            handle_weather_toggle.run_if(in_state(crate::core::AppState::InGame)),
        );
    }
}
