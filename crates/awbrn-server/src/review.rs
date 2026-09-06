//! Stepping through a match that has already been played.
//!
//! A viewer who wants to see an earlier moment of a match may not be shown the
//! match's own action log: the log is written against the true board, and a
//! fogged match hides part of that board from everybody. So nothing here hands
//! a caller an action. A caller names a boundary — a count of actions taken —
//! and receives the projection one recipient is entitled to, built by the same
//! `observe` the live match answers with.
//!
//! The cost that buys is a replay per seek. A ladder of positions kept along
//! the log is what bounds it: any boundary is at most
//! [`CHECKPOINT_INTERVAL`] actions from a position already held, so two
//! viewers reading different moments of the same match cost the same as one
//! reading the moment beside it.

use std::collections::BTreeMap;

use awbrn_game::{GameSetup, PlayerId, StoredActionEvent};
use awvm::semantic::{Observation, ObservedTransition};

use crate::replay::ReplayError;
use crate::server::GameServer;

/// How many actions separate the positions a seek may start from.
///
/// Every position held costs a copy of the board, and every action between two
/// of them costs a replay. Thirty-two keeps the worst seek near the cost of a
/// single turn while holding a handful of copies for a match of any length.
const CHECKPOINT_INTERVAL: usize = 32;

/// Where a match stood at one boundary.
///
/// The board itself is not here: this is what a caller reads to label a
/// moment and to find the moment a turn begins, and it stays small enough to
/// send the whole of it at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Boundary {
    pub day: u64,
    /// The seat that took the action reaching this boundary. `None` opens the
    /// match, where no action has been taken.
    pub acting_slot: Option<u8>,
    /// The seat holding the turn here, or `None` once the match is over.
    pub active_slot: Option<u8>,
}

/// A match the caller may move around inside.
#[derive(Debug)]
pub struct MatchReview {
    events: Vec<StoredActionEvent>,
    /// One entry for each boundary, opening included, so `outline[k]`
    /// describes the match after `k` actions.
    outline: Vec<Boundary>,
    /// Positions a seek may start from, by boundary.
    checkpoints: BTreeMap<usize, GameServer>,
    /// The end of the log, which is what a newly recorded action extends.
    tip: GameServer,
    cursor: GameServer,
    cursor_index: usize,
    fog_enabled: bool,
}

impl MatchReview {
    /// Replay a whole log once, keeping the positions a later seek starts from.
    pub fn new(setup: GameSetup, events: Vec<StoredActionEvent>) -> Result<Self, ReplayError> {
        let fog_enabled = setup.fog_enabled;
        let mut tip = GameServer::new(setup).map_err(ReplayError::Setup)?;
        let mut outline = vec![boundary(&tip, None)];
        let mut checkpoints = BTreeMap::new();
        checkpoints.insert(0, tip.clone());

        for (index, event) in events.iter().enumerate() {
            tip.replay_stored_action_event(event)
                .map_err(|source| ReplayError::Event { index, source })?;
            outline.push(boundary(&tip, Some(event.player.0)));
            let completed = index + 1;
            if completed.is_multiple_of(CHECKPOINT_INTERVAL) {
                checkpoints.insert(completed, tip.clone());
            }
        }

        Ok(Self {
            cursor: tip.clone(),
            cursor_index: events.len(),
            events,
            outline,
            checkpoints,
            tip,
            fog_enabled,
        })
    }

    /// Record an action the match has just accepted.
    ///
    /// The cursor stays where the viewer left it. Only the end of the log
    /// moves, which is what lets a viewer read an earlier turn while the match
    /// plays on around them.
    pub fn append(&mut self, event: StoredActionEvent) -> Result<(), ReplayError> {
        let index = self.events.len();
        self.tip
            .replay_stored_action_event(&event)
            .map_err(|source| ReplayError::Event { index, source })?;
        self.outline.push(boundary(&self.tip, Some(event.player.0)));
        self.events.push(event);
        let completed = index + 1;
        if completed.is_multiple_of(CHECKPOINT_INTERVAL) {
            self.checkpoints.insert(completed, self.tip.clone());
        }
        Ok(())
    }

    /// The last boundary, which is the match as it stands.
    pub fn latest_index(&self) -> usize {
        self.events.len()
    }

    /// Where the cursor is standing.
    pub fn index(&self) -> usize {
        self.cursor_index
    }

    /// Every boundary, opening included.
    pub fn outline(&self) -> &[Boundary] {
        &self.outline
    }

    /// Move to a boundary.
    ///
    /// Returns the recipient projections of the action just taken when the
    /// move was a single step forward, so the caller can present that action
    /// as it happened. Every other move is a jump, and a jump reports only
    /// the position it arrived at.
    pub fn seek(
        &mut self,
        index: usize,
    ) -> Result<Option<Vec<(PlayerId, ObservedTransition)>>, ReplayError> {
        let index = index.min(self.events.len());
        if index == self.cursor_index {
            return Ok(None);
        }

        if index == self.cursor_index + 1 {
            let event = &self.events[self.cursor_index];
            let observed = self
                .cursor
                .replay_stored_action_event_observations(event)
                .map_err(|source| ReplayError::Event {
                    index: self.cursor_index,
                    source,
                })?;
            self.cursor_index = index;
            return Ok(Some(observed));
        }

        let (start, checkpoint) = self
            .checkpoints
            .range(..=index)
            .next_back()
            .map(|(start, server)| (*start, server.clone()))
            .expect("the opening position is always kept");
        self.cursor = checkpoint;
        self.cursor_index = start;
        for offset in start..index {
            self.cursor
                .replay_stored_action_event(&self.events[offset])
                .map_err(|source| ReplayError::Event {
                    index: offset,
                    source,
                })?;
            self.cursor_index = offset + 1;
        }
        Ok(None)
    }

    /// The board at the cursor, as one viewer is entitled to see it.
    ///
    /// A seat reads its own projection. Somebody watching reads the public
    /// board, which a fogged match does not have, so a fogged match answers
    /// them with nothing rather than with somebody else's view of it.
    pub fn observation(&self, viewer: Option<PlayerId>) -> Option<Observation> {
        match viewer {
            Some(player) => self.cursor.player_observation(player),
            None => self.cursor.spectator_observation(),
        }
    }

    /// The recipient this viewer's projections are addressed to, or `None`
    /// when the match shows them nothing.
    pub fn recipient(&self, viewer: Option<PlayerId>) -> Option<PlayerId> {
        match viewer {
            Some(player) => self.cursor.has_player(player).then_some(player),
            None => self.cursor.spectator_player(),
        }
    }

    pub fn fog_enabled(&self) -> bool {
        self.fog_enabled
    }
}

/// Pick one recipient's projection out of a step's answer.
pub fn transition_for(
    observed: &[(PlayerId, ObservedTransition)],
    recipient: PlayerId,
) -> Option<&ObservedTransition> {
    observed
        .iter()
        .find(|(player, _)| *player == recipient)
        .map(|(_, transition)| transition)
}

fn boundary(server: &GameServer, acting_slot: Option<u8>) -> Boundary {
    Boundary {
        day: server.state().turn.day,
        acting_slot,
        active_slot: server.active_player().map(|player| player.0),
    }
}
