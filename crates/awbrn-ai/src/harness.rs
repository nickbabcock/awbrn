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
//! Accepted events are delivered to the active agent. The authority owns the
//! state and projects each transition before the agent receives it.

use std::time::Instant;

use awvm::random::Entropy;
use awvm::semantic::{
    AwbwVisibility, Match, Observation, Outcome, PlayerId, State, TeamId, observe, observe_events,
    observe_into,
};
use awvm::session::Session;
use awvm::transition::{Command, ExecuteError, ExecuteOutcome, execute_with};
use awvm::violation::Violation;

use crate::agent::{Agent, NodeBudget};
use crate::fingerprint::{FNV1A_OFFSET_BASIS, FNV1A_PRIME};
use crate::mission::TurnEndReason;
use crate::shape::Shape;

type Observer<'a, Error> = &'a mut dyn FnMut(&State, Option<&Command>) -> Result<(), Error>;

fn elapsed_nanos(started: Instant) -> u64 {
    started.elapsed().as_nanos().try_into().unwrap_or(u64::MAX)
}

fn command_preflight(
    agent: &dyn Agent,
    state: &State,
    command: &Command,
) -> Result<Option<Violation>, ExecuteError> {
    if !agent.requires_command_preflight() {
        return Ok(None);
    }
    match awvm::transition::validate(state, command.clone()) {
        Ok(Ok(())) => Ok(None),
        Ok(Err(violation)) => Ok(Some(violation)),
        Err(error) => Err(error),
    }
}

/// Fold one command into a running FNV-1a fingerprint.
///
/// Anything that fingerprints a command stream uses this, so a stream recorded
/// by one caller can be compared with a stream recorded by another.
pub fn next_command_fingerprint(current: u64, command: &Command) -> u64 {
    let mut hash = current;
    for byte in serde_json::to_vec(command).expect("commands serialize") {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV1A_PRIME);
    }
    (hash ^ 0xff).wrapping_mul(FNV1A_PRIME)
}

/// The result of running one agent turn.
#[derive(Clone, Debug)]
pub struct TurnResult {
    /// The state after the accepted end-turn command.
    pub state: State,
    /// The player whose turn was run.
    pub active_player: PlayerId,
    /// Commands accepted during the turn.
    pub commands: Vec<Command>,
    /// Decision duration for each call to `act`.
    pub decision_nanos: Vec<u64>,
    /// Time from `start_turn` to the first command.
    pub first_command_nanos: u64,
    /// Time from `start_turn` to the accepted end-turn command.
    pub total_nanos: u64,
    /// Lifecycle timing counters for this turn.
    pub timing: crate::agent::AgentTiming,
    /// Number of rejected commands.
    pub rejected_commands: u32,
    /// Number of planned commands rejected before execution.
    pub preflight_rejections: u32,
    /// Number of plays that could not become commands.
    ///
    /// This field remains for compatibility. New rejections use
    /// `preflight_rejections`.
    pub unrealizable_plays: u32,
    /// Whether the accepted end-turn command completed the turn.
    pub completed: bool,
    /// Fingerprint of accepted commands.
    pub command_fingerprint: u64,
}

/// Run exactly one agent turn through the authority lifecycle.
///
/// The match-level caller must call [`Agent::start_match`] once before it uses
/// this function for the turns in a match.
pub fn run_agent_turn<E: Entropy>(
    state: State,
    agent: &mut dyn Agent,
    entropy: &mut E,
    node_budget: NodeBudget,
) -> Result<TurnResult, ExecuteError> {
    run_agent_turn_inner(state, agent, entropy, node_budget, true)
}

/// Run one turn without reading the clock.
pub fn run_agent_turn_unmeasured<E: Entropy>(
    state: State,
    agent: &mut dyn Agent,
    entropy: &mut E,
    node_budget: NodeBudget,
) -> Result<TurnResult, ExecuteError> {
    run_agent_turn_inner(state, agent, entropy, node_budget, false)
}

