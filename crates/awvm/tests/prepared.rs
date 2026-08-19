//! Equivalence checks for commands that separate preparation from application.
//!
//! The ordinary reducer remains the oracle while the prepared API is small.
//! Every prepared fixture command must produce the same rejection, state,
//! events, and random count through both paths.

use std::path::PathBuf;

use awvm::conformance::collect_json;
use awvm::prelude::*;
use awvm::semantic::{Location, TileOwner};
use serde_json::Value;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/fixtures")
}

#[test]
fn every_supported_fixture_matches_prepared_execution() {
    let root = fixture_root();
    let mut files = Vec::new();
    collect_json(&root, &mut files).expect("walk fixture root");
    files.sort();

    let mut waits = 0;
    let mut captures = 0;
    let mut supplies = 0;
    let mut hides = 0;
    let mut reveals = 0;
    let mut explodes = 0;
    let mut joins = 0;
    let mut loads = 0;
    let mut attacks = 0;
    let mut repairs = 0;
    let mut launches = 0;
    let mut productions = 0;
    let mut deletes = 0;
    let mut unloads = 0;
    let mut end_turns = 0;
    let mut tags = 0;
    let mut resigns = 0;
    let mut powers = 0;
    for path in files {
        let source = std::fs::read_to_string(&path).expect("read fixture");
        let case: Value = serde_json::from_str(&source).expect("parse fixture");
        let Ok(mut state) = serde_json::from_value::<State>(case["initial_state"].clone()) else {
            continue;
        };

        for step in case["steps"].as_array().into_iter().flatten() {
            let Ok(command) = serde_json::from_value::<Command>(step["command"].clone()) else {
                continue;
            };
            let random: Vec<RandomToken> =
                serde_json::from_value(step["random"].clone()).unwrap_or_default();
            let ordinary = execute(&state, command.clone(), &random);

            let counter = match &command {
                Command::MoveWait { .. } => Some(&mut waits),
                Command::MoveCapture { .. } => Some(&mut captures),
                Command::MoveSupply { .. } => Some(&mut supplies),
                Command::MoveHide { .. } => Some(&mut hides),
                Command::MoveReveal { .. } => Some(&mut reveals),
                Command::MoveExplode { .. } => Some(&mut explodes),
                Command::MoveJoin { .. } => Some(&mut joins),
                Command::MoveLoad { .. } => Some(&mut loads),
                Command::MoveAttack { .. } => Some(&mut attacks),
                Command::MoveRepair { .. } => Some(&mut repairs),
                Command::MoveLaunch { .. } => Some(&mut launches),
                Command::ProduceUnit { .. } => Some(&mut productions),
                Command::DeleteUnit { .. } => Some(&mut deletes),
                Command::Unload { .. } => Some(&mut unloads),
                Command::EndTurn { .. } => Some(&mut end_turns),
                Command::Tag { .. } => Some(&mut tags),
                Command::Resign { .. } => Some(&mut resigns),
                Command::ActivatePower { .. } => Some(&mut powers),
                Command::Unsupported => None,
            };
            if let Some(counter) = counter {
                *counter += 1;
                let prepared = prepare_command(&state, command);
                match (&ordinary, prepared) {
                    (Ok(ExecuteOutcome::Accepted(expected)), Ok(Ok(prepared))) => assert_eq!(
                        execute_prepared(prepared, &random),
                        Ok(expected.clone()),
                        "{} disagreed at {}",
                        path.display(),
                        step["id"]
                    ),
                    (Ok(ExecuteOutcome::Rejected(expected)), Ok(Err(actual))) => assert_eq!(
                        actual,
                        *expected,
                        "{} disagreed at {}",
                        path.display(),
                        step["id"]
                    ),
                    (Err(expected), Err(actual)) => assert_eq!(
                        actual,
                        *expected,
                        "{} disagreed at {}",
                        path.display(),
                        step["id"]
                    ),
                    (expected, actual) => panic!(
                        "{} disagreed at {}: ordinary {expected:?}, prepared {actual:?}",
                        path.display(),
                        step["id"]
                    ),
                }
            }

            if let Ok(ExecuteOutcome::Accepted(execution)) = ordinary {
                state = execution.state;
            }
        }
    }

    assert!(
        waits >= 47,
        "expected the full move-wait corpus, saw {waits}"
    );
    assert!(
        captures >= 14,
        "expected the full move-capture corpus, saw {captures}"
    );
    assert!(
        supplies >= 5,
        "expected the full move-supply corpus, saw {supplies}"
    );
    assert!(
        hides >= 6,
        "expected the full move-hide corpus, saw {hides}"
    );
    assert!(
        reveals >= 2,
        "expected the full move-reveal corpus, saw {reveals}"
    );
    assert!(
        explodes >= 3,
        "expected the full move-explode corpus, saw {explodes}"
    );
    assert!(
        joins >= 6,
        "expected the full move-join corpus, saw {joins}"
    );
    assert!(
        loads >= 6,
        "expected the full move-load corpus, saw {loads}"
    );
    assert!(
        attacks >= 128,
        "expected the full move-attack corpus, saw {attacks}"
    );
    assert!(
        repairs >= 4,
        "expected the full move-repair corpus, saw {repairs}"
    );
    assert!(
        launches >= 4,
        "expected the full move-launch corpus, saw {launches}"
    );
    assert!(
        productions >= 25,
        "expected the full production corpus, saw {productions}"
    );
    assert!(
        deletes >= 6,
        "expected the full delete corpus, saw {deletes}"
    );
    assert!(
        unloads >= 10,
        "expected the full unload corpus, saw {unloads}"
    );
    assert!(
        end_turns >= 24,
        "expected the full end-turn corpus, saw {end_turns}"
    );
    assert!(tags >= 3, "expected the full tag corpus, saw {tags}");
    assert!(
        resigns >= 3,
        "expected the full resign corpus, saw {resigns}"
    );
    assert!(powers >= 58, "expected the full power corpus, saw {powers}");
}

