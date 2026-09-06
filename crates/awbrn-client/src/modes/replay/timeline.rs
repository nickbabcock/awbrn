//! Moving around inside a loaded archive.
//!
//! The keyboard walks an archive one action at a time, which is the right way
//! to read a fight and the wrong way to cross a game. So the position is also
//! something a caller can ask for directly: a boundary, the turn either side
//! of the one being read, or an end of the archive.
//!
//! What counts as a turn is a rule, and the archive does not hold the rules.
//! The turns here are read off the outline AWVM produced when the archive was
//! loaded, so a turn boundary in the controls is a turn boundary in the game.

use bevy::prelude::*;

use crate::features::event_bus::{EventSink, ReplayPositionChanged};
use crate::loading::LoadedReplay;
use crate::modes::replay::controls::advance_replay_action;
use crate::modes::replay::presentation::{ReplayAdvanceLock, ReplaySeekCommand};
use crate::replay_archive::ReplayBoundary;
use awbrn_bevy::replay::ReplayState;

/// Every boundary the loaded archive can be read at, opening included, so
/// `0` is the position before the first action.
#[derive(Resource, Debug, Default)]
pub struct ReplayOutline(pub Vec<ReplayBoundary>);

impl ReplayOutline {
    /// The turn a boundary falls in. Two boundaries in the same turn answer
    /// with the same value.
    fn turn_at(&self, index: usize) -> Option<(u32, Option<u32>)> {
        self.0
            .get(index)
            .map(|boundary| (boundary.day, boundary.active_player))
    }

    /// Where the turn holding this boundary began.
    fn turn_start(&self, index: usize) -> usize {
        let Some(turn) = self.turn_at(index) else {
            return index;
        };
        let mut start = index;
        while start > 0 && self.turn_at(start - 1) == Some(turn) {
            start -= 1;
        }
        start
    }

    /// Where the turn after this boundary's turn begins.
    fn next_turn_start(&self, index: usize) -> Option<usize> {
        let turn = self.turn_at(index)?;
        (index + 1..self.0.len()).find(|candidate| self.turn_at(*candidate) != Some(turn))
    }

    /// Where the turn before this boundary's turn began.
    ///
    /// A viewer part-way through a turn is taken to the start of the turn they
    /// are reading first, which is the step back they meant.
    fn previous_turn_start(&self, index: usize) -> Option<usize> {
        let start = self.turn_start(index);
        if start < index {
            return Some(start);
        }
        (start > 0).then(|| self.turn_start(start - 1))
    }
}

/// A position a viewer asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayTarget {
    /// This many actions from where the viewer is standing.
    Action(i32),
    /// This many turns from the one the viewer is reading.
    Turn(i32),
    /// One named boundary.
    Boundary(u32),
    Start,
    End,
}

/// The viewer asked to stand somewhere else in the archive.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayNavigate {
    pub target: ReplayTarget,
}

pub(crate) fn handle_replay_navigation(
    mut commands: Commands,
    mut requests: MessageReader<ReplayNavigate>,
    mut replay_state: ResMut<ReplayState>,
    outline: Res<ReplayOutline>,
    loaded_replay: Res<LoadedReplay>,
    replay_lock: Res<ReplayAdvanceLock>,
) {
    let total = loaded_replay.0.len();
    let mut blocked = replay_lock.is_active();

    for request in requests.read() {
        if blocked {
            continue;
        }
        let current = replay_state.next_action_index as usize;
        let Some(target) = resolve_target(request.target, current, total, &outline) else {
            continue;
        };
        if target == current {
            continue;
        }

        // One step forward is the step the archive is presented by: the move
        // is animated and the action is watched. Anything else is a jump, and
        // a jump shows where it arrived rather than how it got there.
        if target == current + 1 {
            if advance_replay_action(&mut commands, &mut replay_state, &loaded_replay).is_locked() {
                blocked = true;
            }
            continue;
        }

        commands.queue(ReplaySeekCommand {
            target_index: target as u32,
        });
        blocked = true;
    }
}

