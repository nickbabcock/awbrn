//! Reading an earlier moment of a match that is still being played.
//!
//! A board under review is not a board anybody can play on: the position it
//! shows was left behind, and an order given against it would be spelled
//! against units that have since moved. So review does two things and no more.
//! It stops the play systems, which is what makes the board something to read
//! rather than something to act on, and it holds the live match at the edge
//! until the viewer comes back to it.
//!
//! Nothing here decides what an earlier board looked like. Every position a
//! viewer is shown arrives already projected for them, the same way a live one
//! does, because only the host holds the log a position is rebuilt from.

use awvm::semantic::ObservedTransition;
use bevy::prelude::*;

use crate::loading::PendingLiveTransitions;
use crate::modes::replay::presentation::LiveTransitionCommand;

/// Whether the board is showing a moment the match has moved on from.
#[derive(Resource, Debug, Default)]
pub struct BoardReview {
    active: bool,
}

impl BoardReview {
    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// The viewer asked to read an earlier moment.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardReviewEntered;

/// The viewer asked to come back to the match as it stands.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardReviewExited;

/// A reviewed position waiting to be shown.
///
/// Only the newest is kept. A viewer holding a key down asks for boundaries
/// faster than the host answers, and every answer is a whole board, so showing
/// each one in turn would be showing a scrub the viewer has already finished.
#[derive(Resource, Debug, Default)]
pub struct PendingReviewTransition(pub Option<ObservedTransition>);

/// Run condition: the board is being read rather than played.
pub(crate) fn board_is_under_review(review: Res<BoardReview>) -> bool {
    review.is_active()
}

/// Run condition: the board has just stopped being something to play on.
pub(crate) fn board_review_started(review: Res<BoardReview>) -> bool {
    review.is_changed() && review.is_active()
}

/// Take the board out of the viewer's hands.
///
/// What the viewer had selected is cleared by the play systems' own cleanup,
/// which [`board_review_started`] runs for this one frame. A selection left
/// standing would be a unit lit up on a board it is no longer on.
pub(crate) fn handle_board_review_entered(
    mut entered: MessageReader<BoardReviewEntered>,
    mut review: ResMut<BoardReview>,
) {
    if entered.read().last().is_none() {
        return;
    }
    if review.active {
        return;
    }
    review.active = true;
}

/// Give the board back.
///
/// The transitions held at the edge while the viewer was reading are dropped
/// rather than played out. Each of them describes a step taken against a
/// position the viewer never saw, and every one of them is older than the
/// board the page hands over on its way back in, so playing them would walk
/// the board backwards before it caught up. What the viewer rejoins is the
/// match as it stands, which only the host can say.
pub(crate) fn handle_board_review_exited(
    mut exited: MessageReader<BoardReviewExited>,
    mut review: ResMut<BoardReview>,
    mut pending: Option<ResMut<PendingLiveTransitions>>,
    mut pending_review: ResMut<PendingReviewTransition>,
) {
    if exited.read().last().is_none() {
        return;
    }
    if !review.active {
        return;
    }
    review.active = false;
    pending_review.0 = None;
    if let Some(pending) = pending.as_deref_mut() {
        pending.0.clear();
    }
}

/// Show the position the host answered with.
///
/// A position waits while the one before it is still being watched: an answer
/// that arrived during a move would put the board somewhere else half-way
/// through it.
pub(crate) fn apply_pending_review_transition(
    mut pending: ResMut<PendingReviewTransition>,
    lock: Res<crate::modes::replay::presentation::ReplayAdvanceLock>,
    mut commands: Commands,
) {
    if lock.is_active() {
        return;
    }
    let Some(transition) = pending.0.take() else {
        return;
    };
    commands.queue(LiveTransitionCommand { transition });
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_map::AwbwMapData;
    use awbw_replay::ReplayParser;
    use awvm::semantic::{AwbwVisibility, observe};
    use awvm_awbw::RecordedAdapter;
    use bevy::ecs::system::RunSystemOnce;
    use std::path::Path;

    fn app() -> App {
        let mut app = App::new();
        app.add_message::<BoardReviewEntered>()
            .add_message::<BoardReviewExited>()
            .init_resource::<BoardReview>()
            .init_resource::<PendingReviewTransition>()
            .init_resource::<PendingLiveTransitions>()
            .init_resource::<crate::modes::replay::presentation::ReplayAdvanceLock>();
        app
    }

    /// A transition that reconciles to a board rather than describing a step.
    fn transition() -> ObservedTransition {
        let replay = ReplayParser::new()
            .parse(
                &std::fs::read(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/replays/1362397.zip"),
                )
                .unwrap(),
            )
            .unwrap();
        let map_data: AwbwMapData = serde_json::from_slice(
            &std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/maps/162795.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let adapter = RecordedAdapter::new(&replay, &map_data).unwrap();
        let recipient = adapter.state().players[0].id().clone();
        ObservedTransition {
            post: observe(&AwbwVisibility, adapter.state(), &recipient).unwrap(),
            events: Vec::new(),
        }
    }

    #[test]
    fn the_board_stops_taking_orders_while_it_is_being_read() {
        let mut app = app();
        app.world_mut().write_message(BoardReviewEntered);
        app.world_mut()
            .run_system_once(handle_board_review_entered)
            .unwrap();

        assert!(app.world().resource::<BoardReview>().is_active());
        assert!(
            app.world_mut()
                .run_system_once(board_review_started)
                .unwrap(),
            "the frame the reading starts on is the frame the selection is cleared on"
        );
    }

    #[test]
    fn coming_back_drops_what_the_match_did_while_the_viewer_was_reading() {
        let mut app = app();
        app.world_mut().write_message(BoardReviewEntered);
        app.world_mut()
            .run_system_once(handle_board_review_entered)
            .unwrap();
        app.world_mut()
            .resource_mut::<PendingLiveTransitions>()
            .push(transition());
        app.world_mut().resource_mut::<PendingReviewTransition>().0 = Some(transition());

        app.world_mut().write_message(BoardReviewExited);
        app.world_mut()
            .run_system_once(handle_board_review_exited)
            .unwrap();

        assert!(!app.world().resource::<BoardReview>().is_active());
        assert!(
            app.world()
                .resource::<PendingLiveTransitions>()
                .0
                .is_empty(),
            "every held transition is older than the board the page hands back"
        );
        assert!(
            app.world()
                .resource::<PendingReviewTransition>()
                .0
                .is_none()
        );
    }

    #[test]
    fn a_position_waits_while_the_one_before_it_is_still_being_watched() {
        let mut app = app();
        app.world_mut().resource_mut::<PendingReviewTransition>().0 = Some(transition());
        // Standing in for an animation the board is part-way through.
        let entity = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<crate::modes::replay::presentation::ReplayAdvanceLock>()
            .hold_for_test(entity);

        app.world_mut()
            .run_system_once(apply_pending_review_transition)
            .unwrap();

        assert!(
            app.world()
                .resource::<PendingReviewTransition>()
                .0
                .is_some(),
            "the position is kept until the board is done moving"
        );
    }
}