fn run_agent_turn_inner<E: Entropy>(
    state: State,
    agent: &mut dyn Agent,
    entropy: &mut E,
    node_budget: NodeBudget,
    measure: bool,
) -> Result<TurnResult, ExecuteError> {
    const REFUSAL_LIMIT: u32 = 64;

    let mut session = Session::new(state);
    let active_player = session.state().turn.active_player.clone();
    let mut view = observe(&AwbwVisibility, session.state(), &active_player)
        .expect("the active player can observe the position they act on");
    let timing_before = agent.timing().unwrap_or_default();
    let turn_started = measure.then(Instant::now);
    agent.start_turn(&view);
    let mut commands = Vec::new();
    let mut decision_nanos = Vec::new();
    let mut first_command_nanos = None;
    let mut rejected_commands: u32 = 0;
    let mut preflight_rejections: u32 = 0;
    let unrealizable_plays: u32 = 0;
    let mut refusals_in_a_row = 0;
    let mut command_fingerprint = FNV1A_OFFSET_BASIS;
    let completed;

    loop {
        // A command can finish the match part way through a turn: the last
        // enemy unit falls, or a headquarters changes hands. The reducer
        // refuses every command after that, end-turn included, so a loop that
        // keeps asking never leaves.
        if matches!(session.state().match_state, Match::Finished { .. }) {
            agent
                .finalize_trace(TurnEndReason::MatchFinished)
                .expect("the agent finalizes its turn trace");
            agent.clear_trace();
            completed = false;
            break;
        }
        let decision_started = measure.then(Instant::now);
        let mut unrealizable_play = false;
        let (command, reason) = if refusals_in_a_row >= REFUSAL_LIMIT {
            (
                Command::EndTurn {
                    player: active_player.clone(),
                },
                TurnEndReason::RefusalLimit,
            )
        } else {
            match agent.act(&view, node_budget) {
                None => (
                    Command::EndTurn {
                        player: active_player.clone(),
                    },
                    TurnEndReason::AgentPass,
                ),
                Some(play) => match play.command(&session) {
                    None => {
                        unrealizable_play = true;
                        (
                            Command::EndTurn {
                                player: active_player.clone(),
                            },
                            TurnEndReason::UnrealizablePlay,
                        )
                    }
                    Some(command) => {
                        let reason = if matches!(command, Command::EndTurn { .. }) {
                            TurnEndReason::ExplicitEndTurn
                        } else {
                            TurnEndReason::AgentPass
                        };
                        (command, reason)
                    }
                },
            }
        };
        decision_nanos.push(decision_started.map(elapsed_nanos).unwrap_or(0));
        if first_command_nanos.is_none()
            && let Some(started) = turn_started
        {
            first_command_nanos = Some(elapsed_nanos(started));
        }

        if unrealizable_play {
            preflight_rejections = preflight_rejections.saturating_add(1);
            refusals_in_a_row = refusals_in_a_row.saturating_add(1);
            agent.reject(&view);
            continue;
        }

        let ends_turn = matches!(command, Command::EndTurn { .. });
        let accepted_command = command.clone();
        if command_preflight(agent, session.state(), &command)?.is_some() {
            preflight_rejections = preflight_rejections.saturating_add(1);
            refusals_in_a_row = refusals_in_a_row.saturating_add(1);
            agent.reject(&view);
            continue;
        }
        agent.classify_command(&view, &command);
        match execute_with(session.state(), command, entropy) {
            Ok(ExecuteOutcome::Accepted(execution)) => {
                let observed_events = observe_events(
                    &AwbwVisibility,
                    session.state(),
                    &execution.state,
                    &execution.events,
                    &active_player,
                )
                .expect("the active player can observe the accepted transition");
                agent.observe(&observed_events);
                command_fingerprint =
                    next_command_fingerprint(command_fingerprint, &accepted_command);
                commands.push(accepted_command);
                session.reset(execution.state);
                let match_finished = matches!(session.state().match_state, Match::Finished { .. });
                if ends_turn || match_finished {
                    let reason = if match_finished {
                        TurnEndReason::MatchFinished
                    } else {
                        reason
                    };
                    agent
                        .finalize_trace(reason)
                        .expect("the agent finalizes its turn trace");
                    agent.clear_trace();
                    completed = ends_turn;
                    break;
                }
                observe_into(&AwbwVisibility, session.state(), &active_player, &mut view)
                    .expect("the active player can observe the refreshed position");
                agent.refresh(&view);
                refusals_in_a_row = 0;
            }
            Ok(ExecuteOutcome::Rejected(_)) => {
                rejected_commands = rejected_commands.saturating_add(1);
                refusals_in_a_row = refusals_in_a_row.saturating_add(1);
                agent.reject(&view);
            }
            Err(error) => return Err(error),
        }
    }

    Ok(TurnResult {
        state: session.state().clone(),
        active_player,
        commands,
        decision_nanos,
        first_command_nanos: first_command_nanos.unwrap_or(0),
        total_nanos: turn_started.map(elapsed_nanos).unwrap_or(0),
        timing: agent.timing().unwrap_or_default().since(timing_before),
        rejected_commands,
        preflight_rejections,
        unrealizable_plays,
        completed,
        command_fingerprint,
    })
}

