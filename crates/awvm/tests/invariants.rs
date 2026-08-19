//! `State::validate` against the corpus, from both directions.
//!
//! A checker that rejects a legal state is worse than none, so the first test
//! requires every state the specification calls valid to pass — not just the
//! 313 written by hand, but every state the reducer produces from them, which
//! is where a relational invariant is most likely to be broken by a bug in
//! execution rather than by a bad input.
//!
//! The second requires each invariant to actually fire, because a checker that
//! accepts everything would pass the first test perfectly.

use std::path::PathBuf;

use awvm::conformance::collect_json;
use awvm::prelude::*;
use awvm::semantic::{Location, Roster, StateInvariant, UnitAction};
use serde_json::Value;

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

/// Every state in the corpus, and every state executing it reaches, is valid.
#[test]
fn no_state_the_specification_admits_is_reported_invalid() {
    let mut checked = 0;
    for (relative, case) in corpus() {
        // Equivalence cases carry their states one level down.
        let roots: Vec<&Value> = ["initial_state", "left", "right"]
            .iter()
            .filter_map(|key| case.get(key))
            .map(|value| value.get("initial_state").unwrap_or(value))
            .collect();

        for root in roots {
            let Ok(mut state) = serde_json::from_value::<State>(root.clone()) else {
                continue;
            };
            state
                .validate()
                .unwrap_or_else(|error| panic!("{relative}: initial state rejected: {error}"));
            checked += 1;

            for step in case["steps"].as_array().into_iter().flatten() {
                let Ok(command) = serde_json::from_value::<Command>(step["command"].clone()) else {
                    continue;
                };
                let random: Vec<RandomToken> =
                    serde_json::from_value(step["random"].clone()).unwrap_or_default();
                let Ok(ExecuteOutcome::Accepted(execution)) = execute(&state, command, &random)
                else {
                    continue;
                };
                execution.state.validate().unwrap_or_else(|error| {
                    let id = step["id"].as_str().unwrap_or("?");
                    panic!("{relative}: state after {id} rejected: {error}")
                });
                checked += 1;
                state = execution.state;
            }

            // The fixture's own recorded post-states are assertions about what
            // a conforming implementation produces, so they must pass too.
            for step in case["steps"].as_array().into_iter().flatten() {
                let Ok(expected) = serde_json::from_value::<State>(step["expect"]["state"].clone())
                else {
                    continue;
                };
                expected.validate().unwrap_or_else(|error| {
                    let id = step["id"].as_str().unwrap_or("?");
                    panic!("{relative}: expected state of {id} rejected: {error}")
                });
                checked += 1;
            }
        }
    }
    assert!(checked >= 893, "expected the whole corpus, saw {checked}");
}

/// A state known good, so each case below differs from a valid one in exactly
/// the way it is named for.
fn valid() -> State {
    // Two own units on the board, one of them a transport, so every case below
    // has the pieces it needs to break exactly one invariant.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/transport/load-infantry-into-apc.json");
    let case: Value = serde_json::from_str(&std::fs::read_to_string(root).expect("read fixture"))
        .expect("parse fixture");
    let state: State =
        serde_json::from_value(case["initial_state"].clone()).expect("decode fixture state");
    state
        .validate()
        .expect("the fixture is valid to begin with");
    state
}

#[test]
fn two_units_on_one_tile_are_caught() {
    let mut state = valid();
    let (first, second) = (state.units[0].id, state.units[1].id);
    let position = match state.units[0].location {
        Location::Board { position } => position,
        Location::Cargo { .. } => panic!("the fixture's first unit is on the board"),
    };
    state.units[1].location = Location::Board { position };

    assert_eq!(
        state.validate(),
        Err(StateInvariant::TileOccupiedTwice {
            position,
            first,
            second
        })
    );
}

#[test]
fn a_unit_owned_by_nobody_is_caught() {
    let mut state = valid();
    let unit = state.units[0].id;
    // A seat is minted from the roster that holds it, so the only way one ends
    // up naming nobody is a roster swapped for a shorter one afterwards. That
    // is what this reproduces, and what `validate` is here to catch.
    let mut seated = state.players.iter().cloned().collect::<Vec<_>>();
    let stranger = seated[0].renamed(PlayerId::from("nobody"));
    seated.push(stranger);
    let roster = Roster::new(seated).expect("two players fit a roster");
    let nobody = roster
        .seat(&PlayerId::from("nobody"))
        .expect("the roster seats them");
    state.units[0].owner = nobody;

    assert_eq!(
        state.validate(),
        Err(StateInvariant::UnitOwnerOffTheRoster { unit, seat: nobody })
    );
}

#[test]
fn a_stale_next_unit_id_is_caught() {
    let mut state = valid();
    let highest = state
        .units
        .iter()
        .map(|unit| unit.id.get())
        .max()
        .expect("the fixture has units");
    state.next_unit_id = Some(highest);

    assert_eq!(
        state.validate(),
        Err(StateInvariant::NextUnitIdIsNotFresh {
            next_unit_id: highest,
            highest: UnitId::new(highest),
        })
    );

    // Absence is not a fault: `spec/model/state.md:139` lets a state that never
    // spawns units omit the field, and production reports its absence itself.
    state.next_unit_id = None;
    assert_eq!(state.validate(), Ok(()));
}

#[test]
fn cargo_pointing_at_a_unit_that_is_not_in_play_is_caught() {
    let mut state = valid();
    let unit = state.units[0].id;
    state.units[0].location = Location::Cargo {
        transport: UnitId::new(9_999),
        slot: 0,
    };

    let Err(StateInvariant::Cargo {
        unit: reported,
        transport,
        ..
    }) = state.validate()
    else {
        panic!("a dangling transport reference must be caught");
    };
    assert_eq!(reported, unit);
    assert_eq!(transport, UnitId::new(9_999));
}

#[test]
fn a_turn_position_outside_the_order_is_caught() {
    let mut state = valid();
    state.turn.position = state.turn.order.len();

    assert_eq!(
        state.validate(),
        Err(StateInvariant::TurnPositionOutOfRange {
            position: state.turn.order.len(),
            length: state.turn.order.len(),
        })
    );
}

#[test]
fn a_second_moved_unit_is_caught() {
    let mut state = valid();
    let active = state
        .player_index(&state.turn.active_player.clone())
        .expect("the active player is on the roster");
    let mine: Vec<usize> = (0..state.units.len())
        .filter(|index| state.units[*index].owner == active)
        .collect();
    assert!(mine.len() >= 2, "the fixture has two units for one player");
    state.units[mine[0]].action = UnitAction::Moved;
    state.units[mine[1]].action = UnitAction::Moved;

    assert!(matches!(
        state.validate(),
        Err(StateInvariant::SeveralMovedUnits { .. })
    ));
}
