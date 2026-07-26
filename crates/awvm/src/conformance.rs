//! Fixture case playback for the conformance protocol.
//!
//! The runner talks to an implementation through [`Peer`], a single
//! request/response exchange. [`Subprocess`] drives an external adapter over
//! the JSON Lines protocol; [`InProcess`] calls this crate's own
//! [`crate::protocol::handle`]. Both go through the same protocol envelopes, so
//! the in-process test path cannot drift from what an external implementation
//! sees.

use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

use crate::protocol::{self, PROTOCOL_VERSION};

#[derive(Debug, thiserror::Error)]
pub enum ConformanceError {
    #[error("start {program}: {source}")]
    Start {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("stop implementation: {0}")]
    Stop(#[source] std::io::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("invalid response: {source}: {line}")]
    InvalidResponse {
        #[source]
        source: serde_json::Error,
        line: String,
    },
    #[error("{}: {source}", path.display())]
    FixtureIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{}: {source}", path.display())]
    FixtureJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{0}")]
    Message(String),
}

impl From<String> for ConformanceError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for ConformanceError {
    fn from(message: &str) -> Self {
        Self::Message(message.into())
    }
}

/// One request/response exchange with an implementation under test.
pub trait Peer {
    fn exchange(&mut self, request: Value) -> Result<Value, ConformanceError>;
}

/// An external adapter driven over the JSON Lines protocol.
pub struct Subprocess {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Subprocess {
    pub fn spawn(program: &str) -> Result<Self, ConformanceError> {
        let mut child = Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|source| ConformanceError::Start {
                program: program.into(),
                source,
            })?;
        let input = BufWriter::new(child.stdin.take().unwrap());
        let output = BufReader::new(child.stdout.take().unwrap());
        Ok(Self {
            child,
            input,
            output,
        })
    }

    pub fn shutdown(mut self) -> Result<(), ConformanceError> {
        self.child.kill().map_err(ConformanceError::Stop)?;
        let _ = self.child.wait();
        Ok(())
    }
}

impl Peer for Subprocess {
    fn exchange(&mut self, request: Value) -> Result<Value, ConformanceError> {
        serde_json::to_writer(&mut self.input, &request)?;
        writeln!(&mut self.input)?;
        self.input.flush()?;
        let mut line = String::new();
        if self.output.read_line(&mut line)? == 0 {
            return Err(ConformanceError::Message(
                "implementation closed stdout".into(),
            ));
        }
        serde_json::from_str(&line)
            .map_err(|source| ConformanceError::InvalidResponse { source, line })
    }
}

/// This crate's own implementation, without a subprocess.
///
/// The request is serialized and handed to [`crate::protocol::handle`] rather
/// than dispatched directly to `execute`/`observe`, so that a fixture run
/// exercises the same decoding an external adapter would.
#[derive(Default)]
pub struct InProcess;

impl Peer for InProcess {
    fn exchange(&mut self, request: Value) -> Result<Value, ConformanceError> {
        // A runner compares a response against a fixture's JSON, so this is the
        // one caller that wants a tree rather than the bytes the adapter writes.
        Ok(serde_json::to_value(protocol::handle(&request.to_string()))
            .expect("a protocol response is representable as JSON"))
    }
}

/// How a run reports progress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Progress {
    /// Print one line per assertion and per skipped case, as the binary does.
    Verbose,
    /// Print nothing; collect failures into the summary.
    #[default]
    Silent,
}

#[derive(Debug, Default)]
pub struct Summary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    /// One entry per failed assertion, in encounter order.
    pub failures: Vec<String>,
}

impl Summary {
    fn pass(&mut self, label: &str, progress: Progress) {
        self.passed += 1;
        if progress == Progress::Verbose {
            println!("PASS {label}");
        }
    }

    fn fail(&mut self, message: String, progress: Progress) {
        self.failed += 1;
        if progress == Progress::Verbose {
            eprintln!("FAIL {message}");
        }
        self.failures.push(message);
    }
}

