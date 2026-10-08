use crate::core::SpriteSize;
use crate::core::coords::position_to_world_translation;
use crate::render::UiAtlas;
use crate::render::animation::{
    Animation, UnitPathAnimation, UnitVisualState, ease_out_quint, flip_x_for_lateral_direction,
    flip_x_for_movement, restore_unit_visual_state, set_unit_animation_state,
};
use crate::render::course_arrow::{
    COURSE_ARROW_SPRITE_SIZE, CourseArrowSpriteKind, course_arrow_tip,
};
use awbrn_bevy::replay::AwbwUnitId;
use awbrn_bevy::world::{Faction, GameMap, Unit, UnitActive};
use awbrn_map::Pos;
use awbrn_types::GraphicalMovement;
use awbw_replay::turn_models::{MoveAction, TargetedPlayer};
use bevy::prelude::*;
use std::time::Duration;

use crate::modes::replay::presentation::{ReplayAdvanceLock, ReplayFollowupCommand};

/// Multiplier for replay path-related animation timing.
pub const REPLAY_PATH_ANIMATION_SPEED_FACTOR: f32 = 3.0;
/// Time for one tile at full speed.
pub const UNIT_PATH_CRUISE_TILE_MS: u64 = 140;
/// Time to go from stop to full speed.
pub const UNIT_PATH_ACCEL_MS: u64 = 300;
/// Time to go from full speed to stop. It is longer than the acceleration
/// so that the unit settles softly on the destination tile.
pub const UNIT_PATH_DECEL_MS: u64 = 540;

pub(crate) const COURSE_ARROW_LAYER_OFFSET: f32 = 0.5;
pub(crate) const COURSE_ARROW_BASE_SCALE: f32 = 0.8;
pub(crate) const COURSE_ARROW_REVEAL_MS: u64 = 75;
pub(crate) const COURSE_ARROW_LIFETIME_MS: u64 = 250;
pub(crate) const COURSE_ARROW_STAGGER_MS: u64 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayPathTile {
    pub position: Pos,
    pub unit_visible: bool,
}

#[derive(Component, Debug, Clone)]
#[component(storage = "SparseSet")]
pub struct PendingCourseArrows {
    pub path: Vec<ReplayPathTile>,
}

pub fn scaled_animation_duration(base_ms: u64) -> Duration {
    let speed = REPLAY_PATH_ANIMATION_SPEED_FACTOR.max(f32::EPSILON);
    if (speed - 1.0).abs() < f32::EPSILON {
        return Duration::from_millis(base_ms);
    }

    let nanos = ((base_ms as f64 * 1_000_000.0) / speed as f64).round() as u64;
    Duration::from_nanos(nanos)
}

pub fn action_requires_path_animation(action: &awbw_replay::turn_models::Action) -> bool {
    action
        .move_action()
        .and_then(|move_action| {
            let (targeted_player, _) = replay_move_view(move_action)?;
            replay_path_tiles(move_action, targeted_player)
        })
        .is_some_and(|path| path.len() >= 2)
}

/// Returns the first visible unit projection carried by an archived move.
pub(crate) fn replay_move_view(
    move_action: &MoveAction,
) -> Option<(TargetedPlayer, &awbw_replay::turn_models::UnitProperty)> {
    move_action
        .unit
        .get(&TargetedPlayer::Global)
        .and_then(awbw_replay::Hidden::get_value)
        .map(|unit| (TargetedPlayer::Global, unit))
        .or_else(|| {
            move_action.unit.iter().find_map(|(targeted_player, unit)| {
                unit.get_value().map(|unit| (*targeted_player, unit))
            })
        })
}

pub fn movement_direction(from: Pos, to: Pos) -> GraphicalMovement {
    if from.y > to.y {
        GraphicalMovement::Up
    } else if from.y < to.y {
        GraphicalMovement::Down
    } else {
        GraphicalMovement::Lateral
    }
}

/// Speed profile for a unit that moves along a path of tiles.
///
/// The unit starts from a stop, increases speed to a cruise speed, and
/// decreases speed to a stop on the last tile. The speed changes follow a
/// half cosine, thus the speed and the acceleration have no sudden changes.
/// The unit does not go past the destination tile.
///
/// A path that is too short for the full ramps uses shorter ramps and a lower
/// peak speed. The ramp ratio stays the same.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitPathMotion {
    distance: f32,
    peak_speed: f32,
    accel_secs: f32,
    cruise_secs: f32,
    decel_secs: f32,
}

