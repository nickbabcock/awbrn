//! Regression positions from a match that a person won against the Hard
//! profile.
//!
//! Each fixture is the position at the start of one turn of the AI seat on
//! Amber Valley. Drake plays the AI seat and Adder plays the person. In each
//! position, the AI played a turn that lost army value to the reply of the
//! person. Better turns were available in each position.
//!
//! A test plays the Hard turn and then one reply by the fixed planner of
//! `ai-hard-v3`. Both turns use the middle of each luck range. The test
//! measures the change in army value of the AI seat over the two turns: the
//! value of our units less the value of the enemy units, in funds. A turn
//! passes when the mean change over three agent seeds reaches the threshold.
//!
//! Each threshold is between the result of the `ai-hard-v3` turn and the
//! tenth best turn that one changed order and the Hard policy can make. The
//! reply is a stronger opponent than the reply model of the planner, so a
//! turn cannot pass only because it exploits that model.
//!
//! An ignored test is a position that the current profile does not pass. Do
//! not lower its threshold to make it pass. Remove the `ignore` attribute when
//! a profile passes it.

use awbrn_ai::HARD;
use awbrn_ai::agent::{Agent, NodeBudget};
use awbrn_ai::harness::run_agent_turn_unmeasured;
use awbrn_ai::planner::{PlannerAgent, PlannerConfig};
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

/// The mean change in army balance over the Hard turn and the reply.
fn mean_swing(name: &str) -> f64 {
    let start = fixture(name);
    let seat = start
        .players
        .seat(&start.turn.active_player)
        .expect("the active player holds a seat");
    let total: f64 = SEEDS
        .iter()
        .map(|&seed| {
            let mut agent = HARD.agent(seed);
            agent.start_match();
            let after = play_turn(start.clone(), &mut *agent, HARD.node_budget());
            let mut reply = PlannerAgent::with_config(REPLY_SEED, PlannerConfig::V3);
            reply.start_match();
            let replied = play_turn(after, &mut reply, NodeBudget::SIXTEEN);
            army_balance(&replied, seat) - army_balance(&start, seat)
        })
        .sum();
    total / SEEDS.len() as f64
}

fn check(name: &str, threshold: f64) {
    let swing = mean_swing(name);
    assert!(
        swing >= threshold,
        "{name}: army balance changed by {swing:.0}, threshold {threshold:.0}"
    );
}

/// Print the change in army balance for each position.
#[test]
#[ignore = "prints a report"]
fn print_replay_swings() {
    for name in [
        "amber-valley-day06",
        "amber-valley-day07",
        "amber-valley-day08",
        "amber-valley-day09",
        "amber-valley-day10",
        "amber-valley-day11",
    ] {
        println!("{name}: {:.0}", mean_swing(name));
    }
}

/// A tank attacks a capturing infantry from a tile where the enemy tank and
/// recon can reach it.
#[test]
#[ignore = "planner-v5 does not pass this position"]
fn day_six_does_not_trade_a_tank_for_a_capture_stop() {
    check("amber-valley-day06", 0.0);
}

/// A damaged tank attacks again into enemy fire and is destroyed.
#[test]
#[ignore = "planner-v5 does not pass this position"]
fn day_seven_does_not_attack_into_a_counterattack() {
    check("amber-valley-day07", -4_000.0);
}

/// Three units attack one tank from tiles that the enemy army covers.
#[test]
fn day_nine_does_not_leave_its_attackers_exposed() {
    check("amber-valley-day09", -3_500.0);
}

/// The army near the headquarters loses most of its value in one exchange.
#[test]
#[ignore = "planner-v5 does not pass this position"]
fn day_eleven_limits_the_loss_near_its_headquarters() {
    check("amber-valley-day11", -9_500.0);
}
