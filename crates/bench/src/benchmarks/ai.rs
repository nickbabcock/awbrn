//! AWVM command and turn-enumeration costs at the scale of a real match.
//!
//! [`late_game`] measures complete turns on fixed replay positions.
//!
//! Small specification fixtures hide the state clone that each accepted
//! command creates. The command cases use 20x20 boards with a full deployment
//! so the clone has the size that an opponent search pays for each node.
//!
//! Turn enumeration is separate because a policy asks for every legal action
//! before it chooses one. Movement actions use read-only preparation. The
//! authoritative and observed cases show the remaining cost of enumerating a
//! complete turn and the extra work that a fog-safe policy needs.
//!
//! The cycle cases keep execute, observation, reification, and action
//! enumeration separate. This identifies the stage that controls the cost of
//! one search node. Fog-on and fog-off cases use the same board and command.

pub mod late_game;

use ::awvm::query;
use ::awvm::random::RandomTape;
use ::awvm::ruleset::UnitKind;
use ::awvm::semantic::{
    AwbwVisibility, CellIdx, Location, Match, Observation, Pos, State, UnitAction, UnitId, observe,
    observe_into,
};
use ::awvm::session::{Order, OrderKind, OrderMask, Session, UnitIdx};
use ::awvm::transition::{Command, ExecuteOutcome, execute};
use awbrn_ai::agent::{Agent, NodeBudget};
use awbrn_ai::agents::{GreedyAgent, Weights};
use awbrn_ai::board::amber_valley;
use awbrn_ai::eval::{EvalWeights, Evaluator};
use awbrn_ai::harness::{Limits, Record, play};
use awbrn_ai::rng::Rng;
use awbrn_ai::threat::ThreatMap;

use super::{awvm, server};

const WIDTH: u8 = 20;
const HEIGHT: u8 = 20;
const UNITS: usize = 30;

/// One accepted command and its input state.
#[derive(Debug)]
pub struct CommandCase {
    pub state: State,
    pub command: Command,
}

/// One accepted command and a session already open on the position it names.
///
/// Opening the session clones the state, which is what a server pays once at
/// its edge and not per command. Holding it here keeps the clone out of the
/// measured region.
#[derive(Debug)]
pub struct ResolveCase {
    pub session: Session,
    pub command: Command,
}

impl From<CommandCase> for ResolveCase {
    fn from(case: CommandCase) -> Self {
        Self {
            session: Session::new(case.state),
            command: case.command,
        }
    }
}

pub fn projected_move() -> CommandCase {
    let source = awvm::CASES[0].1;
    let projected = awvm::project(source, WIDTH, HEIGHT, UNITS);
    let case = awvm::load(source);
    CommandCase {
        state: projected.state,
        command: serde_json::from_value(case.command).expect("decode command"),
    }
}

pub fn server_move() -> CommandCase {
    let state = server::state(server::DUEL, false);
    CommandCase {
        command: Command::MoveWait {
            player: state.turn.active_player.clone(),
            unit: UnitId::new(1),
            path: vec![Pos::new(0, 3), Pos::new(0, 4), Pos::new(0, 5)],
        },
        state,
    }
}

pub fn server_produce() -> CommandCase {
    let state = server::state(server::DUEL, false);
    CommandCase {
        command: Command::ProduceUnit {
            player: state.turn.active_player.clone(),
            position: Pos::new(2, 0),
            kind: UnitKind::Infantry,
        },
        state,
    }
}

pub fn server_capture() -> CommandCase {
    let mut state = server::state(server::DUEL, false);
    let unit = state.units.get_mut(UnitId::new(1)).expect("mover exists");
    let profile = ::awvm::ruleset::profile(UnitKind::Infantry);
    unit.kind = UnitKind::Infantry;
    unit.fuel = profile.max_fuel;
    unit.ammo = profile.max_ammo;
    unit.location = Location::Board {
        position: Pos::new(3, 9),
    };
    CommandCase {
        command: Command::MoveCapture {
            player: state.turn.active_player.clone(),
            unit: UnitId::new(1),
            path: vec![Pos::new(3, 9), Pos::new(3, 10)],
        },
        state,
    }
}

pub fn run_command(case: &CommandCase) -> usize {
    match execute(&case.state, case.command.clone(), &[]) {
        Ok(ExecuteOutcome::Accepted(execution)) => execution.events.len(),
        Ok(ExecuteOutcome::Rejected(violation)) => {
            panic!("benchmark command was rejected: {violation:?}")
        }
        Err(error) => panic!("benchmark command did not execute: {error:?}"),
    }
}

fn execute_state(case: &CommandCase) -> State {
    match execute(&case.state, case.command.clone(), &[]) {
        Ok(ExecuteOutcome::Accepted(execution)) => execution.state,
        Ok(ExecuteOutcome::Rejected(violation)) => {
            panic!("benchmark command was rejected: {violation:?}")
        }
        Err(error) => panic!("benchmark command did not execute: {error:?}"),
    }
}

/// The server's edge cost. One wire command becomes one order.
///
/// This replaced preparation. A server resolves once and applies. A search
/// never calls it, because it holds orders already.
pub fn run_resolve(case: &ResolveCase) -> usize {
    let order = case
        .session
        .resolve(&case.command)
        .unwrap_or_else(|error| panic!("benchmark command did not resolve: {error:?}"));
    std::hint::black_box(order);
    1
}

