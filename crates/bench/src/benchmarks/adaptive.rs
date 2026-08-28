//! Benchmarks for adaptive horizon selection.

use awbrn_ai::adaptive::{SelectionPolicy, SelectionResult, select};
use awbrn_ai::agent::Play;
use awbrn_ai::agents::{
    MissionBook, StratifiedScripts, Weights, generate_plan, generate_stratified_candidates,
};
use awbrn_ai::board::arena;
use awvm::semantic::{AwbwVisibility, State, observe};
use std::hint::black_box;

const DAYS: u32 = 35;
const MAX_FIXTURE_SEED: u64 = 128;

#[derive(Clone)]
struct SelectionCase {
    state: State,
    candidates: Vec<Vec<Play>>,
    seed: u64,
    days: u32,
    policy: SelectionPolicy,
}

#[derive(Clone, Copy)]
enum Fixture {
    Agreement,
    Extension,
    StandardFour,
    AlwaysEight,
}

impl Fixture {
    const fn policy(self) -> SelectionPolicy {
        match self {
            Self::Agreement | Self::Extension => SelectionPolicy::Adaptive,
            Self::StandardFour => SelectionPolicy::StandardFour,
            Self::AlwaysEight => SelectionPolicy::AlwaysEight,
        }
    }

    const fn needs_disagreement(self) -> Option<bool> {
        match self {
            Self::Agreement => Some(false),
            Self::Extension => Some(true),
            Self::StandardFour | Self::AlwaysEight => None,
        }
    }
}

fn selection_case(fixture: Fixture) -> SelectionCase {
    for seed in 1..=MAX_FIXTURE_SEED {
        let mut case = build_case(seed);
        let Some(expected_disagreement) = fixture.needs_disagreement() else {
            case.policy = fixture.policy();
            return case;
        };
        let result = select(
            &case.state,
            &case.candidates,
            case.seed,
            case.days,
            SelectionPolicy::Adaptive,
        )
        .expect("the adaptive fixture selects a candidate");
        if result.disagreement == expected_disagreement {
            case.policy = fixture.policy();
            return case;
        }
    }
    panic!("no adaptive fixture matched the requested branch");
}

fn build_case(seed: u64) -> SelectionCase {
    let state = arena(false, seed);
    let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
        .expect("the benchmark root observes");
    let mut candidates = Vec::new();
    if let Some(baseline) = generate_plan(&view, seed, Weights::BASELINE) {
        candidates.push(baseline);
    }

    let mut missions = MissionBook::new();
    let assignment = StratifiedScripts::default();
    if let Some(plans) = generate_stratified_candidates(&view, seed, &mut missions, assignment) {
        candidates.extend(plans.into_iter().map(|plan| plan.plays));
    }
    deduplicate(&mut candidates);
    assert!(
        !candidates.is_empty(),
        "the benchmark root has no candidates"
    );
    SelectionCase {
        state,
        candidates,
        seed,
        days: DAYS,
        policy: SelectionPolicy::Adaptive,
    }
}

fn deduplicate(candidates: &mut Vec<Vec<Play>>) {
    let mut unique = Vec::with_capacity(candidates.len());
    for candidate in candidates.drain(..) {
        if unique.iter().all(|other| *other != candidate) {
            unique.push(candidate);
        }
    }
    *candidates = unique;
}

fn run_selection(case: &SelectionCase, policy: SelectionPolicy) -> u64 {
    awvm::benchmark::reset_adaptive_counters();
    let result = select(&case.state, &case.candidates, case.seed, case.days, policy)
        .expect("the benchmark selects a candidate");
    black_box(selection_token(result))
}

fn selection_token(result: SelectionResult) -> u64 {
    result.selected_index as u64
        ^ ((result.four_round_replays as u64) << 16)
        ^ ((result.eight_round_replays as u64) << 32)
        ^ u64::from(result.disagreement) << 48
}

pub mod criterion_benches {
    use super::*;
    use criterion::Criterion;

    fn adaptive(c: &mut Criterion) {
        let mut group = c.benchmark_group("ai-adaptive-selection");
        for (name, fixture) in [
            ("agreement-four-round", Fixture::Agreement),
            ("disagreement-extend-eight", Fixture::Extension),
        ] {
            let case = selection_case(fixture);
            group.bench_function(name, |b| {
                b.iter(|| run_selection(&case, fixture.policy()));
            });
        }
        group.finish();
    }

    fn references(c: &mut Criterion) {
        let mut group = c.benchmark_group("ai-adaptive-selection-reference");
        for (name, fixture) in [
            ("standard-four-round", Fixture::StandardFour),
            ("always-eight-round", Fixture::AlwaysEight),
        ] {
            let case = selection_case(fixture);
            group.bench_function(name, |b| {
                b.iter(|| run_selection(&case, fixture.policy()));
            });
        }
        group.finish();
    }

    criterion::criterion_group!(adaptive_benches, adaptive, references);
}

#[cfg(not(target_family = "wasm"))]
pub mod gungraun_benches {
    use super::*;
    use gungraun::{library_benchmark, library_benchmark_group};

    #[library_benchmark(setup = selection_case)]
    #[bench::agreement_four_round(Fixture::Agreement)]
    #[bench::disagreement_extend_eight(Fixture::Extension)]
    fn adaptive(case: SelectionCase) -> u64 {
        let case = std::mem::ManuallyDrop::new(case);
        run_selection(&case, SelectionPolicy::Adaptive)
    }

    #[library_benchmark(setup = selection_case)]
    #[bench::standard_four_round(Fixture::StandardFour)]
    #[bench::always_eight_round(Fixture::AlwaysEight)]
    fn references(case: SelectionCase) -> u64 {
        let case = std::mem::ManuallyDrop::new(case);
        run_selection(&case, case.policy)
    }

    library_benchmark_group!(
        name = adaptive_benches,
        benchmarks = [adaptive, references,]
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_counters() {
        let case = selection_case(Fixture::Extension);
        awvm::benchmark::reset_adaptive_counters();
        run_selection(&case, SelectionPolicy::Adaptive);
        let counters = awvm::benchmark::adaptive_counters();

        println!("adaptive benchmark counters");
        println!("greedy_actions={}", counters.greedy_actions);
        println!("attack_target_calls={}", counters.attack_target_calls);
        println!(
            "attack_target_calls_per_greedy_action={:.3}",
            counters.attack_target_calls as f64 / counters.greedy_actions.max(1) as f64
        );
        println!("destinations_inspected={}", counters.destinations_inspected);
        println!("unit_targets_found={}", counters.unit_targets_found);
        println!("tile_targets_found={}", counters.tile_targets_found);
        println!("candidate_units_sorted={}", counters.candidate_units_sorted);
        println!("empty_target_searches={}", counters.empty_target_searches);
        println!("forecasts_calculated={}", counters.forecasts_calculated);

        assert!(counters.greedy_actions > 0);
        assert!(counters.attack_target_calls > 0);
        assert!(counters.destinations_inspected > 0);
        assert!(counters.unit_targets_found + counters.tile_targets_found > 0);
        assert!(counters.forecasts_calculated > 0);
    }
}
