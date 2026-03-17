use bevy::prelude::*;

/// Resource tracking the current state of replay playback.
#[derive(Resource, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Resource)]
pub struct ReplayState {
    pub next_action_index: u32,
    pub day: u32,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            next_action_index: 0,
            day: 1,
        }
    }
}

#[derive(Default)]
pub(crate) struct ReplayControlState {
    pub(crate) suppress_exhausted_repeat: bool,
}