/// A stable result from one complete pass through a turn's action space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enumeration {
    pub destinations: usize,
    pub actions: usize,
}

fn production_sites() -> [Pos; 3] {
    [Pos::new(2, 0), Pos::new(7, 0), Pos::new(12, 0)]
}

/// The authoritative counterpart of [`enumerate_observation`].
///
/// The production sweep is counted beside the unit sweep, exactly as the
/// observed form counts it, so the two ask the same question of one position
/// through two projections.
pub fn enumerate_state(state: &State) -> Enumeration {
    let session = Session::new(state.clone());
    let mut result = enumerate_session(&session);
    result.actions += enumerate_production(&session);
    result
}

/// One complete pass through a turn's action space, through the session.
///
/// Destinations and orders rather than destinations and action sets. The same
/// sweep, named the way the reducer is now asked about it.
///
/// This form allocates its own buffers, as a caller asking once does.
/// [`run_session_enumerate`] is the same pass with the buffers held across
/// calls, the way the API is meant to be driven and the way a search drives
/// it.
pub fn enumerate_session(session: &Session) -> Enumeration {
    let mut units = Vec::new();
    let mut orders = Vec::new();
    enumerate_session_into(session, &mut units, &mut orders)
}

/// [`enumerate_session`] against buffers the caller holds.
///
/// A destination counts when it admits at least one order, which is all the
/// session reports. Deletion belongs to no destination and does not count as
/// an action, so the action total matches what an action set would have said.
pub fn enumerate_session_into(
    session: &Session,
    units: &mut Vec<UnitIdx>,
    orders: &mut Vec<Order>,
) -> Enumeration {
    let legal = session.legal();
    let mut result = Enumeration {
        destinations: 0,
        actions: 0,
    };
    units.clear();
    legal.units(units);
    for unit in units.iter().copied() {
        orders.clear();
        legal.unit_orders(unit, orders);
        // The sweep visits one destination at a time, so a change of
        // destination is a new one.
        let mut last = None;
        for order in orders.iter() {
            if order.kind() == OrderKind::Delete {
                continue;
            }
            if last != Some(order.destination()) {
                result.destinations += 1;
                last = Some(order.destination());
            }
            result.actions += 1;
        }
    }
    result
}

pub fn enumerate_reachable(state: &State) -> usize {
    let seat = state.player_index(&state.turn.active_player);
    state
        .units
        .iter()
        .filter(|unit| Some(unit.owner) == seat && unit.action == UnitAction::Ready)
        .map(|unit| {
            query::reachable(state, unit.id)
                .expect("benchmark unit is on the board")
                .reach()
                .count()
        })
        .sum()
}

/// The affordable rows every fixture facility offers, through the session.
///
/// One statement of what a site may build serves the menu, the action space
/// and this count.
pub fn enumerate_production(session: &Session) -> usize {
    let legal = session.legal();
    let dimensions = session.state().board.dimensions();
    let mut rows = Vec::new();
    for position in production_sites() {
        let Some(cell) = dimensions.cell_index(position) else {
            continue;
        };
        legal.production_options(cell, &mut rows);
    }
    rows.iter().filter(|row| row.affordable).count()
}

pub fn observation(state: &State) -> Observation {
    observe(&AwbwVisibility, state, &state.turn.active_player).expect("observe benchmark state")
}

/// Inputs and intermediate values for one execute-to-enumeration cycle.
#[derive(Debug)]
pub struct CycleCase {
    command: CommandCase,
    post_state: State,
    observation: Observation,
    session: Session,
}

pub fn cycle_case(fog: bool) -> CycleCase {
    let state = server::state(server::DUEL, fog);
    let command = CommandCase {
        command: Command::MoveWait {
            player: state.turn.active_player.clone(),
            unit: UnitId::new(1),
            path: vec![Pos::new(0, 3), Pos::new(0, 4), Pos::new(0, 5)],
        },
        state,
    };
    let post_state = execute_state(&command);
    let observation = observation(&post_state);
    let session = Session::from_observation(&observation).expect("reify benchmark observation");
    CycleCase {
        command,
        post_state,
        observation,
        session,
    }
}

pub fn run_observe(state: &State) -> usize {
    observation(state).units.len()
}

pub fn run_reify(observation: &Observation) -> usize {
    query::reify(observation)
        .expect("reify benchmark observation")
        .units
        .len()
}

pub fn run_cycle(case: &CycleCase) -> Enumeration {
    let state = execute_state(&case.command);
    let observation = observation(&state);
    enumerate_observation(&observation)
}

pub fn enumerate_observation(observation: &Observation) -> Enumeration {
    let session = Session::from_observation(observation).expect("reify benchmark observation");
    enumerate_observed_session(&session)
}

