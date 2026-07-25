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

fn fixtures() -> Vec<(String, Value)> {
    let root = fixture_root();
    let mut files = Vec::new();
    collect_json(&root, &mut files).expect("walk fixture root");
    files.sort();
    files
        .into_iter()
        .filter_map(|file| {
            let case: Value =
                serde_json::from_str(&std::fs::read_to_string(&file).expect("read fixture"))
                    .expect("parse fixture");
            // Equivalence cases assert two sides against each other rather than
            // against a literal; they have no single projection to record.
            if case.get("left").is_some() {
                return None;
            }
            let relative = file
                .strip_prefix(&root)
                .expect("fixture under root")
                .to_string_lossy()
                .replace('\\', "/");
            Some((relative, case))
        })
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
