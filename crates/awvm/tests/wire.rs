//! Both directions of every wire type, over the whole fixture corpus.
//!
//! `execute`, `observe` and `observe-events` are the three operations a client,
//! a server and a replay viewer all sit on, and until now the types only went
//! one way: a `Command` could be decoded but not encoded, an `Observation`
//! encoded but not decoded. A consumer therefore had to mirror the types to
//! talk to the reducer at all, and a mirrored type is one that can drift.
//!
//! These tests assert the two halves are inverses, using the corpus as the
//! source of realistic values rather than hand-written ones — every command the
//! spec fixtures issue, and every observation and observed event the reference
//! implementation projects from them.

use std::path::PathBuf;

use awvm::conformance::collect_json;
use awvm::random::RandomToken;
use awvm::semantic::{
    AwbwVisibility, Observation, ObservedEvent, PlayerId, State, observe, observe_transition,
};
use awvm::transition::{Command, ExecuteOutcome, execute};
use serde_json::Value;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/fixtures")
}

fn corpus() -> Vec<(String, Value)> {
    let root = fixture_root();
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

/// Every command in the corpus survives decode-then-encode byte for byte.
///
/// This is what lets a client build a `Command` as a value instead of hand
/// assembling JSON: whatever the reducer accepts, the type re-emits.
#[test]
fn every_fixture_command_round_trips_through_its_type() {
    let mut checked = 0;
    for (relative, case) in corpus() {
        let Some(steps) = case["steps"].as_array() else {
            continue;
        };
        for step in steps {
            let wire = &step["command"];
            if wire.is_null() {
                continue;
            }
            let command: Command = serde_json::from_value(wire.clone())
                .unwrap_or_else(|error| panic!("{relative}: decoding {wire}: {error}"));
            let encoded = serde_json::to_value(&command).expect("a command re-encodes");
            assert_eq!(&encoded, wire, "{relative}: command did not round-trip");
            checked += 1;
        }
    }
    assert!(checked >= 348, "expected the whole corpus, saw {checked}");
}

/// Every observation and observed-event set the corpus produces survives
/// encode-then-decode-then-encode.
///
/// The projection is `Serialize`-first, so the fixed point that matters is the
/// JSON: a viewer decodes what a server sent, and must reach the same value.
#[test]
fn every_projection_round_trips_through_its_type() {
    let mut observations = 0;
    let mut event_sets = 0;
    for (relative, case) in corpus() {
        let Ok(mut state) = serde_json::from_value::<State>(case["initial_state"].clone()) else {
            continue;
        };
        let recipients: Vec<PlayerId> = state
            .players
            .iter()
            .map(|player| player.id.clone())
            .collect();

        for recipient in &recipients {
            let projected = observe(&AwbwVisibility, &state, recipient).expect("project the state");
            round_trip::<Observation>(&relative, &projected);
            observations += 1;
        }

        for step in case["steps"].as_array().into_iter().flatten() {
            let Ok(command) = serde_json::from_value::<Command>(step["command"].clone()) else {
                continue;
            };
            let random: Vec<RandomToken> =
                serde_json::from_value(step["random"].clone()).unwrap_or_default();
            let Ok(ExecuteOutcome::Accepted(execution)) = execute(&state, command, &random) else {
                continue;
            };
            for recipient in &recipients {
                let transition = observe_transition(
                    &AwbwVisibility,
                    &state,
                    &execution.state,
                    &execution.events,
                    recipient,
                )
                .expect("project the transition");
                round_trip::<Observation>(&relative, &transition.post);
                round_trip::<Vec<ObservedEvent>>(&relative, &transition.events);
                observations += 1;
                event_sets += 1;
            }
            state = execution.state;
        }
    }
    assert!(
        observations >= 1_134 && event_sets >= 558,
        "expected the whole corpus, saw {observations} observations and {event_sets} event sets"
    );
}

/// Encode, decode, encode again, and require the two encodings to agree.
fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(
    relative: &str,
    value: &impl serde::Serialize,
) {
    let wire = serde_json::to_value(value).expect("the projection encodes");
    let decoded: T = serde_json::from_value(wire.clone())
        .unwrap_or_else(|error| panic!("{relative}: decoding {}: {error}", type_name::<T>()));
    let again = serde_json::to_value(&decoded).expect("the decoded value re-encodes");
    assert_eq!(
        again,
        wire,
        "{relative}: {} did not round-trip",
        type_name::<T>()
    );
}

fn type_name<T>() -> &'static str {
    std::any::type_name::<T>()
}
