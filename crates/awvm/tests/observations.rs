//! Golden observation projections, captured before the typed-event refactor.
//!
//! Only 22 of the 306 fixtures assert observations, so the conformance corpus
//! alone does not pin down `observe` and `observe_events`. These goldens do:
//! they record what every fixture projects to every recipient, at the initial
//! state and after every step.
//!
//! Two layers, because the full corpus is ~4 MB of JSON:
//!
//! * `goldens/observations.txt` — one digest line per fixture. Complete
//!   coverage, small diffs, and this is the gate.
//! * `goldens/detail/**` — full JSON for the fixtures where fog and
//!   concealment logic actually varies, so a digest change can be read.
//!
//! Regenerate both with `AWVM_UPDATE_GOLDENS=1 cargo test -p awvm`. When a
//! digest changes for a fixture outside the detail set, dump it on demand with
//! `AWVM_GOLDEN_DETAIL=<path substring> cargo test -p awvm --test observations`
//! and diff the result.
//!
//! Observation output is deterministic across processes; this was verified
//! before the goldens were committed.

use std::fmt::Write as _;
use std::path::PathBuf;

use awvm::conformance::{InProcess, Peer, collect_json};
use awvm::prelude::*;
use awvm::semantic::{DrawReason, Match, ObserveError, Outcome, RulesetRevision, observe_into};
use serde_json::{Value, json};

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_root() -> PathBuf {
    manifest().join("../../spec/fixtures")
}

fn goldens() -> PathBuf {
    manifest().join("tests/goldens")
}

fn updating() -> bool {
    std::env::var_os("AWVM_UPDATE_GOLDENS").is_some()
}

/// Fixtures whose full projections are checked in, so a digest change is
/// readable without regenerating anything. Anything exercising fog, hiding, or
/// vision belongs here; the rest is covered by the digest table.
fn in_detail_set(relative: &str, case: &Value) -> bool {
    relative.starts_with("fog/")
        || relative.starts_with("concealment/")
        || relative.starts_with("combat/visibility-")
        || case
            .get("initial_observations")
            .is_some_and(|v| !v.is_null())
        || case["steps"].as_array().is_some_and(|steps| {
            steps.iter().any(|step| {
                step["expect"].get("observations").is_some()
                    || step["expect"].get("observed_events").is_some()
            })
        })
}

/// Everything one fixture projects, in playback order.
fn project(case: &Value) -> Value {
    let mut peer = InProcess;
    let mut state = case["initial_state"].clone();
    let recipients: Vec<String> = state["players"]
        .as_array()
        .map(|players| {
            players
                .iter()
                .filter_map(|player| player["id"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let observe = |peer: &mut InProcess, state: &Value, recipient: &str| -> Value {
        let response = peer
            .exchange(json!({
                "protocol_version":"0.1.0","request_id":"golden","operation":"observe",
                "ruleset":case["ruleset"],"state":state,"recipient":recipient
            }))
            .expect("observe");
        assert_eq!(response["status"], "ok", "observe failed: {response}");
        response["observation"].clone()
    };

    let mut initial = serde_json::Map::new();
    for recipient in &recipients {
        initial.insert(recipient.clone(), observe(&mut peer, &state, recipient));
    }

    let mut steps = Vec::new();
    for step in case["steps"].as_array().cloned().unwrap_or_default() {
        let previous_state = state.clone();
        let result = peer
            .exchange(json!({
                "protocol_version":"0.1.0","request_id":"golden","operation":"execute",
                "ruleset":case["ruleset"],"state":previous_state,"command":step["command"],
                "random":step["random"]
            }))
            .expect("execute");
        if result["status"] == "accepted" {
            state = result["state"].clone();
        }
        let events = result.get("events").cloned().unwrap_or_else(|| json!([]));

        let mut observations = serde_json::Map::new();
        let mut observed_events = serde_json::Map::new();
        for recipient in &recipients {
            observations.insert(recipient.clone(), observe(&mut peer, &state, recipient));
            let response = peer
                .exchange(json!({
                    "protocol_version":"0.1.0","request_id":"golden","operation":"observe-events",
                    "ruleset":case["ruleset"],"state":previous_state,"next_state":state,
                    "events":events,"recipient":recipient
                }))
                .expect("observe-events");
            assert_eq!(
                response["status"], "ok",
                "observe-events failed: {response}"
            );
            observed_events.insert(recipient.clone(), response["observed_events"].clone());
        }
        steps.push(json!({
            "id": step["id"],
            "status": result["status"],
            "observations": observations,
            "observed_events": observed_events,
        }));
    }

    json!({ "initial_observations": initial, "steps": steps })
}

/// FNV-1a. Inlined rather than pulled in as a dependency: this crate
/// deliberately depends only on serialization support, and the digest only has
/// to be stable, not fast or cryptographic.
fn digest(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn count_observed_events(projection: &Value) -> usize {
    projection["steps"]
        .as_array()
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| step["observed_events"].as_object())
                .flat_map(|per_recipient| per_recipient.values())
                .filter_map(Value::as_array)
                .map(Vec::len)
                .sum()
        })
        .unwrap_or(0)
}

fn count_observations(projection: &Value) -> usize {
    let initial = projection["initial_observations"]
        .as_object()
        .map_or(0, serde_json::Map::len);
    let stepwise: usize = projection["steps"]
        .as_array()
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| step["observations"].as_object())
                .map(serde_json::Map::len)
                .sum()
        })
        .unwrap_or(0);
    initial + stepwise
}

