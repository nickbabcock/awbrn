//! `query` against `execute`, over the whole corpus.
//!
//! `actions_at` uses reducer preparation. It cannot disagree with execution by
//! construction. `reachable` does not. It is a search written beside the
//! rules, which is exactly the arrangement this module exists to spare
//! consumers. It therefore needs separate equivalence coverage.
//!
//! The ground truth here is `execute` itself. Fixture boards are at most 21
//! tiles, so every simple path a unit could submit can be enumerated and put to
//! the reducer, and the set of destinations it accepts is what `reachable` must
//! report. Not a re-derivation of the movement rules: the reducer's own
//! verdicts, on every path.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use awvm::combat::DamageRange;
use awvm::conformance::collect_json;
use awvm::prelude::*;
use awvm::query::{self, can_act};
use awvm::semantic::{KnownReason, Location, Reason, RulesetRevision, UnitAction};
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

/// Prepared movement gives the same action verdicts as execution.
#[test]
fn prepared_action_queries_agree_with_execution() {
    let mut checked = 0;
    for (relative, case) in corpus() {
        for state in states(&case) {
            for subject in state.units.iter() {
                if subject.owner != state.turn.active_player
                    || can_act(&state, subject.id) != Ok(Ok(()))
                {
                    continue;
                }
                let field = query::reachable(&state, subject.id).expect("an on-board unit");
                let active = match prepare_active_unit(&state, &subject.owner, subject.id)
                    .expect("an active unit can be prepared")
                {
                    PrepareActiveUnitOutcome::Prepared(active) => active,
                    PrepareActiveUnitOutcome::Rejected(violation) => {
                        panic!("{relative}: active unit was rejected: {violation:?}")
                    }
                };
                let prepared_field = PreparedMoveField::new(active)
                    .unwrap_or_else(|error| panic!("{relative}: {error}"));
                for (destination, _) in field.reach() {
                    let path = field
                        .path_to(destination)
                        .expect("a reachable tile has a path");
                    let actions = query::actions_at(&state, subject.id, destination)
                        .unwrap_or_else(|error| panic!("{relative}: {error}"));
                    let from_path = query::actions_for_path(&state, subject.id, path.clone())
                        .unwrap_or_else(|error| panic!("{relative}: {error}"));
                    assert_eq!(
                        from_path, actions,
                        "{relative}: unit {} path query disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        prepared_field
                            .actions_at(destination)
                            .unwrap_or_else(|error| panic!("{relative}: {error}")),
                        actions,
                        "{relative}: unit {} prepared field disagreed at {destination}",
                        subject.id
                    );
                    let accepts = |command| {
                        matches!(
                            execute(&state, command, &[]),
                            Ok(ExecuteOutcome::Accepted(_))
                        )
                    };
                    let occupant = state.units.iter().find_map(|unit| match unit.location {
                        Location::Board { position } if position == destination => Some(unit.id),
                        _ => None,
                    });

                    assert_eq!(
                        actions.wait,
                        accepts(Command::MoveWait {
                            player: subject.owner.clone(),
                            unit: subject.id,
                            path: path.clone(),
                        }),
                        "{relative}: unit {} Wait disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.capture,
                        accepts(Command::MoveCapture {
                            player: subject.owner.clone(),
                            unit: subject.id,
                            path: path.clone(),
                        }),
                        "{relative}: unit {} Capture disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.supply,
                        accepts(Command::MoveSupply {
                            player: subject.owner.clone(),
                            unit: subject.id,
                            path: path.clone(),
                        }),
                        "{relative}: unit {} Supply disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.hide,
                        accepts(Command::MoveHide {
                            player: subject.owner.clone(),
                            unit: subject.id,
                            path: path.clone(),
                        }),
                        "{relative}: unit {} Hide disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.reveal,
                        accepts(Command::MoveReveal {
                            player: subject.owner.clone(),
                            unit: subject.id,
                            path: path.clone(),
                        }),
                        "{relative}: unit {} Reveal disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.join,
                        occupant.is_some_and(|target| accepts(Command::MoveJoin {
                            player: subject.owner.clone(),
                            unit: subject.id,
                            path: path.clone(),
                            target,
                        })),
                        "{relative}: unit {} Join disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.load,
                        occupant.is_some_and(|transport| accepts(Command::MoveLoad {
                            player: subject.owner.clone(),
                            unit: subject.id,
                            path: path.clone(),
                            transport,
                        })),
                        "{relative}: unit {} Load disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.explode,
                        accepts(Command::MoveExplode {
                            player: subject.owner.clone(),
                            unit: subject.id,
                            path,
                        }),
                        "{relative}: unit {} Explode disagreed at {destination}",
                        subject.id
                    );
                    checked += 1;
                }
            }
        }
    }

    assert!(
        checked >= 704,
        "expected the full movement corpus, saw {checked} destinations"
    );
}

