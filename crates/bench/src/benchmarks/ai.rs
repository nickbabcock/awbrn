//! AWVM command and turn-enumeration costs at the scale of a real match.
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

use ::awvm::query;
use ::awvm::ruleset::UnitKind;
use ::awvm::semantic::{
    AwbwVisibility, Location, Observation, ObservedUnitRef, Pos, State, UnitAction, UnitId, observe,
};
use ::awvm::transition::{
    ActiveTurn, Command, ExecuteOutcome, PreparedMovement, execute, prepare_command,
    prepare_movement,
};

use super::{awvm, server};

const WIDTH: u8 = 20;
const HEIGHT: u8 = 20;
const UNITS: usize = 30;

/// One accepted command and its input state.
pub struct CommandCase {
    pub state: State,
    pub command: Command,
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

pub fn run_prepare(case: &CommandCase) -> usize {
    match prepare_command(&case.state, case.command.clone()) {
        Ok(Ok(prepared)) => {
            std::hint::black_box(prepared);
            1
        }
        Ok(Err(violation)) => panic!("benchmark command was rejected: {violation:?}"),
        Err(error) => panic!("benchmark command was not prepared: {error:?}"),
    }
}

pub fn prepared_movement(case: &CommandCase) -> PreparedMovement<'_> {
    let (player, unit, path) = match &case.command {
        Command::MoveWait { player, unit, path } | Command::MoveCapture { player, unit, path } => {
            (player, *unit, path.clone())
        }
        command => panic!("benchmark command has no prepared movement: {command:?}"),
    };
    match prepare_movement(&case.state, player, unit, path) {
        Ok(Ok(movement)) => movement,
        Ok(Err(violation)) => panic!("benchmark movement was rejected: {violation:?}"),
        Err(error) => panic!("benchmark movement was not prepared: {error:?}"),
    }
}

pub fn run_prepare_movement(case: &CommandCase) -> usize {
    std::hint::black_box(prepared_movement(case));
    1
}

/// A stable result from one complete pass through a turn's action space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enumeration {
    pub destinations: usize,
    pub actions: usize,
}

fn count_actions(actions: &query::ActionSet) -> usize {
    [
        actions.wait,
        actions.capture,
        actions.join,
        actions.load,
        actions.supply,
        actions.hide,
        actions.reveal,
        actions.explode,
    ]
    .into_iter()
    .filter(|available| *available)
    .count()
        + actions.attack.len()
        + actions.repair.len()
        + actions.launch.len()
}

fn count_observed_actions(actions: &query::ObservedActionSet) -> usize {
    [
        actions.wait,
        actions.capture,
        actions.join,
        actions.load,
        actions.supply,
        actions.hide,
        actions.reveal,
        actions.explode,
    ]
    .into_iter()
    .filter(|available| *available)
    .count()
        + actions.attack.len()
        + actions.repair.len()
        + actions.launch.len()
}

fn production_sites() -> [Pos; 3] {
    [Pos::new(2, 0), Pos::new(7, 0), Pos::new(12, 0)]
}

