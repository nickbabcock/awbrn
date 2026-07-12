use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

fn main() {
    match run() {
        Ok(summary) if summary.failed == 0 => {
            println!(
                "PASS: {} assertions; {} cases skipped",
                summary.passed, summary.skipped
            );
        }
        Ok(summary) => {
            eprintln!(
                "FAIL: {} assertions passed, {} failed, {} cases skipped",
                summary.passed, summary.failed, summary.skipped
            );
            std::process::exit(1);
        }
        Err(message) => {
            eprintln!("ERROR: {message}");
            std::process::exit(2);
        }
    }
}

#[derive(Default)]
struct Summary {
    passed: usize,
    failed: usize,
    skipped: usize,
}

fn run() -> Result<Summary, String> {
    let mut args = env::args().skip(1);
    let implementation = args
        .next()
        .ok_or("usage: awvm-conformance <implementation-executable> [fixture-root]")?;
    let root = PathBuf::from(args.next().unwrap_or_else(|| "spec/fixtures".into()));
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let mut peer = Peer::spawn(&implementation)?;
    let capabilities = peer.exchange(
        json!({"protocol_version":"0.1.0","request_id":"capabilities","operation":"capabilities"}),
    )?;
    if capabilities["status"] != "ok" {
        return Err(format!("capabilities failed: {capabilities}"));
    }
    let features: HashSet<&str> = capabilities["features"]
        .as_array()
        .ok_or("capabilities.features is not an array")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut files = Vec::new();
    collect_json(&root, &mut files)?;
    files.sort();
    let mut summary = Summary::default();
    for file in files {
        let case: Value = serde_json::from_str(
            &fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?,
        )
        .map_err(|e| format!("{}: {e}", file.display()))?;
        let Some(feature) = case["feature"].as_str() else {
            return Err(format!("{}: missing feature", file.display()));
        };
        if !is_supported(feature, &features) {
            summary.skipped += 1;
            println!("SKIP {} ({feature})", case["id"]);
            continue;
        }
        if case.get("left").is_some() {
            run_equivalence_case(&mut peer, &case, &mut summary)?;
            continue;
        }
        let steps = case["steps"]
            .as_array()
            .ok_or_else(|| format!("{}: steps is not an array", file.display()))?;
        let mut state = case["initial_state"].clone();
        check_literal_observations(
            &mut peer,
            &case,
            &state,
            case.get("initial_observations"),
            &format!("{}/initial", case["id"].as_str().unwrap_or("case")),
            &mut summary,
        )?;
        for step in steps {
            let request_id = format!(
                "{}/{}",
                case["id"].as_str().unwrap_or("case"),
                step["id"].as_str().unwrap_or("step")
            );
            let previous_state = state.clone();
            let actual=peer.exchange(json!({"protocol_version":"0.1.0","request_id":request_id,"operation":"execute","ruleset":case["ruleset"],"state":previous_state,"command":step["command"],"random":step["random"]}))?;
            let expected = &step["expect"];
            let comparable = match expected["status"].as_str() {
                Some("accepted") => {
                    json!({"status":actual["status"],"state":actual["state"],"events":actual["events"],"random_consumed":actual["random_consumed"]})
                }
                Some("rejected") => {
                    json!({"status":actual["status"],"violation":actual["violation"],"random_consumed":actual["random_consumed"]})
                }
                _ => return Err(format!("{request_id}: invalid expected status")),
            };
            let mut expected_core = expected.clone();
            if let Some(object) = expected_core.as_object_mut() {
                object.remove("observations");
                object.remove("observed_events");
            }
            if comparable == expected_core {
                summary.passed += 1;
                println!("PASS {request_id}");
                if expected["status"] == "accepted" {
                    state = actual["state"].clone();
                }
            } else {
                summary.failed += 1;
                eprintln!(
                    "FAIL {request_id}: {}",
                    first_difference(&expected_core, &comparable, "$")
                );
            }
            check_literal_observations(
                &mut peer,
                &case,
                &state,
                expected.get("observations"),
                &format!("{request_id}/observation"),
                &mut summary,
            )?;
            if let Some(expected_events) =
                expected.get("observed_events").and_then(Value::as_object)
            {
                for (recipient, expected_projection) in expected_events {
                    let response = peer.exchange(json!({
                        "protocol_version":"0.1.0",
                        "request_id":format!("{request_id}/events/{recipient}"),
                        "operation":"observe-events",
                        "ruleset":case["ruleset"],
                        "state":previous_state,
                        "next_state":state,
                        "events":actual.get("events").cloned().unwrap_or_else(|| json!([])),
                        "recipient":recipient
                    }))?;
                    compare(
                        expected_projection,
                        &response["observed_events"],
                        &format!("{request_id}/events/{recipient}"),
                        &mut summary,
                    );
                }
            }
        }
    }
    peer.child
        .kill()
        .map_err(|e| format!("stop implementation: {e}"))?;
    let _ = peer.child.wait();
    Ok(summary)
}

fn check_literal_observations(
    peer: &mut Peer,
    case: &Value,
    state: &Value,
    expected: Option<&Value>,
    label: &str,
    summary: &mut Summary,
) -> Result<(), String> {
    let Some(expected) = expected.and_then(Value::as_object) else {
        return Ok(());
    };
    for (recipient, observation) in expected {
        let response = peer.exchange(json!({
            "protocol_version":"0.1.0",
            "request_id":format!("{label}/{recipient}"),
            "operation":"observe",
            "ruleset":case["ruleset"],
            "state":state,
            "recipient":recipient
        }))?;
        compare(
            observation,
            &response["observation"],
            &format!("{label}/{recipient}"),
            summary,
        );
    }
    Ok(())
}