/// What stops a game the agents do not finish.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Candidate turn plans one decision may evaluate.
    pub nodes: NodeBudget,
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
        nodes: NodeBudget::FOUR,
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
    /// Planned commands rejected before reducer execution.
    pub preflight_rejections: u64,
    /// Legacy count of plays that could not be resolved.
    ///
    /// New runs report these in `preflight_rejections`.
    pub unrealizable_plays: u64,
    /// Rejected commands by roster seat.
    pub refusals_by_seat: Vec<u64>,
    /// Preflight rejections by roster seat.
    pub preflight_rejections_by_seat: Vec<u64>,
    /// Unrealizable plays by roster seat.
    pub unrealizable_plays_by_seat: Vec<u64>,
    /// Commands refused by the reducer.
    pub refusal_traces: Vec<RefusalTrace>,
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
    /// Fingerprint of accepted commands by seat when measured or observed.
    ///
    /// Empty for an unmeasured game without an observer.
    pub command_fingerprints: Vec<u64>,
    /// Complete accepted-turn durations by roster seat, in nanoseconds.
    pub complete_turn_times_by_seat: Vec<Vec<u64>>,
}

/// One command refused by the reducer.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct RefusalTrace {
    /// The state used to make the plan, when the agent provides it.
    pub planned_state: Option<State>,
    /// The command used by the plan, when the agent provides it.
    pub planned_command: Option<Command>,
    /// The authoritative state used for execution.
    pub executed_state: State,
    /// The command sent to the reducer.
    pub command: Command,
    /// The reducer violation.
    pub violation: Violation,
    /// Commands accepted earlier in the same turn.
    pub preceding_commands: Vec<Command>,
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
) -> Result<Record, ExecuteError> {
    play_inner::<E, ExecuteError>(state, session, agents, entropy, limits, None, false)
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
) -> Result<Record, ExecuteError> {
    play_inner::<E, ExecuteError>(state, session, agents, entropy, limits, None, true)
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
) -> Result<Record, ExecuteError> {
    let mut observer = |state: &State, command: Option<&Command>| {
        observer(state, command);
        Ok(())
    };
    play_inner::<E, ExecuteError>(
        state,
        session,
        agents,
        entropy,
        limits,
        Some(&mut observer),
        true,
    )
}