struct Run<'a> {
    peer: &'a mut dyn Peer,
    progress: Progress,
    summary: Summary,
}

/// Run every fixture under `root` against `peer`.
pub fn run(
    peer: &mut dyn Peer,
    root: &Path,
    progress: Progress,
) -> Result<Summary, ConformanceError> {
    let capabilities = peer.exchange(
        json!({"protocol_version":PROTOCOL_VERSION,"request_id":"capabilities","operation":"capabilities"}),
    )?;
    if capabilities["status"] != "ok" {
        return Err(format!("capabilities failed: {capabilities}").into());
    }
    let features: HashSet<&str> = capabilities["features"]
        .as_array()
        .ok_or("capabilities.features is not an array")?
        .iter()
        .filter_map(Value::as_str)
        .collect();

    let mut files = Vec::new();
    collect_json(root, &mut files)?;
    files.sort();

    let mut run = Run {
        peer,
        progress,
        summary: Summary::default(),
    };

    for file in files {
        let source = fs::read_to_string(&file).map_err(|source| ConformanceError::FixtureIo {
            path: file.clone(),
            source,
        })?;
        let case: Value =
            serde_json::from_str(&source).map_err(|source| ConformanceError::FixtureJson {
                path: file.clone(),
                source,
            })?;
        let Some(feature) = case["feature"].as_str() else {
            return Err(format!("{}: missing feature", file.display()).into());
        };
        if !is_supported(feature, &features) {
            run.summary.skipped += 1;
            if run.progress == Progress::Verbose {
                println!("SKIP {} ({feature})", case["id"]);
            }
            continue;
        }
        if case.get("left").is_some() {
            run.equivalence_case(&case)?;
            continue;
        }
        run.sequence_case(&case, &file)?;
    }

    Ok(run.summary)
}