fn run_equivalence_case(
    peer: &mut Peer,
    case: &Value,
    summary: &mut Summary,
) -> Result<(), String> {
    let id = case["id"].as_str().unwrap_or("case");
    let recipient = case["recipient"]
        .as_str()
        .ok_or_else(|| format!("{id}: missing recipient"))?;
    let mut left = case["left"]["initial_state"].clone();
    let mut right = case["right"]["initial_state"].clone();
    compare_equivalence_observations(peer, case, &left, &right, recipient, id, summary)?;

    let left_steps = case["left"]["steps"]
        .as_array()
        .ok_or_else(|| format!("{id}: left steps is not an array"))?;
    let right_steps = case["right"]["steps"]
        .as_array()
        .ok_or_else(|| format!("{id}: right steps is not an array"))?;
    if left_steps.len() != right_steps.len() {
        return Err(format!(
            "{id}: equivalence sides have different step counts"
        ));
    }
    for (index, (left_step, right_step)) in left_steps.iter().zip(right_steps).enumerate() {
        let left_pre = left.clone();
        let right_pre = right.clone();
        let left_result = peer.exchange(json!({
            "protocol_version":"0.1.0", "request_id":format!("{id}/left/{index}"),
            "operation":"execute", "ruleset":case["ruleset"], "state":left_pre,
            "command":left_step["command"], "random":left_step["random"]
        }))?;
        let right_result = peer.exchange(json!({
            "protocol_version":"0.1.0", "request_id":format!("{id}/right/{index}"),
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
        compare_equivalence_observations(
            peer,
            case,
            &left,
            &right,
            recipient,
            &format!("{id}/{index}"),
            summary,
        )?;
        if case["assert"] == "equal-observations-and-events" {
            let left_projection = observe_transition(
                peer,
                case,
                &left_pre,
                &left,
                &left_events,
                recipient,
                &format!("{id}/left-events/{index}"),
            )?;
            let right_projection = observe_transition(
                peer,
                case,
                &right_pre,
                &right,
                &right_events,
                recipient,
                &format!("{id}/right-events/{index}"),
            )?;
            compare(
                &left_projection,
                &right_projection,
                &format!("{id}/{index}/event-equivalence"),
                summary,
            );
        }
    }
    Ok(())
}

fn compare_equivalence_observations(
    peer: &mut Peer,
    case: &Value,
    left: &Value,
    right: &Value,
    recipient: &str,
    label: &str,
    summary: &mut Summary,
) -> Result<(), String> {
    let request = |state: &Value, side: &str| {
        json!({
            "protocol_version":"0.1.0", "request_id":format!("{label}/{side}"),
            "operation":"observe", "ruleset":case["ruleset"], "state":state,
            "recipient":recipient
        })
    };
    let left_observation = peer.exchange(request(left, "left-observation"))?;
    let right_observation = peer.exchange(request(right, "right-observation"))?;
    if left_observation["status"] != "ok" || right_observation["status"] != "ok" {
        return Err(format!(
            "{label}: observation operation failed: left={left_observation}, right={right_observation}"
        ));
    }
    compare(
        &left_observation["observation"],
        &right_observation["observation"],
        &format!("{label}/observation-equivalence"),
        summary,
    );
    Ok(())
}

fn observe_transition(
    peer: &mut Peer,
    case: &Value,
    state: &Value,
    next_state: &Value,
    events: &Value,
    recipient: &str,
    request_id: &str,
) -> Result<Value, String> {
    let response = peer.exchange(json!({
        "protocol_version":"0.1.0", "request_id":request_id,
        "operation":"observe-events", "ruleset":case["ruleset"],
        "state":state, "next_state":next_state, "events":events,
        "recipient":recipient
    }))?;
    if response["status"] != "ok" {
        return Err(format!("{request_id}: observe-events failed: {response}"));
    }
    Ok(response["observed_events"].clone())
}

fn compare(expected: &Value, actual: &Value, label: &str, summary: &mut Summary) {
    if expected == actual {
        summary.passed += 1;
        println!("PASS {label}");
    } else {
        summary.failed += 1;
        eprintln!("FAIL {label}: {}", first_difference(expected, actual, "$"));
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

fn collect_json(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))? {
        let p = entry.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            collect_json(&p, out)?
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p)
        }
    }
    Ok(())
}

fn first_difference(expected: &Value, actual: &Value, path: &str) -> String {
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

struct Peer {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
}
impl Peer {
    fn spawn(program: &str) -> Result<Self, String> {
        let mut child = Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("start {program}: {e}"))?;
        let input = BufWriter::new(child.stdin.take().unwrap());
        let output = BufReader::new(child.stdout.take().unwrap());
        Ok(Self {
            child,
            input,
            output,
        })
    }
    fn exchange(&mut self, request: Value) -> Result<Value, String> {
        serde_json::to_writer(&mut self.input, &request).map_err(|e| e.to_string())?;
        writeln!(&mut self.input).map_err(|e| e.to_string())?;
        self.input.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        if self
            .output
            .read_line(&mut line)
            .map_err(|e| e.to_string())?
            == 0
        {
            return Err("implementation closed stdout".into());
        }
        serde_json::from_str(&line).map_err(|e| format!("invalid response: {e}: {line}"))
    }
}
