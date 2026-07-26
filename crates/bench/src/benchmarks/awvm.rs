//! `awvm::transition::execute` on representative fixtures.
//!
//! The four cases are the ones recorded as the redesign baseline in
//! `handoff.md`. They span the cost range: a plain move, a tile attack, a
//! commander combat (the most ruleset-table-heavy path), and a turn boundary.
//!
//! Fixtures are embedded rather than read from disk so these also build for
//! wasm.

use awvm::random::RandomToken;
use awvm::semantic::State;
use awvm::transition::{Command, execute};
use serde_json::Value;

pub const CASES: &[(&str, &str)] = &[
    (
        "movement-infantry-plain-move",
        include_str!("../../../../spec/fixtures/movement/infantry-plain-move.json"),
    ),
    (
        "combat-tile-indirect-lethal",
        include_str!("../../../../spec/fixtures/combat/tile-indirect-lethal.json"),
    ),
    (
        "commander-colin-cop-combat",
        include_str!("../../../../spec/fixtures/commander/colin-cop-combat.json"),
    ),
    (
        "turn-end-turn-income-ready",
        include_str!("../../../../spec/fixtures/turn/end-turn-income-ready.json"),
    ),
];

/// A fixture's initial state and first step, already decoded, so the benchmark
/// measures `execute` rather than deserialization.
pub struct Case {
    pub state: State,
    pub command: Value,
    pub random: Vec<RandomToken>,
}

pub fn load(source: &str) -> Case {
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    Case {
        state: serde_json::from_value(case["initial_state"].clone()).expect("decode state"),
        command: case["steps"][0]["command"].clone(),
        random: serde_json::from_value(case["steps"][0]["random"].clone())
            .expect("decode random tokens"),
    }
}

pub fn run(case: &Case) -> usize {
    let command: Command = serde_json::from_value(case.command.clone()).expect("decode command");
    match execute(&case.state, command, &case.random) {
        Ok(execution) => execution.events.len(),
        Err(error) => panic!("fixture step did not execute: {error:?}"),
    }
}

pub mod criterion_benches {
    use super::*;
    use criterion::{BenchmarkId, Criterion};
    use std::hint::black_box;

    fn transition(c: &mut Criterion) {
        let mut group = c.benchmark_group("awvm-execute");
        for (name, source) in CASES {
            let case = load(source);
            group.bench_function(BenchmarkId::from_parameter(name), |b| {
                b.iter(|| black_box(run(&case)));
            });
        }
        group.finish();
    }

    criterion::criterion_group!(awvm_benches, transition);
}

#[cfg(not(target_family = "wasm"))]
pub mod gungraun_benches {
    use super::*;
    use gungraun::{library_benchmark, library_benchmark_group};

    fn setup(source: &str) -> Case {
        load(source)
    }

    #[library_benchmark(setup = setup)]
    #[bench::movement(CASES[0].1)]
    #[bench::combat(CASES[1].1)]
    #[bench::commander(CASES[2].1)]
    #[bench::turn(CASES[3].1)]
    fn transition(case: Case) -> usize {
        run(&case)
    }

    library_benchmark_group!(name = awvm_benches, benchmarks = [transition,]);
}
