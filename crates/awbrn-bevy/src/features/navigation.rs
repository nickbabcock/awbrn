use crate::core::map::GameMap;
use crate::core::{Faction, SpriteSize, Unit, UnitActive, position_to_world_translation};
use crate::render::UiAtlas;
use crate::render::animation::{
    Animation, UnitPathAnimation, UnitVisualState, ease_out_quint, flip_x_for_lateral_direction,
    flip_x_for_movement, restore_unit_visual_state, set_unit_animation_state,
};
use awbrn_core::GraphicalMovement;
use awbrn_map::Position;
use bevy::prelude::*;
use std::time::Duration;

use crate::modes::replay::commands::{ReplayAdvanceLock, ReplayFollowupCommand};

/// Multiplier for replay path-related animation timing.
pub const REPLAY_PATH_ANIMATION_SPEED_FACTOR: f32 = 3.0;
pub const UNIT_PATH_SINGLE_SEGMENT_MS: u64 = 400;
pub const UNIT_PATH_EDGE_SEGMENT_MS: u64 = 350;
pub const UNIT_PATH_INTERIOR_SEGMENT_MS: u64 = 140;

pub(crate) const COURSE_ARROW_LAYER_OFFSET: f32 = 0.5;
pub(crate) const COURSE_ARROW_BASE_SCALE: f32 = 0.8;
pub(crate) const COURSE_ARROW_REVEAL_MS: u64 = 75;
pub(crate) const COURSE_ARROW_LIFETIME_MS: u64 = 250;
pub(crate) const COURSE_ARROW_STAGGER_MS: u64 = 25;
pub(crate) const COURSE_ARROW_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: 16.0,
    height: 16.0,
    z_index: 0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayPathTile {
    pub position: Position,
    pub unit_visible: bool,
}

#[derive(Component, Debug, Clone)]
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
        .and_then(global_path_tiles)
        .is_some_and(|path| path.len() >= 2)
}

pub fn movement_direction(from: Position, to: Position) -> GraphicalMovement {
    if from.y > to.y {
        GraphicalMovement::Up
    } else if from.y < to.y {
        GraphicalMovement::Down
    } else {
        GraphicalMovement::Lateral
    }
}

pub fn unit_path_segment_durations(path_len: usize) -> Option<Vec<Duration>> {
    if path_len < 2 {
        return None;
    }

    let segment_count = path_len - 1;
    if segment_count == 1 {
        return Some(vec![scaled_animation_duration(UNIT_PATH_SINGLE_SEGMENT_MS)]);
    }

    let total_duration_ms = UNIT_PATH_EDGE_SEGMENT_MS * 2
        + UNIT_PATH_INTERIOR_SEGMENT_MS * segment_count.saturating_sub(2) as u64;
    let per_segment_ms = total_duration_ms / segment_count as u64;
    let remainder_ms = total_duration_ms % segment_count as u64;
    let mut durations = vec![scaled_animation_duration(per_segment_ms); segment_count];
    if let Some(last) = durations.last_mut() {
        *last += scaled_animation_duration(remainder_ms);
    }

    Some(durations)
}

pub(crate) fn path_positions(path: &[ReplayPathTile]) -> Vec<Position> {
    path.iter().map(|tile| tile.position).collect()
}

