//! Measure planner turn time on one thread, for native and Wasm builds.
//!
//! The server runs the AI in Wasm on one thread. This program plays the
//! planner against fixed Hard on the non-fog development maps, one game at a
//! time, and prints the distribution of complete planner turn times. Build it
//! for `wasm32-wasip1` and run it under Node to measure Wasm time on V8, the
//! engine of a Worker. Build it natively to get the ratio for the same games.
//! The work counts must be equal in the two builds.
//!
//! `uncapped` is planner-v2 with no work limit. `v7` is the current Hard
//! configuration.
//!
//! ```text
//! cargo build --release -p awbrn-ai-diagnostics --example planner_timing \
//!   --target wasm32-wasip1
//! node scripts/run-wasi.mjs \
//!   target/wasm32-wasip1/release/examples/planner_timing.wasm . \
//!   /w/assets/ai-diagnostics/global-league-pool/manifest.json 1 v7
//! ```
//!
//! Usage: `planner_timing <manifest> <pairs-per-map> <uncapped|v3|v4|v5|v6|v6-canonical|v7> [map-id]`

use std::time::Instant;

use awbrn_ai::agent::Agent;
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai::harness::{Limits, play_measured};
use awbrn_ai::planner::{PlannerAgent, PlannerConfig};
use awbrn_ai::profile::{HARD, HARD_V2};
use awbrn_ai::rng::Rng;
use awbrn_ai_diagnostics::{MapManifest, MapRegistry};
use awvm::session::Session;

const MAPS: [u32; 14] = [
    159501, 77060, 69201, 180298, 126428, 166877, 133665, 172238, 182023, 132144, 160882, 173362,
    169069, 169382,
];
const RUN_SEED: u64 = 0x7a11_0002;

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let [_, manifest, pairs, mode, selected @ ..] = arguments.as_slice() else {
        eprintln!(
            "usage: planner_timing <manifest> <pairs-per-map> <uncapped|v3|v4|v5|v6|v6-canonical|v7> [map-id]"
        );
        std::process::exit(2);
    };
    let selected = match selected {
        [] => None,
        [map] => {
            let map = map.parse::<u32>().expect("map ID is a number");
            assert!(MAPS.contains(&map), "map ID is in the timing set");
            Some(map)
        }
        _ => panic!("expected at most one map ID"),
    };
    let pairs: std::num::NonZeroUsize = pairs.parse().expect("pairs must be positive");
    let config = match mode.as_str() {
        "uncapped" => PlannerConfig::V2,
        "v3" => PlannerConfig::V3,
        "v4" => PlannerConfig::V4,
        "v5" => PlannerConfig::V5,
        "v6" => PlannerConfig::V6,
        "v6-canonical" => PlannerConfig::V6_CANONICAL,
        "v7" => PlannerConfig::V7,
        other => panic!("unknown mode {other}"),
    };
    let registry = MapRegistry::load(&MapManifest::read(manifest).expect("manifest reads"))
        .expect("maps load");

    let mut match_nanos = 0;
    let mut planner_turns = Vec::new();
    let mut hard_turns = Vec::new();
    let mut fallbacks = 0;
    let mut work = 0;
    let mut max_turn_work = 0;
    let mut plans = 0;
    for map_id in MAPS
        .into_iter()
        .filter(|map| selected.is_none_or(|selected| selected == *map))
    {
        let map = registry.get(map_id).expect("map is in the manifest");
        for pair in 0..pairs.get() {
            let match_seed = Rng::mix(RUN_SEED ^ (u64::from(map_id) << 32) ^ pair as u64);
            for planner_seat in 0..2 {
                let state = map.state(match_seed).expect("state builds");
                let mut session = Session::new(state.clone());
                let mut entropy = Rng::from_seed(BaselineConfig::LOCKED.entropy_seed(match_seed));
                let mut planner = PlannerAgent::with_config(
                    BaselineConfig::LOCKED.agent_seed(match_seed, planner_seat),
                    config,
                );
                let mut hard =
                    HARD_V2.agent(BaselineConfig::LOCKED.agent_seed(match_seed, 1 - planner_seat));
                let mut agents: [&mut dyn Agent; 2] = if planner_seat == 0 {
                    [&mut planner, &mut *hard]
                } else {
                    [&mut *hard, &mut planner]
                };
                let match_started = Instant::now();
                let record = play_measured(
                    state,
                    &mut session,
                    &mut agents,
                    &mut entropy,
                    Limits {
                        nodes: HARD.node_budget(),
                        ..Limits::DEFAULT
                    },
                )
                .expect("game plays");
                let elapsed = match_started.elapsed().as_nanos();
                match_nanos += elapsed;
                planner_turns.extend(
                    record.complete_turn_times_by_seat[planner_seat]
                        .iter()
                        .copied(),
                );
                hard_turns.extend(
                    record.complete_turn_times_by_seat[1 - planner_seat]
                        .iter()
                        .copied(),
                );
                fallbacks += planner.stats().fallbacks;
                plans += planner.stats().plans;
                work += planner.stats().work;
                max_turn_work = max_turn_work.max(planner.stats().max_turn_work);
                eprintln!(
                    "map {map_id} pair {pair} seat {planner_seat}: {} days, {} commands, fingerprints {:x?}, match {:.3} ms, work {}, plans {}, mismatches {}, refusals {}, preflight {}, outcome {:?}",
                    record.days,
                    record.commands,
                    record.command_fingerprints,
                    elapsed as f64 / 1e6,
                    planner.stats().work,
                    planner.stats().plans,
                    planner.stats().mismatches,
                    record.refusals,
                    record.preflight_rejections,
                    record.outcome
                );
            }
        }
    }
    println!("complete match time {:.3} ms", match_nanos as f64 / 1e6);
    let total: u64 = planner_turns.iter().sum();
    report(config.identifier, &mut planner_turns);
    report("hard", &mut hard_turns);
    println!("planner plans {plans}, hard fallback decisions {fallbacks}");
    println!(
        "planner work {work} decisions, most in one turn {max_turn_work}, {:.1} us of turn time per decision",
        total as f64 / work.max(1) as f64 / 1e3
    );
}

fn report(name: &str, turns: &mut [u64]) {
    turns.sort_unstable();
    let ms = |index: usize| turns[index.min(turns.len() - 1)] as f64 / 1e6;
    let at = |q: f64| ms(((turns.len() as f64) * q) as usize);
    let over = |limit_ms: f64| turns.iter().filter(|t| **t as f64 / 1e6 > limit_ms).count();
    println!(
        "{name}: turns {} median {:.1} ms p95 {:.1} ms p99 {:.1} ms max {:.1} ms; >800 ms {}, >1000 ms {}",
        turns.len(),
        at(0.5),
        at(0.95),
        at(0.99),
        ms(turns.len() - 1),
        over(800.0),
        over(1000.0),
    );
}
