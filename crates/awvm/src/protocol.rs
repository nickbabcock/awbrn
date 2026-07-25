//! The JSON Lines conformance protocol defined by `spec/protocol.md`.
//!
//! [`handle`] turns one request line into one response value. It is the single
//! entry point for both the `awvm-jsonl` adapter binary and the in-process
//! conformance backend in [`crate::conformance`], so a test run cannot exercise
//! a different path than an external implementation sees.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::semantic::{self, AwbwVisibility, PlayerId, RulesetRef, State};
use crate::transition::{self, Command, ExecuteError};

pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Feature paths this implementation claims. Listing an ID claims its entire
/// subtree, as defined in `spec/protocol.md`.
pub const FEATURES: &[&str] = &[
    "movement-v1.move-wait.plain",
    "movement-v1.capture-reset",
    "movement-v1.validation",
    "lab-capture-v1",
    "capture-limit-v1",
    "day-limit-v1",
    "combat-neutral-v1",
    "combat-movement-v1",
    "combat-power-charge-v1",
    "combat-tile-target-v1",
    "combat-visibility-v1",
    "move-launch-v1",
    "move-explode-v1",
    "delete-unit-v1",
    "tag-v1",
    "commander-combat-v1.flak.day-to-day",
    "commander-combat-v1.flak.cop",
    "commander-combat-v1.flak.scop",
    "commander-combat-v1.hawke.day-to-day",
    "commander-combat-v1.hawke.cop",
    "commander-combat-v1.hawke.scop",
    "commander-combat-v1.grimm.scop",
    "commander-combat-v1.grimm.day-to-day",
    "commander-combat-v1.grimm.cop",
    "commander-combat-v1.eagle.cop",
    "commander-combat-v1.eagle.scop",
    "commander-combat-v1.eagle.day-to-day",
    "commander-combat-v1.colin.scop",
    "commander-combat-v1.colin.cop",
    "commander-combat-v1.colin.day-to-day",
    "commander-combat-v1.hachi.day-to-day",
    "commander-combat-v1.hachi.cop",
    "commander-combat-v1.hachi.scop",
    "commander-combat-v1.jugger.day-to-day",
    "commander-combat-v1.jugger.cop",
    "commander-combat-v1.jugger.scop",
    "commander-combat-v1.koal.day-to-day",
    "commander-combat-v1.koal.cop",
    "commander-combat-v1.koal.scop",
    "commander-combat-v1.kindle.day-to-day",
    "commander-combat-v1.kindle.cop",
    "commander-combat-v1.kindle.scop",
    "commander-combat-v1.lash.day-to-day",
    "commander-combat-v1.lash.cop",
    "commander-combat-v1.lash.scop",
    "commander-combat-v1.nell.day-to-day",
    "commander-combat-v1.nell.cop",
    "commander-combat-v1.nell.scop",
    "commander-combat-v1.max.day-to-day",
    "commander-combat-v1.max.cop",
    "commander-combat-v1.max.scop",
    "commander-combat-v1.olaf.day-to-day",
    "commander-combat-v1.olaf.cop",
    "commander-combat-v1.olaf.scop",
    "commander-combat-v1.rachel.day-to-day",
    "commander-combat-v1.rachel.cop",
    "commander-combat-v1.rachel.scop",
    "commander-combat-v1.sami.day-to-day",
    "commander-combat-v1.sami.cop",
    "commander-combat-v1.sami.scop",
    "commander-combat-v1.sasha.day-to-day",
    "commander-combat-v1.sasha.cop",
    "commander-combat-v1.sasha.scop",
    "commander-effective-values-v1.sasha.day-to-day",
    "commander-effective-values-v1.eagle.day-to-day",
    "commander-effective-values-v1.colin.day-to-day",
    "commander-effective-values-v1.colin.cop",
    "commander-effective-values-v1.colin.scop",
    "commander-effective-values-v1.hachi.scop",
    "commander-effective-values-v1.hachi.day-to-day",
    "commander-effective-values-v1.hachi.cop",
    "commander-effective-values-v1.koal.cop",
    "commander-effective-values-v1.koal.scop",
    "commander-effective-values-v1.lash.cop",
    "commander-effective-values-v1.lash.scop",
    "commander-effective-values-v1.max.day-to-day",
    "commander-effective-values-v1.max.cop",
    "commander-effective-values-v1.max.scop",
    "commander-effective-values-v1.olaf.day-to-day",
    "commander-effective-values-v1.rachel.day-to-day",
    "commander-effective-values-v1.sami.day-to-day",
    "commander-effective-values-v1.sami.cop",
    "commander-effective-values-v1.sami.scop",
    "commander-effective-values-v1.grit.day-to-day",
    "commander-effective-values-v1.sonja.day-to-day",
    "commander-effective-values-v1.sonja.cop",
    "commander-effective-values-v1.sonja.scop",
    "commander-power-v1.adder.cop",
    "commander-power-v1.adder.scop",
    "commander-power-v1.andy.cop",
    "commander-power-v1.andy.scop",
    "commander-power-v1.colin.cop",
    "commander-power-v1.colin.scop",
    "commander-power-v1.eagle.scop",
    "commander-power-v1.eagle.cop",
    "commander-power-v1.flak.cop",
    "commander-power-v1.flak.scop",
    "commander-power-v1.grimm.cop",
    "commander-power-v1.grimm.scop",
    "commander-power-v1.grit.cop",
    "commander-power-v1.grit.scop",
    "commander-power-v1.jake.cop",
    "commander-power-v1.jake.scop",
    "commander-power-v1.javier.cop",
    "commander-power-v1.javier.scop",
    "commander-power-v1.kanbei.cop",
    "commander-power-v1.kanbei.scop",
    "commander-power-v1.jess.cop",
    "commander-power-v1.jess.scop",
    "commander-power-v1.hawke.cop",
    "commander-power-v1.hawke.scop",
    "commander-power-v1.hachi.cop",
    "commander-power-v1.hachi.scop",
    "commander-power-v1.jugger.cop",
    "commander-power-v1.jugger.scop",
    "commander-power-v1.kindle.cop",
    "commander-power-v1.kindle.scop",
    "commander-power-v1.lash.cop",
    "commander-power-v1.lash.scop",
    "commander-power-v1.nell.cop",
    "commander-power-v1.nell.scop",
    "commander-power-v1.max.cop",
    "commander-power-v1.max.scop",
    "commander-power-v1.koal.cop",
    "commander-power-v1.koal.scop",
    "commander-power-v1.olaf.cop",
    "commander-power-v1.olaf.scop",
    "commander-power-v1.sasha.cop",
    "commander-power-v1.sasha.scop",
    "commander-power-v1.drake.cop",
    "commander-power-v1.drake.scop",
    "commander-power-v1.rachel.scop",
    "commander-power-v1.rachel.cop",
    "commander-power-v1.sami.cop",
    "commander-power-v1.sami.scop",
    "commander-power-v1.sensei.cop",
    "commander-power-v1.sensei.scop",
    "commander-power-v1.sonja.cop",
    "commander-power-v1.sonja.scop",
    "commander-combat-v1.sensei.day-to-day",
    "commander-combat-v1.sensei.cop",
    "commander-combat-v1.sensei.scop",
    "commander-effective-values-v1.sensei.day-to-day",
    "commander-combat-v1.sonja.day-to-day",
    "commander-combat-v1.sonja.cop",
    "commander-combat-v1.sonja.scop",
    "commander-combat-v1.sturm.day-to-day",
    "commander-combat-v1.sturm.cop",
    "commander-combat-v1.sturm.scop",
    "commander-power-v1.sturm.cop",
    "commander-power-v1.sturm.scop",
    "commander-power-v1.von-bolt.scop",
    "commander-combat-v1.von-bolt.day-to-day",
    "commander-combat-v1.von-bolt.scop",
    "commander-effective-values-v1.sturm.day-to-day",
    "commander-combat-v1.grit.day-to-day",
    "commander-combat-v1.grit.cop",
    "commander-combat-v1.grit.scop",
    "commander-effective-values-v1.grit.cop",
    "commander-effective-values-v1.grit.scop",
    "commander-combat-v1.jake.day-to-day",
    "commander-combat-v1.jake.cop",
    "commander-combat-v1.jake.scop",
    "commander-effective-values-v1.jake.cop",
    "commander-effective-values-v1.jake.scop",
    "commander-combat-v1.javier.day-to-day",
    "commander-combat-v1.javier.cop",
    "commander-combat-v1.javier.scop",
    "commander-combat-v1.kanbei.day-to-day",
    "commander-combat-v1.kanbei.cop",
    "commander-combat-v1.kanbei.scop",
    "commander-effective-values-v1.kanbei.day-to-day",
    "commander-effective-values-v1.kanbei.cop",
    "commander-effective-values-v1.kanbei.scop",
    "commander-combat-v1.andy.day-to-day",
    "commander-combat-v1.andy.cop",
    "commander-combat-v1.andy.scop",
    "commander-effective-values-v1.andy.scop",
    "commander-combat-v1.adder.day-to-day",
    "commander-combat-v1.adder.cop",
    "commander-combat-v1.adder.scop",
    "commander-effective-values-v1.adder.cop",
    "commander-effective-values-v1.adder.scop",
    "commander-combat-v1.drake.day-to-day",
    "commander-combat-v1.drake.cop",
    "commander-combat-v1.drake.scop",
    "commander-effective-values-v1.drake.day-to-day",
    "commander-combat-v1.jess.day-to-day",
    "commander-combat-v1.jess.cop",
    "commander-combat-v1.jess.scop",
    "commander-effective-values-v1.jess.cop",
    "commander-effective-values-v1.jess.scop",
    "fog-observation-v1",
    "capture-v1",
    "production-v1",
    "lab-unit-production-v1",
    "join-v1",
    "supply-v1",
    "repair-v1",
    "transport-v1",
    "concealment-v1",
    "turn-boundary-v1",
    "turn-hooks-v1",
    "elimination-v1",
];

