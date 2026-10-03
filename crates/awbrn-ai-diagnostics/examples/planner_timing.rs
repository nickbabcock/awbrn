//! Measure planner turn time on one thread, for native and Wasm builds.
//!
//! The server runs the AI in Wasm on one thread. This program plays the
//! planner against fixed Hard on the non-fog development maps, one game at a
//! time, and prints the distribution of complete planner turn times. Build it
//! for `wasm32-wasip1` and run it under Node to measure Wasm time on V8, the
//! engine of a Worker. Build it natively to get the ratio for the same games.
//! The work counts must be equal in the two builds.
//!
//! `uncapped` is planner-v2 with no work limit. `v3` is the production
//! configuration.
//!
//! ```text
//! cargo build --release -p awbrn-ai-diagnostics --example planner_timing \
//!   --target wasm32-wasip1
//! node scripts/run-wasi.mjs \
//!   target/wasm32-wasip1/release/examples/planner_timing.wasm . \
//!   /w/assets/ai-diagnostics/global-league-pool/manifest.json 1 v3
//! ```
//!
//! Usage: `planner_timing <manifest> <pairs-per-map> <uncapped|v3|v4>`

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
    let [_, manifest, pairs, mode] = arguments.as_slice() else {
        eprintln!("usage: planner_timing <manifest> <pairs-per-map> <uncapped|v3|v4>");
        std::process::exit(2);
    };
    let pairs: std::num::NonZeroUsize = pairs.parse().expect("pairs must be positive");
    let config = match mode.as_str() {
        "uncapped" => PlannerConfig::V2,
        "v3" => PlannerConfig::V3,
        "v4" => PlannerConfig::V4,
        other => panic!("unknown mode {other}"),
    };
    let registry = MapRegistry::load(&MapManifest::read(manifest).expect("manifest reads"))
        .expect("maps load");

    let mut planner_turns = Vec::new();
    let mut hard_turns = Vec::new();
    let mut fallbacks = 0;
    let mut work = 0;
    let mut max_turn_work = 0;
    let mut plans = 0;
    for map_id in MAPS {
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
                    "map {map_id} pair {pair} seat {planner_seat}: {} days",
                    record.days
                );
            }
        }
    }
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
