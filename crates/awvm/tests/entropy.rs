//! An authority that rolls its own values stays replayable.
//!
//! `execute` takes a tape because a fixture and a replay both already know
//! every outcome. A live game does not: it holds an RNG, and the value it must
//! roll is bounded by the acting commander's luck domain, which is only known
//! once the reducer has assembled the combat context. `execute_with` asks at
//! that point instead.
//!
//! What makes that safe to build a server on is the round trip below. Whatever
//! a rolling authority produced, `Recording` hands back as a tape, and replaying
//! that tape through the ordinary `execute` must reach the identical state,
//! events and draw count — otherwise a live game could not be checked against
//! the same conformance path a fixture takes.

use std::path::PathBuf;

use awvm::commander::Domain;
use awvm::conformance::collect_json;
use awvm::prelude::*;
use serde_json::Value;

/// An authority whose rolls are its own business, here the extremes of whatever
/// domain the reducer asks for.
///
/// A real one would consult an RNG. What matters is that it never has to derive
/// the domain: the reducer supplies it, so this cannot roll out of range, and
/// no part of the commander algebra is restated here.
struct Extremes {
    high: bool,
    weather: WeatherKind,
}

impl Entropy for Extremes {
    fn luck(&mut self, _: Luck, domain: Domain) -> Result<i64, RandomError> {
        self.high = !self.high;
        Ok(if self.high {
            domain.maximum
        } else {
            domain.minimum
        })
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        Ok(self.weather)
    }
}

fn corpus() -> Vec<(String, Value)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/fixtures");
    let mut files = Vec::new();
    collect_json(&root, &mut files).expect("walk fixture root");
    files.sort();
    files
        .iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            let text = std::fs::read_to_string(path).expect("read fixture");
            (
                relative,
                serde_json::from_str(&text).expect("parse fixture"),
            )
        })
        .collect()
}

/// Every command in the corpus, executed against a rolling authority, replays
/// from the tape that authority produced.
#[test]
fn a_rolling_authority_produces_a_tape_that_replays_identically() {
    let mut rolled = 0;
    let mut with_draws = 0;
    for (relative, case) in corpus() {
        let Ok(mut state) = serde_json::from_value::<State>(case["initial_state"].clone()) else {
            continue;
        };
        for step in case["steps"].as_array().into_iter().flatten() {
            let Ok(command) = serde_json::from_value::<Command>(step["command"].clone()) else {
                continue;
            };

            // Roll rather than replay. The weather is taken from the fixture's
            // own tape where it has one, because a weather selection is a
            // semantic outcome the case asserts, not a free roll.
            let recorded: Vec<RandomToken> =
                serde_json::from_value(step["random"].clone()).unwrap_or_default();
            let weather = recorded
                .iter()
                .find_map(|token| match token {
                    RandomToken::WeatherSelection(kind) => Some(*kind),
                    _ => None,
                })
                .unwrap_or(WeatherKind::Clear);
            let mut authority = Recording::new(Extremes {
                high: false,
                weather,
            });

            let rolled_outcome = execute_with(&state, command.clone(), &mut authority);
            let (_, tape) = authority.into_parts();
            let replayed = execute(&state, command.clone(), &tape);

            match (rolled_outcome, replayed) {
                (Ok(ExecuteOutcome::Accepted(live)), Ok(ExecuteOutcome::Accepted(replay))) => {
                    assert_eq!(live.state, replay.state, "{relative}: state diverged");
                    assert_eq!(live.events, replay.events, "{relative}: events diverged");
                    assert_eq!(
                        live.random_consumed, replay.random_consumed,
                        "{relative}: draw count diverged"
                    );
                    assert_eq!(
                        live.random_consumed,
                        tape.len(),
                        "{relative}: the recorded tape is not what the run drew"
                    );
                    if !tape.is_empty() {
                        with_draws += 1;
                    }
                    rolled += 1;

                    // Continue from the live state, so later steps in the case
                    // are exercised against a state a rolling authority reached
                    // rather than the one the fixture recorded.
                    state = live.state;
                }
                (Ok(ExecuteOutcome::Rejected(live)), Ok(ExecuteOutcome::Rejected(replay))) => {
                    assert_eq!(live, replay, "{relative}: violation diverged");
                    rolled += 1;
                }
                (Err(live), Err(replay)) => {
                    assert_eq!(
                        format!("{live}"),
                        format!("{replay}"),
                        "{relative}: error diverged"
                    );
                    rolled += 1;
                }
                (live, replay) => {
                    panic!("{relative}: rolling and replaying disagreed: {live:?} vs {replay:?}")
                }
            }
        }
    }
    assert!(
        rolled >= 300 && with_draws >= 40,
        "expected the whole corpus, saw {rolled} commands and {with_draws} that drew"
    );
}

/// A source that hands back a value outside the domain it was given is reported
/// the same way a malformed tape is, rather than reaching combat arithmetic.
#[test]
fn an_out_of_domain_roll_is_rejected_like_a_malformed_tape() {
    struct Cheat;
    impl Entropy for Cheat {
        fn luck(&mut self, _: Luck, domain: Domain) -> Result<i64, RandomError> {
            Ok(domain.maximum + 1_000)
        }
        fn weather(&mut self) -> Result<WeatherKind, RandomError> {
            Ok(WeatherKind::Clear)
        }
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/combat/movement-direct-fog-capture.json");
    let case: Value =
        serde_json::from_str(&std::fs::read_to_string(root).expect("read fixture")).unwrap();
    let state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();

    let attack = case["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|step| {
            let command: Command = serde_json::from_value(step["command"].clone()).ok()?;
            matches!(command, Command::MoveAttack { .. }).then_some(command)
        })
        .expect("the fixture attacks");

    // The same command against the fixture's own tape is accepted, so the only
    // difference is where the number came from. The fixture draws four luck
    // values, so `Cheat` is reached.
    let honest: Vec<RandomToken> = case["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|step| serde_json::from_value(step["random"].clone()).ok())
        .unwrap_or_default();
    assert!(matches!(
        execute(&state, attack.clone(), &honest),
        Ok(ExecuteOutcome::Accepted(_))
    ));

    assert!(
        matches!(
            execute_with(&state, attack, &mut Cheat),
            Err(ExecuteError::InvalidRandom(RandomError::OutOfDomain { .. }))
        ),
        "an out-of-domain roll must not resolve"
    );
}
