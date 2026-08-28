//! The session API against the machinery it replaces, over the whole corpus.
//!
//! A [`Session`] answers the same questions [`query`] and [`execute`] already
//! answer, in a shape a search can afford. That is worth something only if the
//! answers match, so every case below is an equivalence: the mask against the
//! action set, an order against the command it spells, and a rewind against
//! the state it claims to restore.

use std::collections::BTreeSet;
use std::path::PathBuf;

use awvm::conformance::fixture_documents;
use awvm::prelude::*;
use awvm::query;
use awvm::semantic::{CellIdx, Location, TileOwner};
use serde_json::Value;

fn corpus() -> Vec<State> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/fixtures");
    fixture_documents(&root)
        .expect("read fixture corpus")
        .into_iter()
        .flat_map(|(_, case)| {
            ["initial_state", "left", "right"]
                .iter()
                .filter_map(|key| case.get(*key).cloned())
                .map(|value| value.get("initial_state").cloned().unwrap_or(value))
                .filter_map(|value| serde_json::from_value::<State>(value).ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The tile a target stands on, which is how an order names it.
fn cell_of(state: &State, target: &AttackTarget) -> Option<CellIdx> {
    let position = match target {
        AttackTarget::Tile { position } => *position,
        AttackTarget::Unit { unit } => match state.units.get(*unit)?.location {
            Location::Board { position } => position,
            Location::Cargo { .. } => return None,
        },
    };
    state.board.dimensions().cell_index(position)
}

/// Every seat the session offers, collected.
fn seats(legal: &Legal<'_>) -> Vec<UnitIdx> {
    let mut out = Vec::new();
    legal.units(&mut out);
    out
}

/// One target list, collected.
fn target_cells(legal: &Legal<'_>, seat: UnitIdx, cell: CellIdx, kind: TargetKind) -> Vec<CellIdx> {
    let mut out = Vec::new();
    legal.targets(seat, cell, kind, &mut out);
    out
}

/// Every order in a position, collected.
fn all_orders(legal: &Legal<'_>) -> Vec<Order> {
    let mut out = Vec::new();
    legal.orders(&mut out);
    out
}

#[derive(Default)]
struct TypedCollector {
    orders: Vec<Order>,
    attacks: usize,
}

impl LegalVisitor for TypedCollector {
    const ATTACK_CONTEXT: bool = true;

    fn order(&mut self, order: Order) {
        self.orders.push(order);
    }

    fn attack(&mut self, candidate: AttackCandidate<'_>) {
        assert!(matches!(candidate.order.kind(), OrderKind::Attack(_)));
        assert!(candidate.order.unit().is_some());
        self.attacks += 1;
        self.orders.push(candidate.order);
    }
}

#[test]
fn typed_visitation_preserves_the_complete_order_stream() {
    let mut positions = 0;
    let mut attacks = 0;
    for state in corpus() {
        let session = Session::new(state);
        let legal = session.legal();
        let expected = all_orders(&legal);
        let mut visitor = TypedCollector::default();
        legal.visit_orders(&mut visitor);
        assert_eq!(visitor.orders, expected);
        positions += 1;
        attacks += visitor.attacks;
    }
    assert!(positions > 100, "the fixture corpus must contain positions");
    assert!(attacks > 0, "the fixture corpus must contain legal attacks");
}

#[test]
fn scoped_visitation_matches_filtering_the_complete_stream() {
    let mut positions = 0;
    for state in corpus() {
        let session = Session::new(state);
        let legal = session.legal();
        let selected: Vec<_> = seats(&legal)
            .into_iter()
            .step_by(2)
            .map(|unit| unit_id(session.state(), unit))
            .collect();
        for unitless in [false, true] {
            let expected: Vec<_> = all_orders(&legal)
                .into_iter()
                .filter(|order| match order.unit() {
                    Some(unit) => selected.contains(&unit_id(session.state(), unit)),
                    None => unitless,
                })
                .collect();
            let mut visitor = TypedCollector::default();
            legal.visit_scoped(
                LegalScope {
                    units: &selected,
                    unitless,
                },
                &mut visitor,
            );
            assert_eq!(visitor.orders, expected);
        }
        positions += 1;
    }
    assert!(positions > 100, "the fixture corpus must contain positions");
}

#[test]
fn target_oriented_attacks_match_destination_queries() {
    let mut units = 0;
    for state in corpus() {
        let session = Session::new(state);
        let legal = session.legal();
        for unit in seats(&legal) {
            let mut expected = Vec::new();
            if legal.can_delete(unit) {
                let subject = session
                    .state()
                    .units
                    .at(usize::from(unit.get()))
                    .expect("a legal unit exists");
                let Location::Board { position } = subject.location else {
                    continue;
                };
                let cell = session
                    .state()
                    .board
                    .dimensions()
                    .cell_index(position)
                    .expect("a board unit has a cell");
                expected.push(Order::new(unit, cell, OrderKind::Delete));
            }
            let mut destinations = Vec::new();
            legal.field(unit, |field| {
                destinations.extend(field.reach().filter_map(|(position, _)| {
                    session.state().board.dimensions().cell_index(position)
                }));
            });
            for destination in destinations {
                legal.orders_at(unit, destination, &mut expected);
            }
            let mut actual = Vec::new();
            legal.unit_orders(unit, &mut actual);
            assert_eq!(actual, expected);
            units += 1;
        }
    }
    assert!(units > 100, "the fixture corpus must contain legal units");
}

fn unit_id(state: &State, seat: UnitIdx) -> UnitId {
    state
        .units
        .at(usize::from(seat.get()))
        .expect("a seat the session reported")
        .id
}

/// The mask says exactly what the action set says, for every destination of
/// every unit that may still act.
#[test]
fn a_mask_agrees_with_the_action_set() {
    let mut checked = 0_u32;
    for state in corpus() {
        let session = Session::new(state.clone());
        let legal = session.legal();
        for seat in seats(&legal) {
            let unit = unit_id(&state, seat);
            let Ok(field) = query::reachable(&state, unit) else {
                continue;
            };
            let mut masks: Vec<(CellIdx, OrderMask)> = Vec::new();
            legal.destinations(seat, &mut masks);
            // Every tile the unit can arrive at, not only the ones it can
            // come to rest on. A tile a friendly unit stands on admits join
            // and load, and sweeping resting places alone would lose both.
            for (position, _) in field.reach() {
                let actions =
                    query::actions_at(&state, unit, position).expect("enumerate a destination");
                let cell = state
                    .board
                    .dimensions()
                    .cell_index(position)
                    .expect("a destination is on the board");
                let mask = masks
                    .iter()
                    .find(|(candidate, _)| *candidate == cell)
                    .map(|(_, mask)| *mask)
                    .unwrap_or_default();
                assert_eq!(
                    actions.is_empty(),
                    mask.is_empty(),
                    "{position} disagrees on whether anything is legal"
                );
                for (available, kind) in [
                    (actions.wait, OrderKind::Wait),
                    (actions.capture, OrderKind::Capture),
                    (actions.supply, OrderKind::Supply),
                    (actions.hide, OrderKind::Hide),
                    (actions.reveal, OrderKind::Reveal),
                    (actions.explode, OrderKind::Explode),
                    (actions.join, OrderKind::Join),
                    (actions.load, OrderKind::Load),
                ] {
                    assert_eq!(
                        available,
                        mask.allows(kind),
                        "{position} disagrees on {kind:?}"
                    );
                }
                for (targets, kind) in [
                    (!actions.attack.is_empty(), TargetKind::Attack),
                    (!actions.repair.is_empty(), TargetKind::Repair),
                    (!actions.launch.is_empty(), TargetKind::Launch),
                ] {
                    assert_eq!(targets, mask.has(kind), "{position} disagrees on {kind:?}");
                }

                let attacks: BTreeSet<CellIdx> =
                    target_cells(&legal, seat, cell, TargetKind::Attack)
                        .into_iter()
                        .collect();
                let expected: BTreeSet<CellIdx> = actions
                    .attack
                    .iter()
                    .filter_map(|target| cell_of(&state, target))
                    .collect();
                assert_eq!(attacks, expected, "{position} disagrees on attack targets");
                assert_eq!(
                    attacks.len(),
                    expected.len(),
                    "{position} names two attack targets on one tile"
                );

                let repairs: BTreeSet<CellIdx> =
                    target_cells(&legal, seat, cell, TargetKind::Repair)
                        .into_iter()
                        .collect();
                let expected: BTreeSet<CellIdx> = actions
                    .repair
                    .iter()
                    .filter_map(|unit| cell_of(&state, &AttackTarget::Unit { unit: *unit }))
                    .collect();
                assert_eq!(repairs, expected, "{position} disagrees on repair targets");

                let launches: BTreeSet<CellIdx> =
                    target_cells(&legal, seat, cell, TargetKind::Launch)
                        .into_iter()
                        .collect();
                let expected: BTreeSet<CellIdx> = actions
                    .launch
                    .iter()
                    .filter_map(|position| state.board.dimensions().cell_index(*position))
                    .collect();
                assert_eq!(launches, expected, "{position} disagrees on launch targets");
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "the corpus produced no destination to check");
}

/// Every order the session offers is one the reducer accepts, and spelling it
/// and resolving it again names the same order.
#[test]
fn every_offered_order_is_accepted_and_round_trips() {
    let mut applied = 0_u32;
    for state in corpus() {
        let session = Session::new(state.clone());
        let orders: Vec<Order> = all_orders(&session.legal());
        for order in orders {
            let command = session
                .spell(order)
                .unwrap_or_else(|| panic!("{order:?} has no wire form"));
            match execute(&state, command.clone(), &[]) {
                Ok(ExecuteOutcome::Accepted(_)) => {}
                Ok(ExecuteOutcome::Rejected(violation)) => {
                    panic!("{order:?} was offered and refused: {violation:?}")
                }
                // Combat asks for entropy this call did not supply. That the
                // command reached a draw at all is the acceptance being tested.
                Err(ExecuteError::InvalidRandom(_)) => {}
                // Resignation prepares on a one-player fixture and then
                // faults looking for a successor. The disagreement is the
                // reducer's and predates the session. Enumeration reports what
                // preparation accepts.
                Err(ExecuteError::InvalidState(_)) if order.kind() == OrderKind::Resign => {}
                Err(error) => panic!("{order:?} did not execute: {error:?}"),
            }
            assert_eq!(
                session.resolve(&command).ok(),
                Some(order),
                "{command:?} does not resolve back to {order:?}"
            );
            applied += 1;
        }
    }
    assert!(applied > 0, "the corpus offered no order");
}

/// Applying and rewinding leaves the position exactly as it was.
#[test]
fn a_rewind_restores_the_position() {
    let mut rewound = 0_u32;
    for state in corpus() {
        let mut session = Session::new(state.clone());
        let orders: Vec<Order> = all_orders(&session.legal());
        // Resignation and end of turn are legal everywhere and would end the
        // position under test. A unit order is what a search descends
        // through.
        let Some(order) = orders
            .into_iter()
            .find(|order| order.unit().is_some() && !matches!(order.kind(), OrderKind::Delete))
        else {
            continue;
        };
        let Ok(mark) = session.apply(order, &mut RandomTape::new(&[]), &mut ()) else {
            continue;
        };
        assert_ne!(session.state(), &state, "{order:?} changed nothing");
        session.rewind(mark);
        assert_eq!(session.state(), &state, "{order:?} did not rewind cleanly");
        assert_eq!(session.depth(), 0);
        rewound += 1;
    }
    assert!(rewound > 0, "the corpus offered no order to rewind");
}

/// The build menu and the action space agree about what may be built.
///
/// They are two readings of one walk. Because it is one walk, a menu never
/// offers a build the search does not have and never hides one it does.
#[test]
fn the_build_menu_agrees_with_the_action_space() {
    let mut checked = 0_u32;
    for state in corpus() {
        let session = Session::new(state.clone());
        let legal = session.legal();
        let dimensions = state.board.dimensions();
        let offered: BTreeSet<(CellIdx, awvm::ruleset::UnitKind)> = all_orders(&legal)
            .into_iter()
            .filter_map(|order| match order.kind() {
                OrderKind::Produce(kind) => Some((order.destination(), kind)),
                _ => None,
            })
            .collect();

        let mut rows = Vec::new();
        let mut listed = BTreeSet::new();
        for position in dimensions.positions() {
            let Some(cell) = dimensions.cell_index(position) else {
                continue;
            };
            rows.clear();
            legal.production_options(cell, &mut rows);
            // Ordered by price, so a menu reads the cheapest first.
            assert!(
                rows.windows(2).all(|pair| pair[0].cost <= pair[1].cost),
                "{position} lists a build menu out of price order"
            );
            for row in rows.iter().filter(|row| row.affordable) {
                listed.insert((cell, row.kind));
            }
            checked += rows.len() as u32;
        }
        assert_eq!(
            listed, offered,
            "the build menu and the action space disagree"
        );
    }
    assert!(checked > 0, "the corpus produced no build menu at all");
}

/// An order is eight bytes, which is the budget the whole shape rests on.
#[test]
fn an_order_stays_in_a_register() {
    assert_eq!(size_of::<Order>(), 8);
    assert!(size_of::<query::MoveField>() > size_of::<Order>());
}

/// Load one fixture and its first command.
fn fixture(source: &str) -> (State, Command) {
    let case: Value = serde_json::from_str(source).expect("parse fixture");
    (
        serde_json::from_value(case["initial_state"].clone()).expect("decode state"),
        serde_json::from_value(case["steps"][0]["command"].clone()).expect("decode command"),
    )
}

/// The session offers `order`, and applying it lands where `execute` lands.
fn assert_offered_and_equivalent(state: &State, order: Order, command: Command) {
    let mut session = Session::new(state.clone());
    assert!(
        all_orders(&session.legal()).contains(&order),
        "{order:?} was not offered"
    );
    let mut events: Vec<Event> = Vec::new();
    session
        .apply(order, &mut RandomTape::new(&[]), &mut events)
        .unwrap_or_else(|error| panic!("{order:?} was refused: {error:?}"));

    let Ok(ExecuteOutcome::Accepted(expected)) = execute(state, command.clone(), &[]) else {
        panic!("{command:?} is not accepted by the reducer")
    };
    assert_eq!(session.state(), &expected.state, "{order:?} diverged");
    assert_eq!(events, expected.events, "{order:?} emitted other events");
}

/// One destination, two actions. The movement is resolved once and each action
/// is offered and applied against it.
#[test]
fn one_destination_offers_both_wait_and_capture() {
    let (state, command) = fixture(include_str!(
        "../../../spec/fixtures/capture/capture-city-partial.json"
    ));
    let Command::MoveCapture { player, unit, path } = command else {
        panic!("fixture starts with capture")
    };
    let wait = Command::MoveWait {
        player: player.clone(),
        unit,
        path: path.clone(),
    };
    let capture = Command::MoveCapture { player, unit, path };
    let session = Session::new(state.clone());

    for command in [wait, capture] {
        let order = session
            .resolve(&command)
            .expect("resolve the fixture command");
        assert_offered_and_equivalent(&state, order, command);
    }
}

#[test]
fn a_production_site_offers_its_kind() {
    let (state, command) = fixture(include_str!(
        "../../../spec/fixtures/production/produce-infantry-on-base.json"
    ));
    let order = Session::new(state.clone())
        .resolve(&command)
        .expect("resolve production");
    assert!(matches!(order.kind(), OrderKind::Produce(_)));
    assert_offered_and_equivalent(&state, order, command);
}

#[test]
fn an_active_unit_offers_its_deletion() {
    let (state, command) = fixture(include_str!(
        "../../../spec/fixtures/delete/delete-unit-capture-reset.json"
    ));
    let order = Session::new(state.clone())
        .resolve(&command)
        .expect("resolve delete");
    assert_eq!(order.kind(), OrderKind::Delete);
    assert_offered_and_equivalent(&state, order, command);
}

#[test]
fn a_transport_offers_its_cargo_a_destination() {
    let (state, command) = fixture(include_str!(
        "../../../spec/fixtures/transport/unload-infantry-from-apc.json"
    ));
    let order = Session::new(state.clone())
        .resolve(&command)
        .expect("resolve unload");
    assert!(matches!(order.kind(), OrderKind::Unload(_)));
    assert_offered_and_equivalent(&state, order, command);
}

/// A turn boundary is an order like any other. It used to be the one family a
/// caller could not resolve before applying it, and it is the command a search
/// issues most.
#[test]
fn a_turn_boundary_is_offered_and_unsupported_is_a_fault() {
    let (state, _) = fixture(include_str!(
        "../../../spec/fixtures/movement/infantry-plain-move.json"
    ));
    let session = Session::new(state.clone());
    assert!(
        all_orders(&session.legal())
            .iter()
            .any(|order| order.kind() == OrderKind::EndTurn)
    );
    assert!(matches!(
        session.resolve(&Command::Unsupported),
        Err(SessionError::Failed(ExecuteError::UnsupportedCommand))
    ));
}

/// A hidden unit stops the mover short, and the action it was moving to take
/// does not happen.
#[test]
fn a_hidden_trap_suppresses_a_capture() {
    let (mut state, wait) = fixture(include_str!(
        "../../../spec/fixtures/movement/teleporter-hidden-trap.json"
    ));
    let tile = state.board.tile_mut(Pos::new(4, 0));
    tile.terrain = awvm::ruleset::Terrain::City;
    tile.owner = TileOwner::Neutral;
    tile.capture_points = Some(20);
    let Command::MoveWait { player, unit, path } = wait else {
        panic!("fixture starts with move-wait")
    };
    let command = Command::MoveCapture { player, unit, path };

    let mut session = Session::new(state);
    let order = session.resolve(&command).expect("resolve capture");
    let mut events: Vec<Event> = Vec::new();
    session
        .apply(order, &mut RandomTape::new(&[]), &mut events)
        .expect("the capture is accepted");

    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::MovementTrapped { .. }))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::CaptureChanged { .. }))
    );
    assert_eq!(
        session.state().units.get(unit).map(|unit| &unit.location),
        Some(&Location::Board {
            position: Pos::new(0, 0)
        })
    );
}
