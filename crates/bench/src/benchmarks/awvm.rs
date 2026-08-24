//! `awvm::transition::execute` on representative fixtures.
//!
//! The four cases are the ones recorded as the redesign baseline in
//! `handoff.md`. They span the cost range: a plain move, a tile attack, a
//! commander combat (the most ruleset-table-heavy path), and a turn boundary.
//!
//! Fixtures are embedded rather than read from disk so these also build for
//! wasm.

use awvm::event::Event;
use awvm::random::RandomToken;
use awvm::semantic::{AwbwVisibility, PlayerId, State, observe, observe_transition};
use awvm::transition::{Command, ExecuteOutcome, execute};
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
#[derive(Debug)]
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
        Ok(ExecuteOutcome::Accepted(execution)) => execution.events.len(),
        Ok(ExecuteOutcome::Rejected(violation)) => {
            panic!("fixture step was rejected: {violation:?}")
        }
        Err(error) => panic!("fixture step did not execute: {error:?}"),
    }
}

/// One command already executed, so the projection benchmarks measure `observe`
/// rather than the reducer that produced their inputs.
#[derive(Debug)]
pub struct Projected {
    pub state: State,
    pub next_state: State,
    pub events: Vec<Event>,
    pub recipient: PlayerId,
}

/// A fixture grown to the scale a real match runs at, with fog on.
///
/// Fixtures are two to four units on a handful of tiles, which is the one size
/// at which the projection's cost does not show: `observe` is quadratic in
/// (tiles x sighting units) under fog, so a board that is 200 times larger is
/// what tells whether it is cheap. AWBW maps run to roughly 20x20 with 20-50
/// units.
pub fn project(source: &str, width: u8, height: u8, units: usize) -> Projected {
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    let mut wire = case["initial_state"].clone();
    let rows = wire["board"]["tiles"]
        .as_array()
        .expect("board rows")
        .clone();
    let origin_height = u8::try_from(rows.len()).expect("a fixture board fits a byte");
    let origin_width =
        u8::try_from(rows[0].as_array().expect("a board row").len()).expect("a fixture row fits");
    assert!(
        width >= origin_width && height >= origin_height,
        "boards only grow"
    );
    // The board is grown on the wire rather than through `Board`, so a tile's
    // rare state — a pipe seam's HP, a teleporter's pairing — survives the
    // growth. The fixture's own tiles keep their coordinates, so the command it
    // comes with still names what it named; everything beyond them is filler.
    let filler = rows[0].as_array().expect("a board row")[0].clone();
    let grown: Vec<Value> = (0..height)
        .map(|y| {
            let mut row: Vec<Value> = if y < origin_height {
                rows[usize::from(y)]
                    .as_array()
                    .expect("a board row")
                    .clone()
            } else {
                Vec::new()
            };
            row.resize(usize::from(width), filler.clone());
            Value::Array(row)
        })
        .collect();
    wire["board"] = serde_json::json!({
        "width": width,
        "height": height,
        "tiles": grown,
    });

    // Filler units go on the rows the fixture never had, for the same reason.
    let mut army = wire["units"].as_array().expect("fixture units").clone();
    let template = army[0].clone();
    let mut candidate = 0u32;
    while army.len() < units {
        let free = u32::from(width) * u32::from(height - origin_height);
        assert!(candidate < free, "not enough room for {units} units");
        let position = [
            (candidate % u32::from(width)) as u8,
            origin_height + (candidate / u32::from(width)) as u8,
        ];
        candidate += 1;
        let mut unit = template.clone();
        unit["id"] = serde_json::json!(1_000 + candidate);
        unit["location"] = serde_json::json!({"type": "board", "position": position});
        army.push(unit);
    }
    wire["units"] = Value::Array(army);

    let mut state: State = serde_json::from_value(wire).expect("decode grown state");
    state.settings.fog = true;

    let command: Command =
        serde_json::from_value(case["steps"][0]["command"].clone()).expect("decode command");
    let random: Vec<RandomToken> =
        serde_json::from_value(case["steps"][0]["random"].clone()).expect("decode random tokens");
    let (next_state, events) = match execute(&state, command, &random) {
        Ok(ExecuteOutcome::Accepted(execution)) => (execution.state, execution.events),
        other => panic!("grown fixture did not execute: {other:?}"),
    };
    let recipient = state.turn.active_player.clone();
    Projected {
        state,
        next_state,
        events,
        recipient,
    }
}

pub fn run_observe(projected: &Projected) -> usize {
    observe(&AwbwVisibility, &projected.state, &projected.recipient)
        .expect("observe")
        .units
        .len()
}

pub fn run_observe_transition(projected: &Projected) -> usize {
    observe_transition(
        &AwbwVisibility,
        &projected.state,
        &projected.next_state,
        &projected.events,
        &projected.recipient,
    )
    .expect("observe-transition")
    .events
    .len()
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

    fn projection(c: &mut Criterion) {
        let projected = project(CASES[1].1, 20, 20, 30);
        let mut group = c.benchmark_group("awvm-observe");
        group.bench_function("observe-fog-20x20-30units", |b| {
            b.iter(|| black_box(run_observe(&projected)));
        });
        group.bench_function("observe-transition-fog-20x20-30units", |b| {
            b.iter(|| black_box(run_observe_transition(&projected)));
        });
        group.finish();
    }

    criterion::criterion_group!(awvm_benches, transition, projection);
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

    fn grown(source: &str) -> Projected {
        project(source, 20, 20, 30)
    }

    #[library_benchmark(setup = grown)]
    #[bench::fog_game_scale(CASES[1].1)]
    fn projection(projected: Projected) -> usize {
        run_observe(&projected)
    }

    #[library_benchmark(setup = grown)]
    #[bench::fog_game_scale(CASES[1].1)]
    fn transition_projection(projected: Projected) -> usize {
        run_observe_transition(&projected)
    }

    library_benchmark_group!(
        name = awvm_benches,
        benchmarks = [transition, projection, transition_projection,]
    );
}
