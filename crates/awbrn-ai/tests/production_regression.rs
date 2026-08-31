//! Regression locks for the promoted production agent.

use awbrn_ai::agent::Agent;
use awbrn_ai::baseline::{
    BaselineConfig, PRODUCTION_IDENTIFIER, production_agent, production_configuration_fingerprint,
};
use awbrn_ai::board::arena;
use awbrn_ai::harness::{Limits, Record, play_observed};
use awbrn_ai::rng::Rng;
use awvm::session::Session;
use awvm::transition::Command;

const RUN_SEED: u64 = 1;

fn fingerprint(commands: &[Command]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    for command in commands {
        for byte in serde_json::to_vec(command).expect("commands serialize") {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

fn play_match(production: bool, production_first: bool) -> (Record, Vec<Command>) {
    let seed = BaselineConfig::LOCKED.game_seed(RUN_SEED, 0);
    let state = arena(false, seed);
    let mut session = Session::new(state.clone());
    let mut entropy = Rng::from_seed(BaselineConfig::LOCKED.entropy_seed(seed));
    let mut production_agent = production_agent(BaselineConfig::LOCKED.agent_seed(seed, 0));
    let mut baseline_slot_zero =
        BaselineConfig::LOCKED.build_greedy(BaselineConfig::LOCKED.agent_seed(seed, 0));
    let mut baseline_slot_one =
        BaselineConfig::LOCKED.build_greedy(BaselineConfig::LOCKED.agent_seed(seed, 1));
    let mut commands = Vec::new();
    let mut observer = |_: &_, command: Option<&Command>| {
        if let Some(command) = command {
            commands.push(command.clone());
        }
    };
    let mut agents: [&mut dyn Agent; 2] = if production && production_first {
        [&mut production_agent, &mut baseline_slot_one]
    } else if production {
        [&mut baseline_slot_one, &mut production_agent]
    } else if production_first {
        [&mut baseline_slot_zero, &mut baseline_slot_one]
    } else {
        [&mut baseline_slot_one, &mut baseline_slot_zero]
    };
    let record = play_observed(
        state,
        &mut session,
        &mut agents,
        &mut entropy,
        Limits::DEFAULT,
        &mut observer,
    );
    (record, commands)
}

#[test]
fn production_identity_and_configuration_are_locked() {
    assert_eq!(BaselineConfig::LOCKED.identifier, "greedy-baseline-v1");
    assert_eq!(BaselineConfig::LOCKED.fingerprint(), "79aa8a6e0491065f");
    assert_eq!(BaselineConfig::PRODUCTION.identifier, PRODUCTION_IDENTIFIER);
    assert_eq!(production_configuration_fingerprint(), "81496db7e594d1bc");
    assert_eq!(
        production_agent(7).config(),
        BaselineConfig::PRODUCTION,
        "production_agent uses the promoted configuration"
    );
}

#[test]
fn production_matches_its_locked_regression_fingerprints_against_baseline() {
    let baseline_first = play_match(false, true);
    let production_first = play_match(true, true);
    let baseline_second = play_match(false, false);
    let production_second = play_match(true, false);

    assert_ne!(baseline_first.1, production_first.1);
    assert_ne!(baseline_second.1, production_second.1);
    assert_eq!(fingerprint(&baseline_first.1), 17_699_916_440_832_964_813);
    assert_eq!(fingerprint(&baseline_second.1), 5_902_654_325_709_691_999);
    assert_eq!(fingerprint(&production_first.1), 3_799_485_981_112_380_887);
    assert_eq!(fingerprint(&production_second.1), 4_695_877_162_590_766_318);
}