impl Run<'_> {
    fn sequence_case(&mut self, case: &Value, file: &Path) -> Result<(), ConformanceError> {
        let steps = case["steps"]
            .as_array()
            .ok_or_else(|| format!("{}: steps is not an array", file.display()))?;
        let mut state = case["initial_state"].clone();
        self.literal_observations(
            case,
            &state,
            case.get("initial_observations"),
            &format!("{}/initial", case["id"].as_str().unwrap_or("case")),
        )?;
        for step in steps {
            let request_id = format!(
                "{}/{}",
                case["id"].as_str().unwrap_or("case"),
                step["id"].as_str().unwrap_or("step")
            );
            let previous_state = state.clone();
            let actual = self.peer.exchange(json!({
                "protocol_version":PROTOCOL_VERSION,"request_id":request_id,"operation":"execute",
                "ruleset":case["ruleset"],"state":previous_state,"command":step["command"],
                "random":step["random"]
            }))?;
            let expected = &step["expect"];
            let comparable = match expected["status"].as_str() {
                Some("accepted") => {
                    json!({"status":actual["status"],"state":actual["state"],"events":actual["events"],"random_consumed":actual["random_consumed"]})
                }
                Some("rejected") => {
                    json!({"status":actual["status"],"violation":actual["violation"],"random_consumed":actual["random_consumed"]})
                }
                _ => return Err(format!("{request_id}: invalid expected status").into()),
            };
            let mut expected_core = expected.clone();
            if let Some(object) = expected_core.as_object_mut() {
                object.remove("observations");
                object.remove("observed_events");
            }
            if comparable == expected_core {
                self.summary.pass(&request_id, self.progress);
                if expected["status"] == "accepted" {
                    state = actual["state"].clone();
                }
            } else {
                let message = format!(
                    "{request_id}: {}",
                    first_difference(&expected_core, &comparable, "$")
                );
                self.summary.fail(message, self.progress);
            }
            self.literal_observations(
                case,
                &state,
                expected.get("observations"),
                &format!("{request_id}/observation"),
            )?;
            if let Some(expected_events) =
                expected.get("observed_events").and_then(Value::as_object)
            {
                for (recipient, expected_projection) in expected_events {
                    let response = self.peer.exchange(json!({
                        "protocol_version":PROTOCOL_VERSION,
                        "request_id":format!("{request_id}/events/{recipient}"),
                        "operation":"observe-events",
                        "ruleset":case["ruleset"],
                        "state":previous_state,
                        "next_state":state,
                        "events":actual.get("events").cloned().unwrap_or_else(|| json!([])),
                        "recipient":recipient
                    }))?;
                    self.compare(
                        expected_projection,
                        &response["observed_events"],
                        &format!("{request_id}/events/{recipient}"),
                    );
                }
            }
        }
        Ok(())
    }

    fn literal_observations(
        &mut self,
        case: &Value,
        state: &Value,
        expected: Option<&Value>,
        label: &str,
    ) -> Result<(), ConformanceError> {
        let Some(expected) = expected.and_then(Value::as_object) else {
            return Ok(());
        };
        for (recipient, observation) in expected {
            let response = self.peer.exchange(json!({
                "protocol_version":PROTOCOL_VERSION,
                "request_id":format!("{label}/{recipient}"),
                "operation":"observe",
                "ruleset":case["ruleset"],
                "state":state,
                "recipient":recipient
            }))?;
            self.compare(
                observation,
                &response["observation"],
                &format!("{label}/{recipient}"),
            );
        }
        Ok(())
    }

    fn equivalence_case(&mut self, case: &Value) -> Result<(), ConformanceError> {
        let id = case["id"].as_str().unwrap_or("case");
        let recipient = case["recipient"]
            .as_str()
            .ok_or_else(|| format!("{id}: missing recipient"))?;
        let mut left = case["left"]["initial_state"].clone();
        let mut right = case["right"]["initial_state"].clone();
        self.equivalence_observations(case, &left, &right, recipient, id)?;

        let left_steps = case["left"]["steps"]
            .as_array()
            .ok_or_else(|| format!("{id}: left steps is not an array"))?
            .clone();
        let right_steps = case["right"]["steps"]
            .as_array()
            .ok_or_else(|| format!("{id}: right steps is not an array"))?
            .clone();
        if left_steps.len() != right_steps.len() {
            return Err(format!("{id}: equivalence sides have different step counts").into());
        }
        for (index, (left_step, right_step)) in left_steps.iter().zip(&right_steps).enumerate() {
            let left_pre = left.clone();
            let right_pre = right.clone();
            let left_result = self.peer.exchange(json!({
                "protocol_version":PROTOCOL_VERSION, "request_id":format!("{id}/left/{index}"),
                "operation":"execute", "ruleset":case["ruleset"], "state":left_pre,
                "command":left_step["command"], "random":left_step["random"]
            }))?;
            let right_result = self.peer.exchange(json!({
                "protocol_version":PROTOCOL_VERSION, "request_id":format!("{id}/right/{index}"),
                "operation":"execute", "ruleset":case["ruleset"], "state":right_pre,
                "command":right_step["command"], "random":right_step["random"]
            }))?;
            let left_events = left_result
                .get("events")
                .cloned()
                .unwrap_or_else(|| json!([]));
            let right_events = right_result
                .get("events")
                .cloned()
                .unwrap_or_else(|| json!([]));
            if left_result["status"] == "accepted" {
                left = left_result["state"].clone();
            }
            if right_result["status"] == "accepted" {
                right = right_result["state"].clone();
            }
            self.equivalence_observations(
                case,
                &left,
                &right,
                recipient,
                &format!("{id}/{index}"),
            )?;
            if case["assert"] == "equal-observations-and-events" {
                let left_projection = self.observe_transition(
                    case,
                    &left_pre,
                    &left,
                    &left_events,
                    recipient,
                    &format!("{id}/left-events/{index}"),
                )?;
                let right_projection = self.observe_transition(
                    case,
                    &right_pre,
                    &right,
                    &right_events,
                    recipient,
                    &format!("{id}/right-events/{index}"),
                )?;
                self.compare(
                    &left_projection,
                    &right_projection,
                    &format!("{id}/{index}/event-equivalence"),
                );
            }
        }
        Ok(())
    }

    fn equivalence_observations(
        &mut self,
        case: &Value,
        left: &Value,
        right: &Value,
        recipient: &str,
        label: &str,
    ) -> Result<(), ConformanceError> {
        let request = |state: &Value, side: &str| {
            json!({
                "protocol_version":PROTOCOL_VERSION, "request_id":format!("{label}/{side}"),
                "operation":"observe", "ruleset":case["ruleset"], "state":state,
                "recipient":recipient
            })
        };
        let left_observation = self.peer.exchange(request(left, "left-observation"))?;
        let right_observation = self.peer.exchange(request(right, "right-observation"))?;
        if left_observation["status"] != "ok" || right_observation["status"] != "ok" {
            return Err(format!(
                "{label}: observation operation failed: left={left_observation}, right={right_observation}"
            )
            .into());
        }
        self.compare(
            &left_observation["observation"],
            &right_observation["observation"],
            &format!("{label}/observation-equivalence"),
        );
        Ok(())
    }

    fn observe_transition(
        &mut self,
        case: &Value,
        state: &Value,
        next_state: &Value,
        events: &Value,
        recipient: &str,
        request_id: &str,
    ) -> Result<Value, ConformanceError> {
        let response = self.peer.exchange(json!({
            "protocol_version":PROTOCOL_VERSION, "request_id":request_id,
            "operation":"observe-events", "ruleset":case["ruleset"],
            "state":state, "next_state":next_state, "events":events,
            "recipient":recipient
        }))?;
        if response["status"] != "ok" {
            return Err(format!("{request_id}: observe-events failed: {response}").into());
        }
        Ok(response["observed_events"].clone())
    }

    fn compare(&mut self, expected: &Value, actual: &Value, label: &str) {
        if expected == actual {
            self.summary.pass(label, self.progress);
        } else {
            let message = format!("{label}: {}", first_difference(expected, actual, "$"));
            self.summary.fail(message, self.progress);
        }
    }
}

