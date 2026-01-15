use crate::{
    BridgeType, Faction, GraphicalTerrain, MissileSiloStatus, PipeRubbleType, PipeSeamType,
    PipeType, PlayerFaction, Property, RiverType, RoadType, SeaDirection, ShoalDirection, Weather,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Default)]
pub struct SpritesheetIndex {
    index: u16,
    animation_frames: u8,
}

impl SpritesheetIndex {
    #[inline]
    pub const fn new(index: u16, animation_frames: u8) -> Self {
        Self {
            index,
            animation_frames,
        }
    }

    #[inline]
    pub const fn index(&self) -> u16 {
        self.index
    }

    #[inline]
    pub const fn animation_frames(&self) -> u8 {
        self.animation_frames
    }
}

include!("generated/spritesheet_index.rs");
