//! The headless driver: two agents, one board, no client and no network.
//!
//! [`play`] holds the loop every measurement runs. Observe, ask the agent,
//! spell the play against the true state, execute, observe again. The agent
//! never touches the state, so an agent cannot see through fog by accident.
//!
//! This is not the server. The server is the authority for a real match, with
//! recorded entropy and a client on the other end. The harness plays ten
//! thousand throwaway games from a seeded tape, and keeps one [`Session`]
//! across all of them.
//!
//! **Events are not delivered.** [`Agent::observe`] exists and nothing calls
//! it. Delivering the events correctly costs one `observe_events` for each
//! player for each command, and that cost lands inside the number the
//! throughput measurement compares against `ai-cycle-complete`. No agent keeps
//! a belief about hidden units yet, so no agent needs them. The first agent
//! that does is what pays for the wiring.

use awvm::random::Entropy;
use awvm::semantic::{AwbwVisibility, Match, Outcome, State, TeamId, observe};
use awvm::session::Session;
use awvm::transition::{Command, ExecuteOutcome, execute_with};

use crate::agent::Agent;

/// What stops a game the agents do not finish.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Player turns before the harness abandons the game.
    ///
    /// A random agent almost never captures a headquarters, so its games do
    /// not end on their own.
    pub turns: u32,
    /// Refusals in a row that end the turn by force.
    ///
    /// A refused offer changes nothing, so a loop that ends a turn only when
    /// no offer is left has no guarantee that it makes progress. This bounds
    /// it: `turns` then rises whatever the reducer says, and the turn cap
    /// always stops the game.
    pub refusals: u32,
}

impl Limits {
    /// What a game costs unless a caller says otherwise.
    ///
    /// Sixty player turns is about a thirty-day duel, which is the length of a
    /// played game. The refusal cap is high enough that an ordinary fog refusal
    /// does not end a turn early; [`Record::refusals`] is what says whether
    /// that holds.
    pub const DEFAULT: Self = Self {
        turns: 60,
        refusals: 64,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What one game did.
#[derive(Clone, Debug)]
pub struct Record {
    /// How the match ended, or `None` when it reached a limit instead.
    pub outcome: Option<Outcome>,
    /// Player turns played, counting each forced turn end.
    pub turns: u32,
    /// Commands the reducer accepted.
    pub commands: u64,
    /// Offers the reducer refused.
    ///
    /// Each one costs a complete cycle and counts as no command, so a large
    /// share makes a measured rate pessimistic. Under fog a refusal is the
    /// honest answer to a hidden blocker, not a fault.
    pub refusals: u64,
    /// Units on the board when the game stopped.
    ///
    /// This is the first thing to read when a measured rate and a modeled rate
    /// disagree: units collect over a game, and the state clone, the
    /// projection and the enumeration all grow with them.
    pub units: usize,
}

impl Record {
    /// Whether the game stopped at a limit rather than at its own end.
    pub const fn abandoned(&self) -> bool {
        self.outcome.is_none()
    }

    /// The teams that won, empty for a draw and for an abandoned game.
    pub fn winners(&self) -> &[TeamId] {
        match &self.outcome {
            Some(Outcome::Victory { winners, .. }) => winners,
            _ => &[],
        }
    }
}

/// Play one game to its end or to a limit.
///
/// `session` is scratch. It is reset onto `state` first and reset again after
/// every accepted command, which is what keeps the board-sized tables it
/// allocated instead of handing them back between games.
///
/// `agents` is indexed by seat: the agent at index `n` plays the roster's seat
/// `n`. A roster with more seats than agents panics, because a game one seat
/// cannot play is not a game.
pub fn play<E: Entropy>(
    state: State,
    session: &mut Session,
    agents: &mut [&mut dyn Agent],
    entropy: &mut E,
    limits: Limits,
) -> Record {
    assert!(
        state.players.len() <= agents.len(),
        "the roster seats {} players and {} agents were given",
        state.players.len(),
        agents.len(),
    );
    session.reset(state);

    let mut turns = 0;
    let mut commands = 0;
    let mut refusals = 0;
    let mut refusals_in_a_row = 0;

    loop {
        let outcome = match &session.state().match_state {
            Match::Active { .. } => None,
            Match::Finished { outcome } => Some(outcome.clone()),
        };
        if outcome.is_some() || turns >= limits.turns {
            return Record {
                outcome,
                turns,
                commands,
                refusals,
                units: session.state().units.iter().count(),
            };
        }

        let player = session.state().turn.active_player.clone();
        let seat = session
            .state()
            .players
            .seat(&player)
            .expect("the active player holds a seat in their own roster");
        let end_turn = || Command::EndTurn {
            player: player.clone(),
        };

        let command = if refusals_in_a_row >= limits.refusals {
            refusals_in_a_row = 0;
            end_turn()
        } else {
            let view = observe(&AwbwVisibility, session.state(), &player)
                .expect("the active player can observe the position they act on");
            // A `None` from the agent ends the turn. A `None` from the play
            // ends it too: the true state holds no such route, which a hidden
            // blocker does, and passing is the honest answer.
            match agents[seat.get()]
                .act(&view)
                .and_then(|play| play.command(session))
            {
                Some(command) => command,
                None => end_turn(),
            }
        };

        // Only an accepted end turn raises the count: a refused one changes
        // nothing, and an agent that ends its own turn must count the same as
        // one the harness ends for it.
        let ends_turn = matches!(command, Command::EndTurn { .. });

        match execute_with(session.state(), command, entropy) {
            Ok(ExecuteOutcome::Accepted(execution)) => {
                session.reset(execution.state);
                commands += 1;
                refusals_in_a_row = 0;
                if ends_turn {
                    turns += 1;
                }
            }
            Ok(ExecuteOutcome::Rejected(_)) => {
                refusals += 1;
                refusals_in_a_row += 1;
            }
            Err(error) => panic!("the reducer failed on a generated command: {error:?}"),
        }
    }
}
