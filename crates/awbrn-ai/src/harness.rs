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

type Observer<'a> = &'a mut dyn FnMut(&State, Option<&Command>);

/// What stops a game the agents do not finish.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Days before the harness abandons the game.
    ///
    /// A random agent almost never captures a headquarters, so its games do
    /// not end on their own.
    ///
    /// The cap is a day rather than a player turn, because a day is what a
    /// played match is measured in and a player turn is not: the same cap of
    /// sixty turns is thirty days of a duel and twenty of a three-player
    /// match. The harness stops once the reducer rolls the day past this one,
    /// so a cap of 35 plays day 35 to its end.
    pub days: u32,
    /// Refusals in a row that end the turn by force.
    ///
    /// A refused offer changes nothing, so a loop that ends a turn only when
    /// no offer is left has no guarantee that it makes progress. This bounds
    /// it: the turn then ends whatever the reducer says, the day rises, and
    /// the day cap always stops the game.
    pub refusals: u32,
}

impl Limits {
    /// What a game costs unless a caller says otherwise.
    ///
    /// Thirty-five days is the length of a played game. The refusal cap is
    /// high enough that an ordinary fog refusal does not end a turn early;
    /// [`Record::refusals`] is what says whether that holds.
    pub const DEFAULT: Self = Self {
        days: 35,
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
    /// The day the game stopped on.
    ///
    /// One more than the cap when the game was abandoned, because the harness
    /// stops when the day rolls past it.
    pub days: u32,
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
    play_inner(state, session, agents, entropy, limits, None)
}

/// Play one game and report its initial state and each accepted command.
///
/// The observer runs after the session adopts the new state. Its command is
/// `None` for the initial state and `Some` after an accepted command.
pub fn play_observed<E: Entropy>(
    state: State,
    session: &mut Session,
    agents: &mut [&mut dyn Agent],
    entropy: &mut E,
    limits: Limits,
    mut observer: impl FnMut(&State, Option<&Command>),
) -> Record {
    play_inner(state, session, agents, entropy, limits, Some(&mut observer))
}

fn play_inner<E: Entropy>(
    state: State,
    session: &mut Session,
    agents: &mut [&mut dyn Agent],
    entropy: &mut E,
    limits: Limits,
    mut observer: Option<Observer<'_>>,
) -> Record {
    assert!(
        state.players.len() <= agents.len(),
        "the roster seats {} players and {} agents were given",
        state.players.len(),
        agents.len(),
    );
    session.reset(state);
    if let Some(observer) = observer.as_mut() {
        observer(session.state(), None);
    }

    let mut turns = 0;
    let mut commands = 0;
    let mut refusals = 0;
    let mut refusals_in_a_row = 0;

    loop {
        let outcome = match &session.state().match_state {
            Match::Active { .. } => None,
            Match::Finished { outcome } => Some(outcome.clone()),
        };
        let day = u32::try_from(session.state().turn.day).unwrap_or(u32::MAX);
        if outcome.is_some() || day > limits.days {
            return Record {
                outcome,
                turns,
                days: day,
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

        let observed_command = observer.is_some().then(|| command.clone());
        match execute_with(session.state(), command, entropy) {
            Ok(ExecuteOutcome::Accepted(execution)) => {
                session.reset(execution.state);
                if let Some(observer) = observer.as_mut() {
                    observer(session.state(), observed_command.as_ref());
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::RandomAgent;
    use crate::board::arena;
    use crate::rng::Rng;

    /// The cap is a day, and the harness stops on the day after it.
    ///
    /// Two random agents on the arena board never finish a game, so what stops
    /// this one is the cap, and what it stops on is the number the cap names.
    /// A cap counted in player turns would mean a different length of match on
    /// a board with three seats.
    #[test]
    fn a_game_stops_the_day_after_the_cap() {
        const LIMITS: Limits = Limits {
            days: 4,
            ..Limits::DEFAULT
        };

        let mut session = Session::new(arena(false, 1));
        let mut entropy = Rng::from_seed(1);
        let mut first = RandomAgent::from_seed(2);
        let mut second = RandomAgent::from_seed(3);
        let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut second];

        let record = play(
            arena(false, 1),
            &mut session,
            &mut agents,
            &mut entropy,
            LIMITS,
        );

        assert!(
            record.abandoned(),
            "the game ended on its own, so the cap was never reached"
        );
        assert_eq!(record.days, LIMITS.days + 1);
        // Two seats, so a day is two player turns.
        assert_eq!(record.turns, LIMITS.days * 2);
    }
}
