//! `query` against `execute`, over the whole corpus.
//!
//! `actions_at` and `attack_targets` run the reducer, so they cannot disagree
//! with it by construction. `reachable` does not — it is a search written
//! beside the rules, which is exactly the arrangement this module exists to
//! spare consumers — so it is the one thing that needs holding to account.
//!
//! The ground truth here is `execute` itself. Fixture boards are at most 21
//! tiles, so every simple path a unit could submit can be enumerated and put to
//! the reducer, and the set of destinations it accepts is what `reachable` must
//! report. Not a re-derivation of the movement rules: the reducer's own
//! verdicts, on every path.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use awvm::conformance::collect_json;
use awvm::prelude::*;
use awvm::query::{self, can_act};
use awvm::semantic::{Location, UnitAction};
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

fn is_teleporter(state: &State, position: Pos) -> bool {
    state
        .board
        .get(position)
        .is_some_and(|tile| tile.terrain == TerrainId::Teleporter)
}

fn states(case: &Value) -> Vec<State> {
    ["initial_state", "left", "right"]
        .iter()
        .filter_map(|key| case.get(key))
        .map(|value| value.get("initial_state").unwrap_or(value))
        .filter_map(|value| serde_json::from_value::<State>(value.clone()).ok())
        .collect()
}

/// What the reducer says about every simple path this unit could submit.
///
/// Returns the destinations it accepts, and the wider set of tiles a path can
/// legally arrive at — a tile whose occupant blocks resting there, or a
/// teleporter, is refused for the destination alone, and both remain reachable
/// for the families that license them.
fn reference(state: &State, unit: UnitId, player: &PlayerId, origin: Pos) -> (Set, Set) {
    let mut destinations = BTreeSet::new();
    let mut arrivals = BTreeSet::new();
    let mut path = vec![origin];
    let mut visited = BTreeSet::from([origin]);
    let mut explored = 0_u32;

    walk(
        state,
        unit,
        player,
        &mut path,
        &mut visited,
        &mut destinations,
        &mut arrivals,
        &mut explored,
    );
    (destinations, arrivals)
}

type Set = BTreeSet<Pos>;

#[expect(
    clippy::too_many_arguments,
    reason = "a depth-first walk's whole state"
)]
fn walk(
    state: &State,
    unit: UnitId,
    player: &PlayerId,
    path: &mut Vec<Pos>,
    visited: &mut Set,
    destinations: &mut Set,
    arrivals: &mut Set,
    explored: &mut u32,
) {
    *explored += 1;
    assert!(
        *explored < 2_000_000,
        "path enumeration ran away; a fixture board grew past what this can cover"
    );

    let here = *path.last().expect("a path holds at least its origin");
    match execute(
        state,
        Command::MoveWait {
            player: player.clone(),
            unit,
            path: path.clone(),
        },
        &[],
    ) {
        Ok(ExecuteOutcome::Accepted(_)) => {
            destinations.insert(here);
            arrivals.insert(here);
        }
        // The route was fine and only resting here was refused, so the tile is
        // reachable — this is what join, load and a moving attack rely on.
        Ok(ExecuteOutcome::Rejected(Violation::DestinationOccupied { .. })) => {
            arrivals.insert(here);
        }
        Ok(ExecuteOutcome::Rejected(Violation::TerrainImpassable { index, .. }))
            if index == Some(path.len().saturating_sub(1))
                && path.len() > 1
                && is_teleporter(state, here) =>
        {
            // A teleporter is crossed but never rested on, so the refusal
            // names the final index and the tile is still reachable. The same
            // violation at the same index means terrain this unit cannot enter
            // at all, which is not.
            arrivals.insert(here);
        }
        // Every other refusal is about the route, so extending it cannot help:
        // costs only grow and no repeat is allowed.
        Ok(ExecuteOutcome::Rejected(_)) | Err(_) => {
            if path.len() > 1 {
                return;
            }
        }
    }

    for next in here.orthogonal() {
        if !state.board.contains(next) || !visited.insert(next) {
            continue;
        }
        path.push(next);
        walk(
            state,
            unit,
            player,
            path,
            visited,
            destinations,
            arrivals,
            explored,
        );
        path.pop();
        visited.remove(&next);
    }
}

