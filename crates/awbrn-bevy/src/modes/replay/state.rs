use bevy::prelude::*;

/// Resource tracking the current state of replay playback.
#[derive(Resource, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Resource)]
pub struct ReplayState {
    pub next_action_index: u32,
    pub day: u32,
    /// The player whose turn it currently is. Set at bootstrap from turn order
    /// and updated by `apply_end()` from `UpdatedInfo.next_player_id`.
    #[reflect(ignore)]
    pub active_player_id: Option<awbrn_core::AwbwGamePlayerId>,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            next_action_index: 0,
            day: 1,
            active_player_id: None,
        }
    }
}

#[derive(Default)]
pub(crate) struct ReplayControlState {
    pub(crate) suppress_exhausted_repeat: bool,
}
