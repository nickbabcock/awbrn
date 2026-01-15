use crate::{GraphicalMovement, PlayerFaction, SpritesheetIndex, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitAnimationData {
    frames: [u16; 16],
    idle_end: u8,
    move_up_end: u8,
    move_down_end: u8,
    move_side_end: u8,
}

impl UnitAnimationData {
    pub fn new(
        idle_frames: &[u16],
        move_up_frames: &[u16],
        move_down_frames: &[u16],
        move_side_frames: &[u16],
    ) -> Self {
        let mut frames = [0u16; 16];
        let mut pos = 0;
        frames[pos..][..idle_frames.len()].copy_from_slice(idle_frames);
        pos += idle_frames.len();
        frames[pos..][..move_up_frames.len()].copy_from_slice(move_up_frames);
        pos += move_up_frames.len();
        frames[pos..][..move_down_frames.len()].copy_from_slice(move_down_frames);
        pos += move_down_frames.len();
        frames[pos..][..move_side_frames.len()].copy_from_slice(move_side_frames);

        let idle_end = idle_frames.len() as u8;
        let move_up_end = idle_end + move_up_frames.len() as u8;
        let move_down_end = move_up_end + move_down_frames.len() as u8;
        let move_side_end = move_down_end + move_side_frames.len() as u8;

        Self {
            frames,
            idle_end,
            move_up_end,
            move_down_end,
            move_side_end,
        }
    }

    pub fn get_frames_for_movement(&self, movement: GraphicalMovement) -> &[u16] {
        match movement {
            GraphicalMovement::Idle => &self.frames[0..self.idle_end as usize],
            GraphicalMovement::Up => {
                &self.frames[self.idle_end as usize..self.move_up_end as usize]
            }
            GraphicalMovement::Down => {
                &self.frames[self.move_up_end as usize..self.move_down_end as usize]
            }
            GraphicalMovement::Lateral => {
                &self.frames[self.move_down_end as usize..self.move_side_end as usize]
            }
        }
    }

    pub fn get_frame_durations(&self, movement: GraphicalMovement) -> &[u16] {
        self.get_frames_for_movement(movement)
    }

    /// The number of total frames across all movements
    pub fn total_frames(&self) -> u8 {
        self.move_side_end
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UnitAnimationFrames {
    start_index: u16,
    frame_durations: [u16; 4],
    end_index: u8, // 0-4, indicates how many frames are valid
}

impl UnitAnimationFrames {
    pub fn new(start_index: u16, frame_durations: &[u16]) -> Self {
        let mut durations = [0u16; 4];
        durations[..frame_durations.len()].copy_from_slice(frame_durations);

        Self {
            start_index,
            frame_durations: durations,
            end_index: frame_durations.len() as u8,
        }
    }

    pub const fn frame_count(&self) -> usize {
        self.end_index as usize
    }

    pub const fn start_index(&self) -> u16 {
        self.start_index
    }

    pub fn total_duration(&self) -> u32 {
        self.frame_durations
            .iter()
            .copied()
            .map(u32::from)
            .sum::<u32>()
    }

    #[inline]
    pub fn raw(&self) -> [u16; 4] {
        self.frame_durations
    }
}

// Build-time generated data
include!("./unit_animation_data.rs");

/// Calculate sprite index and animation frames for a unit
pub fn get_unit_animation_frames(
    movement: GraphicalMovement,
    unit: Unit,
    faction: PlayerFaction,
) -> UnitAnimationFrames {
    let (unit_offset, unit_data) = get_animation_data(unit);
    let frames_per_faction = UnitAnimationData::TOTAL_FRAMES as u16;

    // Each faction's sprites start at faction_index * frames_per_faction
    let faction_offset = frames_per_faction * u16::from(faction.index());

    // Calculate offset within this unit's animations based on movement type
    let movement_offset = calculate_movement_offset(movement, unit);

    let unit_sprite_index = faction_offset + unit_offset + movement_offset;

    let frame_durations = unit_data.get_frames_for_movement(movement);

    UnitAnimationFrames::new(unit_sprite_index, frame_durations)
}

pub fn unit_spritesheet_index(
    movement: GraphicalMovement,
    unit: Unit,
    faction: PlayerFaction,
) -> SpritesheetIndex {
    let frames = get_unit_animation_frames(movement, unit, faction);
    SpritesheetIndex::new(frames.start_index(), frames.frame_count() as u8)
}

fn calculate_movement_offset(movement: GraphicalMovement, unit: Unit) -> u16 {
    let (_, unit_data) = get_animation_data(unit);

    match movement {
        GraphicalMovement::Idle => 0,
        GraphicalMovement::Up => unit_data.idle_end as u16,
        GraphicalMovement::Down => unit_data.move_up_end as u16,
        GraphicalMovement::Lateral => unit_data.move_down_end as u16,
    }
}
