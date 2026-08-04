use crate::core::{RenderLayer, SpriteSize};
use awbrn_map::Position;

pub(crate) const COURSE_ARROW_SPRITE_SIZE: SpriteSize = SpriteSize {
    width: 16.0,
    height: 16.0,
    z_index: RenderLayer::COURSE_ARROW,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CourseArrowSpriteKind {
    Body,
    Curved,
    Tip,
}

impl CourseArrowSpriteKind {
    pub(crate) fn sprite_name(self) -> &'static str {
        match self {
            Self::Body => "Arrow_Body.png",
            Self::Curved => "Arrow_Curved.png",
            Self::Tip => "Arrow_Tip.png",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CourseArrowSpawn {
    pub(crate) kind: CourseArrowSpriteKind,
    pub(crate) position: Position,
    pub(crate) rotation_degrees: f32,
    /// Index in the source path; replay presentation uses it for its stagger
    /// and visibility mask.
    pub(crate) path_index: usize,
}

pub(crate) fn course_arrow_tip(
    previous: Position,
    position: Position,
    path_index: usize,
) -> CourseArrowSpawn {
    let diff_x = position.x as isize - previous.x as isize;
    let diff_y = position.y as isize - previous.y as isize;
    let rotation_degrees = if diff_x > 0 {
        90.0
    } else if diff_x < 0 {
        -90.0
    } else if diff_y > 0 {
        0.0
    } else {
        180.0
    };
    CourseArrowSpawn {
        kind: CourseArrowSpriteKind::Tip,
        position,
        rotation_degrees,
        path_index,
    }
}

/// Mode-neutral arrow geometry for a board path.
pub(crate) fn build_course_arrow_spawns(path: &[Position]) -> Vec<CourseArrowSpawn> {
    if path.len() < 2 {
        return Vec::new();
    }

    let mut spawns = Vec::with_capacity(path.len() - 1);
    for index in 1..path.len() - 1 {
        let (previous, current, next) = (path[index - 1], path[index], path[index + 1]);
        let diff_x = next.x as isize - previous.x as isize;
        let diff_y = next.y as isize - previous.y as isize;
        let (kind, rotation_degrees) = if diff_x.abs() >= 2 {
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
            let prev_x = current.x as isize - previous.x as isize;
            let prev_y = current.y as isize - previous.y as isize;
            let connects_north = prev_y > 0 || current.y as isize - next.y as isize > 0;
            let connects_east = prev_x < 0 || next.x as isize - current.x as isize > 0;
            let connects_south = prev_y < 0 || next.y as isize - current.y as isize > 0;
            let connects_west = prev_x > 0 || current.x as isize - next.x as isize > 0;
            let rotation = if connects_west && connects_north {
                Some(0.0)
            } else if connects_north && connects_east {
                Some(-90.0)
            } else if connects_east && connects_south {
                Some(180.0)
            } else if connects_south && connects_west {
                Some(90.0)
            } else {
                None
            };
            let Some(rotation) = rotation else {
                continue;
            };
            (CourseArrowSpriteKind::Curved, rotation)
        };
        spawns.push(CourseArrowSpawn {
            kind,
            position: current,
            rotation_degrees,
            path_index: index,
        });
    }
    spawns.push(course_arrow_tip(
        path[path.len() - 2],
        path[path.len() - 1],
        path.len() - 1,
    ));
    spawns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_straight_and_curved_geometry() {
        let straight = build_course_arrow_spawns(&[
            Position::new(0, 0),
            Position::new(1, 0),
            Position::new(2, 0),
        ]);
        assert_eq!(straight[0].kind, CourseArrowSpriteKind::Body);
        assert_eq!(straight[1].kind, CourseArrowSpriteKind::Tip);

        let curved = build_course_arrow_spawns(&[
            Position::new(0, 0),
            Position::new(1, 0),
            Position::new(1, 1),
        ]);
        assert_eq!(curved[0].kind, CourseArrowSpriteKind::Curved);
        assert_eq!(curved[1].kind, CourseArrowSpriteKind::Tip);
    }

    #[test]
    fn repeated_middle_tile_skips_only_that_segment() {
        let spawns = build_course_arrow_spawns(&[
            Position::new(0, 0),
            Position::new(1, 0),
            Position::new(1, 0),
            Position::new(2, 0),
            Position::new(3, 0),
        ]);

        assert_eq!(spawns.len(), 2);
        assert_eq!(spawns[0].path_index, 3);
        assert_eq!(spawns[1].kind, CourseArrowSpriteKind::Tip);
    }
}