impl UnitPathMotion {
    pub fn new(segment_count: usize) -> Option<Self> {
        if segment_count == 0 {
            return None;
        }

        let distance = segment_count as f32;
        let cruise_speed = 1.0
            / scaled_animation_duration(UNIT_PATH_CRUISE_TILE_MS)
                .as_secs_f32()
                .max(f32::EPSILON);
        let accel_secs = scaled_animation_duration(UNIT_PATH_ACCEL_MS).as_secs_f32();
        let decel_secs = scaled_animation_duration(UNIT_PATH_DECEL_MS).as_secs_f32();

        // A half cosine ramp covers half the distance of the same time at
        // the peak speed.
        let ramp_distance = cruise_speed * (accel_secs + decel_secs) / 2.0;
        if distance >= ramp_distance {
            return Some(Self {
                distance,
                peak_speed: cruise_speed,
                accel_secs,
                cruise_secs: (distance - ramp_distance) / cruise_speed,
                decel_secs,
            });
        }

        // Make the ramps and the peak speed smaller by the same scale. The
        // ramp distance then decreases by the square of the scale.
        let scale = (distance / ramp_distance).sqrt();
        Some(Self {
            distance,
            peak_speed: cruise_speed * scale,
            accel_secs: accel_secs * scale,
            cruise_secs: 0.0,
            decel_secs: decel_secs * scale,
        })
    }

    pub fn duration(&self) -> Duration {
        Duration::from_secs_f32(self.accel_secs + self.cruise_secs + self.decel_secs)
    }

    /// Distance in tiles from the start of the path at the given time.
    pub fn distance_at(&self, elapsed: Duration) -> f32 {
        use std::f32::consts::PI;

        let t = elapsed.as_secs_f32();
        let v = self.peak_speed;
        let accel_distance = v * self.accel_secs / 2.0;

        let distance = if t < self.accel_secs {
            let phase = PI * t / self.accel_secs;
            v / 2.0 * (t - self.accel_secs / PI * phase.sin())
        } else if t < self.accel_secs + self.cruise_secs {
            accel_distance + v * (t - self.accel_secs)
        } else {
            let u = (t - self.accel_secs - self.cruise_secs).min(self.decel_secs);
            let phase = PI * u / self.decel_secs.max(f32::EPSILON);
            accel_distance
                + v * self.cruise_secs
                + v / 2.0 * (u + self.decel_secs / PI * phase.sin())
        };

        distance.clamp(0.0, self.distance)
    }
}

