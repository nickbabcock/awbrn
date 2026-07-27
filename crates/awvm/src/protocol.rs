//! The JSON Lines conformance protocol defined by `spec/protocol.md`.
//!
//! [`handle`] turns one request line into one response value. It is the single
//! entry point for both the `awvm-jsonl` adapter binary and the in-process
//! conformance backend in [`crate::conformance`], so a test run cannot exercise
//! a different path than an external implementation sees.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::event::Event;
use crate::random::RandomToken;
use crate::ruleset;
use crate::semantic::{
    self, AwbwVisibility, Observation, ObservedEvent, PlayerId, RulesetRef, State,
};
use crate::transition::{self, Command, ExecuteError, ExecuteOutcome};
use crate::violation::Violation;

pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Feature paths this implementation claims. Listing an ID claims its entire
/// subtree, as defined in `spec/protocol.md`.
pub const FEATURES: &[&str] = &[
    "movement-v1.move-wait.plain",
    "movement-v1.move-wait.teleporter",
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
    "commander-power-v1.validation",
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
    random: Vec<RandomToken>,
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
    events: Vec<Event>,
    recipient: PlayerId,
}

#[derive(Deserialize)]
struct Envelope {
    protocol_version: String,
    request_id: String,
    operation: String,
}

/// One protocol response, as `spec/protocol.md` shapes it.
///
/// [`handle`] used to build a `serde_json::Value`, which the adapter binary
/// then serialized a second time — the whole post-state written into a tree so
/// it could be written again into bytes. Returning the typed shape means the
/// production path serializes once, straight to its writer; the conformance
/// runner, which genuinely needs a `Value` to compare against a fixture, is now
/// the only caller that builds one.
#[derive(Debug, Serialize)]
pub struct Response {
    pub protocol_version: &'static str,
    pub request_id: String,
    #[serde(flatten)]
    pub body: ResponseBody,
}

impl Response {
    fn new(request_id: impl Into<String>, body: ResponseBody) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            body,
        }
    }

    /// A transport, decoding, unsupported-feature, or internal failure.
    pub fn error(
        request_id: impl Into<String>,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self::new(
            request_id,
            ResponseBody::Error {
                code,
                message: message.into(),
            },
        )
    }
}

/// What a response carries beyond its envelope.
///
/// Three different payloads all report `status: "ok"`, which is why this is
/// serialize-only: the tag does not identify the branch, so there is nothing
/// for a reader to dispatch on. A client decodes the payload it asked for.
#[derive(Debug, Serialize)]
#[serde(tag = "status")]
pub enum ResponseBody {
    #[serde(rename = "ok")]
    Capabilities { features: &'static [&'static str] },
    #[serde(rename = "ok")]
    Observed { observation: Observation },
    #[serde(rename = "ok")]
    ObservedEvents { observed_events: Vec<ObservedEvent> },
    #[serde(rename = "accepted")]
    Accepted {
        state: State,
        events: Vec<Event>,
        random_consumed: usize,
    },
    #[serde(rename = "rejected")]
    Rejected {
        violation: Violation,
        random_consumed: usize,
    },
    #[serde(rename = "error")]
    Error { code: &'static str, message: String },
}

/// Handle one protocol request line and produce its response.
///
/// The line is scanned for its envelope before the payload is decoded, because
/// an unsupported protocol version has to be reported even when the payload is
/// malformed. That pre-scan skips the heavy fields rather than materializing
/// them: only the operation's own request type ever builds a `State`.
pub fn handle(line: &str) -> Response {
    let envelope: Envelope = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return Response::error("", "INVALID_REQUEST", e.to_string()),
    };
    if envelope.protocol_version != PROTOCOL_VERSION {
        return Response::error(
            &envelope.request_id,
            "UNSUPPORTED_PROTOCOL",
            envelope.protocol_version,
        );
    }
    match envelope.operation.as_str() {
        "capabilities" => Response::new(
            envelope.request_id,
            ResponseBody::Capabilities { features: FEATURES },
        ),
        "execute" => execute(line, &envelope.request_id),
        "observe" => observe(line, &envelope.request_id),
        "observe-events" => observe_events(line, &envelope.request_id),
        _ => Response::error(
            &envelope.request_id,
            "UNSUPPORTED_OPERATION",
            envelope.operation,
        ),
    }
}