/// The same sweep as [`enumerate_session`], with the production sites a
/// recipient can see counted beside it.
fn enumerate_observed_session(session: &Session) -> Enumeration {
    let mut result = enumerate_session(session);
    result.actions += enumerate_production(session);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_cases_are_accepted() {
        for case in [
            projected_move(),
            server_move(),
            server_capture(),
            server_produce(),
        ] {
            assert!(run_command(&case) > 0, "command produced no events");
        }
    }

    #[test]
    fn command_cases_resolve_to_orders() {
        for case in [server_move(), server_capture(), server_produce()] {
            assert_eq!(run_resolve(&ResolveCase::from(case)), 1);
        }
    }

    #[test]
    fn both_query_families_find_actions() {
        let state = server::state(server::DUEL, false);
        let authoritative = enumerate_state(&state);
        let observed = enumerate_observation(&observation(&state));
        assert!(authoritative.destinations > 0);
        assert!(authoritative.actions > 0);
        assert_eq!(observed, authoritative);
    }

    /// The fixture match plays out the same way every time.
    ///
    /// Every optimization of the forward model is gated on this: the match is
    /// deterministic in its seed, so a change that only removes work leaves
    /// the record alone, and one that changes an answer shows up here rather
    /// than as a drifting benchmark. The numbers are the record of the match,
    /// not a target — a deliberate change to the agent or the rules moves
    /// them, and re-pinning them is part of that change.
    #[test]
    fn the_amber_valley_match_is_the_same_match_every_time() {
        let record = play_amber_valley_match(amber_valley_match_case());

        assert_eq!(record.turns, 29);
        assert_eq!(record.days, 15);
        assert_eq!(record.commands, 372);
        assert_eq!(record.refusals, 0);
        assert_eq!(record.units, 32);
        let outcome = record.outcome.expect("the match ends in a victory");
        assert_eq!(
            format!("{outcome:?}"),
            r#"Victory { winners: [TeamId("player-1")], reason: HqCapture }"#
        );
    }

    #[test]
    fn complete_cycles_match_the_isolated_enumeration() {
        for fog in [false, true] {
            let case = cycle_case(fog);
            let isolated = enumerate_observed_session(&case.session);
            assert!(isolated.actions > 0);
            assert_eq!(run_cycle(&case), isolated);
        }
    }

    #[test]
    fn greedy_turn_cases_finish_one_turn() {
        for (fog, weights) in [(false, Weights::THREAT), (true, Weights::SCOUT)] {
            let mut case = greedy_turn_case(fog, weights, 1);
            let player = case.session.state().turn.active_player.clone();
            let commands = run_greedy_turn(&mut case);
            assert!(commands > 1, "the agent gives unit orders before passing");
            assert_ne!(case.session.state().turn.active_player, player);
        }
    }

    #[test]
    fn search_node_evaluates_one_leaf_and_rewinds() {
        let mut case = search_node_case();
        let player = case.session.state().turn.active_player.clone();
        let depth = case.session.depth();

        assert!(run_search_node(&mut case).is_finite());
        assert_eq!(case.session.depth(), depth);
        assert_eq!(case.session.state().turn.active_player, player);
    }
}

/// A session on a played position, with one unit order to descend through.
///
/// The order is chosen once so that a case measures the verb and not the
/// choosing. It is a unit order rather than a boundary one, because a boundary
/// order ends the turn and a search node does not.
#[derive(Debug)]
pub struct SessionCase {
    pub session: Session,
    pub order: Order,
    /// The buffers the session API is meant to be driven with, held across
    /// calls the way a search holds them.
    units: Vec<UnitIdx>,
    orders: Vec<Order>,
    masks: Vec<(CellIdx, OrderMask)>,
}

pub fn session_case(fog: bool) -> SessionCase {
    let session = Session::new(server::state(server::DUEL, fog));
    let mut orders = Vec::new();
    session.legal().orders(&mut orders);
    let order = orders
        .iter()
        .copied()
        .find(|order| order.unit().is_some() && order.kind() == OrderKind::Wait)
        .expect("the duel position offers a unit order");
    let mut case = SessionCase {
        session,
        order,
        units: Vec::new(),
        orders,
        masks: Vec::new(),
    };
    // Initialize the lazy ruleset tables and warm the buffers outside the
    // measured region. Criterion does this during warm-up. Callgrind runs the
    // benchmark only once.
    std::hint::black_box(run_session_enumerate(&mut case));
    std::hint::black_box(run_session_destinations(&mut case));
    case
}

/// One complete pass over the action space, named in orders rather than in
/// action sets. This is `enumerate_authoritative` through the session API.
pub fn run_session_enumerate(case: &mut SessionCase) -> usize {
    case.orders.clear();
    case.session.legal().orders(&mut case.orders);
    case.orders.len()
}

/// The mask half alone. Every destination of every ready unit, without the
/// target walks a mask reports as one bit.
pub fn run_session_destinations(case: &mut SessionCase) -> usize {
    let legal = case.session.legal();
    case.units.clear();
    legal.units(&mut case.units);
    case.masks.clear();
    for unit in case.units.iter().copied() {
        legal.destinations(unit, &mut case.masks);
    }
    case.masks.len()
}

/// What a search node costs at the edges: apply one order, then leave the
/// branch. `rewind` had no case until now.
///
/// The session has just swept every unit, so the reach it holds belongs to the
/// last unit and not to this order. Spelling the order therefore searches,
/// which is the pessimistic half of the pair below.
pub fn run_session_apply_rewind(case: &mut SessionCase) -> usize {
    let mark = case
        .session
        .apply(case.order, &mut RandomTape::new(&[]), &mut ())
        .expect("the benchmark order is accepted");
    case.session.rewind(mark);
    case.session.depth()
}

/// A case whose session was last asked about the order's own unit.
///
/// This is where a search that decides one unit at a time stands when it
/// applies. It has just enumerated that unit, and the route the wire command
/// needs is the search it already paid for. Driving
/// [`run_session_apply_rewind`] from here is the other half of the pair. The
/// difference between the two is what holding the reach is worth.
pub fn warm_session_case(fog: bool) -> SessionCase {
    let mut case = session_case(fog);
    let seat = case.order.unit().expect("the benchmark order names a unit");
    case.orders.clear();
    case.session.legal().unit_orders(seat, &mut case.orders);
    case
}