/// Play one game with a fallible observer.
pub fn play_observed_fallible<E: Entropy, Error>(
    state: State,
    session: &mut Session,
    agents: &mut [&mut dyn Agent],
    entropy: &mut E,
    limits: Limits,
    mut observer: impl FnMut(&State, Option<&Command>) -> Result<(), Error>,
) -> Result<Record, Error>
where
    Error: From<ExecuteError>,
{
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

fn play_inner<E: Entropy, Error>(
    state: State,
    session: &mut Session,
    agents: &mut [&mut dyn Agent],
    entropy: &mut E,
    limits: Limits,
    mut observer: Option<Observer<'_, Error>>,
    measure: bool,
) -> Result<Record, Error>
where
    Error: From<ExecuteError>,
{
    const REFUSAL_TRACE_LIMIT: usize = 64;

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
    for agent in agents.iter_mut() {
        agent.start_match();
    }
    if let Some(observer) = observer.as_mut() {
        observer(session.state(), None)?;
    }

    let mut projection: Option<Observation> = None;
    let mut turns = 0;
    let mut commands = 0;
    let mut refusals = 0;
    let mut preflight_rejections = 0;
    let unrealizable_plays = 0;
    let mut refusals_by_seat = vec![0_u64; agents.len()];
    let mut preflight_rejections_by_seat = vec![0_u64; agents.len()];
    let unrealizable_plays_by_seat = vec![0_u64; agents.len()];
    let mut refusals_in_a_row = 0;
    let mut started_turn: Option<(PlayerId, u64, usize)> = None;
    let mut turn_started_at: Option<Instant> = None;
    let mut complete_turn_times_by_seat = vec![Vec::new(); agents.len()];
    let mut turn_commands = Vec::new();
    let mut refusal_traces = Vec::new();
    let track_command_fingerprints = measure || observer.is_some();
    let mut command_fingerprints = if track_command_fingerprints {
        vec![FNV1A_OFFSET_BASIS; agents.len()]
    } else {
        Vec::new()
    };
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
            return Ok(Record {
                outcome,
                turns,
                days: day,
                commands,
                refusals,
                preflight_rejections,
                unrealizable_plays,
                refusals_by_seat,
                preflight_rejections_by_seat,
                unrealizable_plays_by_seat,
                refusal_traces,
                units: session.state().units.iter().count(),
                shape,
                command_fingerprints,
                complete_turn_times_by_seat,
            });
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

        // Refresh the active view before every decision, including a forced
        // refusal-limit end. This keeps lifecycle callbacks authoritative.
        let view = match &mut projection {
            Some(view) => {
                observe_into(&AwbwVisibility, session.state(), &player, view)
                    .expect("the active player can observe the position they act on");
                view
            }
            None => projection.insert(
                observe(&AwbwVisibility, session.state(), &player)
                    .expect("the active player can observe the position they act on"),
            ),
        };
        let turn_key = (
            player.clone(),
            session.state().turn.day,
            session.state().turn.position,
        );
        if started_turn.as_ref() != Some(&turn_key) {
            turn_commands.clear();
            turn_started_at = measure.then(Instant::now);
            agents[seat.get()].start_turn(view);
            started_turn = Some(turn_key);
        }
        let mut unrealizable_play = false;
        let (command, reason) = if refusals_in_a_row >= limits.refusals {
            refusals_in_a_row = 0;
            (end_turn(), TurnEndReason::RefusalLimit)
        } else {
            // A `None` from the agent ends the turn. A `None` from the play
            // discards the stale plan and asks the agent for a new one.
            match agents[seat.get()].act(view, limits.nodes) {
                None => (end_turn(), TurnEndReason::AgentPass),
                Some(play) => match play.command(session) {
                    None => {
                        unrealizable_play = true;
                        (end_turn(), TurnEndReason::UnrealizablePlay)
                    }
                    Some(command) => {
                        let reason = if matches!(command, Command::EndTurn { .. }) {
                            TurnEndReason::ExplicitEndTurn
                        } else {
                            TurnEndReason::AgentPass
                        };
                        (command, reason)
                    }
                },
            }
        };

        if unrealizable_play {
            preflight_rejections += 1;
            preflight_rejections_by_seat[seat.get()] += 1;
            refusals_in_a_row += 1;
            agents[seat.get()].reject(view);
            continue;
        }

        // Only an accepted end turn raises the count: a refused one changes
        // nothing, and an agent that ends its own turn must count the same as
        // one the harness ends for it.
        let ends_turn = matches!(command, Command::EndTurn { .. });

        let observed_command = observer.is_some().then(|| command.clone());
        let accepted_command = command.clone();
        if command_preflight(&*agents[seat.get()], session.state(), &command)?.is_some() {
            preflight_rejections += 1;
            preflight_rejections_by_seat[seat.get()] += 1;
            refusals_in_a_row += 1;
            agents[seat.get()].reject(view);
            continue;
        }
        agents[seat.get()].classify_command(view, &command);
        match execute_with(session.state(), command, entropy) {
            Ok(ExecuteOutcome::Accepted(execution)) => {
                let observed_events = observe_events(
                    &AwbwVisibility,
                    session.state(),
                    &execution.state,
                    &execution.events,
                    &player,
                )
                .expect("the active player can observe the accepted transition");
                agents[seat.get()].observe(&observed_events);
                if track_command_fingerprints {
                    command_fingerprints[seat.get()] = next_command_fingerprint(
                        command_fingerprints[seat.get()],
                        &accepted_command,
                    );
                }
                if measure {
                    // The state the command ran against is the only one that
                    // still holds the units it removed, so the count comes
                    // first.
                    shape.observe(session.state(), &execution.events);
                }
                session.reset(execution.state);
                let match_finished = matches!(session.state().match_state, Match::Finished { .. });
                if !ends_turn && !match_finished {
                    let view = match &mut projection {
                        Some(view) => {
                            observe_into(&AwbwVisibility, session.state(), &player, view)
                                .expect("the active player can observe the refreshed position");
                            view
                        }
                        None => projection.insert(
                            observe(&AwbwVisibility, session.state(), &player)
                                .expect("the active player can observe the refreshed position"),
                        ),
                    };
                    agents[seat.get()].refresh(view);
                } else {
                    let reason = if match_finished {
                        TurnEndReason::MatchFinished
                    } else {
                        reason
                    };
                    agents[seat.get()]
                        .finalize_trace(reason)
                        .expect("the agent finalizes its turn trace");
                    agents[seat.get()].clear_trace();
                }
                if ends_turn && let Some(started) = turn_started_at.take() {
                    complete_turn_times_by_seat[seat.get()].push(elapsed_nanos(started));
                }
                if measure && ends_turn {
                    shape.sample_turn(session.state(), seat);
                }
                if let Some(observer) = observer.as_mut() {
                    observer(session.state(), observed_command.as_ref())?;
                }
                turn_commands.push(accepted_command);
                commands += 1;
                refusals_in_a_row = 0;
                if ends_turn {
                    turns += 1;
                }
            }
            Ok(ExecuteOutcome::Rejected(violation)) => {
                if refusal_traces.len() < REFUSAL_TRACE_LIMIT {
                    refusal_traces.push(RefusalTrace {
                        planned_state: agents[seat.get()].planned_state(),
                        planned_command: agents[seat.get()].planned_command(),
                        executed_state: session.state().clone(),
                        command: accepted_command,
                        violation,
                        preceding_commands: turn_commands.clone(),
                    });
                }
                agents[seat.get()].reject(view);
                refusals += 1;
                refusals_by_seat[seat.get()] += 1;
                refusals_in_a_row += 1;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Play;
    use crate::agents::RandomAgent;
    use crate::board::arena;
    use crate::rng::Rng;
    use awvm::semantic::{CellIdx, UnitId, UnitKindId};
    use awvm::session::OrderKind;

    struct StalePlayAgent {
        offer_stale_play: bool,
        rejections: u32,
    }

    impl Agent for StalePlayAgent {
        fn act(&mut self, _view: &Observation, _budget: NodeBudget) -> Option<Play> {
            if self.offer_stale_play {
                self.offer_stale_play = false;
                Some(Play::new(
                    UnitId::new(u32::MAX),
                    CellIdx::from_raw(0),
                    OrderKind::Wait,
                ))
            } else {
                None
            }
        }

        fn reject(&mut self, _view: &Observation) {
            self.rejections += 1;
        }
    }

    #[test]
    fn a_stale_plan_is_replaced_before_reducer_execution() {
        let mut session = Session::new(arena(false, 1));
        let mut entropy = Rng::from_seed(2);
        let mut stale = StalePlayAgent {
            offer_stale_play: true,
            rejections: 0,
        };
        let mut random = RandomAgent::from_seed(3);
        let mut agents: [&mut dyn Agent; 2] = [&mut stale, &mut random];

        let record = play(
            arena(false, 1),
            &mut session,
            &mut agents,
            &mut entropy,
            Limits {
                days: 1,
                ..Limits::DEFAULT
            },
        )
        .expect("test game executes");

        assert_eq!(stale.rejections, 1);
        assert_eq!(record.preflight_rejections, 1);
        assert_eq!(record.preflight_rejections_by_seat[0], 1);
        assert_eq!(record.unrealizable_plays, 0);
        assert_eq!(record.refusals, 0);
    }

    struct InvalidProductionAgent {
        offered: bool,
        rejections: u32,
    }

    impl Agent for InvalidProductionAgent {
        fn act(&mut self, _view: &Observation, _budget: NodeBudget) -> Option<Play> {
            if self.offered {
                self.offered = false;
                Some(Play::unitless(
                    CellIdx::from_raw(0),
                    OrderKind::Produce(UnitKindId::Infantry),
                ))
            } else {
                None
            }
        }

        fn reject(&mut self, _view: &Observation) {
            self.rejections += 1;
        }
    }

    #[test]
    fn a_command_is_preflighted_before_the_reducer() {
        let mut session = Session::new(arena(false, 1));
        let mut entropy = Rng::from_seed(2);
        let mut invalid = InvalidProductionAgent {
            offered: true,
            rejections: 0,
        };
        let mut random = RandomAgent::from_seed(3);
        let mut agents: [&mut dyn Agent; 2] = [&mut invalid, &mut random];

        let record = play(
            arena(false, 1),
            &mut session,
            &mut agents,
            &mut entropy,
            Limits {
                days: 1,
                ..Limits::DEFAULT
            },
        )
        .expect("test game executes");

        assert_eq!(invalid.rejections, 1);
        assert_eq!(record.preflight_rejections, 1);
        assert_eq!(record.refusals, 0);
    }

    #[test]
    fn preflight_propagates_ruleset_errors() {
        let mut state = arena(false, 1);
        state.ruleset.revision = awvm::semantic::RulesetRevision::from("unsupported");
        let agent = RandomAgent::from_seed(3);
        let command = Command::EndTurn {
            player: state.turn.active_player.clone(),
        };

        assert_eq!(
            command_preflight(&agent, &state, &command),
            Err(ExecuteError::UnsupportedRuleset)
        );
    }

    struct CaptureVictoryAgent {
        calls: u32,
    }

    impl Agent for CaptureVictoryAgent {
        fn act(&mut self, _view: &Observation, _budget: NodeBudget) -> Option<Play> {
            self.calls += 1;
            Some(Play::new(
                UnitId::new(0),
                CellIdx::from_raw(0),
                OrderKind::Capture,
            ))
        }
    }

    #[test]
    fn a_match_ending_command_stops_the_turn_loop() {
        let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../spec/fixtures/capture/capture-hq-victory.json"
        ))
        .expect("capture victory fixture decodes");
        let state: State = serde_json::from_value(fixture["initial_state"].clone())
            .expect("capture victory state decodes");
        let mut agent = CaptureVictoryAgent { calls: 0 };
        let mut entropy = Rng::from_seed(2);
        let result = run_agent_turn(state, &mut agent, &mut entropy, NodeBudget::FOUR)
            .expect("test turn executes");

        assert!(!result.completed);
        assert_eq!(agent.calls, 1);
        assert_eq!(result.commands.len(), 1);
        assert!(matches!(result.state.match_state, Match::Finished { .. }));
        assert_eq!(result.rejected_commands, 0);
        assert_eq!(result.preflight_rejections, 0);
    }

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
        )
        .expect("test game executes");

        assert!(
            !record.abandoned(),
            "the reducer named an outcome, so the harness threw nothing away"
        );
        assert_eq!(record.days, LIMITS.days);
        // Two seats, so a day is two player turns.
        assert_eq!(record.turns, LIMITS.days * 2);
        assert!(record.command_fingerprints.is_empty());
    }
}