/// `reachable` reports exactly the destinations the reducer accepts.
#[test]
fn the_move_field_agrees_with_the_reducer_on_every_tile() {
    let mut units = 0;
    let mut tiles = 0;
    for (relative, case) in corpus() {
        for state in states(&case) {
            for subject in state.units.iter() {
                let Location::Board { position: origin } = subject.location else {
                    continue;
                };
                // A unit the reducer will not act with has no accepted paths at
                // all, so there is nothing to compare against; `can_act` is
                // what an interface asks first, and it is checked below.
                if can_act(&state, subject.id) != Ok(Ok(())) {
                    continue;
                }

                let field = query::reachable(&state, subject.id).expect("an on-board unit");
                let (expected_destinations, expected_arrivals) =
                    reference(&state, subject.id, &subject.owner, origin);

                let offered: Set = field.destinations().map(|(position, _)| position).collect();
                assert_eq!(
                    offered, expected_destinations,
                    "{relative}: unit {} destinations disagree with execute",
                    subject.id
                );

                let arrivals: Set = field.reach().map(|(position, _)| position).collect();
                assert_eq!(
                    arrivals, expected_arrivals,
                    "{relative}: unit {} reachable tiles disagree with execute",
                    subject.id
                );

                // Every path handed out must itself be accepted; agreeing on
                // the set of tiles is not the same as producing a usable route.
                for (position, cost) in field.destinations() {
                    let path = field.path_to(position).expect("a destination has a path");
                    assert_eq!(path.first(), Some(&origin), "{relative}: path skips origin");
                    assert_eq!(
                        path.last(),
                        Some(&position),
                        "{relative}: path misses target"
                    );
                    assert!(
                        matches!(
                            execute(
                                &state,
                                Command::MoveWait {
                                    player: subject.owner.clone(),
                                    unit: subject.id,
                                    path: path.clone(),
                                },
                                &[],
                            ),
                            Ok(ExecuteOutcome::Accepted(_))
                        ),
                        "{relative}: unit {} was handed a path to {position} that execute refused: {path:?}",
                        subject.id
                    );
                    assert!(
                        cost <= field.budget(),
                        "{relative}: cost exceeds the budget"
                    );
                    tiles += 1;
                }
                units += 1;
            }
        }
    }
    assert!(
        units >= 288 && tiles >= 704,
        "expected the whole corpus, saw {units} units over {tiles} tiles"
    );
}

/// Every command the corpus issues and the reducer accepts was on offer.
///
/// The other direction of the same claim: not just that what `query` offers is
/// legal, but that what is legal gets offered. The fixtures are the list of
/// things players actually do, so anything they do that an interface could not
/// have found is a hole.
#[test]
fn every_accepted_fixture_command_was_offered() {
    let mut checked = BTreeMap::<&str, u32>::new();
    for (relative, case) in corpus() {
        let Some(mut state) = states(&case).into_iter().next() else {
            continue;
        };
        for step in case["steps"].as_array().into_iter().flatten() {
            let Ok(command) = serde_json::from_value::<Command>(step["command"].clone()) else {
                continue;
            };
            let random: Vec<RandomToken> =
                serde_json::from_value(step["random"].clone()).unwrap_or_default();
            let Ok(ExecuteOutcome::Accepted(execution)) = execute(&state, command.clone(), &random)
            else {
                continue;
            };

            let family = offered(&state, &command, &relative);
            if let Some(family) = family {
                *checked.entry(family).or_default() += 1;
            }
            state = execution.state;
        }
    }

    // Named families, so a command family losing coverage is visible rather
    // than absorbed into a total.
    for family in [
        "move-wait",
        "move-attack",
        "move-capture",
        "move-join",
        "move-load",
        "move-supply",
        "move-hide",
        "move-reveal",
        "produce-unit",
    ] {
        assert!(
            checked.get(family).is_some_and(|count| *count > 0),
            "no accepted {family} in the corpus was checked; coverage regressed: {checked:?}"
        );
    }
}

#[test]
fn observed_production_options_include_hachi_scop_city_metadata() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/commander/hachi-scop.json");
    let case: Value =
        serde_json::from_str(&std::fs::read_to_string(root).expect("read fixture")).unwrap();
    let state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
    let player = state.turn.active_player.clone();
    let observation = observe(&AwbwVisibility, &state, &player).unwrap();

    let options = query::observed_production_options(&observation, Pos::new(0, 0));
    assert!(
        options
            .iter()
            .any(|option| option.kind == UnitKind::Infantry),
        "Hachi's SCOP city should offer infantry"
    );
    assert_eq!(
        options
            .iter()
            .take(6)
            .map(|option| (option.kind, option.cost, option.affordable))
            .collect::<Vec<_>>(),
        vec![
            (UnitKind::Infantry, 500, true),
            (UnitKind::Mech, 1_500, true),
            (UnitKind::Recon, 2_000, true),
            (UnitKind::Apc, 2_500, true),
            (UnitKind::Artillery, 3_000, true),
            (UnitKind::Tank, 3_500, true),
        ],
        "options should retain base-cost order while exposing Hachi's effective prices"
    );
    assert_eq!(
        options
            .iter()
            .find(|option| option.kind == UnitKind::MdTank)
            .map(|option| (option.cost, option.affordable)),
        Some((8_000, false)),
        "menu metadata should mark unaffordable commander-adjusted options"
    );
}