/// A case runs when its feature ID equals an advertised ID or descends from one
/// on a segment boundary, so advertising `elimination-v1` claims the whole
/// subtree beneath it. See `spec/protocol.md`.
fn is_supported(feature: &str, advertised: &HashSet<&str>) -> bool {
    feature
        .match_indices('.')
        .any(|(i, _)| advertised.contains(&feature[..i]))
        || advertised.contains(feature)
}

pub fn collect_json(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), ConformanceError> {
    let entries = fs::read_dir(path).map_err(|source| ConformanceError::FixtureIo {
        path: path.into(),
        source,
    })?;
    for entry in entries {
        let p = entry?.path();
        if p.is_dir() {
            collect_json(&p, out)?
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p)
        }
    }
    Ok(())
}

pub fn first_difference(expected: &Value, actual: &Value, path: &str) -> String {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            for (k, v) in e {
                let p = format!("{path}.{k}");
                match a.get(k) {
                    Some(x) if x != v => return first_difference(v, x, &p),
                    None => return format!("{p}: missing; expected {v}"),
                    _ => {}
                }
            }
            for k in a.keys() {
                if !e.contains_key(k) {
                    return format!("{path}.{k}: unexpected value {}", a[k]);
                }
            }
            format!("{path}: values differ")
        }
        (Value::Array(e), Value::Array(a)) => {
            for (i, (x, y)) in e.iter().zip(a).enumerate() {
                if x != y {
                    return first_difference(x, y, &format!("{path}[{i}]"));
                }
            }
            if e.len() != a.len() {
                format!("{path}: expected {} items, got {}", e.len(), a.len())
            } else {
                format!("{path}: arrays differ")
            }
        }
        _ => format!("{path}: expected {expected}, got {actual}"),
    }
}
