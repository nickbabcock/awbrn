use crate::{Faction, GraphicalTerrain, PlayerFaction, Property};

/// Frame timing data for animated terrain tiles
/// Uses static slice references for space efficiency (no wasted memory for unused frames)
#[derive(Debug, Clone, Copy)]
pub struct TerrainAnimationFrames {
    frame_durations: &'static [u16],
}

impl TerrainAnimationFrames {
    pub const fn new(frame_durations: &'static [u16]) -> Self {
        Self { frame_durations }
    }

    pub fn get_duration(&self, frame: u8) -> u16 {
        self.frame_durations
            .get(frame as usize)
            .copied()
            .unwrap_or(300) // Fallback
    }

    pub fn frame_count(&self) -> u8 {
        self.frame_durations.len() as u8
    }

    pub fn durations(&self) -> &[u16] {
        self.frame_durations
    }
}

// Generated data will be included here
include!("./generated/terrain_animation_data.rs");

/// Get animation timing data for a terrain tile
/// Returns None if terrain is static (non-animated)
pub fn get_terrain_animation_frames(terrain: GraphicalTerrain) -> Option<TerrainAnimationFrames> {
    get_animation_timing(terrain)
}
