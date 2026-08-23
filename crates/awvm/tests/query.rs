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
use awvm::conformance::fixture_documents;
use awvm::prelude::*;
use awvm::query::{self, can_act};
use awvm::semantic::{CellIdx, KnownReason, Location, Reason, Roster, RulesetRevision, UnitAction};
use serde_json::Value;

fn corpus() -> Vec<(String, Value)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/fixtures");
    fixture_documents(&root).expect("read fixture corpus")
}

/// A session on what `player` can see, and the seat `unit` holds in it.
///
/// Every observed-side question below is asked through here, because the
/// session is the one place the rules are stated for a projection.
fn observed(state: &State, player: &PlayerId, unit: UnitId) -> Option<(Session, UnitIdx)> {
    let observation = observe(&AwbwVisibility, state, player).ok()?;
    let session = Session::from_observation(&observation).ok()?;
    let seat = session.index_of(unit)?;
    Some((session, seat))
}

/// The cell `position` names on a session's board.
fn cell_of(session: &Session, position: Pos) -> Option<CellIdx> {
    session.state().board.dimensions().cell_index(position)
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
                    reference(&state, subject.id, state.player_id(subject.owner), origin);

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
                                    player: state.player_id(subject.owner).clone(),
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

/// Every way of asking what is legal at a destination gives the verdict
/// execution gives.
#[test]
fn action_queries_agree_with_execution() {
    let mut checked = 0;
    for (relative, case) in corpus() {
        for state in states(&case) {
            for subject in state.units.iter() {
                if Some(subject.owner) != state.player_index(&state.turn.active_player)
                    || can_act(&state, subject.id) != Ok(Ok(()))
                {
                    continue;
                }
                let field = query::reachable(&state, subject.id).expect("an on-board unit");
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
                            player: state.player_id(subject.owner).clone(),
                            unit: subject.id,
                            path: path.clone(),
                        }),
                        "{relative}: unit {} Wait disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.capture,
                        accepts(Command::MoveCapture {
                            player: state.player_id(subject.owner).clone(),
                            unit: subject.id,
                            path: path.clone(),
                        }),
                        "{relative}: unit {} Capture disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.supply,
                        accepts(Command::MoveSupply {
                            player: state.player_id(subject.owner).clone(),
                            unit: subject.id,
                            path: path.clone(),
                        }),
                        "{relative}: unit {} Supply disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.hide,
                        accepts(Command::MoveHide {
                            player: state.player_id(subject.owner).clone(),
                            unit: subject.id,
                            path: path.clone(),
                        }),
                        "{relative}: unit {} Hide disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.reveal,
                        accepts(Command::MoveReveal {
                            player: state.player_id(subject.owner).clone(),
                            unit: subject.id,
                            path: path.clone(),
                        }),
                        "{relative}: unit {} Reveal disagreed at {destination}",
                        subject.id
                    );
                    assert_eq!(
                        actions.join,
                        occupant.is_some_and(|target| accepts(Command::MoveJoin {
                            player: state.player_id(subject.owner).clone(),
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
                            player: state.player_id(subject.owner).clone(),
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
                            player: state.player_id(subject.owner).clone(),
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

    // A turn naming a player the roster does not hold is a broken state, and
    // every prepared query must report that rather than act on it.
    let mut invalid = state;
    invalid.turn.active_player = PlayerId::from("unknown");
    let unknown = PlayerId::from("unknown");
    assert!(matches!(
        execute(
            &invalid,
            Command::MoveWait {
                player: unknown.clone(),
                unit,
                path: path.clone(),
            },
            &[],
        ),
        Err(ExecuteError::InvalidState(_))
    ));
    // The unit still names its owner by seat, so the query reaches the turn
    // check first and simply offers nothing.
    let offered = query::actions_for_path(&invalid, unit, path.clone())
        .expect("a broken turn is not a query fault");
    assert!(
        !offered.wait,
        "a unit whose turn is not open offers nothing"
    );
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

    let session = Session::from_observation(&observation).expect("the observation reifies");
    let mut options = Vec::new();
    session.legal().production_options(
        cell_of(&session, Pos::new(0, 0)).expect("the site is on the board"),
        &mut options,
    );
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

        let (session, seat) =
            observed(&state, &PlayerId::from("red"), attacker).expect("red sees the attacker");
        assert_eq!(
            session.legal().forecast(
                seat,
                cell_of(&session, from).expect("the firing tile is on the board"),
                cell_of(&session, ally_position).expect("the ally is on the board"),
            ),
            None
        );
    }
}

/// The occupancy index names a hidden unit; the reducer still refuses it.
///
/// Attack enumeration walks the tiles inside the weapon's range and asks the
/// index who stands on each one. The index answers whether or not the moving
/// team sees the occupant, so this holds that concealment is still what
/// decides the offer.
#[test]
fn a_concealed_unit_is_not_an_attack_target() {
    let case: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spec/fixtures/fog/hidden-unit-absence-equivalence.json"
    )))
    .unwrap();
    let mut value = case["left"]["initial_state"].clone();
    // A battleship reaches both the submerged sub at [3,0] and the exposed
    // lander at [4,0] without moving. Map fog off leaves the sub concealed and
    // the lander plainly in sight, so only concealment separates the two.
    value["settings"]["fog"] = Value::Bool(false);
    value["units"][0]["kind"] = Value::String("battleship".into());
    let state: State = serde_json::from_value(value).unwrap();

    let attacks = query::actions_at(&state, UnitId::new(0), Pos::new(0, 0))
        .unwrap()
        .attack;
    assert_eq!(
        attacks,
        vec![AttackTarget::Unit {
            unit: UnitId::new(2)
        }]
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
            let observation = observe(&AwbwVisibility, state, player)
                .unwrap_or_else(|error| panic!("{relative}: could not observe state: {error}"));
            let session = Session::from_observation(&observation)
                .unwrap_or_else(|error| panic!("{relative}: could not reify: {error}"));
            let cell = cell_of(&session, *position)
                .unwrap_or_else(|| panic!("{relative}: {position} is off the board"));
            let mut rows = Vec::new();
            session.legal().production_options(cell, &mut rows);
            assert!(
                rows.iter().any(|row| row.kind == *kind && row.affordable),
                "{relative}: producing {kind} at {position} not offered, saw {rows:?}"
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
    // The fixture seats one player, so the other side is added here.
    let mut opponent = state.players[0].renamed(PlayerId::from("blue"));
    opponent.team = "blue-team".into();
    state.teams.push(awvm::semantic::Team {
        id: "blue-team".into(),
        status: awvm::semantic::TeamStatus::Active,
    });
    state.players = Roster::new(state.players.iter().cloned().chain([opponent]).collect())
        .expect("two players fit a roster");
    let mine = state.units[0].owner;
    state.units[0].owner = state
        .player_index(&PlayerId::from("blue"))
        .expect("the opponent is seated");
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
/// A reified projection must answer exactly what the state it came from does.
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
                if state.player_id(subject.owner) != &recipient
                    || can_act(&state, subject.id) != Ok(Ok(()))
                {
                    continue;
                }
                let Ok(field) = query::reachable(&state, subject.id) else {
                    continue;
                };
                let destinations: Vec<_> = field
                    .destinations()
                    .map(|(destination, _)| destination)
                    .collect();
                let (session, seat) = observed(&state, &recipient, subject.id)
                    .unwrap_or_else(|| panic!("{relative}: the recipient sees its own unit"));
                let session_legal = session.legal();
                let observed_attacks: Vec<Vec<Pos>> = destinations
                    .iter()
                    .map(|from| {
                        let mut cells = Vec::new();
                        if let Some(cell) = cell_of(&session, *from) {
                            session_legal.targets(seat, cell, TargetKind::Attack, &mut cells);
                        }
                        cells
                            .into_iter()
                            .filter_map(|cell| session.state().board.dimensions().position_of(cell))
                            .collect()
                    })
                    .collect();

                for (destination, attacks) in destinations.into_iter().zip(observed_attacks) {
                    let path = field.path_to(destination).expect("destination has path");
                    let authoritative = query::actions_at(&state, subject.id, destination)
                        .map(|actions| query::by_position(&state, actions));
                    let observed = query::actions_at(&reified, subject.id, destination)
                        .map(|actions| query::by_position(&reified, actions));
                    let observed_from_path = query::actions_for_path(&reified, subject.id, path)
                        .map(|actions| query::by_position(&reified, actions));
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
                    let mut expected = observed
                        .as_ref()
                        .expect("observed actions are available")
                        .attack
                        .clone();
                    let mut found = attacks;
                    expected.sort_unstable();
                    found.sort_unstable();
                    assert_eq!(
                        found, expected,
                        "{relative}: unit {} session attacks disagreed at {destination:?}",
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
            let reified = query::reify(&observation).expect("fog-free observation must reify");

            for subject in state
                .units
                .iter()
                .filter(|unit| state.player_id(unit.owner) == &recipient)
            {
                // The session answers only for a unit that may act, the
                // question an interface drawing a range asks. Whether the
                // rebuilt state searches the same for every unit, spent ones
                // included, is the reification's own property, checked against
                // the same search over both states.
                if can_act(&state, subject.id) == Ok(Ok(()))
                    && let Some(through_session) = observed(&state, &recipient, subject.id)
                        .and_then(|(session, seat)| session.legal().field(seat, Clone::clone))
                {
                    let direct = query::reachable(&state, subject.id).expect("an active unit");
                    assert_eq!(
                        through_session.reach().collect::<Vec<_>>(),
                        direct.reach().collect::<Vec<_>>(),
                        "{relative}: unit {} reaches elsewhere through the session",
                        subject.id
                    );
                }

                let authoritative = query::reachable(&state, subject.id);
                let observed = query::reachable(&reified, subject.id);
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
    let (session, seat) = observed(&state, &recipient, UnitId::new(0)).expect("the transport");

    let mut unloads = Vec::new();
    session.legal().unloads(seat, &mut unloads);
    assert_eq!(
        unloads
            .iter()
            .map(|unload| (unload.cargo, unload.cargo_kind, unload.destination))
            .collect::<Vec<_>>(),
        vec![(
            UnitId::new(1),
            UnitKindId::Infantry,
            cell_of(&session, Pos::new(0, 0)).expect("the tile is on the board"),
        )]
    );
}

/// Every attack the corpus resolves must land inside the bracket the forecast
/// showed before it was ordered.
///
/// This is the only property that makes a forecast worth showing. It is checked
/// against the resolved outcome rather than against a second implementation of
/// the formula, so a forecast that agreed with a wrong model would still fail
/// here. Fog states are skipped for the reason [`Legal::forecast`] gives. A
/// projection can be honestly wrong, and the corpus cannot tell that apart
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
                // Only the forecast itself may come back empty here, and only
                // for what the attacker cannot see. A projection that refuses
                // to reify, a mover without a seat in its own observation, or
                // a tile off the board is a fault, not a fog case.
                let (session, seat) = observed(&state, player, *unit)
                    .unwrap_or_else(|| panic!("{relative}: unit {unit} holds no seat"));
                let legal = session.legal();
                let from = cell_of(&session, from)
                    .unwrap_or_else(|| panic!("{relative}: {from} is off the board"));
                let target = cell_of(&session, target_position)
                    .unwrap_or_else(|| panic!("{relative}: {target_position} is off the board"));
                let forecast = legal.forecast(seat, from, target);
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

/// `Travel` against `reachable`, over the whole corpus.
///
/// `reachable` searches forward from one unit and is already held to the
/// reducer's own verdicts by the tests above. `Travel` searches backward from
/// a set of targets, so it is the same movement rules read in the other
/// direction and it needs its own equivalence coverage.
///
/// Two claims, and the second is the one that catches the mistake this search
/// is easy to get wrong. A route walked backward must pay for the tile it came
/// from rather than the tile it arrives at, because movement charges on entry.
/// Charging the wrong end still gives a plausible-looking table: it differs
/// from the truth only by the two endpoint costs, so it is right wherever the
/// endpoints happen to cost the same and quietly wrong everywhere else. The
/// equality below is what refuses it.
#[test]
fn travel_agrees_with_reachable_in_the_other_direction() {
    let mut checked = 0usize;
    let mut exact = 0usize;
    for (name, case) in corpus() {
        for state in states(&case) {
            // The table ignores units in the way, so it can only be a lower
            // bound while anything else stands on the board. With one unit
            // there is nothing to block, and the bound must be tight.
            let alone = state
                .units
                .iter()
                .filter(|unit| matches!(unit.location, Location::Board { .. }))
                .count()
                == 1;
            for unit in state.units.iter() {
                let Location::Board { position: origin } = unit.location else {
                    continue;
                };
                let seat = unit.owner;
                let profile = awvm::ruleset::profile(unit.kind);
                let class = profile.movement_class;
                let allowance =
                    awvm::commander::effective_move(&state, unit, profile.movement, profile.domain)
                        as u16;
                let Some(mut travel) = query::Travel::open(&state, seat) else {
                    continue;
                };
                let Ok(field) = query::reachable(&state, unit.id) else {
                    continue;
                };
                let dimensions = state.board.dimensions();
                let Some(home) = dimensions.cell_index(origin) else {
                    continue;
                };
                let mut points = Vec::new();
                // The corpus holds boards built to exercise a rule rather
                // than to be playable, and one of them stands a battleship on
                // a plain. `reachable` always reports the tile a unit already
                // occupies, whatever it costs to enter, so a unit standing
                // where its own class cannot go has a reach this table is
                // right to call unreachable. Nothing below can say anything
                // about such a unit.
                travel.points_to(class, allowance, [origin], &mut points);
                if points[usize::from(home.get())].is_none() {
                    continue;
                }
                for (destination, forward) in field.reach() {
                    if destination == origin {
                        continue;
                    }
                    travel.points_to(class, allowance, [destination], &mut points);
                    let backward = u64::from(points[usize::from(home.get())]
                        .unwrap_or_else(|| {
                            panic!(
                                "{name}: {origin:?} reaches {destination:?} forward, so a route exists"
                            )
                        }));
                    assert!(
                        backward <= forward,
                        "{name}: travel {backward} beats the reducer's own route {forward} \
                         from {origin:?} to {destination:?}"
                    );
                    checked += 1;
                    if alone {
                        assert_eq!(
                            backward, forward,
                            "{name}: nothing is in the way from {origin:?} to {destination:?}, \
                             so the backward cost must be the forward one"
                        );
                        exact += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 0, "the corpus offered no routes to check");
    assert!(
        exact > 0,
        "no single-unit board was found, so the equality above never ran and \
         the direction of the search is untested"
    );
}