#[derive(Deserialize)]
struct ExecuteRequest {
    #[serde(rename = "protocol_version")]
    _protocol_version: String,
    request_id: String,
    #[serde(rename = "operation")]
    _operation: String,
    ruleset: RulesetRef,
    state: State,
    command: Value,
    random: Vec<Value>,
}

#[derive(Deserialize)]
struct ObserveRequest {
    #[serde(rename = "protocol_version")]
    _protocol_version: String,
    request_id: String,
    ruleset: RulesetRef,
    state: State,
    recipient: PlayerId,
}

#[derive(Deserialize)]
struct ObserveEventsRequest {
    #[serde(rename = "protocol_version")]
    _protocol_version: String,
    request_id: String,
    ruleset: RulesetRef,
    state: State,
    next_state: State,
    events: Vec<Value>,
    recipient: PlayerId,
}

#[derive(Deserialize)]
struct Envelope {
    protocol_version: String,
    request_id: String,
    operation: String,
}

/// Handle one protocol request line and produce its response value.
pub fn handle(line: &str) -> Value {
    let envelope: Envelope = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return error("", "INVALID_REQUEST", e.to_string()),
    };
    if envelope.protocol_version != PROTOCOL_VERSION {
        return error(
            &envelope.request_id,
            "UNSUPPORTED_PROTOCOL",
            envelope.protocol_version,
        );
    }
    match envelope.operation.as_str() {
        "capabilities" => json!({
            "protocol_version": PROTOCOL_VERSION,
            "request_id": envelope.request_id,
            "status": "ok",
            "features": FEATURES,
        }),
        "execute" => execute(line, &envelope.request_id),
        "observe" => observe(line, &envelope.request_id),
        "observe-events" => observe_events(line, &envelope.request_id),
        _ => error(
            &envelope.request_id,
            "UNSUPPORTED_OPERATION",
            envelope.operation,
        ),
    }
}