fn resolve_target(
    target: ReplayTarget,
    current: usize,
    total: usize,
    outline: &ReplayOutline,
) -> Option<usize> {
    let resolved = match target {
        ReplayTarget::Start => 0,
        ReplayTarget::End => total,
        ReplayTarget::Boundary(index) => index as usize,
        ReplayTarget::Action(delta) => current.saturating_add_signed(delta as isize),
        ReplayTarget::Turn(delta) => {
            let mut index = current;
            for _ in 0..delta.unsigned_abs() {
                let next = if delta > 0 {
                    outline.next_turn_start(index).unwrap_or(total)
                } else {
                    outline.previous_turn_start(index).unwrap_or(0)
                };
                // An end of the archive is as far as the steps go. Without
                // this, a step count larger than the archive keeps asking the
                // outline for a turn that is not there.
                if next == index {
                    break;
                }
                index = next;
            }
            index
        }
    };
    Some(resolved.min(total))
}

/// Tell the page where the viewer is standing, and where the turns beside
/// them begin.
pub(crate) fn emit_replay_position(
    sink: Option<Res<EventSink<ReplayPositionChanged>>>,
    replay_state: Option<Res<ReplayState>>,
    outline: Option<Res<ReplayOutline>>,
    loaded_replay: Option<Res<LoadedReplay>>,
) {
    let (Some(sink), Some(replay_state), Some(outline), Some(loaded_replay)) =
        (sink, replay_state, outline, loaded_replay)
    else {
        return;
    };

    let index = replay_state.next_action_index as usize;
    let total = loaded_replay.0.len();
    sink.emit(ReplayPositionChanged {
        index: index as u32,
        total: total as u32,
        day: outline
            .turn_at(index)
            .map(|(day, _)| day)
            .unwrap_or(replay_state.day),
        active_player_id: outline.turn_at(index).and_then(|(_, player)| player),
        previous_turn_index: outline.previous_turn_start(index).map(|start| start as u32),
        next_turn_index: outline.next_turn_start(index).map(|start| start as u32),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two seats, three actions each, over two days.
    fn outline() -> ReplayOutline {
        let turns = [(1, 0), (1, 1), (2, 0), (2, 1)];
        let mut boundaries = Vec::new();
        for (day, player) in turns {
            for _ in 0..3 {
                boundaries.push(ReplayBoundary {
                    day,
                    active_player: Some(player),
                });
            }
        }
        ReplayOutline(boundaries)
    }

    #[test]
    fn a_turn_forward_lands_on_the_first_boundary_of_the_next_turn() {
        assert_eq!(outline().next_turn_start(0), Some(3));
        assert_eq!(outline().next_turn_start(4), Some(6));
        assert_eq!(
            outline().next_turn_start(11),
            None,
            "the last turn has nothing after it"
        );
    }

    #[test]
    fn a_turn_back_from_the_middle_of_a_turn_returns_to_its_start() {
        assert_eq!(outline().previous_turn_start(4), Some(3));
    }

    #[test]
    fn a_turn_back_from_the_start_of_a_turn_reaches_the_turn_before_it() {
        assert_eq!(outline().previous_turn_start(3), Some(0));
        assert_eq!(
            outline().previous_turn_start(0),
            None,
            "the first turn has nothing before it"
        );
    }

    #[test]
    fn turn_steps_accumulate() {
        let outline = outline();
        assert_eq!(
            resolve_target(ReplayTarget::Turn(2), 0, 12, &outline),
            Some(6)
        );
        assert_eq!(
            resolve_target(ReplayTarget::Turn(-2), 7, 12, &outline),
            Some(3),
            "a step back from the middle of a turn spends itself reaching that turn's start"
        );
    }

    #[test]
    fn a_target_never_leaves_the_archive() {
        let outline = outline();
        assert_eq!(
            resolve_target(ReplayTarget::Action(-5), 2, 12, &outline),
            Some(0)
        );
        assert_eq!(
            resolve_target(ReplayTarget::Action(99), 2, 12, &outline),
            Some(12)
        );
        assert_eq!(
            resolve_target(ReplayTarget::Boundary(99), 2, 12, &outline),
            Some(12)
        );
    }

    #[test]
    fn position_emission_waits_for_replay_state() {
        let mut app = App::new();
        app.insert_resource(EventSink::<ReplayPositionChanged>::new(|_| {}));
        app.init_resource::<ReplayOutline>();
        app.add_systems(
            Update,
            emit_replay_position
                .run_if(resource_exists::<EventSink<ReplayPositionChanged>>)
                .run_if(
                    resource_exists_and_changed::<ReplayState>
                        .or_else(resource_exists_and_changed::<ReplayOutline>),
                ),
        );

        app.update();
    }
}
