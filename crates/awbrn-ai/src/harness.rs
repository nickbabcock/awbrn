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
use crate::shape::Shape;

type Observer<'a> = &'a mut dyn FnMut(&State, Option<&Command>);

/// What stops a game the agents do not finish.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Days the game may last.
    ///
    /// A random agent almost never captures a headquarters, so its games do
    /// not end on their own.
    ///
    /// This is the ruleset's own day limit, not an abort: [`play`] writes it
    /// into `Settings::day_limit`, so the reducer ends the match at the end of
    /// this day and awards it to whoever holds the most properties, or draws
    /// it when they are level. A game that reaches the cap is therefore a
    /// decided game and not a thrown-away one, which is what a scored
    /// tournament needs. The harness keeps a check of its own for the day
    /// after, which nothing should reach.
    ///
    /// The cap is a day rather than a player turn, because a day is what a
    /// played match is measured in and a player turn is not: the same cap of
    /// sixty turns is thirty days of a duel and twenty of a three-player
    /// match.
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
    /// What the game looked like, seat by seat.
    ///
    /// A result says which agent won. This says what the game it won was made
    /// of, which is what a term that prices combat must be measured against.
    /// See [`crate::shape`].
    ///
    /// Empty unless [`play_measured`] or [`play_observed`] played the game.
    /// Counting the shape costs about 7% of the command rate, and [`play`] is
    /// what the throughput measurement runs.
    pub shape: Shape,
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
    play_inner(state, session, agents, entropy, limits, None, false)
}

/// Play one game and count what it was made of.
///
/// The same game [`play`] plays, with [`Record::shape`] filled in. The count
/// reads the events of every command and samples each seat at the end of each
/// of its turns, which costs about 7% of the command rate. That is why it is
/// a second entry point and not the only one: the throughput measurement
/// prices the agent, not the measurement of the agent.
pub fn play_measured<E: Entropy>(
    state: State,
    session: &mut Session,
    agents: &mut [&mut dyn Agent],
    entropy: &mut E,
    limits: Limits,
) -> Record {
    play_inner(state, session, agents, entropy, limits, None, true)
}

/// Play one game and report its initial state and each accepted command.
///
/// The observer runs after the session adopts the new state. Its command is
/// `None` for the initial state and `Some` after an accepted command. The
/// shape is counted, because a caller that renders every turn does not need
/// the command rate.
pub fn play_observed<E: Entropy>(
    state: State,
    session: &mut Session,
    agents: &mut [&mut dyn Agent],
    entropy: &mut E,
    limits: Limits,
    mut observer: impl FnMut(&State, Option<&Command>),
) -> Record {
    play_inner(
        state,
        session,
        agents,
        entropy,
        limits,
        Some(&mut observer),
        true,
    )
}

fn play_inner<E: Entropy>(
    state: State,
    session: &mut Session,
    agents: &mut [&mut dyn Agent],
    entropy: &mut E,
    limits: Limits,
    mut observer: Option<Observer<'_>>,
    measure: bool,
) -> Record {
    assert!(
        state.players.len() <= agents.len(),
        "the roster seats {} players and {} agents were given",
        state.players.len(),
        agents.len(),
    );
    // The cap is the ruleset's, so a game that reaches it is decided on
    // properties held rather than abandoned. Anything the caller set is
    // replaced: two limits on one game is one limit too many.
    let mut state = state;
    state.settings.day_limit = Some(u64::from(limits.days));
    session.reset(state);
    if let Some(observer) = observer.as_mut() {
        observer(session.state(), None);
    }

    let mut turns = 0;
    let mut commands = 0;
    let mut refusals = 0;
    let mut refusals_in_a_row = 0;
    let mut shape = if measure {
        Shape::new(session.state().players.len())
    } else {
        Shape::default()
    };

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
                shape,
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
                if measure {
                    // The state the command ran against is the only one that
                    // still holds the units it removed, so the count comes
                    // first.
                    shape.observe(session.state(), &execution.events);
                }
                session.reset(execution.state);
                if measure && ends_turn {
                    shape.sample_turn(session.state(), seat);
                }
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

    /// The cap decides the game, and it decides it on the day it names.
    ///
    /// Two random agents on the arena board never finish a game, so what ends
    /// this one is the cap. It ends it the way the ruleset ends a day limit,
    /// with the properties counted and a winner named, and not by throwing the
    /// position away: a tournament that scores an abandoned game as a draw
    /// measures the cap instead of the agents. A cap counted in player turns
    /// would also mean a different length of match on a board with three
    /// seats.
    #[test]
    fn a_game_reaching_the_cap_is_decided_on_the_day_it_names() {
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
            !record.abandoned(),
            "the reducer named an outcome, so the harness threw nothing away"
        );
        assert_eq!(record.days, LIMITS.days);
        // Two seats, so a day is two player turns.
        assert_eq!(record.turns, LIMITS.days * 2);
    }
}