/// Every fixture of the corpus, in path order, equivalence cases included.
fn all_fixtures() -> Vec<(String, Value)> {
    let root = fixture_root();
    let mut files = Vec::new();
    collect_json(&root, &mut files).expect("walk fixture root");
    files.sort();
    files
        .into_iter()
        .map(|file| {
            let case: Value =
                serde_json::from_str(&std::fs::read_to_string(&file).expect("read fixture"))
                    .expect("parse fixture");
            let relative = file
                .strip_prefix(&root)
                .expect("fixture under root")
                .to_string_lossy()
                .replace('\\', "/");
            (relative, case)
        })
        .collect()
}

fn fixtures() -> Vec<(String, Value)> {
    all_fixtures()
        .into_iter()
        // Equivalence cases assert two sides against each other rather than
        // against a literal; they have no single projection to record.
        .filter(|(_, case)| case.get("left").is_none())
        .collect()
}

#[test]
fn observation_digests_are_unchanged() {
    let mut table = String::from(
        "# AWVM observation goldens. One line per fixture, in path order.\n\
         # Regenerate with AWVM_UPDATE_GOLDENS=1 cargo test -p awvm\n\
         # <digest>  obs=<observations> ev=<observed events>  <fixture>\n",
    );
    for (relative, case) in fixtures() {
        let projection = project(&case);
        let canonical = projection.to_string();
        writeln!(
            table,
            "{:016x}  obs={} ev={}  {relative}",
            digest(&canonical),
            count_observations(&projection),
            count_observed_events(&projection),
        )
        .expect("write table");
    }

    let path = goldens().join("observations.txt");
    if updating() {
        std::fs::create_dir_all(path.parent().expect("goldens dir")).expect("create goldens dir");
        std::fs::write(&path, &table).expect("write goldens");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing {}: {error}\nregenerate with AWVM_UPDATE_GOLDENS=1 cargo test -p awvm",
            path.display()
        )
    });
    assert_eq!(
        expected, table,
        "observation projections changed.\n\
         If intended, regenerate with AWVM_UPDATE_GOLDENS=1 cargo test -p awvm.\n\
         To see what changed in one fixture: \
         AWVM_GOLDEN_DETAIL=<path substring> cargo test -p awvm --test observations"
    );
}

#[test]
fn detailed_projections_are_unchanged() {
    let filter = std::env::var("AWVM_GOLDEN_DETAIL").ok();
    let root = goldens().join("detail");
    let mut checked = 0usize;

    for (relative, case) in fixtures() {
        let selected = match &filter {
            Some(needle) => relative.contains(needle.as_str()),
            None => in_detail_set(&relative, &case),
        };
        if !selected {
            continue;
        }
        checked += 1;

        let projection = project(&case);
        let rendered = format!(
            "{}\n",
            serde_json::to_string_pretty(&projection).expect("render projection")
        );
        let path = root.join(&relative);

        if updating() || filter.is_some() {
            std::fs::create_dir_all(path.parent().expect("detail dir")).expect("create detail dir");
            std::fs::write(&path, &rendered).expect("write detail golden");
            continue;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "missing {}: {error}\nregenerate with AWVM_UPDATE_GOLDENS=1 cargo test -p awvm",
                path.display()
            )
        });
        assert_eq!(expected, rendered, "projection changed for {relative}");
    }

    // Guards against `in_detail_set` silently matching nothing, which would
    // leave this test passing while checking no projections at all. The set is
    // 31 fixtures today; the floor is loose so that adding or moving one does
    // not require touching this number.
    if filter.is_none() {
        assert!(
            checked >= 25,
            "detail set collapsed to {checked} fixtures; it should cover fog, concealment, \
             visibility, and every fixture asserting observations"
        );
    }
}

/// Every state of the corpus, projected to every recipient it names.
///
/// The steps are not replayed here: a fixture's initial states already cover
/// every board shape, roster size and fog setting the corpus holds, which is
/// what the buffer below has to survive.
fn corpus_states() -> Vec<(String, State)> {
    let mut states = Vec::new();
    for (relative, case) in all_fixtures() {
        for key in ["initial_state", "left", "right"] {
            let Some(value) = case.get(key) else {
                continue;
            };
            let value = value.get("initial_state").unwrap_or(value);
            if let Ok(state) = serde_json::from_value::<State>(value.clone()) {
                states.push((format!("{relative}:{key}"), state));
            }
        }
    }
    states
}