fn execute(line: &str, request_id: &str) -> Value {
    let request: ExecuteRequest = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return error(request_id, "INVALID_REQUEST", e.to_string()),
    };
    if request.ruleset != request.state.ruleset {
        return error(
            &request.request_id,
            "RULESET_MISMATCH",
            "request ruleset does not equal state ruleset",
        );
    }
    let command: Command = match serde_json::from_value(request.command) {
        Ok(v) => v,
        Err(e) => return error(&request.request_id, "INVALID_COMMAND", e.to_string()),
    };
    match transition::execute(&request.state, command, &request.random) {
        Ok(result) => {
            json!({"protocol_version":PROTOCOL_VERSION,"request_id":request.request_id,"status":"accepted","state":result.state,"events":result.events,"random_consumed":result.random_consumed})
        }
        Err(ExecuteError::Violation(violation)) => {
            json!({"protocol_version":PROTOCOL_VERSION,"request_id":request.request_id,"status":"rejected","violation":violation,"random_consumed":0})
        }
        Err(ExecuteError::UnsupportedCommand) => error(
            &request.request_id,
            "UNSUPPORTED_COMMAND",
            "command is not implemented by this adapter",
        ),
        Err(ExecuteError::UnsupportedRuleset) => error(
            &request.request_id,
            "UNSUPPORTED_RULESET",
            "only awbw/2026-07-10 is implemented",
        ),
        Err(ExecuteError::InvalidState(message)) => {
            error(&request.request_id, "INVALID_STATE", message)
        }
        Err(ExecuteError::InvalidRandom(message)) => {
            error(&request.request_id, "INVALID_RANDOM", message)
        }
    }
}