/// A position and evaluator held outside the measured region.
#[derive(Debug)]
pub struct EvaluationCase {
    pub session: Session,
    pub seat: ::awvm::semantic::PlayerIdx,
    pub evaluator: Evaluator,
}

/// Build an evaluator case at the server-sized match scale.
pub fn evaluation_case(players: usize, fog: bool, weights: EvalWeights) -> EvaluationCase {
    let state = server::state(players, fog);
    let seat = state
        .player_index(&state.turn.active_player)
        .expect("the active player has a seat");
    EvaluationCase {
        session: Session::new(state),
        seat,
        evaluator: Evaluator::new(weights),
    }
}

/// Read one position with scratch and session tables already allocated.
pub fn run_evaluation(case: &mut EvaluationCase) -> f64 {
    case.evaluator.value_in(&case.session, case.seat)
}

/// A threat map and the session it reads.
#[derive(Debug)]
pub struct ThreatCase {
    pub session: Session,
    pub seat: ::awvm::semantic::PlayerIdx,
    pub cell: CellIdx,
    pub map: ThreatMap,
}

/// Build one threat map for one active seat.
pub fn threat_case(players: usize, fog: bool) -> ThreatCase {
    let state = server::state(players, fog);
    let cell = state
        .board
        .dimensions()
        .cell_index(Pos::new(0, 0))
        .expect("the benchmark board has its first cell");
    let seat = state
        .player_index(&state.turn.active_player)
        .expect("the active player has a seat");
    ThreatCase {
        session: Session::new(state),
        seat,
        cell,
        map: ThreatMap::new(),
    }
}

/// Build a threat map and read one result so the build remains observable.
pub fn run_threat_build(case: &mut ThreatCase) -> f64 {
    case.map.build(&case.session, case.seat);
    case.map.immediate(case.cell, UnitKind::Tank)
}

/// One greedy decision with the observation and agent held between calls.
#[derive(Debug)]
pub struct GreedyDecisionCase {
    pub agent: GreedyAgent,
    pub view: Observation,
}

/// Advance the fixture without measuring the commands used to reach it.
pub fn state_after_end_turns(mut state: State, turns: usize) -> State {
    for _ in 0..turns {
        let player = state.turn.active_player.clone();
        state = match execute(&state, Command::EndTurn { player }, &[]) {
            Ok(ExecuteOutcome::Accepted(execution)) => execution.state,
            Ok(ExecuteOutcome::Rejected(violation)) => {
                panic!("benchmark end turn was rejected: {violation:?}")
            }
            Err(error) => panic!("benchmark end turn did not execute: {error:?}"),
        };
    }
    state
}

/// Build one retained agent and one view for a decision at a turn position.
pub fn greedy_decision_case(
    fog: bool,
    weights: Weights,
    seed: u64,
    end_turns: usize,
) -> GreedyDecisionCase {
    let state = state_after_end_turns(server::state(server::DUEL, fog), end_turns);
    let player = state.turn.active_player.clone();
    let view = observe(&AwbwVisibility, &state, &player)
        .expect("the active player observes the benchmark position");
    GreedyDecisionCase {
        agent: GreedyAgent::with_weights(seed, weights),
        view,
    }
}

/// Run one decision while retaining the agent's maps and scratch.
pub fn run_greedy_act(case: &mut GreedyDecisionCase) -> Option<awbrn_ai::agent::Play> {
    case.agent.act(&case.view, NodeBudget::FOUR)
}

/// One agent playing one authoritative turn from a fixed position.
#[derive(Debug)]
pub struct GreedyTurnCase {
    pub agent: GreedyAgent,
    pub session: Session,
    pub view: Observation,
    pub entropy: Rng,
}

/// Build one complete-turn case with all allocations outside the measurement.
pub fn greedy_turn_case(fog: bool, weights: Weights, seed: u64) -> GreedyTurnCase {
    let state = server::state(server::DUEL, fog);
    let view = observation(&state);
    GreedyTurnCase {
        agent: GreedyAgent::with_weights(Rng::mix(seed ^ 0x1), weights),
        session: Session::new(state),
        view,
        entropy: Rng::from_seed(Rng::mix(seed ^ 0x2)),
    }
}

/// Play until the active player changes and return accepted commands.
pub fn run_greedy_turn(case: &mut GreedyTurnCase) -> usize {
    let starting_player = case.session.state().turn.active_player.clone();
    let mut commands = 0;
    while case.session.state().turn.active_player == starting_player
        && matches!(case.session.state().match_state, Match::Active { .. })
    {
        observe_into(
            &AwbwVisibility,
            case.session.state(),
            &starting_player,
            &mut case.view,
        )
        .expect("the active player observes the turn position");
        let command = case
            .agent
            .act(&case.view, NodeBudget::FOUR)
            .and_then(|play| play.command(&case.session))
            .unwrap_or_else(|| Command::EndTurn {
                player: starting_player.clone(),
            });
        case.session
            .apply_command(command, &mut case.entropy, &mut ())
            .expect("the greedy benchmark command is accepted");
        commands += 1;
    }
    commands
}