/// Assert that `command`, which the reducer accepted, is one `query` offers.
///
/// Returns the family checked, or `None` for the commands that are not tied to
/// a unit standing somewhere and so have no destination to enumerate.
fn offered(state: &State, command: &Command, relative: &str) -> Option<&'static str> {
    let destination = |path: &[Pos]| *path.last().expect("a path holds at least its origin");
    let set = |unit: UnitId, path: &[Pos]| {
        query::actions_at(state, unit, destination(path))
            .unwrap_or_else(|error| panic!("{relative}: {error}"))
    };

    match command {
        Command::MoveWait { unit, path, .. } => {
            assert!(set(*unit, path).wait, "{relative}: move-wait not offered");
            Some("move-wait")
        }
        Command::MoveCapture { unit, path, .. } => {
            assert!(
                set(*unit, path).capture,
                "{relative}: move-capture not offered"
            );
            Some("move-capture")
        }
        Command::MoveSupply { unit, path, .. } => {
            assert!(
                set(*unit, path).supply,
                "{relative}: move-supply not offered"
            );
            Some("move-supply")
        }
        Command::MoveHide { unit, path, .. } => {
            assert!(set(*unit, path).hide, "{relative}: move-hide not offered");
            Some("move-hide")
        }
        Command::MoveReveal { unit, path, .. } => {
            assert!(
                set(*unit, path).reveal,
                "{relative}: move-reveal not offered"
            );
            Some("move-reveal")
        }
        Command::MoveExplode { unit, path, .. } => {
            assert!(
                set(*unit, path).explode,
                "{relative}: move-explode not offered"
            );
            Some("move-explode")
        }
        Command::MoveJoin { unit, path, .. } => {
            assert!(set(*unit, path).join, "{relative}: move-join not offered");
            Some("move-join")
        }
        Command::MoveLoad { unit, path, .. } => {
            assert!(set(*unit, path).load, "{relative}: move-load not offered");
            Some("move-load")
        }
        Command::MoveRepair {
            unit, path, target, ..
        } => {
            assert!(
                set(*unit, path).repair.contains(target),
                "{relative}: move-repair on {target} not offered"
            );
            Some("move-repair")
        }
        Command::MoveAttack {
            unit, path, target, ..
        } => {
            let targets = query::attack_targets(state, *unit, destination(path))
                .unwrap_or_else(|error| panic!("{relative}: {error}"));
            assert!(
                targets.contains(target),
                "{relative}: attack on {target:?} not offered, saw {targets:?}"
            );
            Some("move-attack")
        }
        Command::MoveLaunch {
            unit, path, target, ..
        } => {
            assert!(
                set(*unit, path).launch.contains(target),
                "{relative}: launch at {target:?} not offered"
            );
            Some("move-launch")
        }
        Command::ProduceUnit {
            player,
            position,
            kind,
        } => {
            let kinds = query::production_options(state, player, *position);
            assert!(
                kinds.contains(kind),
                "{relative}: producing {kind} at {position} not offered, saw {kinds:?}"
            );
            let observation = observe(&AwbwVisibility, state, player)
                .unwrap_or_else(|error| panic!("{relative}: could not observe state: {error}"));
            let observed = query::observed_production_options(&observation, *position);
            assert!(
                observed.iter().any(|option| option.kind == *kind),
                "{relative}: observed production omitted {kind} at {position}, saw {observed:?}"
            );
            Some("produce-unit")
        }
        // Not destination-scoped: these belong to the player, not to a unit
        // standing somewhere, and an interface reaches them from the turn
        // controls rather than from a selected tile.
        Command::DeleteUnit { .. }
        | Command::Unload { .. }
        | Command::ActivatePower { .. }
        | Command::Tag { .. }
        | Command::EndTurn { .. }
        | Command::Resign { .. }
        | Command::Unsupported => None,
    }
}

/// `can_act` explains a unit an interface should grey out.
#[test]
fn can_act_reports_the_violation_the_reducer_would() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/movement/infantry-plain-move.json");
    let case: Value =
        serde_json::from_str(&std::fs::read_to_string(root).expect("read fixture")).unwrap();
    let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();

    let unit = state.units[0].id;
    assert_eq!(can_act(&state, unit), Ok(Ok(())));

    // A unit belonging to someone who is not the active player is refused, and
    // the reason names the thing an interface should say: it is not your turn.
    // `can_act` asks on behalf of the unit's own owner, which is why an enemy
    // unit reports this rather than `UnitNotOwned`.
    let mine = state.units[0].owner.clone();
    state.units[0].owner = PlayerId::from("nobody");
    assert!(matches!(
        can_act(&state, unit),
        Ok(Err(Violation::NotActivePlayer { .. }))
    ));
    state.units[0].owner = mine;

    // A unit that has already acted is refused with its own reason.
    state.units[0].action = awvm::semantic::UnitAction::Spent;
    assert_eq!(
        can_act(&state, unit),
        Ok(Err(Violation::UnitAlreadyActed { unit }))
    );

    assert_eq!(
        can_act(&state, UnitId::new(4_242)),
        Err(query::QueryError::UnitNotFound(UnitId::new(4_242)))
    );
}