#[test]
fn one_movement_can_prepare_wait_and_capture_destinations() {
    let source = include_str!("../../../spec/fixtures/capture/capture-city-partial.json");
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    let state: State = serde_json::from_value(case["initial_state"].clone()).expect("decode state");
    let command: Command =
        serde_json::from_value(case["steps"][0]["command"].clone()).expect("decode command");
    let Command::MoveCapture { player, unit, path } = command else {
        panic!("fixture starts with capture")
    };
    let movement = prepare_movement(&state, &player, unit, path.clone())
        .expect("prepare")
        .unwrap_or_else(|violation| panic!("movement rejected: {violation:?}"));
    let wait = movement
        .clone()
        .prepare_destination()
        .prepare_wait()
        .expect("prepare wait")
        .unwrap_or_else(|violation| panic!("wait rejected: {violation:?}"));
    let capture = movement
        .prepare_destination()
        .prepare_capture()
        .expect("prepare capture")
        .unwrap_or_else(|violation| panic!("capture rejected: {violation:?}"));

    let ordinary_wait = execute(
        &state,
        Command::MoveWait {
            player: player.clone(),
            unit,
            path: path.clone(),
        },
        &[],
    );
    let ordinary_capture = execute(&state, Command::MoveCapture { player, unit, path }, &[]);

    assert_eq!(
        execute_prepared(wait, &[]).map(ExecuteOutcome::Accepted),
        ordinary_wait
    );
    assert_eq!(
        execute_prepared(capture, &[]).map(ExecuteOutcome::Accepted),
        ordinary_capture
    );
}

#[test]
fn one_production_site_can_prepare_a_kind() {
    let source = include_str!("../../../spec/fixtures/production/produce-infantry-on-base.json");
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    let state: State = serde_json::from_value(case["initial_state"].clone()).expect("decode state");
    let command: Command =
        serde_json::from_value(case["steps"][0]["command"].clone()).expect("decode command");
    let Command::ProduceUnit {
        player,
        position,
        kind,
    } = command.clone()
    else {
        panic!("fixture starts with production")
    };
    let site = prepare_production_site(&state, &player, position)
        .expect("prepare site")
        .unwrap_or_else(|violation| panic!("production site rejected: {violation:?}"));
    let prepared = site
        .prepare_kind(kind)
        .expect("prepare kind")
        .unwrap_or_else(|violation| panic!("production rejected: {violation:?}"));

    assert_eq!(
        execute_prepared(prepared, &[]).map(ExecuteOutcome::Accepted),
        execute(&state, command, &[])
    );
}

