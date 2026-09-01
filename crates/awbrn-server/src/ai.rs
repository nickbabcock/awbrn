//! A seat the server plays for.
//!
//! An AI seat is not a second way to run a match. It submits the same
//! [`GameCommand`]s a person's seat submits, through the same authority, and
//! leaves the same [`StoredActionEvent`](crate::StoredActionEvent) behind, so a
//! match with an opponent in it replays through the one path every match
//! replays through and its result is recorded the one way every result is.
//!
//! The agent decides in AWVM's own vocabulary, against the fog-limited
//! observation its seat is entitled to — never the true state. What it names is
//! spelled against the authority's position and then written back down as a
//! wire command, which is the record.
//!
//! The caller owns the loop. Ask for a command, submit it, say whether it was
//! taken, ask again. The seat says the turn is over by answering `None`, which
//! is what stops a host from having to know when an agent is done.

use awbrn_ai::{Agent, AiProfile, NodeBudget};
use awbrn_game::{GameCommand, PlayerId, game_command};
use awvm::session::Session;

use crate::GameServer;

/// The most commands one turn may issue.
///
/// A turn is bounded by the units on the board, so this is not a rule of the
/// game but a stop on a host that cannot afford an agent that will not finish.
pub const MAX_COMMANDS_PER_TURN: u32 = 512;

/// Refusals in a row that end the turn.
///
/// A play built against a projection can be refused by the true state, and one
/// refusal is an answer rather than a fault. A seat that cannot get a command
/// taken this many times running is not going to.
const REFUSAL_LIMIT: u32 = 16;

/// One seat, playing its own turn.
pub struct AiSeat {
    player: PlayerId,
    agent: Box<dyn Agent>,
    budget: NodeBudget,
    issued: u32,
    refusals: u32,
    finished: bool,
}

impl std::fmt::Debug for AiSeat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiSeat")
            .field("player", &self.player)
            .field("issued", &self.issued)
            .field("refusals", &self.refusals)
            .field("finished", &self.finished)
            .finish()
    }
}

impl AiSeat {
    /// Seat one profile, ready to be asked for the turn it is on.
    ///
    /// The seed decides every choice the agent makes, so a caller that derives
    /// it from the match's own seed gets a seat that plays the same way whether
    /// the match has been open for an hour or was rebuilt from its log.
    pub fn new(player: PlayerId, profile: &AiProfile, seed: u64) -> Self {
        let mut agent = profile.agent(seed);
        agent.start_match();
        Self {
            player,
            agent,
            budget: profile.node_budget(),
            issued: 0,
            refusals: 0,
            finished: false,
        }
    }

    /// The seat this plays.
    pub const fn player(&self) -> PlayerId {
        self.player
    }

    /// Commands this seat has had accepted this turn.
    pub const fn issued(&self) -> u32 {
        self.issued
    }

    /// Open the turn on the position the seat can see.
    pub fn begin_turn(&mut self, server: &GameServer) {
        if let Some(view) = server.player_observation(self.player) {
            self.agent.start_turn(&view);
        }
    }

    /// The next command to submit, or `None` once the turn is over.
    ///
    /// A play the true state cannot spell ends the turn rather than being
    /// guessed at: the agent chose it against a board it could see, and the
    /// board it could not is the authority's business, not the agent's.
    pub fn next_command(&mut self, server: &GameServer) -> Option<GameCommand> {
        if self.finished {
            return None;
        }
        if self.issued >= MAX_COMMANDS_PER_TURN || self.refusals >= REFUSAL_LIMIT {
            return Some(self.end_turn());
        }

        let Some(view) = server.player_observation(self.player) else {
            return Some(self.end_turn());
        };
        let Some(play) = self.agent.act(&view, self.budget) else {
            return Some(self.end_turn());
        };

        // Spelled against the authority's position, so the route and the
        // target are the board the reducer will validate against.
        let session = Session::new(server.state().clone());
        let Some(command) = play.command(&session) else {
            return Some(self.end_turn());
        };
        let Ok(command) = game_command(&command, server.state()) else {
            return Some(self.end_turn());
        };

        // A seat ends its turn by passing, and only the host may time a seat
        // out. Either spelled as a play is the end of the turn and nothing
        // more.
        if matches!(command, GameCommand::EndTurn | GameCommand::Timeout) {
            return Some(self.end_turn());
        }

        self.issued = self.issued.saturating_add(1);
        Some(command)
    }

    /// Tell the seat its last command was accepted.
    pub fn accepted(&mut self, server: &GameServer) {
        self.refusals = 0;
        if let Some(view) = server.player_observation(self.player) {
            self.agent.refresh(&view);
        }
    }

    /// Tell the seat its last command was refused.
    pub fn refused(&mut self) {
        self.refusals = self.refusals.saturating_add(1);
    }

    fn end_turn(&mut self) -> GameCommand {
        self.finished = true;
        GameCommand::EndTurn
    }
}
