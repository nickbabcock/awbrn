use bevy::prelude::*;

/// Resource tracking the current state of replay playback.
#[derive(Resource)]
pub struct ReplayState {
    pub turn: u32,
    pub day: u32,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self { turn: 0, day: 1 }
    }
}

#[derive(Default)]
pub(crate) struct ReplayControlState {
    pub(crate) suppress_exhausted_repeat: bool,
}
