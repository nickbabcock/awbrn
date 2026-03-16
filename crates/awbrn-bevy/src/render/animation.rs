use crate::core::{Faction, INACTIVE_UNIT_COLOR, Unit};
use awbrn_core::{GraphicalMovement, get_unit_animation_frames};
use bevy::prelude::*;
use std::time::Duration;

#[derive(Component)]
pub(crate) struct Animation {
    pub(crate) start_index: u16,
    pub(crate) frame_durations: [u16; 4],
    pub(crate) current_frame: u8,
    pub(crate) frame_timer: Timer,
}

#[derive(Component)]
pub(crate) struct TerrainAnimation {
    pub(crate) start_index: u16,
    pub(crate) frame_count: u8,
    pub(crate) current_frame: u8,
    pub(crate) frame_timer: Timer,
    pub(crate) frame_durations: Option<awbrn_core::TerrainAnimationFrames>,
}

/// Component for unit path movement animation
#[derive(Component, Debug, Clone)]
#[component(storage = "SparseSet")]
pub struct UnitPathAnimation {
    pub path: Vec<awbrn_map::Position>,
    pub segment_durations: Vec<Duration>,
    pub total_duration: Duration,
    pub elapsed: Duration,
    pub current_segment: usize,
    pub current_movement: GraphicalMovement,
    pub idle_flip_x: bool,
}

impl UnitPathAnimation {
    pub fn new(path: Vec<awbrn_map::Position>, idle_flip_x: bool) -> Option<Self> {
        if path.len() < 2 {
            return None;
        }

        let segment_durations =
            crate::features::navigation::unit_path_segment_durations(path.len())?;
        let total_duration = segment_durations.iter().copied().sum();

        Some(Self {
            current_movement: crate::features::navigation::movement_direction(path[0], path[1]),
            path,
            segment_durations,
            total_duration,
            elapsed: Duration::ZERO,
            current_segment: 0,
            idle_flip_x,
        })
    }
}

pub(crate) fn unit_animation_for(
    unit: Unit,
    faction: Faction,
    movement: GraphicalMovement,
) -> (awbrn_core::UnitAnimationFrames, Animation) {
    let animation_frames = get_unit_animation_frames(movement, unit.0, faction.0);
    let frame_durations = animation_frames.raw();
    let animation = Animation {
        start_index: animation_frames.start_index(),
        frame_durations,
        current_frame: 0,
        frame_timer: Timer::new(
            Duration::from_millis(frame_durations[0] as u64),
            TimerMode::Once,
        ),
    };

    (animation_frames, animation)
}

#[derive(Clone, Copy)]
pub(crate) struct UnitVisualState {
    pub(crate) unit: Unit,
    pub(crate) faction: Faction,
    pub(crate) flip_x: bool,
}

pub(crate) fn flip_x_for_movement(idle_flip_x: bool, movement: GraphicalMovement) -> bool {
    match movement {
        GraphicalMovement::Idle => idle_flip_x,
        GraphicalMovement::Up | GraphicalMovement::Down => false,
        GraphicalMovement::Lateral => idle_flip_x,
    }
}

pub(crate) fn flip_x_for_lateral_direction(moving_right: bool) -> bool {
    !moving_right
}

pub(crate) fn set_unit_pose(
    sprite: &mut Sprite,
    visual_state: UnitVisualState,
    movement: GraphicalMovement,
) -> awbrn_core::UnitAnimationFrames {
    let animation_frames =
        get_unit_animation_frames(movement, visual_state.unit.0, visual_state.faction.0);
    sprite.flip_x = visual_state.flip_x;
    if let Some(atlas) = &mut sprite.texture_atlas {
        atlas.index = animation_frames.start_index() as usize;
    }
    animation_frames
}

pub(crate) fn set_unit_animation_state(
    commands: &mut Commands,
    entity: Entity,
    sprite: &mut Sprite,
    animation: Option<Mut<Animation>>,
    visual_state: UnitVisualState,
    movement: GraphicalMovement,
) {
    set_unit_pose(sprite, visual_state, movement);
    let (_, new_animation) = unit_animation_for(visual_state.unit, visual_state.faction, movement);
    sprite.color = Color::WHITE;

    if let Some(mut animation) = animation {
        animation.start_index = new_animation.start_index;
        animation.frame_durations = new_animation.frame_durations;
        animation.current_frame = 0;
        animation.frame_timer = new_animation.frame_timer;
    } else {
        commands.entity(entity).insert(new_animation);
    }
}

pub(crate) fn restore_unit_visual_state(
    commands: &mut Commands,
    entity: Entity,
    sprite: &mut Sprite,
    animation: Option<Mut<Animation>>,
    visual_state: UnitVisualState,
    has_active: bool,
) {
    set_unit_pose(sprite, visual_state, GraphicalMovement::Idle);
    if has_active {
        set_unit_animation_state(
            commands,
            entity,
            sprite,
            animation,
            visual_state,
            GraphicalMovement::Idle,
        );
    } else {
        sprite.color = INACTIVE_UNIT_COLOR;
        commands.entity(entity).remove::<Animation>();
    }
}

pub(crate) fn ease_out_quint(progress: f32) -> f32 {
    1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(5)
}

pub(crate) fn animate_units(time: Res<Time>, mut query: Query<(&mut Animation, &mut Sprite)>) {
    for (mut animation, mut sprite) in query.iter_mut() {
        animation.frame_timer.tick(time.delta());

        if animation.frame_timer.just_finished() {
            let start_frame = animation.current_frame;
            loop {
                animation.current_frame =
                    (animation.current_frame + 1) % animation.frame_durations.len() as u8;

                if animation.current_frame == start_frame {
                    break;
                }

                if animation.frame_durations[animation.current_frame as usize] > 0 {
                    break;
                }
            }

            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = animation.start_index as usize + animation.current_frame as usize;
            }

            let next_duration = animation.frame_durations[animation.current_frame as usize];
            if next_duration > 0 {
                animation.frame_timer =
                    Timer::new(Duration::from_millis(next_duration as u64), TimerMode::Once);
            }
        }
    }
}

pub(crate) fn animate_terrain(
    time: Res<Time>,
    mut query: Query<(&mut TerrainAnimation, &mut Sprite)>,
) {
    for (mut animation, mut sprite) in query.iter_mut() {
        animation.frame_timer.tick(time.delta());

        if animation.frame_timer.just_finished() {
            animation.current_frame = (animation.current_frame + 1) % animation.frame_count;

            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = animation.start_index as usize + animation.current_frame as usize;
            }

            let next_duration = animation
                .frame_durations
                .as_ref()
                .map(|f| f.get_duration(animation.current_frame))
                .unwrap_or(300);
            animation.frame_timer =
                Timer::new(Duration::from_millis(next_duration as u64), TimerMode::Once);
        }
    }
}

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (animate_units, animate_terrain).run_if(in_state(crate::core::AppState::InGame)),
        );
    }
}