#[test]
fn one_active_unit_can_prepare_delete() {
    let source = include_str!("../../../spec/fixtures/delete/delete-unit-capture-reset.json");
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    let state: State = serde_json::from_value(case["initial_state"].clone()).expect("decode state");
    let command: Command =
        serde_json::from_value(case["steps"][0]["command"].clone()).expect("decode command");
    let Command::DeleteUnit { player, unit } = command.clone() else {
        panic!("fixture starts with delete")
    };
    let active = prepare_active_unit(&state, &player, unit)
        .expect("prepare unit")
        .unwrap_or_else(|violation| panic!("active unit rejected: {violation:?}"));
    let prepared = active
        .prepare_delete()
        .expect("prepare delete")
        .unwrap_or_else(|violation| panic!("delete rejected: {violation:?}"));

    assert_eq!(
        execute_prepared(prepared, &[]).map(ExecuteOutcome::Accepted),
        execute(&state, command, &[])
    );
}

#[test]
fn one_transport_and_cargo_can_prepare_an_unload() {
    let source = include_str!("../../../spec/fixtures/transport/unload-infantry-from-apc.json");
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    let state: State = serde_json::from_value(case["initial_state"].clone()).expect("decode state");
    let command: Command =
        serde_json::from_value(case["steps"][0]["command"].clone()).expect("decode command");
    let Command::Unload {
        player,
        transport,
        cargo,
        destination,
    } = command.clone()
    else {
        panic!("fixture starts with unload")
    };
    let transport = prepare_unload_transport(&state, &player, transport)
        .expect("prepare transport")
        .unwrap_or_else(|violation| panic!("transport rejected: {violation:?}"));
    let cargo = transport
        .prepare_cargo(cargo)
        .expect("prepare cargo")
        .unwrap_or_else(|violation| panic!("cargo rejected: {violation:?}"));
    let prepared = cargo
        .prepare_destination(destination)
        .expect("prepare destination")
        .unwrap_or_else(|violation| panic!("unload rejected: {violation:?}"));

    assert_eq!(
        execute_prepared(prepared, &[]).map(ExecuteOutcome::Accepted),
        execute(&state, command, &[])
    );
}

#[test]
fn a_hidden_trap_suppresses_prepared_capture() {
    let source = include_str!("../../../spec/fixtures/movement/teleporter-hidden-trap.json");
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    let mut state: State =
        serde_json::from_value(case["initial_state"].clone()).expect("decode state");
    let destination = Pos::new(4, 0);
    let tile = state.board.tile_mut(destination);
    tile.terrain = awvm::ruleset::Terrain::City;
    tile.owner = TileOwner::Neutral;
    tile.capture_points = Some(20);
    let wait: Command =
        serde_json::from_value(case["steps"][0]["command"].clone()).expect("decode command");
    let Command::MoveWait { player, unit, path } = wait else {
        panic!("fixture starts with move-wait")
    };
    let command = Command::MoveCapture { player, unit, path };

    let prepared = prepare_command(&state, command)
        .expect("prepare capture")
        .unwrap_or_else(|violation| panic!("capture rejected: {violation:?}"));
    let execution = execute_prepared(prepared, &[]).expect("execute capture");

    assert!(
        execution
            .events
            .iter()
            .any(|event| matches!(event, Event::MovementTrapped { .. }))
    );
    assert!(
        !execution
            .events
            .iter()
            .any(|event| matches!(event, Event::CaptureChanged { .. }))
    );
    assert_eq!(
        execution.state.units.get(unit).map(|unit| &unit.location),
        Some(&Location::Board {
            position: Pos::new(0, 0)
        })
    );
}

#[test]
fn preparation_covers_every_command_this_adapter_implements() {
    let source = include_str!("../../../spec/fixtures/movement/infantry-plain-move.json");
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    let state: State = serde_json::from_value(case["initial_state"].clone()).expect("decode state");

    // Turn boundaries used to be the one family a caller could not resolve
    // ahead of applying it, which is the command a search issues most.
    let end_turn = Command::EndTurn {
        player: state.turn.active_player.clone(),
    };
    assert!(matches!(prepare_command(&state, end_turn), Ok(Ok(_))));

    assert_eq!(
        prepare_command(&state, Command::Unsupported).unwrap_err(),
        ExecuteError::UnsupportedCommand
    );
}
