//! Release controls for the promoted AI profile.

use awbrn_ai::agent::{Agent, NodeBudget};
use awbrn_ai::harness::{Limits, TurnResult, play_observed, run_agent_turn};
use awbrn_ai::rng::Rng;
use awbrn_ai::{HARD, STANDARD};
use awvm::semantic::{AwbwVisibility, Pos, State, UnitId, observe};
use awvm::session::Session;
use awvm::transition::Command;
use serde::Deserialize;

const FIXTURE_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/release_controls/"
);
const MAX_COMPLETE_TURN_NANOS: u64 = 1_000_000_000;

#[derive(Deserialize)]
struct SavedCase {
    positions: Vec<SavedPosition>,
}

#[derive(Deserialize)]
struct SavedPosition {
    label: String,
    state: State,
}

fn fixture(name: &str) -> State {
    let path = format!("{FIXTURE_ROOT}{name}.json");
    let bytes = std::fs::read(path).expect("release fixture exists");
    serde_json::from_slice(&bytes).expect("release fixture decodes")
}

fn saved_root(name: &str) -> State {
    let path = format!("{FIXTURE_ROOT}{name}.json");
    let bytes = std::fs::read(path).expect("saved release fixture exists");
    let case: SavedCase = serde_json::from_slice(&bytes).expect("saved release fixture decodes");
    case.positions
        .into_iter()
        .find(|position| position.label == "root")
        .expect("saved release fixture has a root")
        .state
}

fn run_fixture(name: &str, seed: u64) -> TurnResult {
    let mut agent = HARD.agent(seed);
    agent.start_match();
    let mut entropy = Rng::from_seed(seed ^ 0x9e37_79b9);
    let result = run_agent_turn(fixture(name), &mut *agent, &mut entropy, HARD.node_budget());
    assert!(result.completed, "{name} did not complete its turn");
    assert!(
        result.total_nanos <= MAX_COMPLETE_TURN_NANOS,
        "{name} exceeded the complete-turn budget"
    );
    assert_eq!(result.rejected_commands, 0, "{name} had reducer refusals");
    assert_eq!(
        result.preflight_rejections, 0,
        "{name} had preflight rejections"
    );
    assert_eq!(
        result.unrealizable_plays, 0,
        "{name} had unrealizable plays"
    );
    assert!(
        matches!(result.commands.last(), Some(Command::EndTurn { .. })),
        "{name} did not end with EndTurn: {:#?}",
        result.commands
    );
    result
}

fn has_capture(commands: &[Command]) -> bool {
    commands
        .iter()
        .any(|command| matches!(command, Command::MoveCapture { .. }))
}

fn has_production(commands: &[Command]) -> bool {
    commands
        .iter()
        .any(|command| matches!(command, Command::ProduceUnit { .. }))
}

fn has_attack(commands: &[Command]) -> bool {
    commands
        .iter()
        .any(|command| matches!(command, Command::MoveAttack { .. }))
}

fn has_wait(commands: &[Command]) -> bool {
    commands
        .iter()
        .any(|command| matches!(command, Command::MoveWait { .. }))
}

fn run_fixture_twice(name: &str, seed: u64) -> TurnResult {
    let first = run_fixture(name, seed);
    let second = run_fixture(name, seed);
    assert_eq!(
        first.commands, second.commands,
        "{name} is not deterministic"
    );
    assert_eq!(
        first.command_fingerprint, second.command_fingerprint,
        "{name} fingerprint changed on replay"
    );
    first
}

#[test]
fn fixed_behavior_corpus_passes_twice_with_identical_commands() {
    let expansion = run_fixture_twice("expansion-production", 0x291);
    assert!(has_capture(&expansion.commands));
    assert!(has_production(&expansion.commands));

    let combat = run_fixture_twice("combat", 0x293);
    assert!(has_attack(&combat.commands));

    let defense = run_fixture_twice("immediate-defense", 0x294);
    assert!(has_capture(&defense.commands));
    assert!(has_wait(&defense.commands));

    let saving = run_fixture_twice("justified-saving", 0x295);
    assert_eq!(saving.commands.len(), 1);
    assert!(matches!(saving.commands[0], Command::EndTurn { .. }));
}

#[test]
fn reply_exposure_control_avoids_the_known_losing_attack() {
    let state = saved_root("reply-exposure");
    let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
        .expect("reply exposure root is observable");
    let projection = Session::from_observation(&view).expect("reply exposure root reifies");
    let mut agent = HARD.agent(0x301);
    let command = agent
        .act(&view, NodeBudget::FOUR)
        .expect("reply exposure control selects a play")
        .command(&projection)
        .expect("reply exposure play is realizable");

    assert!(!matches!(
        command,
        Command::MoveAttack { unit, path, .. }
            if unit == UnitId::new(67) && path.last() == Some(&Pos::new(17, 10))
    ));
}

#[test]
fn promoted_profile_completes_a_legal_match_in_both_seat_orders() {
    for candidate_first in [true, false] {
        let seed = 0x303 + u64::from(!candidate_first);
        let state = awbrn_ai::board::arena(false, seed);
        let mut session = Session::new(state.clone());
        let mut entropy = Rng::from_seed(seed ^ 0x9e37_79b9);
        let mut candidate = HARD.agent(seed ^ 0x2);
        let mut baseline = STANDARD.agent(seed ^ 0x3);
        let mut agents: [&mut dyn Agent; 2] = if candidate_first {
            [&mut *candidate, &mut *baseline]
        } else {
            [&mut *baseline, &mut *candidate]
        };
        let record = play_observed(
            state,
            &mut session,
            &mut agents,
            &mut entropy,
            Limits::DEFAULT,
            |_, _| {},
        );

        assert!(!record.abandoned(), "candidate-first={candidate_first}");
        assert_eq!(record.refusals, 0, "candidate-first={candidate_first}");
        assert_eq!(
            record.preflight_rejections, 0,
            "candidate-first={candidate_first}"
        );
        assert_eq!(
            record.unrealizable_plays, 0,
            "candidate-first={candidate_first}"
        );
        assert!(
            record
                .complete_turn_times_by_seat
                .iter()
                .all(|times| !times.is_empty())
        );
        assert!(
            record
                .complete_turn_times_by_seat
                .iter()
                .flatten()
                .all(|nanos| *nanos <= MAX_COMPLETE_TURN_NANOS),
            "candidate-first={candidate_first} exceeded the complete-turn budget"
        );
    }
}