/// One evaluated search leaf with all reusable search state retained.
#[derive(Debug)]
pub struct SearchNodeCase {
    pub session: Session,
    friendly_plan: Vec<Order>,
    opponent: GreedyAgent,
    view: Observation,
    entropy: Rng,
    evaluator: Evaluator,
    friendly_seat: ::awvm::semantic::PlayerIdx,
}

/// Build one candidate turn plan and return to its root position.
pub fn search_node_case() -> SearchNodeCase {
    let state = server::state(server::DUEL, false);
    let friendly_seat = state
        .player_index(&state.turn.active_player)
        .expect("the active player has a seat");
    let mut session = Session::new(state);
    let mut view = observation(session.state());
    let mut friendly = GreedyAgent::with_weights(Rng::mix(1), Weights::THREAT);
    let mut entropy = Rng::from_seed(Rng::mix(2));
    let replay_entropy = entropy.clone();
    let starting_player = session.state().turn.active_player.clone();
    let mut friendly_plan = Vec::new();
    let mut root = None;

    while session.state().turn.active_player == starting_player
        && matches!(session.state().match_state, Match::Active { .. })
    {
        observe_into(
            &AwbwVisibility,
            session.state(),
            &starting_player,
            &mut view,
        )
        .expect("the active player observes the candidate turn");
        let command = friendly
            .act(&view, NodeBudget::ONE)
            .and_then(|play| play.command(&session))
            .unwrap_or_else(|| Command::EndTurn {
                player: starting_player.clone(),
            });
        let order = session
            .resolve(&command)
            .expect("the candidate command resolves to an order");
        let mark = session
            .apply(order, &mut entropy, &mut ())
            .expect("the candidate order is accepted");
        root.get_or_insert(mark);
        friendly_plan.push(order);
    }
    session.rewind(root.expect("a candidate turn contains an end-turn order"));

    SearchNodeCase {
        view: observation(session.state()),
        session,
        friendly_plan,
        opponent: GreedyAgent::with_weights(Rng::mix(3), Weights::THREAT),
        entropy: replay_entropy,
        evaluator: Evaluator::new(EvalWeights::STANDARD),
        friendly_seat,
    }
}

/// Apply one friendly plan, play one greedy reply, evaluate, and rewind.
///
/// This entire operation is one search node. The orders inside either turn do
/// not count as nodes.
pub fn run_search_node(case: &mut SearchNodeCase) -> f64 {
    let mut root = None;
    for order in case.friendly_plan.iter().copied() {
        let mark = case
            .session
            .apply(order, &mut case.entropy, &mut ())
            .expect("the candidate order is accepted");
        root.get_or_insert(mark);
    }

    let opponent_player = case.session.state().turn.active_player.clone();
    while case.session.state().turn.active_player == opponent_player
        && matches!(case.session.state().match_state, Match::Active { .. })
    {
        observe_into(
            &AwbwVisibility,
            case.session.state(),
            &opponent_player,
            &mut case.view,
        )
        .expect("the opponent observes the reply position");
        let command = case
            .opponent
            .act(&case.view, NodeBudget::ONE)
            .and_then(|play| play.command(&case.session))
            .unwrap_or_else(|| Command::EndTurn {
                player: opponent_player.clone(),
            });
        case.session
            .apply_command(command, &mut case.entropy, &mut ())
            .expect("the opponent command is accepted");
    }

    let value = case.evaluator.value_in(&case.session, case.friendly_seat);
    case.session
        .rewind(root.expect("the candidate plan contains an order"));
    value
}

fn without_position(mut weights: EvalWeights) -> EvalWeights {
    weights.exposure = 0.0;
    weights.contest = 0.0;
    weights.front = 0.0;
    weights
}

fn exposure_only(mut weights: EvalWeights) -> EvalWeights {
    weights.exposure = 1.0;
    weights.contest = 0.0;
    weights.front = 0.0;
    weights
}

fn contest_front_only(mut weights: EvalWeights) -> EvalWeights {
    weights.exposure = 0.0;
    weights
}

const AMBER_VALLEY_MATCH_SEED: u64 = 1;

#[derive(Debug)]
pub struct AmberValleyMatchCase {
    state: State,
    session: Session,
}

pub fn amber_valley_match_case() -> AmberValleyMatchCase {
    let game = Rng::mix(AMBER_VALLEY_MATCH_SEED);
    let state = amber_valley(false, game);
    AmberValleyMatchCase {
        session: Session::new(state.clone()),
        state,
    }
}

/// Plays the fixture match and answers the whole record of it.
pub fn play_amber_valley_match(case: AmberValleyMatchCase) -> Record {
    let AmberValleyMatchCase { state, mut session } = case;
    let game = Rng::mix(AMBER_VALLEY_MATCH_SEED);
    let mut entropy = Rng::from_seed(Rng::mix(game ^ 0x1));
    let mut threat = GreedyAgent::with_weights(Rng::mix(game ^ 0x2), Weights::THREAT);
    let mut deny = GreedyAgent::from_seed(Rng::mix(game ^ 0x3));
    let mut agents: [&mut dyn Agent; 2] = [&mut threat, &mut deny];

    play(
        state,
        &mut session,
        &mut agents,
        &mut entropy,
        Limits::DEFAULT,
    )
}

pub fn run_amber_valley_match(case: AmberValleyMatchCase) -> u64 {
    u64::from(play_amber_valley_match(case).turns)
}

