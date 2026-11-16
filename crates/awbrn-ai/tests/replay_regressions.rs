//! Regression positions from a match that a person won against the Hard
//! profile.
//!
//! Each fixture is the position at the start of one turn of the AI seat on
//! Amber Valley. Drake plays the AI seat and Adder plays the person. In each
//! position, the AI played a turn that lost army value to the reply of the
//! person. Better turns were available in each position.
//!
//! The test plays the turn of each seated profile and then one reply by the
//! fixed `planner-v3`. Both turns use the middle of each luck range. The
//! test measures the change in army value of the AI seat over the two turns:
//! the value of our units less the value of the enemy units, in funds, as the
//! mean over three agent seeds. A turn passes a position when the change
//! reaches its threshold.
//!
//! Each threshold is between the result of the `planner-v3` turn and the
//! tenth best turn that one changed order and the Hard policy can make. The
//! reply is a stronger opponent than the reply model of the planner, so a
//! turn cannot pass only because it exploits that model. Do not lower a
//! threshold to make a profile pass.

use awbrn_ai::agent::{Agent, NodeBudget};
use awbrn_ai::harness::run_agent_turn_unmeasured;
use awbrn_ai::planner::{PlannerAgent, PlannerConfig};
use awbrn_ai::profile::{AiProfile, CURRENT_PROFILES};
use awvm::commander::Domain;
use awvm::random::{Entropy, Luck, RandomError};
use awvm::ruleset::{self, WeatherKind};
use awvm::semantic::{PlayerIdx, State};

const FIXTURE_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/replay_regressions/"
);

/// The agent seeds that each position plays.
const SEEDS: [u64; 3] = [1, 2, 3];

/// The seed of the reply planner.
const REPLY_SEED: u64 = 0x5eed;

/// Luck at the middle of each range. Weather stays clear.
struct MeanLuck;

impl Entropy for MeanLuck {
    fn luck(&mut self, _polarity: Luck, domain: Domain) -> Result<i64, RandomError> {
        Ok(domain.minimum + (domain.maximum - domain.minimum) / 2)
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        Ok(WeatherKind::Clear)
    }
}

fn fixture(name: &str) -> State {
    let path = format!("{FIXTURE_ROOT}{name}.json");
    let bytes = std::fs::read(path).expect("replay fixture exists");
    serde_json::from_slice(&bytes).expect("replay fixture decodes")
}

/// Our army value less the enemy army value, in funds.
fn army_balance(state: &State, seat: PlayerIdx) -> f64 {
    state
        .units
        .iter()
        .map(|unit| {
            let value = ruleset::profile(unit.kind).cost as f64 * f64::from(unit.hp) / 100.0;
            if unit.owner == seat { value } else { -value }
        })
        .sum()
}

fn play_turn(state: State, agent: &mut dyn Agent, budget: NodeBudget) -> State {
    let result =
        run_agent_turn_unmeasured(state, agent, &mut MeanLuck, budget).expect("the turn executes");
    assert!(result.completed, "the turn did not complete");
    assert_eq!(result.rejected_commands, 0, "the turn had reducer refusals");
    result.state
}

/// The mean change in army balance over the turn of `profile` and the reply.
fn mean_swing(name: &str, profile: &AiProfile) -> f64 {
    let start = fixture(name);
    let seat = start
        .players
        .seat(&start.turn.active_player)
        .expect("the active player holds a seat");
    let total: f64 = SEEDS
        .iter()
        .map(|&seed| {
            let mut agent = profile.agent(seed);
            agent.start_match();
            let after = play_turn(start.clone(), &mut *agent, profile.node_budget());
            let mut reply = PlannerAgent::with_config(REPLY_SEED, PlannerConfig::V3);
            reply.start_match();
            let replied = play_turn(after, &mut reply, NodeBudget::SIXTEEN);
            army_balance(&replied, seat) - army_balance(&start, seat)
        })
        .sum();
    total / SEEDS.len() as f64
}

/// The seated profiles, in the order of the columns of [`SWINGS`].
const COLUMNS: [&str; 3] = ["ai-easy-v2", "ai-medium-v2", "ai-hard-v5"];

/// Each position, its threshold, and the mean change in army balance of each
/// seated profile, rounded to the fund, in the columns of [`COLUMNS`]. Days 8
/// and 10 have no threshold.
const SWINGS: [(&str, Option<i64>, [i64; 3]); 6] = [
    // A tank attacks a capturing infantry from a tile where the enemy tank
    // and recon can reach it.
    ("amber-valley-day06", Some(0), [-1_740, -1_740, 1_570]),
    // A damaged tank attacks again into enemy fire and is destroyed.
    ("amber-valley-day07", Some(-4_000), [-5_573, -1_130, -6_410]),
    ("amber-valley-day08", None, [-2_660, -4_590, -6_500]),
    // Three units attack one tank from tiles that the enemy army covers.
    ("amber-valley-day09", Some(-3_500), [-2_117, -2_530, -2_903]),
    ("amber-valley-day10", None, [8_060, -3_040, -323]),
    // The army near the headquarters loses most of its value in one
    // exchange.
    (
        "amber-valley-day11",
        Some(-9_500),
        [-13_873, -12_600, -10_550],
    ),
];

/// Each seated profile changes the army balance of each position by the
/// amount in [`SWINGS`]. A better and a worse result both fail the test:
/// change the table to accept a change of play. The message marks each
/// result that misses its threshold.
#[test]
fn the_seated_profiles_make_the_swings_of_the_table() {
    assert_eq!(CURRENT_PROFILES.map(|profile| profile.id), COLUMNS);
    let actual = SWINGS.map(|(name, threshold, _)| {
        let swings = CURRENT_PROFILES.map(|profile| mean_swing(name, &profile).round() as i64);
        (name, threshold, swings)
    });
    let misses = actual
        .iter()
        .flat_map(|(name, threshold, swings)| {
            COLUMNS
                .iter()
                .zip(swings)
                .filter(move |(_, swing)| threshold.is_some_and(|threshold| **swing < threshold))
                .map(move |(profile, swing)| format!("{profile} {name}: {swing}"))
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, SWINGS, "below the threshold: {misses:#?}");
}