fn execute(line: &str, request_id: &str) -> Response {
    let request: ExecuteRequest = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return Response::error(request_id, "INVALID_REQUEST", e.to_string()),
    };
    if request.ruleset != request.state.ruleset {
        return Response::error(
            request.request_id,
            "RULESET_MISMATCH",
            "request ruleset does not equal state ruleset",
        );
    }
    let command: Command = match serde_json::from_value(request.command) {
        Ok(v) => v,
        Err(e) => return Response::error(request.request_id, "INVALID_COMMAND", e.to_string()),
    };
    let body = match transition::execute(&request.state, command, &request.random) {
        Ok(ExecuteOutcome::Accepted(result)) => ResponseBody::Accepted {
            state: result.state,
            events: result.events,
            random_consumed: result.random_consumed,
        },
        Ok(ExecuteOutcome::Rejected(violation)) => ResponseBody::Rejected {
            violation,
            random_consumed: 0,
        },
        Err(ExecuteError::UnsupportedCommand) => {
            return Response::error(
                request.request_id,
                "UNSUPPORTED_COMMAND",
                "command is not implemented by this adapter",
            );
        }
        Err(ExecuteError::UnsupportedRuleset) => {
            return Response::error(
                request.request_id,
                "UNSUPPORTED_RULESET",
                "only awbw/2026-07-10 is implemented",
            );
        }
        Err(ExecuteError::InvalidState(failure)) => {
            return Response::error(request.request_id, "INVALID_STATE", failure.to_string());
        }
        Err(ExecuteError::InvalidRandom(failure)) => {
            return Response::error(request.request_id, "INVALID_RANDOM", failure.to_string());
        }
    };
    Response::new(request.request_id, body)
}

fn observe(line: &str, request_id: &str) -> Response {
    let request: ObserveRequest = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(failure) => return Response::error(request_id, "INVALID_REQUEST", failure.to_string()),
    };
    if let Err((code, message)) = check_ruleset(&request.ruleset, [&request.state]) {
        return Response::error(request.request_id, code, message);
    }
    match semantic::observe(&AwbwVisibility, &request.state, &request.recipient) {
        Ok(observation) => {
            Response::new(request.request_id, ResponseBody::Observed { observation })
        }
        Err(failure) => {
            Response::error(request.request_id, "OBSERVATION_ERROR", failure.to_string())
        }
    }
}

fn observe_events(line: &str, request_id: &str) -> Response {
    let request: ObserveEventsRequest = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(failure) => return Response::error(request_id, "INVALID_REQUEST", failure.to_string()),
    };
    if let Err((code, message)) =
        check_ruleset(&request.ruleset, [&request.state, &request.next_state])
    {
        return Response::error(request.request_id, code, message);
    }
    match semantic::observe_events(
        &AwbwVisibility,
        &request.state,
        &request.next_state,
        &request.events,
        &request.recipient,
    ) {
        Ok(observed_events) => Response::new(
            request.request_id,
            ResponseBody::ObservedEvents { observed_events },
        ),
        Err(failure) => {
            Response::error(request.request_id, "OBSERVATION_ERROR", failure.to_string())
        }
    }
}

fn check_ruleset<'a>(
    ruleset: &RulesetRef,
    states: impl IntoIterator<Item = &'a State>,
) -> Result<(), (&'static str, String)> {
    // The same predicate `ActiveTurn::open` gates `execute` on. It used to be
    // restated here as an inline pair of string compares, which is exactly the
    // shape that goes stale when a second revision is lowered.
    if !ruleset::supports(ruleset) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The response as it goes on the wire, which is what these assert about.
    fn respond(request: &Value) -> Value {
        serde_json::to_value(handle(&request.to_string())).expect("a response encodes")
    }

    fn random_weather_request(random: Value) -> Value {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/turn-hooks/random-weather-outcomes.json"
        ))
        .unwrap();
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "request_id": "malformed-random",
            "operation": "execute",
            "ruleset": case["ruleset"],
            "state": case["initial_state"],
            "command": case["steps"][0]["command"],
            "random": random,
        })
    }

    #[test]
    fn malformed_random_tokens_fail_request_decoding() {
        let request = random_weather_request(json!([
            {"type": "weather-selection", "value": "sandstorm"}
        ]));
        let response = respond(&request);

        assert_eq!(response["status"], "error");
        assert_eq!(response["code"], "INVALID_REQUEST");
    }

    #[test]
    fn typed_random_errors_keep_their_protocol_code_and_message() {
        let request = random_weather_request(json!([]));
        let response = respond(&request);

        assert_eq!(
            response,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": "malformed-random",
                "status": "error",
                "code": "INVALID_RANDOM",
                "message": "the random tape is missing a token",
            })
        );
    }

    #[test]
    fn typed_observation_errors_keep_their_protocol_message() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/turn-hooks/random-weather-outcomes.json"
        ))
        .unwrap();
        let request = json!({
            "protocol_version": PROTOCOL_VERSION,
            "request_id": "unknown-recipient",
            "operation": "observe",
            "ruleset": case["ruleset"],
            "state": case["initial_state"],
            "recipient": "ghost",
        });

        let response = respond(&request);

        assert_eq!(response["status"], "error");
        assert_eq!(response["code"], "OBSERVATION_ERROR");
        assert_eq!(response["message"], "UnknownRecipient(PlayerId(\"ghost\"))");
    }
}
