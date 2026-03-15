use crate::core::{AppState, GameMode};
use bevy::prelude::*;

fn handle_game_input() {
    // TODO: Implement game-specific input handling
}

pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            handle_game_input.run_if(in_state(GameMode::Game).and(in_state(AppState::InGame))),
        );
    }
}