pub mod criterion_benches {
    use super::*;
    use criterion::{BatchSize, BenchmarkId, Criterion};
    use std::hint::black_box;

    fn commands(c: &mut Criterion) {
        let mut group = c.benchmark_group("ai-execute");
        for (name, case) in [
            ("move-projected-20x20-30units", projected_move()),
            ("move-server-20x20-40units", server_move()),
            ("produce-server-20x20-40units", server_produce()),
        ] {
            group.bench_function(BenchmarkId::from_parameter(name), |b| {
                b.iter(|| black_box(run_command(&case)));
            });
        }
        group.finish();
    }

    fn enumerate(c: &mut Criterion) {
        let state = server::state(server::DUEL, false);
        let view = observation(&state);
        let mut group = c.benchmark_group("ai-enumerate-turn");
        group.bench_function("state-server-20x20-40units", |b| {
            b.iter(|| black_box(enumerate_state(&state)));
        });
        group.bench_function("observed-server-20x20-40units", |b| {
            b.iter(|| black_box(enumerate_observation(&view)));
        });
        group.finish();

        let mut group = c.benchmark_group("ai-reachable-turn");
        group.bench_function("state-server-20x20-40units", |b| {
            b.iter(|| black_box(enumerate_reachable(&state)));
        });
        group.finish();
    }

    fn resolve(c: &mut Criterion) {
        let wait = ResolveCase::from(server_move());
        let capture = ResolveCase::from(server_capture());
        let produce = ResolveCase::from(server_produce());
        let mut group = c.benchmark_group("ai-resolve");
        for (name, case) in [
            ("wait-server-20x20-40units", &wait),
            ("capture-server-20x20-40units", &capture),
            ("produce-server-20x20-40units", &produce),
        ] {
            group.bench_function(name, |b| {
                b.iter(|| black_box(run_resolve(case)));
            });
        }
        group.finish();

        let session = Session::new(server::state(server::DUEL, false));
        let mut group = c.benchmark_group("ai-enumerate-production");
        group.bench_function("state-server-20x20-40units", |b| {
            b.iter(|| black_box(enumerate_production(&session)));
        });
        group.finish();
    }

    fn cycle(c: &mut Criterion) {
        let cases = [("fog-off", cycle_case(false)), ("fog-on", cycle_case(true))];

        let mut group = c.benchmark_group("ai-cycle-execute");
        for (name, case) in &cases {
            group.bench_function(*name, |b| {
                b.iter(|| black_box(run_command(&case.command)));
            });
        }
        group.finish();

        let mut group = c.benchmark_group("ai-cycle-observe");
        for (name, case) in &cases {
            group.bench_function(*name, |b| {
                b.iter(|| black_box(run_observe(&case.post_state)));
            });
        }
        group.finish();

        let mut group = c.benchmark_group("ai-cycle-reify");
        for (name, case) in &cases {
            group.bench_function(*name, |b| {
                b.iter(|| black_box(run_reify(&case.observation)));
            });
        }
        group.finish();

        let mut group = c.benchmark_group("ai-cycle-enumerate");
        for (name, case) in &cases {
            group.bench_function(*name, |b| {
                b.iter(|| black_box(enumerate_observed_session(&case.session)));
            });
        }
        group.finish();

        let mut group = c.benchmark_group("ai-cycle-complete");
        for (name, case) in &cases {
            group.bench_function(*name, |b| {
                b.iter(|| black_box(run_cycle(case)));
            });
        }
        group.finish();
    }

    fn session(c: &mut Criterion) {
        let mut cases = [
            ("fog-off", session_case(false)),
            ("fog-on", session_case(true)),
        ];

        let mut group = c.benchmark_group("ai-session-enumerate");
        for (name, case) in &mut cases {
            group.bench_function(*name, |b| {
                b.iter(|| black_box(run_session_enumerate(case)));
            });
        }
        group.finish();

        let mut group = c.benchmark_group("ai-session-destinations");
        for (name, case) in &mut cases {
            group.bench_function(*name, |b| {
                b.iter(|| black_box(run_session_destinations(case)));
            });
        }
        group.finish();

        let mut group = c.benchmark_group("ai-session-apply-rewind");
        for (name, _) in &cases {
            let fog = *name == "fog-on";
            group.bench_function(*name, |b| {
                b.iter_batched_ref(
                    || session_case(fog),
                    |case| black_box(run_session_apply_rewind(case)),
                    BatchSize::LargeInput,
                );
            });
        }
        group.finish();

        let mut group = c.benchmark_group("ai-session-apply-rewind-warm");
        for (name, _) in &cases {
            let fog = *name == "fog-on";
            group.bench_function(*name, |b| {
                b.iter_batched_ref(
                    || warm_session_case(fog),
                    |case| black_box(run_session_apply_rewind(case)),
                    BatchSize::LargeInput,
                );
            });
        }
        group.finish();
    }