pub fn enumerate_state(state: &State) -> Enumeration {
    let player = &state.turn.active_player;
    let seat = state.player_index(player);
    let mut result = Enumeration {
        destinations: 0,
        actions: 0,
    };
    let turn = ActiveTurn::open(state, player)
        .expect("benchmark state can open a turn")
        .unwrap_or_else(|violation| panic!("benchmark turn was rejected: {violation:?}"));
    for unit in state
        .units
        .iter()
        .filter(|unit| Some(unit.owner) == seat && unit.action == UnitAction::Ready)
    {
        let field = turn
            .move_field(unit.id)
            .expect("benchmark unit can be checked")
            .expect("benchmark unit has a prepared movement field");
        for (destination, _) in field.reach() {
            result.destinations += 1;
            result.actions += count_actions(
                &field
                    .actions_at(destination)
                    .expect("benchmark destination can be queried"),
            );
        }
    }
    result.actions += production_sites()
        .into_iter()
        .map(|position| query::production_options(state, player, position).len())
        .sum::<usize>();
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

pub fn enumerate_production(state: &State) -> usize {
    let player = &state.turn.active_player;
    production_sites()
        .into_iter()
        .map(|position| query::production_options(state, player, position).len())
        .sum()
}

pub fn observation(state: &State) -> Observation {
    observe(&AwbwVisibility, state, &state.turn.active_player).expect("observe benchmark state")
}

/// Inputs and intermediate values for one execute-to-enumeration cycle.
pub struct CycleCase {
    command: CommandCase,
    post_state: State,
    observation: Observation,
    query: query::ObservedQuery,
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
    let query = query::ObservedQuery::new(&observation).expect("reify benchmark observation");
    CycleCase {
        command,
        post_state,
        observation,
        query,
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
    let query = query::ObservedQuery::new(observation).expect("reify benchmark observation");
    enumerate_observed_query(observation, &query)
}

fn enumerate_observed_query(
    observation: &Observation,
    query: &query::ObservedQuery,
) -> Enumeration {
    let mut result = Enumeration {
        destinations: 0,
        actions: 0,
    };
    let turn = query
        .turn()
        .expect("observed benchmark state can open a turn")
        .expect("observed benchmark recipient may command");
    for unit in observation.units.iter().filter_map(|unit| {
        if unit.owner == observation.recipient && unit.action == UnitAction::Ready {
            match unit.reference {
                ObservedUnitRef::Friendly { unit } => Some(unit),
                ObservedUnitRef::Enemy { .. } => None,
            }
        } else {
            None
        }
    }) {
        let field = turn
            .move_field(unit)
            .expect("observed benchmark unit can be queried")
            .expect("observed benchmark unit can act");
        for (destination, _) in field.reach() {
            result.destinations += 1;
            result.actions += count_observed_actions(
                &field
                    .observed_actions_at(destination)
                    .expect("observed benchmark destination can be queried"),
            );
        }
    }
    result.actions += production_sites()
        .into_iter()
        .map(|position| {
            query::observed_production_options(observation, position)
                .into_iter()
                .filter(|option| option.affordable)
                .count()
        })
        .sum::<usize>();
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
    fn move_wait_case_can_be_prepared() {
        assert_eq!(run_prepare(&server_move()), 1);
        assert_eq!(run_prepare_movement(&server_move()), 1);
    }

    #[test]
    fn move_capture_case_can_be_prepared() {
        assert_eq!(run_prepare(&server_capture()), 1);
        assert_eq!(run_prepare_movement(&server_capture()), 1);
    }

    #[test]
    fn production_case_can_be_prepared() {
        assert_eq!(run_prepare(&server_produce()), 1);
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

    #[test]
    fn complete_cycles_match_the_isolated_enumeration() {
        for fog in [false, true] {
            let case = cycle_case(fog);
            let isolated = enumerate_observed_query(&case.observation, &case.query);
            assert!(isolated.actions > 0);
            assert_eq!(run_cycle(&case), isolated);
        }
    }
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

    fn prepare(c: &mut Criterion) {
        let wait = server_move();
        let capture = server_capture();
        let produce = server_produce();
        let mut group = c.benchmark_group("ai-prepare");
        group.bench_function("movement-wait-server-20x20-40units", |b| {
            b.iter(|| black_box(run_prepare_movement(&wait)));
        });
        group.bench_function("command-wait-server-20x20-40units", |b| {
            b.iter(|| black_box(run_prepare(&wait)));
        });
        group.bench_function("movement-capture-server-20x20-40units", |b| {
            b.iter(|| black_box(run_prepare_movement(&capture)));
        });
        group.bench_function("command-capture-server-20x20-40units", |b| {
            b.iter(|| black_box(run_prepare(&capture)));
        });
        group.bench_function("command-produce-server-20x20-40units", |b| {
            b.iter(|| black_box(run_prepare(&produce)));
        });
        group.finish();

        let state = server::state(server::DUEL, false);
        let mut group = c.benchmark_group("ai-enumerate-production");
        group.bench_function("state-server-20x20-40units", |b| {
            b.iter(|| black_box(enumerate_production(&state)));
        });
        group.finish();

        let wait_movement = prepared_movement(&wait);
        let capture_movement = prepared_movement(&capture);
        let mut group = c.benchmark_group("ai-prepare-action");
        group.bench_function("wait-from-movement", |b| {
            b.iter_batched(
                || wait_movement.clone(),
                |movement| black_box(movement.prepare_wait().expect("prepare wait")),
                BatchSize::SmallInput,
            );
        });
        group.bench_function("capture-from-movement", |b| {
            b.iter_batched(
                || capture_movement.clone(),
                |movement| black_box(movement.prepare_capture().expect("prepare capture")),
                BatchSize::SmallInput,
            );
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
                b.iter(|| black_box(enumerate_observed_query(&case.observation, &case.query)));
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

    criterion::criterion_group!(ai_benches, commands, enumerate, prepare, cycle);
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

    #[library_benchmark(setup = command_case)]
    #[bench::wait(CommandFixture::ServerMove)]
    #[bench::capture(CommandFixture::ServerCapture)]
    #[bench::produce(CommandFixture::ServerProduce)]
    fn prepare(case: CommandCase) -> usize {
        // Gungraun counts drops in the benchmark function. Keep the fixture
        // alive so this case measures preparation instead of State teardown.
        let case = std::mem::ManuallyDrop::new(case);
        run_prepare(&case)
    }

    #[library_benchmark(setup = command_case)]
    #[bench::wait(CommandFixture::ServerMove)]
    #[bench::capture(CommandFixture::ServerCapture)]
    fn prepare_movement(case: CommandCase) -> usize {
        let case = std::mem::ManuallyDrop::new(case);
        run_prepare_movement(&case)
    }

    fn authoritative_state() -> State {
        server::state(server::DUEL, false)
    }

    #[library_benchmark(setup = authoritative_state)]
    #[bench::state()]
    fn enumerate_authoritative(state: State) -> Enumeration {
        enumerate_state(&state)
    }

    fn observed_state() -> Observation {
        observation(&server::state(server::DUEL, false))
    }

    fn profiled_cycle_case(fog: bool) -> CycleCase {
        let case = cycle_case(fog);
        // Initialize lazy ruleset tables outside the measured region. Criterion
        // does this during warm-up; Callgrind runs the benchmark only once.
        std::hint::black_box(enumerate_observed_query(&case.observation, &case.query));
        case
    }

    #[library_benchmark(setup = observed_state)]
    #[bench::observed()]
    fn enumerate_observed(observation: Observation) -> Enumeration {
        enumerate_observation(&observation)
    }

    #[library_benchmark(setup = authoritative_state)]
    #[bench::production()]
    fn enumerate_authoritative_production(state: State) -> usize {
        enumerate_production(&state)
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
        enumerate_observed_query(&case.observation, &case.query)
    }

    #[library_benchmark(setup = cycle_case)]
    #[bench::fog_off(false)]
    #[bench::fog_on(true)]
    fn complete_cycle(case: CycleCase) -> Enumeration {
        let case = std::mem::ManuallyDrop::new(case);
        run_cycle(&case)
    }

    library_benchmark_group!(
        name = ai_benches,
        benchmarks = [
            command,
            prepare,
            prepare_movement,
            enumerate_authoritative,
            enumerate_observed,
            enumerate_authoritative_production,
            cycle_execute,
            cycle_observe,
            cycle_reify,
            cycle_enumerate,
            complete_cycle,
        ]
    );
}