pub(crate) fn replay_path_tiles(
    move_action: &MoveAction,
    targeted_player: TargetedPlayer,
) -> Option<Vec<ReplayPathTile>> {
    move_action
        .paths
        .get(&TargetedPlayer::Global)
        .or_else(|| move_action.paths.get(&targeted_player))
        .map(|path| {
            path.iter()
                .map(|tile| ReplayPathTile {
                    position: Pos::new(tile.x as u8, tile.y as u8),
                    unit_visible: tile.unit_visible,
                })
                .collect()
        })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ReplayCourseArrowSpawn {
    pub(crate) kind: CourseArrowSpriteKind,
    pub(crate) position: Pos,
    pub(crate) rotation_degrees: f32,
    pub(crate) start_delay: Duration,
}

#[allow(dead_code)]
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct CourseArrowPiece {
    pub(crate) owner: Entity,
    pub(crate) kind: CourseArrowSpriteKind,
    pub(crate) rotation_degrees: f32,
    pub(crate) start_delay: Duration,
    pub(crate) reveal_duration: Duration,
    pub(crate) total_duration: Duration,
    pub(crate) elapsed: Duration,
}

fn build_course_arrow_spawns(path: &[ReplayPathTile]) -> Vec<ReplayCourseArrowSpawn> {
    let positions: Vec<_> = path.iter().map(|tile| tile.position).collect();
    crate::render::course_arrow::build_course_arrow_spawns(&positions)
        .into_iter()
        .filter(|spawn| path[spawn.path_index].unit_visible)
        .map(|mut spawn| {
            if path
                .get(spawn.path_index + 1)
                .is_some_and(|tile| !tile.unit_visible)
            {
                spawn = course_arrow_tip(
                    path[spawn.path_index - 1].position,
                    spawn.position,
                    spawn.path_index,
                );
            }
            spawn
        })
        .enumerate()
        .map(|(visible_index, spawn)| ReplayCourseArrowSpawn {
            kind: spawn.kind,
            position: spawn.position,
            rotation_degrees: spawn.rotation_degrees,
            start_delay: scaled_animation_duration(visible_index as u64 * COURSE_ARROW_STAGGER_MS),
        })
        .collect()
}

fn current_segment_and_progress(path_animation: &UnitPathAnimation) -> (usize, f32) {
    let last_segment = path_animation.path.len().saturating_sub(2);
    if path_animation.elapsed >= path_animation.total_duration {
        return (last_segment, 1.0);
    }

    let distance = path_animation.motion.distance_at(path_animation.elapsed);
    let segment_index = (distance.floor() as usize).min(last_segment);
    (segment_index, distance - segment_index as f32)
}

pub(crate) fn spawn_pending_course_arrows(
    trigger: On<Insert<PendingCourseArrows>>,
    mut commands: Commands,
    ui_atlas: UiAtlas,
    game_map: Res<GameMap>,
    query: Query<&PendingCourseArrows>,
    existing_arrows: Query<(Entity, &CourseArrowPiece)>,
) {
    let owner = trigger.entity;

    let Ok(pending) = query.get(owner) else {
        return;
    };

    for (entity, arrow) in &existing_arrows {
        if arrow.owner == owner {
            commands.entity(entity).despawn();
        }
    }

    let spawns = build_course_arrow_spawns(&pending.path);

    for spawn in spawns {
        let mut transform = Transform::from_translation(
            position_to_world_translation(
                &COURSE_ARROW_SPRITE_SIZE,
                spawn.position,
                game_map.as_ref(),
            ) + Vec3::new(0.0, 0.0, COURSE_ARROW_LAYER_OFFSET),
        );
        transform.rotation = Quat::from_rotation_z(spawn.rotation_degrees.to_radians());
        transform.scale = Vec3::splat(COURSE_ARROW_BASE_SCALE);

        commands.spawn((
            ui_atlas.sprite_for(spawn.kind.sprite_name()),
            transform,
            Visibility::Hidden,
            CourseArrowPiece {
                owner,
                kind: spawn.kind,
                rotation_degrees: spawn.rotation_degrees,
                start_delay: spawn.start_delay,
                reveal_duration: scaled_animation_duration(COURSE_ARROW_REVEAL_MS),
                total_duration: scaled_animation_duration(COURSE_ARROW_LIFETIME_MS),
                elapsed: Duration::ZERO,
            },
        ));
    }

    commands.entity(owner).remove::<PendingCourseArrows>();
}

pub(crate) fn animate_course_arrows(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(
        Entity,
        &mut CourseArrowPiece,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    for (entity, mut arrow, mut transform, mut visibility) in &mut query {
        arrow.elapsed += time.delta();

        if arrow.elapsed < arrow.start_delay {
            *visibility = Visibility::Hidden;
            continue;
        }

        *visibility = Visibility::Visible;
        let visible_elapsed = arrow.elapsed.saturating_sub(arrow.start_delay);
        let reveal_progress = if arrow.reveal_duration.is_zero() {
            1.0
        } else {
            visible_elapsed.as_secs_f32() / arrow.reveal_duration.as_secs_f32()
        };
        let scale = COURSE_ARROW_BASE_SCALE
            + (1.0 - COURSE_ARROW_BASE_SCALE) * ease_out_quint(reveal_progress);
        transform.scale = Vec3::splat(scale);

        if visible_elapsed >= arrow.total_duration {
            commands.entity(entity).despawn();
        }
    }
}

type UnitPathAnimationQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static SpriteSize,
        &'static mut UnitPathAnimation,
        &'static mut Sprite,
        &'static Unit,
        &'static Faction,
        Option<&'static AwbwUnitId>,
        Option<&'static mut Animation>,
        Has<UnitActive>,
        &'static mut Visibility,
    ),
>;

pub(crate) fn animate_unit_paths(
    mut commands: Commands,
    time: Res<Time>,
    game_map: Res<GameMap>,
    mut replay_lock: ResMut<ReplayAdvanceLock>,
    viewer: Res<crate::features::visibility::ViewerVisibility>,
    mut query: UnitPathAnimationQuery,
) {
    for (
        entity,
        mut transform,
        sprite_size,
        mut path_animation,
        mut sprite,
        unit,
        faction,
        unit_id,
        animation,
        has_active,
        mut visibility,
    ) in &mut query
    {
        let idle_visual_state = UnitVisualState {
            unit: *unit,
            faction: *faction,
            flip_x: path_animation.idle_flip_x,
        };

        if path_animation.path.len() < 2 {
            commands.entity(entity).remove::<UnitPathAnimation>();
            restore_unit_visual_state(
                &mut commands,
                entity,
                &mut sprite,
                animation,
                idle_visual_state,
                has_active,
            );
            continue;
        }

        let previous_elapsed = path_animation.elapsed;
        path_animation.elapsed =
            (path_animation.elapsed + time.delta()).min(path_animation.total_duration);
        let (segment_index, segment_t) = current_segment_and_progress(&path_animation);

        let moving_right = if segment_index + 1 < path_animation.path.len() {
            path_animation.path[segment_index + 1].x > path_animation.path[segment_index].x
        } else {
            false
        };
        let movement = if segment_index + 1 < path_animation.path.len() {
            movement_direction(
                path_animation.path[segment_index],
                path_animation.path[segment_index + 1],
            )
        } else {
            path_animation.current_movement
        };
        let flip_x = if matches!(movement, GraphicalMovement::Lateral) {
            flip_x_for_lateral_direction(moving_right)
        } else {
            flip_x_for_movement(path_animation.idle_flip_x, movement)
        };
        let moving_visual_state = UnitVisualState {
            unit: *unit,
            faction: *faction,
            flip_x,
        };
        // Restart the walk cycle only when the pose changes. A restart on
        // each tile keeps the unit on the first frame, because the unit
        // crosses a tile faster than one frame.
        if previous_elapsed.is_zero() || movement != path_animation.current_movement {
            path_animation.current_movement = movement;
            set_unit_animation_state(
                &mut commands,
                entity,
                &mut sprite,
                animation,
                moving_visual_state,
                movement,
            );
        } else if sprite.flip_x != flip_x {
            sprite.flip_x = flip_x;
        }

        // The animated path is the one the selected projection reported, so
        // every tile on it is a tile the viewer could watch. What remains to
        // ask is whether the mover itself is disclosed.
        let disclosed = unit_id.is_none_or(|unit_id| viewer.unit_visible(unit_id.0));
        let target = if disclosed {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(target);

        let start_world = position_to_world_translation(
            sprite_size,
            path_animation.path[segment_index],
            game_map.as_ref(),
        );
        let end_world = position_to_world_translation(
            sprite_size,
            path_animation.path[segment_index + 1],
            game_map.as_ref(),
        );
        transform.translation = start_world.lerp(end_world, segment_t);

        if path_animation.elapsed >= path_animation.total_duration {
            transform.translation = position_to_world_translation(
                sprite_size,
                *path_animation.path.last().unwrap(),
                game_map.as_ref(),
            );
            commands.entity(entity).remove::<UnitPathAnimation>();
            restore_unit_visual_state(
                &mut commands,
                entity,
                &mut sprite,
                None,
                idle_visual_state,
                has_active,
            );

            if let Some(followup) = replay_lock.release_for(entity) {
                commands.queue(ReplayFollowupCommand {
                    transitions: followup.transitions,
                });
            }
        }
    }
}

#[derive(Debug)]
pub struct NavigationPlugin;

impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawn_pending_course_arrows).add_systems(
            Update,
            (
                animate_course_arrows,
                animate_unit_paths.before(crate::render::animation::animate_units),
            )
                .run_if(in_state(crate::core::AppState::InGame)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn course_arrow_generation_matches_expected_rotations() {
        let straight = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Pos::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(2, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(3, 1),
                unit_visible: true,
            },
        ]);
        assert_eq!(straight.len(), 2);
        assert_eq!(straight[0].kind, CourseArrowSpriteKind::Body);
        assert_eq!(straight[0].rotation_degrees, 90.0);
        assert_eq!(straight[1].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(straight[1].rotation_degrees, 90.0);

        let curved = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Pos::new(3, 3),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(2, 3),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(2, 2),
                unit_visible: true,
            },
        ]);
        assert_eq!(curved.len(), 2);
        assert_eq!(curved[0].kind, CourseArrowSpriteKind::Curved);
        assert_eq!(curved[0].rotation_degrees, -90.0);
        assert_eq!(curved[1].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(curved[1].rotation_degrees, 180.0);
    }

    #[test]
    fn course_arrow_generation_skips_hidden_tiles() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Pos::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(2, 1),
                unit_visible: false,
            },
            ReplayPathTile {
                position: Pos::new(3, 1),
                unit_visible: true,
            },
        ]);

        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[0].position, Pos::new(3, 1));
    }

    #[test]
    fn hidden_tiles_do_not_leave_stagger_gaps() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Pos::new(0, 0),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(1, 0),
                unit_visible: false,
            },
            ReplayPathTile {
                position: Pos::new(2, 0),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(3, 0),
                unit_visible: true,
            },
        ]);

        assert_eq!(spawns.len(), 2);
        assert_eq!(spawns[0].start_delay, scaled_animation_duration(0));
        assert_eq!(
            spawns[1].start_delay,
            scaled_animation_duration(COURSE_ARROW_STAGGER_MS)
        );
    }

    #[test]
    fn hidden_tail_promotes_last_visible_middle_tile_to_tip() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Pos::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(2, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(3, 1),
                unit_visible: false,
            },
        ]);

        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[0].position, Pos::new(2, 1));
        assert_eq!(spawns[0].start_delay, scaled_animation_duration(0));
        assert_eq!(spawns[0].rotation_degrees, 90.0);
    }

    #[test]
    fn s_curve_generates_complementary_curve_rotations() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Pos::new(0, 0),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(1, 0),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(2, 1),
                unit_visible: true,
            },
        ]);

        assert_eq!(spawns.len(), 3);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Curved);
        assert_eq!(spawns[0].rotation_degrees, 90.0);
        assert_eq!(spawns[1].kind, CourseArrowSpriteKind::Curved);
        assert_eq!(spawns[1].rotation_degrees, -90.0);
        assert_eq!(spawns[2].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[2].rotation_degrees, 90.0);
    }

    #[test]
    fn leftward_tip_points_left() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Pos::new(3, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Pos::new(2, 1),
                unit_visible: true,
            },
        ]);

        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[0].rotation_degrees, -90.0);
    }

    fn sample_motion(motion: &UnitPathMotion, steps: u32) -> Vec<f32> {
        let duration = motion.duration();
        (0..=steps)
            .map(|step| motion.distance_at(duration.mul_f32(step as f32 / steps as f32)))
            .collect()
    }

    #[test]
    fn unit_path_motion_starts_and_ends_on_tiles() {
        for segments in [1, 2, 3, 5, 12] {
            let motion = UnitPathMotion::new(segments).unwrap();
            assert_eq!(motion.distance_at(Duration::ZERO), 0.0);
            assert!((motion.distance_at(motion.duration()) - segments as f32).abs() < 1e-4);
            assert_eq!(
                motion.distance_at(motion.duration() * 2),
                segments as f32,
                "distance stays on the destination after the end"
            );
        }
        assert!(UnitPathMotion::new(0).is_none());
    }

    #[test]
    fn unit_path_motion_never_moves_backward_or_past_the_destination() {
        for segments in [1, 2, 3, 5, 12] {
            let motion = UnitPathMotion::new(segments).unwrap();
            let samples = sample_motion(&motion, 500);
            for pair in samples.windows(2) {
                assert!(pair[1] >= pair[0] - 1e-5, "motion reversed: {pair:?}");
                assert!(pair[1] <= segments as f32);
            }
        }
    }

    #[test]
    fn unit_path_motion_eases_in_and_out() {
        let motion = UnitPathMotion::new(6).unwrap();
        let samples = sample_motion(&motion, 600);
        let steps: Vec<f32> = samples.windows(2).map(|pair| pair[1] - pair[0]).collect();
        let peak = steps.iter().copied().fold(0.0, f32::max);

        // The first and the last steps are much shorter than the peak step.
        assert!(steps[0] < peak * 0.05);
        assert!(steps[steps.len() - 1] < peak * 0.05);

        // The slow down takes more time than the speed up.
        let slow_start = steps.iter().position(|step| *step >= peak * 0.5).unwrap();
        let slow_end = steps.len() - steps.iter().rposition(|step| *step >= peak * 0.5).unwrap();
        assert!(slow_end > slow_start);
    }

    #[test]
    fn unit_path_motion_duration_grows_with_path_length() {
        let durations: Vec<_> = (1..=10)
            .map(|segments| UnitPathMotion::new(segments).unwrap().duration())
            .collect();
        assert!(durations.windows(2).all(|pair| pair[1] > pair[0]));

        // A long path adds one cruise tile of time for each extra tile.
        let cruise_tile = scaled_animation_duration(UNIT_PATH_CRUISE_TILE_MS).as_secs_f32();
        let extra = durations[9].as_secs_f32() - durations[8].as_secs_f32();
        assert!((extra - cruise_tile).abs() < 1e-3);
    }
}