    fn evaluation(c: &mut Criterion) {
        let standard = EvalWeights::STANDARD;
        let fog = EvalWeights::FOG;
        let cases = [
            (
                "standard-baseline-duel",
                evaluation_case(server::DUEL, false, without_position(standard)),
            ),
            (
                "standard-exposure-duel",
                evaluation_case(server::DUEL, false, exposure_only(standard)),
            ),
            (
                "standard-exposure-six-player",
                evaluation_case(server::SIX_PLAYER, false, exposure_only(standard)),
            ),
            (
                "standard-contest-front-duel",
                evaluation_case(server::DUEL, false, contest_front_only(standard)),
            ),
            (
                "standard-all-duel",
                evaluation_case(server::DUEL, false, standard),
            ),
            (
                "fog-baseline-duel",
                evaluation_case(server::DUEL, true, without_position(fog)),
            ),
            (
                "fog-exposure-duel",
                evaluation_case(server::DUEL, true, exposure_only(fog)),
            ),
            (
                "fog-exposure-six-player",
                evaluation_case(server::SIX_PLAYER, true, exposure_only(fog)),
            ),
            (
                "fog-contest-front-duel",
                evaluation_case(server::DUEL, true, contest_front_only(fog)),
            ),
            ("fog-all-duel", evaluation_case(server::DUEL, true, fog)),
        ];
        let mut group = c.benchmark_group("ai-evaluation");
        for (name, mut case) in cases {
            group.bench_function(name, |b| {
                b.iter(|| black_box(run_evaluation(&mut case)));
            });
        }
        group.finish();
    }

    fn threat_map(c: &mut Criterion) {
        let cases = [
            ("duel-fog-off", threat_case(server::DUEL, false)),
            ("six-player-fog-off", threat_case(server::SIX_PLAYER, false)),
            ("duel-fog-on", threat_case(server::DUEL, true)),
            ("six-player-fog-on", threat_case(server::SIX_PLAYER, true)),
        ];
        let mut group = c.benchmark_group("ai-evaluation-threat-map");
        for (name, mut case) in cases {
            group.bench_function(name, |b| {
                b.iter(|| black_box(run_threat_build(&mut case)));
            });
        }
        group.finish();
    }

    fn greedy_act(c: &mut Criterion) {
        let positions = [("early", 0), ("middle", 14), ("late", 28)];
        let mut cases = Vec::new();
        for (position, end_turns) in positions {
            cases.push((
                format!("standard-without-threat-{position}"),
                greedy_decision_case(false, Weights::TIER1, 1, end_turns),
            ));
            cases.push((
                format!("standard-with-threat-{position}"),
                greedy_decision_case(false, Weights::THREAT, 1, end_turns),
            ));
            cases.push((
                format!("fog-scout-{position}"),
                greedy_decision_case(true, Weights::SCOUT, 1, end_turns),
            ));
        }

        let mut group = c.benchmark_group("ai-greedy-act");
        for (name, mut case) in cases {
            group.bench_function(name, |b| {
                b.iter(|| black_box(run_greedy_act(&mut case)));
            });
        }
        group.finish();
    }

    fn greedy_turn(c: &mut Criterion) {
        let mut group = c.benchmark_group("ai-greedy-turn");
        for (name, fog, weights) in [
            ("standard-threat", false, Weights::THREAT),
            ("fog-scout", true, Weights::SCOUT),
        ] {
            group.bench_function(name, |b| {
                b.iter_batched(
                    || greedy_turn_case(fog, weights, 1),
                    |mut case| black_box(run_greedy_turn(&mut case)),
                    BatchSize::SmallInput,
                );
            });
        }
        group.finish();
    }

    fn search_node(c: &mut Criterion) {
        let mut group = c.benchmark_group("ai-search-node");
        group.bench_function("greedy-reply-standard", |b| {
            b.iter_batched_ref(
                search_node_case,
                |case| black_box(run_search_node(case)),
                BatchSize::SmallInput,
            );
        });
        group.finish();
    }

    fn matches(c: &mut Criterion) {
        let mut group = c.benchmark_group("ai-match");
        group.bench_function("amber-valley-threat-vs-deny", |b| {
            b.iter_batched(
                amber_valley_match_case,
                |case| black_box(run_amber_valley_match(case)),
                BatchSize::SmallInput,
            );
        });
        group.finish();
    }

    criterion::criterion_group!(
        ai_benches,
        commands,
        enumerate,
        resolve,
        cycle,
        session,
        evaluation,
        threat_map,
        greedy_act,
        greedy_turn,
        search_node,
        matches
    );
}

#[cfg(not(target_family = "wasm"))]
pub mod gungraun_benches {
    use super::*;
    use gungraun::{library_benchmark, library_benchmark_group};

    #[derive(Clone, Copy)]
    enum CommandFixture {
        ProjectedMove,
        ServerMove,
        ServerCapture,
        ServerProduce,
    }

    fn command_case(fixture: CommandFixture) -> CommandCase {
        match fixture {
            CommandFixture::ProjectedMove => projected_move(),
            CommandFixture::ServerMove => server_move(),
            CommandFixture::ServerCapture => server_capture(),
            CommandFixture::ServerProduce => server_produce(),
        }
    }

    #[library_benchmark(setup = command_case)]
    #[bench::move_projected(CommandFixture::ProjectedMove)]
    #[bench::move_server(CommandFixture::ServerMove)]
    #[bench::produce_server(CommandFixture::ServerProduce)]
    fn command(case: CommandCase) -> usize {
        let case = std::mem::ManuallyDrop::new(case);
        run_command(&case)
    }

    fn resolve_case(fixture: CommandFixture) -> ResolveCase {
        ResolveCase::from(command_case(fixture))
    }