pub(crate) fn global_path_tiles(
    move_action: &awbw_replay::turn_models::MoveAction,
) -> Option<Vec<ReplayPathTile>> {
    use awbw_replay::turn_models::TargetedPlayer;
    move_action.paths.get(&TargetedPlayer::Global).map(|path| {
        path.iter()
            .map(|tile| ReplayPathTile {
                position: Position::new(tile.x as usize, tile.y as usize),
                unit_visible: tile.unit_visible,
            })
            .collect()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CourseArrowSpriteKind {
    Body,
    Curved,
    Tip,
}

impl CourseArrowSpriteKind {
    fn sprite_name(self) -> &'static str {
        match self {
            Self::Body => "Arrow_Body.png",
            Self::Curved => "Arrow_Curved.png",
            Self::Tip => "Arrow_Tip.png",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CourseArrowSpawn {
    kind: CourseArrowSpriteKind,
    position: Position,
    rotation_degrees: f32,
    start_delay: Duration,
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

pub(crate) fn build_course_arrow_spawns(path: &[ReplayPathTile]) -> Vec<CourseArrowSpawn> {
    if path.len() < 2 {
        return Vec::new();
    }

    let mut spawns = Vec::with_capacity(path.len().saturating_sub(1));

    for i in 1..path.len() - 1 {
        let current = path[i];
        if !current.unit_visible {
            continue;
        }

        let prev = path[i - 1];
        let next = path[i + 1];
        let start_delay = scaled_animation_duration((i as u64 - 1) * COURSE_ARROW_STAGGER_MS);

        let (kind, rotation_degrees) = if !next.unit_visible {
            let head_diff_x = current.position.x as isize - prev.position.x as isize;
            let head_diff_y = current.position.y as isize - prev.position.y as isize;
            let rotation_degrees = if head_diff_x > 0 {
                90.0
            } else if head_diff_x < 0 {
                -90.0
            } else if head_diff_y > 0 {
                0.0
            } else {
                180.0
            };

            (CourseArrowSpriteKind::Tip, rotation_degrees)
        } else {
            let diff_x = next.position.x as isize - prev.position.x as isize;
            let diff_y = next.position.y as isize - prev.position.y as isize;

            if diff_x.abs() >= 2 {
                (
                    CourseArrowSpriteKind::Body,
                    if diff_x > 0 { 90.0 } else { -90.0 },
                )
            } else if diff_y.abs() >= 2 {
                (
                    CourseArrowSpriteKind::Body,
                    if diff_y > 0 { 180.0 } else { 0.0 },
                )
            } else {
                let prev_to_current_x = current.position.x as isize - prev.position.x as isize;
                let prev_to_current_y = current.position.y as isize - prev.position.y as isize;

                let connects_north = prev_to_current_y > 0
                    || current.position.y as isize - next.position.y as isize > 0;
                let connects_east = prev_to_current_x < 0
                    || next.position.x as isize - current.position.x as isize > 0;
                let connects_south = prev_to_current_y < 0
                    || next.position.y as isize - current.position.y as isize > 0;
                let connects_west = prev_to_current_x > 0
                    || current.position.x as isize - next.position.x as isize > 0;

                let rotation_degrees = if connects_west && connects_north {
                    0.0
                } else if connects_north && connects_east {
                    -90.0
                } else if connects_east && connects_south {
                    180.0
                } else if connects_south && connects_west {
                    90.0
                } else {
                    unreachable!("turn piece must connect exactly two orthogonal directions");
                };

                (CourseArrowSpriteKind::Curved, rotation_degrees)
            }
        };

        spawns.push(CourseArrowSpawn {
            kind,
            position: current.position,
            rotation_degrees,
            start_delay,
        });
    }

    let before_head = path[path.len() - 2];
    let head = path[path.len() - 1];
    if head.unit_visible {
        let head_diff_x = head.position.x as isize - before_head.position.x as isize;
        let head_diff_y = head.position.y as isize - before_head.position.y as isize;
        let rotation_degrees = if head_diff_x > 0 {
            90.0
        } else if head_diff_x < 0 {
            -90.0
        } else if head_diff_y > 0 {
            0.0
        } else {
            180.0
        };

        spawns.push(CourseArrowSpawn {
            kind: CourseArrowSpriteKind::Tip,
            position: head.position,
            rotation_degrees,
            start_delay: scaled_animation_duration(
                (path.len() as u64 - 2) * COURSE_ARROW_STAGGER_MS,
            ),
        });
    }

    spawns
}

fn current_segment_and_progress(path_animation: &UnitPathAnimation) -> (usize, f32) {
    let last_segment = path_animation.segment_durations.len().saturating_sub(1);
    if path_animation.elapsed >= path_animation.total_duration {
        return (last_segment, 1.0);
    }

    let mut elapsed = path_animation.elapsed;
    for (index, segment_duration) in path_animation.segment_durations.iter().enumerate() {
        if elapsed < *segment_duration {
            let segment_secs = segment_duration.as_secs_f32().max(f32::EPSILON);
            return (index, elapsed.as_secs_f32() / segment_secs);
        }
        elapsed = elapsed.saturating_sub(*segment_duration);
    }

    (last_segment, 1.0)
}

pub(crate) fn spawn_pending_course_arrows(
    mut commands: Commands,
    ui_atlas: UiAtlas,
    game_map: Res<GameMap>,
    pending: Query<(Entity, &PendingCourseArrows), Added<PendingCourseArrows>>,
    existing_arrows: Query<(Entity, &CourseArrowPiece)>,
) {
    for (owner, pending) in &pending {
        for (entity, arrow) in &existing_arrows {
            if arrow.owner == owner {
                commands.entity(entity).despawn();
            }
        }

        for spawn in build_course_arrow_spawns(&pending.path) {
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
        Option<&'static mut Animation>,
        Has<UnitActive>,
    ),
>;

pub(crate) fn animate_unit_paths(
    mut commands: Commands,
    time: Res<Time>,
    game_map: Res<GameMap>,
    mut replay_lock: ResMut<ReplayAdvanceLock>,
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
        animation,
        has_active,
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

        if previous_elapsed.is_zero()
            || segment_index != path_animation.current_segment
            || movement != path_animation.current_movement
        {
            path_animation.current_segment = segment_index;
            path_animation.current_movement = movement;
            set_unit_animation_state(
                &mut commands,
                entity,
                &mut sprite,
                animation,
                moving_visual_state,
                movement,
            );
        }

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

            if let Some(action) = replay_lock.release_for(entity) {
                commands.queue(ReplayFollowupCommand { action });
            }
        }
    }
}

pub struct NavigationPlugin;

impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                spawn_pending_course_arrows.before(animate_course_arrows),
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
    fn course_arrow_generation_matches_reference_rotations() {
        let straight = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Position::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(3, 1),
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
                position: Position::new(3, 3),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 3),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 2),
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
                position: Position::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 1),
                unit_visible: false,
            },
            ReplayPathTile {
                position: Position::new(3, 1),
                unit_visible: true,
            },
        ]);

        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[0].position, Position::new(3, 1));
    }

    #[test]
    fn hidden_tail_promotes_last_visible_middle_tile_to_tip() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Position::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(3, 1),
                unit_visible: false,
            },
        ]);

        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[0].position, Position::new(2, 1));
        assert_eq!(spawns[0].start_delay, scaled_animation_duration(0));
        assert_eq!(spawns[0].rotation_degrees, 90.0);
    }

    #[test]
    fn s_curve_generates_complementary_curve_rotations() {
        let spawns = build_course_arrow_spawns(&[
            ReplayPathTile {
                position: Position::new(0, 0),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(1, 0),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(1, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 1),
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
                position: Position::new(3, 1),
                unit_visible: true,
            },
            ReplayPathTile {
                position: Position::new(2, 1),
                unit_visible: true,
            },
        ]);

        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, CourseArrowSpriteKind::Tip);
        assert_eq!(spawns[0].rotation_degrees, -90.0);
    }
}