/// With fog off, a recipient sees everything, so the projection is lossless and
/// `observed_actions_at` must answer exactly what `actions_at` answers.
///
/// This is what keeps the reification honest. It rebuilds a state from a
/// projection, and the only way to know the rebuild lost nothing that bears on
/// legality is to ask the reducer both ways and require the same verdict. Fog
/// is excluded because there the projection is *meant* to lose facts; what the
/// client gets then is advisory by contract.
#[test]
fn observed_actions_agree_with_authoritative_actions_without_fog() {
    let mut checked = 0;
    for (relative, case) in corpus() {
        for state in states(&case) {
            if state.settings.fog {
                continue;
            }
            let recipient = state.turn.active_player.clone();
            let Ok(observation) = observe(&AwbwVisibility, &state, &recipient) else {
                continue;
            };
            let reified = query::reify(&observation).expect("fog-free observation must reify");

            for subject in state.units.iter() {
                if subject.owner != recipient || can_act(&state, subject.id) != Ok(Ok(())) {
                    continue;
                }
                let Ok(field) = query::reachable(&state, subject.id) else {
                    continue;
                };

                for (destination, _) in field.destinations() {
                    let authoritative = query::actions_at(&state, subject.id, destination)
                        .map(|actions| query::by_position(&state, actions));
                    let observed = query::actions_at(&reified, subject.id, destination)
                        .map(|actions| query::by_position(&reified, actions));
                    assert_eq!(
                        observed, authoritative,
                        "{relative}: unit {} at {destination:?} disagrees between the \
                         projection and the state it came from",
                        subject.id
                    );
                    checked += 1;
                }
            }
        }
    }

    assert!(
        checked > 100,
        "expected the fog-free corpus to exercise the projection, saw {checked} destinations"
    );
}

/// With fog off, reifying an observation preserves the complete movement
/// field, including route costs, stopping rules, and chosen paths.
#[test]
fn observed_reachable_agrees_with_authoritative_reachable_without_fog() {
    let mut checked = 0;
    for (relative, case) in corpus() {
        for state in states(&case) {
            if state.settings.fog {
                continue;
            }
            let recipient = state.turn.active_player.clone();
            let Ok(observation) = observe(&AwbwVisibility, &state, &recipient) else {
                continue;
            };

            for subject in state.units.iter().filter(|unit| unit.owner == recipient) {
                let authoritative = query::reachable(&state, subject.id);
                let observed = query::observed_reachable(&observation, subject.id);
                match (authoritative, observed) {
                    (Ok(authoritative), Ok(observed)) => {
                        let authoritative_steps: Vec<_> = authoritative
                            .reach()
                            .map(|(position, cost)| {
                                (
                                    position,
                                    cost,
                                    authoritative.can_stop_at(position),
                                    authoritative.path_to(position),
                                )
                            })
                            .collect();
                        let observed_steps: Vec<_> = observed
                            .reach()
                            .map(|(position, cost)| {
                                (
                                    position,
                                    cost,
                                    observed.can_stop_at(position),
                                    observed.path_to(position),
                                )
                            })
                            .collect();
                        assert_eq!(
                            observed_steps, authoritative_steps,
                            "{relative}: unit {} has a different observed movement field",
                            subject.id
                        );
                        checked += 1;
                    }
                    (Err(authoritative), Err(observed)) => {
                        assert_eq!(observed, authoritative, "{relative}")
                    }
                    (authoritative, observed) => panic!(
                        "{relative}: unit {} disagrees while building its movement field: \
                         authoritative={authoritative:?}, observed={observed:?}",
                        subject.id
                    ),
                }
            }
        }
    }

    assert!(
        checked > 20,
        "expected the fog-free corpus to exercise movement"
    );
}

#[test]
fn observed_unloads_are_offered_after_the_transport_is_spent() {
    let case: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spec/fixtures/transport/unload-infantry-from-apc.json"
    )))
    .unwrap();
    let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
    state.units[0].action = UnitAction::Spent;
    let recipient = state.turn.active_player.clone();
    let observation = observe(&AwbwVisibility, &state, &recipient).unwrap();

    assert_eq!(
        query::observed_unloads(&observation, UnitId::new(0)).unwrap(),
        vec![query::ObservedUnload {
            cargo: UnitId::new(1),
            cargo_kind: UnitKindId::Infantry,
            destination: Pos::new(0, 0),
        }]
    );
}