    #[library_benchmark(setup = resolve_case)]
    #[bench::wait(CommandFixture::ServerMove)]
    #[bench::capture(CommandFixture::ServerCapture)]
    #[bench::produce(CommandFixture::ServerProduce)]
    fn resolve(case: ResolveCase) -> usize {
        // Gungraun counts drops in the benchmark function. Keep the fixture
        // alive so this case measures resolution instead of State teardown.
        let case = std::mem::ManuallyDrop::new(case);
        run_resolve(&case)
    }

    fn authoritative_state() -> State {
        server::state(server::DUEL, false)
    }

    /// A session on the same position, so the case measures the sweep and not
    /// the state clone that opening one costs.
    fn authoritative_session() -> Session {
        Session::new(authoritative_state())
    }

    #[library_benchmark(setup = authoritative_session)]
    #[bench::state()]
    fn enumerate_authoritative(session: Session) -> Enumeration {
        let session = std::mem::ManuallyDrop::new(session);
        enumerate_session(&session)
    }

    fn observed_state() -> Observation {
        observation(&server::state(server::DUEL, false))
    }

    fn profiled_cycle_case(fog: bool) -> CycleCase {
        let case = cycle_case(fog);
        // Initialize lazy ruleset tables outside the measured region. Criterion
        // does this during warm-up; Callgrind runs the benchmark only once.
        std::hint::black_box(enumerate_observed_session(&case.session));
        case
    }

    #[library_benchmark(setup = observed_state)]
    #[bench::observed()]
    fn enumerate_observed(observation: Observation) -> Enumeration {
        enumerate_observation(&observation)
    }

    #[library_benchmark(setup = authoritative_session)]
    #[bench::production()]
    fn enumerate_authoritative_production(session: Session) -> usize {
        let session = std::mem::ManuallyDrop::new(session);
        enumerate_production(&session)
    }

    #[library_benchmark(setup = cycle_case)]
    #[bench::fog_off(false)]
    #[bench::fog_on(true)]
    fn cycle_execute(case: CycleCase) -> usize {
        let case = std::mem::ManuallyDrop::new(case);
        run_command(&case.command)
    }

    #[library_benchmark(setup = cycle_case)]
    #[bench::fog_off(false)]
    #[bench::fog_on(true)]
    fn cycle_observe(case: CycleCase) -> usize {
        let case = std::mem::ManuallyDrop::new(case);
        run_observe(&case.post_state)
    }

    #[library_benchmark(setup = cycle_case)]
    #[bench::fog_off(false)]
    #[bench::fog_on(true)]
    fn cycle_reify(case: CycleCase) -> usize {
        let case = std::mem::ManuallyDrop::new(case);
        run_reify(&case.observation)
    }

    #[library_benchmark(setup = profiled_cycle_case)]
    #[bench::fog_off(false)]
    #[bench::fog_on(true)]
    fn cycle_enumerate(case: CycleCase) -> Enumeration {
        let case = std::mem::ManuallyDrop::new(case);
        enumerate_observed_session(&case.session)
    }

    #[library_benchmark(setup = session_case)]
    #[bench::fog_off(false)]
    #[bench::fog_on(true)]
    fn session_enumerate(case: SessionCase) -> usize {
        let mut case = std::mem::ManuallyDrop::new(case);
        run_session_enumerate(&mut case)
    }

    #[library_benchmark(setup = session_case)]
    #[bench::fog_off(false)]
    #[bench::fog_on(true)]
    fn session_destinations(case: SessionCase) -> usize {
        let mut case = std::mem::ManuallyDrop::new(case);
        run_session_destinations(&mut case)
    }

    #[library_benchmark(setup = session_case)]
    #[bench::fog_off(false)]
    #[bench::fog_on(true)]
    fn session_apply_rewind(case: SessionCase) -> usize {
        let mut case = std::mem::ManuallyDrop::new(case);
        run_session_apply_rewind(&mut case)
    }

    #[library_benchmark(setup = warm_session_case)]
    #[bench::fog_off(false)]
    #[bench::fog_on(true)]
    fn session_apply_rewind_warm(case: SessionCase) -> usize {
        let mut case = std::mem::ManuallyDrop::new(case);
        run_session_apply_rewind(&mut case)
    }

    #[library_benchmark(setup = cycle_case)]
    #[bench::fog_off(false)]
    #[bench::fog_on(true)]
    fn complete_cycle(case: CycleCase) -> Enumeration {
        let case = std::mem::ManuallyDrop::new(case);
        run_cycle(&case)
    }

    #[library_benchmark(setup = search_node_case)]
    #[bench::greedy_reply_standard()]
    fn search_node(case: SearchNodeCase) -> f64 {
        let mut case = std::mem::ManuallyDrop::new(case);
        run_search_node(&mut case)
    }

    fn match_case() -> AmberValleyMatchCase {
        amber_valley_match_case()
    }

    #[library_benchmark(setup = match_case)]
    #[bench::standard()]
    fn amber_valley_match(case: AmberValleyMatchCase) -> u64 {
        run_amber_valley_match(case)
    }

    library_benchmark_group!(
        name = ai_benches,
        benchmarks = [
            command,
            resolve,
            enumerate_authoritative,
            enumerate_observed,
            enumerate_authoritative_production,
            cycle_execute,
            cycle_observe,
            cycle_reify,
            cycle_enumerate,
            session_enumerate,
            session_destinations,
            session_apply_rewind,
            session_apply_rewind_warm,
            complete_cycle,
            search_node,
            amber_valley_match,
        ]
    );
}
