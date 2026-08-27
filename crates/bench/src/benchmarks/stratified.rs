//! Benchmarks for stratified turn generation.

use awbrn_ai::agents::{
    MissionBook, StratifiedScripts, generate_stratified_candidates, generate_stratified_plan,
};
use awvm::semantic::{AwbwVisibility, Observation, observe};
use std::hint::black_box;

use super::server;

#[derive(Clone)]
struct StratifiedTurnCase {
    view: Observation,
    seed: u64,
}

fn stratified_turn_case() -> StratifiedTurnCase {
    let state = server::state(server::DUEL, false);
    let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
        .expect("the benchmark root observes");
    StratifiedTurnCase { view, seed: 1 }
}

fn run_stratified_turn(case: &StratifiedTurnCase) -> usize {
    let mut missions = MissionBook::new();
    let plan = generate_stratified_plan(
        &case.view,
        case.seed,
        &mut missions,
        StratifiedScripts::default(),
    )
    .expect("the benchmark root generates a stratified turn");
    black_box(plan).len()
}

fn run_stratified_candidates(case: &StratifiedTurnCase) -> usize {
    let mut missions = MissionBook::new();
    let assignment = StratifiedScripts::default();
    let candidates =
        generate_stratified_candidates(&case.view, case.seed, &mut missions, assignment)
            .expect("the benchmark root generates stratified candidates");
    let generated = candidates
        .into_iter()
        .map(|candidate| black_box(candidate.plays).len())
        .sum::<usize>();
    black_box(generated)
}

pub mod criterion_benches {
    use super::*;
    use criterion::Criterion;

    fn stratified_turn(c: &mut Criterion) {
        let case = stratified_turn_case();
        let mut group = c.benchmark_group("ai-stratified-turn");
        group.bench_function("default-server-20x20-40units", |b| {
            b.iter(|| black_box(run_stratified_turn(&case)));
        });
        group.bench_function("candidate-sweep-server-20x20-40units", |b| {
            b.iter(|| black_box(run_stratified_candidates(&case)));
        });
        group.finish();
    }

    criterion::criterion_group!(stratified_benches, stratified_turn);
}

#[cfg(not(target_family = "wasm"))]
pub mod gungraun_benches {
    use super::*;
    use gungraun::{library_benchmark, library_benchmark_group};

    #[library_benchmark(setup = stratified_turn_case)]
    #[bench::default_server_20x20_40units()]
    fn stratified_turn(case: StratifiedTurnCase) -> usize {
        let case = std::mem::ManuallyDrop::new(case);
        run_stratified_turn(&case)
    }

    #[library_benchmark(setup = stratified_turn_case)]
    #[bench::candidate_sweep_server_20x20_40units()]
    fn stratified_candidates(case: StratifiedTurnCase) -> usize {
        let case = std::mem::ManuallyDrop::new(case);
        run_stratified_candidates(&case)
    }

    library_benchmark_group!(
        name = stratified_benches,
        benchmarks = [stratified_turn, stratified_candidates,]
    );
}