fn observe(line: &str, request_id: &str) -> Value {
    let request: ObserveRequest = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error_) => return error(request_id, "INVALID_REQUEST", error_.to_string()),
    };
    if let Err(response) = check_ruleset(&request.ruleset, [&request.state]) {
        return error(request_id, response.0, response.1);
    }
    let rules = AwbwVisibility::default();
    match semantic::observe(&rules, &request.state, &request.recipient) {
        Ok(observation) => json!({
            "protocol_version": PROTOCOL_VERSION,
            "request_id": request.request_id,
            "status": "ok",
            "observation": observation
        }),
        Err(error_) => error(
            &request.request_id,
            "OBSERVATION_ERROR",
            format!("{error_:?}"),
        ),
    }
}

fn observe_events(line: &str, request_id: &str) -> Value {
    let request: ObserveEventsRequest = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error_) => return error(request_id, "INVALID_REQUEST", error_.to_string()),
    };
    if let Err(response) = check_ruleset(&request.ruleset, [&request.state, &request.next_state]) {
        return error(request_id, response.0, response.1);
    }
    let rules = AwbwVisibility::default();
    match semantic::observe_events(
        &rules,
        &request.state,
        &request.next_state,
        &request.events,
        &request.recipient,
    ) {
        Ok(observed_events) => json!({
            "protocol_version": PROTOCOL_VERSION,
            "request_id": request.request_id,
            "status": "ok",
            "observed_events": observed_events
        }),
        Err(error_) => error(
            &request.request_id,
            "OBSERVATION_ERROR",
            format!("{error_:?}"),
        ),
    }
}

fn check_ruleset<'a>(
    ruleset: &RulesetRef,
    states: impl IntoIterator<Item = &'a State>,
) -> Result<(), (&'static str, String)> {
    if ruleset.id != "awbw" || ruleset.revision != "2026-07-10" {
        return Err((
            "UNSUPPORTED_RULESET",
            "only awbw/2026-07-10 is implemented".into(),
        ));
    }
    if states.into_iter().any(|state| state.ruleset != *ruleset) {
        return Err((
            "RULESET_MISMATCH",
            "request ruleset does not equal state ruleset".into(),
        ));
    }
    Ok(())
}

fn error(request_id: &str, code: &str, message: impl Into<String>) -> Value {
    json!({"protocol_version":PROTOCOL_VERSION,"request_id":request_id,"status":"error","code":code,"message":message.into()})
}