/// `observe_into` answers what `observe` answers, into a buffer that is never
/// emptied between fixtures.
///
/// One buffer walks the whole corpus, so each projection is written over a
/// different board, a different roster and a different weather than the one it
/// is about. A field the fill forgets to overwrite therefore reads as the
/// previous fixture's value, and the comparison against a freshly built
/// projection is what catches it. Emptying the buffer for each case would
/// prove nothing but that the fill can write to an empty vector.
#[test]
fn a_reused_projection_answers_what_a_fresh_one_answers() {
    let states = corpus_states();
    assert!(states.len() > 100, "the corpus should hold real coverage");

    let mut buffer: Option<Observation> = None;
    let mut compared = 0;
    for (name, state) in &states {
        for player in state.players.iter() {
            let recipient = player.id().clone();
            let expected = observe(&AwbwVisibility, state, &recipient).expect("a seated recipient");
            match &mut buffer {
                Some(reused) => {
                    observe_into(&AwbwVisibility, state, &recipient, reused)
                        .expect("a seated recipient");
                    assert_eq!(*reused, expected, "{name}: reused projection differs");
                }
                None => {
                    buffer = Some(
                        observe(&AwbwVisibility, state, &recipient).expect("a seated recipient"),
                    );
                }
            }
            compared += 1;
        }
    }
    assert!(compared > 200, "compared only {compared} projections");
}

/// Each field of a reused projection is written from the position in front of
/// it and not left over from the position before it.
///
/// The corpus walk above cannot prove this on its own. It only ever catches a
/// forgotten field when some fixture disagrees with the fixture before it, and
/// the fixtures agree about most of what an observation carries — every one of
/// them is clear weather on day one of an unfinished match. So this changes one
/// field at a time, projects the changed position into a buffer that holds the
/// unchanged one, and asks whether the change came through.
#[test]
fn a_reused_projection_carries_no_field_of_the_position_before_it() {
    let (name, base) = corpus_states()
        .into_iter()
        .find(|(_, state)| state.players.iter().count() > 1 && state.units.iter().count() > 0)
        .expect("the corpus seats a two-player match with units");
    let recipient = base.players.iter().next().expect("a seat").id().clone();

    /// A named change to one field of a state.
    type Mutation = (&'static str, fn(&mut State));

    let mutations: Vec<Mutation> = vec![
        ("ruleset", |state| {
            state.ruleset.revision = RulesetRevision::from("some-other-revision");
        }),
        ("settings", |state| {
            state.settings.day_limit = Some(state.settings.day_limit.unwrap_or(0) + 7);
        }),
        ("board", |state| {
            let tile = state
                .board
                .get_mut(Pos::new(0, 0))
                .expect("every board holds an origin");
            tile.capture_points = Some(tile.capture_points.unwrap_or(0) + 3);
        }),
        ("turn", |state| {
            state.turn.day += 3;
        }),
        ("weather", |state| {
            state.weather.remaining_turns += 5;
        }),
        ("units", |state| {
            state.units.remove(0);
        }),
        ("match", |state| {
            state.match_state = Match::Finished {
                outcome: Outcome::Draw {
                    teams: state.teams.iter().map(|team| team.id.clone()).collect(),
                    reason: DrawReason::Agreement,
                },
            };
        }),
    ];

    for (field, mutate) in mutations {
        let mut changed = base.clone();
        mutate(&mut changed);
        let expected = observe(&AwbwVisibility, &changed, &recipient).expect("a seated recipient");
        assert_ne!(
            expected,
            observe(&AwbwVisibility, &base, &recipient).expect("a seated recipient"),
            "{name}: changing {field} changed no projection, so it pins nothing",
        );

        let mut buffer = observe(&AwbwVisibility, &base, &recipient).expect("a seated recipient");
        observe_into(&AwbwVisibility, &changed, &recipient, &mut buffer)
            .expect("a seated recipient");
        assert_eq!(buffer, expected, "{name}: {field} was not written");
    }

    // The recipient is the one input that is not a field of the state.
    let other = base.players.get(1).expect("a second seat").id().clone();
    let expected = observe(&AwbwVisibility, &base, &other).expect("a seated recipient");
    let mut buffer = observe(&AwbwVisibility, &base, &recipient).expect("a seated recipient");
    assert_ne!(buffer, expected, "{name}: the two seats see the same thing");
    observe_into(&AwbwVisibility, &base, &other, &mut buffer).expect("a seated recipient");
    assert_eq!(buffer, expected, "{name}: the recipient was not written");
}

/// A refused projection leaves the buffer empty rather than half written.
#[test]
fn a_refused_projection_empties_the_buffer_it_was_given() {
    let (_, state) = corpus_states()
        .into_iter()
        .find(|(_, state)| state.players.iter().next().is_some())
        .expect("the corpus seats a player");
    let recipient = state.players.iter().next().expect("a seat").id().clone();
    let mut buffer = observe(&AwbwVisibility, &state, &recipient).expect("a seated recipient");
    assert!(buffer.board.width() > 0);

    let stranger = PlayerId::from("nobody-on-this-roster");
    let error = observe_into(&AwbwVisibility, &state, &stranger, &mut buffer)
        .expect_err("a recipient off the roster cannot be projected to");
    assert!(matches!(error, ObserveError::UnknownRecipient(_)));
    assert_eq!(
        buffer.board.width(),
        0,
        "the board should have been emptied"
    );
    assert!(buffer.units.is_empty());
    assert!(buffer.players.is_empty());
}