#[test]
fn a_path_from_a_stale_field_is_validated_again() {
    let source = include_str!("../../../spec/fixtures/movement/infantry-plain-move.json");
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    let mut state: State =
        serde_json::from_value(case["initial_state"].clone()).expect("decode state");
    let unit = state.units[0].id;
    let field = query::reachable(&state, unit).expect("compute field");
    let path = field.path_to(Pos::new(1, 0)).expect("destination has path");
    state.units.get_mut(unit).expect("unit exists").action = UnitAction::Spent;

    assert_eq!(
        query::actions_for_path(&state, unit, path),
        Ok(query::ActionSet::default())
    );
}

#[test]
fn prepared_action_queries_propagate_movement_faults() {
    let source = include_str!("../../../spec/fixtures/movement/infantry-plain-move.json");
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    let state: State = serde_json::from_value(case["initial_state"].clone()).expect("decode state");
    let unit = state.units[0].id;
    let Location::Board { position } = state.units[0].location else {
        panic!("fixture unit is on the board");
    };
    let path = vec![position];

    let mut unsupported = state.clone();
    unsupported.ruleset.revision = RulesetRevision::from("unsupported");
    assert_eq!(
        query::actions_for_path(&unsupported, unit, path.clone()),
        Err(QueryError::Transition(ExecuteError::UnsupportedRuleset))
    );
    assert_eq!(
        query::actions_at(&unsupported, unit, position).map(|actions| actions.attack),
        Err(QueryError::Transition(ExecuteError::UnsupportedRuleset))
    );

    let mut invalid = state;
    invalid.turn.active_player = PlayerId::from("unknown");
    invalid.units[0].owner = PlayerId::from("unknown");
    assert!(matches!(
        query::actions_for_path(&invalid, unit, path.clone()),
        Err(QueryError::Transition(ExecuteError::InvalidState(_)))
    ));
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

#[test]
fn allied_units_are_not_attack_targets_or_forecasts() {
    let case: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spec/fixtures/combat/allied-unit-targets-rejected.json"
    )))
    .unwrap();
    let state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();

    for (attacker, from, ally, ally_position) in [
        (
            UnitId::new(0),
            Pos::new(0, 0),
            UnitId::new(1),
            Pos::new(1, 0),
        ),
        (
            UnitId::new(2),
            Pos::new(1, 1),
            UnitId::new(3),
            Pos::new(2, 1),
        ),
    ] {
        assert!(
            !query::actions_at(&state, attacker, from)
                .unwrap()
                .attack
                .contains(&AttackTarget::Unit { unit: ally })
        );

        let observation = observe(&AwbwVisibility, &state, &PlayerId::from("red")).unwrap();
        assert_eq!(
            query::observed_forecasts(&observation, attacker, from, &[ally_position]).unwrap(),
            vec![None]
        );
    }
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
            let targets = query::actions_at(state, *unit, destination(path))
                .map(|actions| actions.attack)
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
            let observed_query =
                query::ObservedQuery::new(&observation).expect("build observed query");

            for subject in state.units.iter() {
                if subject.owner != recipient || can_act(&state, subject.id) != Ok(Ok(())) {
                    continue;
                }
                let Ok(field) = query::reachable(&state, subject.id) else {
                    continue;
                };
                let destinations: Vec<_> = field
                    .destinations()
                    .map(|(destination, _)| destination)
                    .collect();
                let observed_attacks =
                    query::observed_attacks_from(&observation, subject.id, &destinations)
                        .unwrap_or_else(|error| panic!("{relative}: {error}"));

                for (destination, attacks) in destinations.into_iter().zip(observed_attacks) {
                    let path = field.path_to(destination).expect("destination has path");
                    let authoritative = query::actions_at(&state, subject.id, destination)
                        .map(|actions| query::by_position(&state, actions));
                    let observed = query::actions_at(&reified, subject.id, destination)
                        .map(|actions| query::by_position(&reified, actions));
                    let observed_from_path = observed_query.actions_for_path(subject.id, path);
                    assert_eq!(
                        observed, authoritative,
                        "{relative}: unit {} at {destination:?} disagrees between the \
                         projection and the state it came from",
                        subject.id
                    );
                    assert_eq!(
                        observed_from_path, authoritative,
                        "{relative}: unit {} path query disagrees at {destination:?}",
                        subject.id
                    );
                    assert_eq!(attacks.from, destination);
                    assert_eq!(
                        attacks.targets,
                        observed
                            .as_ref()
                            .expect("observed actions are available")
                            .attack,
                        "{relative}: unit {} batch attacks disagreed at {destination:?}",
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

/// Every attack the corpus resolves must land inside the bracket the forecast
/// showed before it was ordered.
///
/// This is the only property that makes a forecast worth showing. It is checked
/// against the resolved outcome rather than against a second implementation of
/// the formula, so a forecast that agreed with a wrong model would still fail
/// here. Fog states are skipped for the reason `observed_forecasts` documents:
/// a projection can be honestly wrong, and the corpus cannot tell that apart
/// from a broken bracket. An attack on hidden HP must not have a forecast.
#[test]
fn the_forecast_brackets_every_attack_the_corpus_resolves() {
    let mut checked = 0;
    let mut countered = 0;
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

            if let Command::MoveAttack {
                player,
                unit,
                path,
                target,
            } = &command
                && !state.settings.fog
                && let Ok(observation) = observe(&AwbwVisibility, &state, player)
            {
                let from = *path.last().expect("a path holds at least its origin");
                let (target_position, target_unit) = match target {
                    AttackTarget::Tile { position } => (*position, None),
                    AttackTarget::Unit { unit } => {
                        match state.units.get(*unit).map(|found| &found.location) {
                            Some(Location::Board { position }) => (*position, Some(*unit)),
                            _ => continue,
                        }
                    }
                };
                let forecast =
                    query::observed_forecasts(&observation, *unit, from, &[target_position])
                        .expect("a fog-free observation forecasts")
                        .remove(0);
                let Some(forecast) = forecast else {
                    assert!(
                        observation.units.iter().any(|unit| {
                            unit.location
                                == (Location::Board {
                                    position: target_position,
                                })
                                && unit.hp.exact().is_none()
                        }),
                        "{relative}: no forecast for a target with disclosed HP"
                    );
                    state = execution.state;
                    continue;
                };

                let dealt = match target_unit {
                    Some(id) => damage_dealt(&execution.events, id, KnownReason::Combat),
                    None => destructible_damage(&execution.events, target_position),
                };
                // The forecast reports raw damage, so the bracket is limited by
                // what the target had before it is compared with what landed.
                assert!(
                    landed(forecast.attack, forecast.target_hp).contains(&dealt),
                    "{relative}: unit {unit} dealt {dealt} against a forecast of {}-{} \
                     against {} health",
                    forecast.attack.low,
                    forecast.attack.high,
                    forecast.target_hp
                );

                let taken = damage_dealt(&execution.events, *unit, KnownReason::CombatCounter);
                match forecast.counter {
                    Some(range) => {
                        assert!(
                            landed(range, forecast.attacker_hp).contains(&taken),
                            "{relative}: unit {unit} took {taken} against a counter forecast of \
                             {}-{} against {} health",
                            range.low,
                            range.high,
                            forecast.attacker_hp
                        );
                        countered += 1;
                    }
                    None => assert_eq!(
                        taken, 0,
                        "{relative}: unit {unit} took {taken} from a counter the forecast ruled out"
                    ),
                }
                checked += 1;
            }
            state = execution.state;
        }
    }

    assert!(
        checked > 20,
        "expected the corpus to resolve attacks, saw {checked}"
    );
    assert!(
        countered > 0,
        "expected the corpus to resolve counters, saw {countered}"
    );
}

/// The damage a raw bracket can actually land on a target with this health.
///
/// A strike cannot take more than the target has, so the reported overkill
/// collapses onto its health the moment it resolves.
fn landed(range: DamageRange, hp: u8) -> std::ops::RangeInclusive<u8> {
    let cap = |value: u16| u8::try_from(value.min(u16::from(hp))).expect("hp bounds this");
    cap(range.low)..=cap(range.high)
}

/// What one strike took off a unit, or zero when that strike never landed.
fn damage_dealt(events: &[Event], unit: UnitId, reason: KnownReason) -> u8 {
    events
        .iter()
        .find_map(|event| match event {
            Event::UnitDamaged {
                unit: damaged,
                from_hp,
                to_hp,
                reason: cause,
            } if *damaged == unit && *cause == Reason::Known(reason) => {
                Some(from_hp.saturating_sub(*to_hp))
            }
            _ => None,
        })
        .unwrap_or(0)
}

/// What one strike took off a destructible tile.
fn destructible_damage(events: &[Event], position: Pos) -> u8 {
    events
        .iter()
        .find_map(|event| match event {
            Event::DestructibleDamaged {
                position: hit,
                from_hp,
                to_hp,
            } if *hit == position => Some(from_hp.saturating_sub(*to_hp)),
            _ => None,
        })
        .unwrap_or(0)
}
