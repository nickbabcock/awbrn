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

#[test]
fn prepared_visibility_matches_individual_queries() {
    use awvm::semantic::{AwbwVisibility, Viewpoint, Visibility};
    for state in corpus() {
        for (_, player) in state.players.seats() {
            let reference = AwbwVisibility.view(&state, &player.team);
            let prepared = AwbwVisibility.view(&state, &player.team);
            prepared.prepare_board();
            for position in state.board.positions() {
                assert_eq!(prepared.position(position), reference.position(position));
                for unit in state.units.iter() {
                    assert_eq!(
                        prepared.unit_at(unit, position),
                        reference.unit_at(unit, position)
                    );
                }
            }
        }
    }
}

#[test]
fn movement_lists_match_point_queries_with_reused_scratch() {
    let mut scratch = query::MoveScratch::new();
    let mut compared = 0;
    for state in corpus() {
        for unit in state.units.iter() {
            let Ok(field) = query::reachable_into(&state, unit.id, &mut scratch) else {
                continue;
            };
            let expected: Vec<_> = state
                .board
                .positions()
                .filter_map(|position| field.step(position).map(|step| (position, step.cost)))
                .collect();
            assert_eq!(field.reach().collect::<Vec<_>>(), expected);
            let stops: Vec<_> = state
                .board
                .positions()
                .filter_map(|position| {
                    let step = field.step(position)?;
                    assert_eq!(field.can_stop_at(position), step.can_stop);
                    step.can_stop.then_some((position, step.cost))
                })
                .collect();
            assert_eq!(field.destinations().collect::<Vec<_>>(), stops);
            let expected: Vec<_> = field
                .reach()
                .map(|(position, _)| {
                    (
                        state.board.dimensions().cell_index(position).unwrap(),
                        field.can_stop_at(position),
                    )
                })
                .collect();
            let mut cells = Vec::new();
            query::Sweep::open(&state, unit.owner)
                .unwrap()
                .reachable_cells_into(unit.id, &mut scratch, &mut cells)
                .unwrap();
            assert_eq!(cells, expected);
            field.recycle(&mut scratch);
            compared += 1;
        }
    }
    assert!(compared > 100);
}

#[test]
fn movement_masks_match_routes_across_word_boundaries() {
    let mut state = corpus()
        .into_iter()
        .find(|state| !state.units.is_empty())
        .unwrap();
    let mut subject = state.units[0];
    let mut scratch = query::MoveScratch::new();
    let mut cells = Vec::new();
    for width in [1, 2, 63, 64, 65, 127, 128, 129, 255] {
        for height in [1, 3, 7] {
            let terrain = [
                Terrain::Plain,
                Terrain::Road,
                Terrain::Mountain,
                Terrain::Sea,
                Terrain::Teleporter,
                Terrain::Wood,
            ];
            state.board = awvm::semantic::Board::new(
                width,
                height,
                (0..usize::from(width) * usize::from(height))
                    .map(|index| awvm::semantic::Tile::new(terrain[index % terrain.len()]))
                    .collect(),
            )
            .unwrap();
            let dimensions = state.board.dimensions();
            for kind in UnitKind::ALL {
                subject.kind = kind;
                for fuel in [0, 1, 20] {
                    subject.fuel = fuel;
                    for position in [
                        Pos::new(0, 0),
                        Pos::new(width / 2, height / 2),
                        Pos::new(width - 1, height - 1),
                    ] {
                        subject.location = Location::Board { position };
                        state.units = awvm::semantic::UnitStore::new(vec![subject]).unwrap();
                        let sweep = query::Sweep::open(&state, subject.owner).unwrap();
                        let field = sweep.reachable_into(subject.id, &mut scratch).unwrap();
                        sweep
                            .reachable_cells_into(subject.id, &mut scratch, &mut cells)
                            .unwrap();
                        let expected: Vec<_> = field
                            .reach()
                            .map(|(position, _)| {
                                (
                                    dimensions.cell_index(position).unwrap(),
                                    field.can_stop_at(position),
                                )
                            })
                            .collect();
                        assert_eq!(
                            cells, expected,
                            "{width}x{height}, {kind:?}, {fuel}, {position}"
                        );
                        field.recycle(&mut scratch);
                    }
                }
            }
        }
    }
}

#[test]
fn retained_movement_matches_fresh_fields_after_blockers_move() {
    let mut state = corpus()
        .into_iter()
        .find(|state| {
            state.players.len() >= 2 && !seats(&Session::new(state.clone()).legal()).is_empty()
        })
        .unwrap();
    let ours = state.players.seat(&state.turn.active_player).unwrap();
    let other = state
        .players
        .seats()
        .map(|(seat, _)| seat)
        .find(|seat| *seat != ours)
        .unwrap();
    let mut subject = *state
        .units
        .iter()
        .find(|unit| {
            unit.owner == ours
                && unit.action == awvm::semantic::UnitAction::Ready
                && matches!(unit.location, Location::Board { .. })
        })
        .unwrap();
    subject.id = UnitId::new(1);
    subject.kind = UnitKind::Infantry;
    subject.location = Location::Board {
        position: Pos::new(2, 4),
    };
    state.settings.fog = false;
    state.board = awvm::semantic::Board::new(
        17,
        9,
        vec![awvm::semantic::Tile::new(Terrain::Plain); 17 * 9],
    )
    .unwrap();
    let dimensions = state.board.dimensions();
    let mut retained = Session::new(state.clone());
    for owner in [other, ours] {
        for fuel in [0, 1, 3, 99] {
            subject.fuel = fuel;
            for position in [
                Pos::new(14, 8),
                Pos::new(12, 8),
                Pos::new(6, 4),
                Pos::new(5, 4),
                Pos::new(3, 4),
                Pos::new(3, 5),
                Pos::new(14, 8),
                Pos::new(15, 8),
            ] {
                state.units = awvm::semantic::UnitStore::new(vec![
                    subject,
                    Unit {
                        id: UnitId::new(2),
                        owner,
                        location: Location::Board { position },
                        ..subject
                    },
                ])
                .unwrap();
                retained.reset(state.clone());
                let fresh = Session::new(state.clone());
                let unit = retained.index_of(subject.id).unwrap();
                let read = |field: &query::MoveField| {
                    dimensions
                        .positions()
                        .map(|position| (field.step(position), field.path_to(position)))
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    retained.legal().field(unit, read),
                    fresh.legal().field(unit, read),
                    "owner {owner:?}, fuel {fuel}, blocker {position}"
                );
            }
        }
    }
}

#[test]
fn retained_travel_matches_fresh_searches_after_resets() {
    let states = corpus();
    let mut retained = Session::new(states[0].clone());
    let mut checked = 0;
    for state in &states {
        retained.reset(state.clone());
        let targets = [
            Pos::new(0, 0),
            Pos::new(state.board.width() - 1, state.board.height() - 1),
            Pos::new(0, 0),
        ];
        for (seat, _) in state.players.seats() {
            let mut travel = retained.travel(seat).expect("the seat has travel tables");
            let mut fresh = query::Travel::open(state, seat).expect("the seat has travel tables");
            for class in awvm::ruleset::MovementClass::ALL {
                for allowance in [1, 3, 7] {
                    let mut expected = Vec::new();
                    fresh.points_to(class, allowance, targets, &mut expected);
                    let mut actual = Vec::new();
                    travel.points_to(class, allowance, targets, &mut actual);
                    assert_eq!(actual, expected);
                    actual.clear();
                    travel.points_to(class, allowance, targets.into_iter().rev(), &mut actual);
                    assert_eq!(actual, expected);
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 1_000);
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
fn action_filters_preserve_the_selected_order_stream() {
    #[derive(Default)]
    struct Filtered<const MODE: u8>(Vec<Order>);
    impl<const MODE: u8> LegalVisitor for Filtered<MODE> {
        const ACTIONS: OrderMask = match MODE {
            0 => OrderMask::from_untargeted(&[
                OrderKind::Wait,
                OrderKind::Capture,
                OrderKind::Supply,
            ])
            .with_targets(TargetKind::Attack),
            1 => OrderMask::from_untargeted(&[]).with_targets(TargetKind::Attack),
            2 => OrderMask::from_untargeted(&[OrderKind::Capture]).with_targets(TargetKind::Attack),
            _ => OrderMask::from_untargeted(&[OrderKind::Capture]),
        };
        const DELETE: bool = false;
        fn order(&mut self, order: Order) {
            self.0.push(order);
        }
    }
    fn compare<const MODE: u8>(legal: &Legal<'_>, all: &[Order]) {
        let expected: Vec<_> = all
            .iter()
            .copied()
            .filter(|order| {
                order.unit().is_none()
                    || matches!(order.kind(), OrderKind::Unload(_))
                    || Filtered::<MODE>::ACTIONS.allows(order.kind())
            })
            .collect();
        let mut filtered = Filtered::<MODE>::default();
        legal.visit_orders(&mut filtered);
        assert_eq!(filtered.0, expected, "action filter {MODE}");
        filtered.0.clear();
        legal.visit_orders(&mut filtered);
        assert_eq!(filtered.0, expected, "repeated action filter {MODE}");
    }
    for state in corpus() {
        let session = Session::new(state);
        let legal = session.legal();
        let all = all_orders(&legal);
        compare::<0>(&legal, &all);
        compare::<1>(&legal, &all);
        compare::<2>(&legal, &all);
        compare::<3>(&legal, &all);
    }
}

#[test]
fn observed_unit_visits_match_individual_destination_queries() {
    let mut compared = 0;
    for state in corpus() {
        let Ok(view) = awvm::semantic::observe(
            &awvm::semantic::AwbwVisibility,
            &state,
            &state.turn.active_player,
        ) else {
            continue;
        };
        let Ok(session) = Session::from_observation(&view) else {
            continue;
        };
        let legal = session.legal();
        let dimensions = session.state().board.dimensions();
        for unit in seats(&legal) {
            let cells: Vec<_> = legal
                .field(unit, |field| {
                    field
                        .reach()
                        .filter_map(|(pos, _)| dimensions.cell_index(pos))
                        .collect()
                })
                .unwrap_or_default();
            let mut actual = Vec::new();
            legal.unit_orders(unit, &mut actual);
            let mut expected = Vec::new();
            if legal.can_delete(unit) {
                let subject = session
                    .state()
                    .units
                    .at(usize::from(unit.get()))
                    .expect("the unit exists");
                if let Location::Board { position } = subject.location {
                    expected.push(Order::new(
                        unit,
                        dimensions
                            .cell_index(position)
                            .expect("the unit is on the board"),
                        OrderKind::Delete,
                    ));
                }
            }
            for cell in cells {
                legal.orders_at(unit, cell, &mut expected);
            }
            assert_eq!(actual, expected);
            compared += 1;
        }
    }
    assert!(compared > 100);
}

#[test]
fn position_keys_follow_the_session_and_change_on_reset() {
    let state = corpus().remove(0);
    let session = Session::new(state.clone());
    let key = session.position_key();
    assert_ne!(key, Session::new(state.clone()).position_key());
    let mut moved = Box::new(session);
    assert_eq!(key, moved.position_key());
    moved.reset(state);
    assert_ne!(key, moved.position_key());
}

#[test]
fn reset_observations_match_fresh_sessions() {
    let states = corpus();
    let mut retained = Session::new(states[0].clone());
    let mut compared = 0;
    for state in &states {
        for (_, player) in state.players.seats() {
            let Ok(view) =
                awvm::semantic::observe(&awvm::semantic::AwbwVisibility, state, player.id())
            else {
                continue;
            };
            let Ok(fresh) = Session::from_observation(&view) else {
                continue;
            };
            retained
                .reset_from_observation(&view)
                .expect("the observation reifies");
            assert_eq!(retained.state(), fresh.state());
            assert_eq!(retained.depth(), 0);
            assert_eq!(retained.is_commandable(), fresh.is_commandable());
            let key = retained.position_key();
            retained
                .reset_from_observation(&view)
                .expect("the observation reifies again");
            assert_eq!(key, retained.position_key());
            let actual = all_orders(&retained.legal());
            assert_eq!(actual, all_orders(&fresh.legal()));
            for order in actual {
                if let OrderKind::Attack(target) = order.kind() {
                    let unit = order.unit().expect("an attack names a unit");
                    assert_eq!(
                        retained.legal().forecast(unit, order.destination(), target),
                        fresh.legal().forecast(unit, order.destination(), target)
                    );
                }
            }
            compared += 1;
        }
    }
    assert!(compared > 100);
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
        let before = session.position_key();
        let Ok(mark) = session.apply(order, &mut RandomTape::new(&[]), &mut ()) else {
            continue;
        };
        assert_ne!(session.state(), &state, "{order:?} changed nothing");
        let applied_key = session.position_key();
        assert_ne!(before, applied_key);
        session.rewind(mark);
        assert_ne!(session.position_key(), before);
        assert_ne!(session.position_key(), applied_key);
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

#[test]
fn retained_unit_orders_preserve_forecasts_and_destination_limits() {
    #[derive(Default, Debug, PartialEq)]
    struct Limited {
        limit: usize,
        count: usize,
        checks: Vec<UnitIdx>,
        orders: Vec<Order>,
        forecasts: Vec<Option<awvm::combat::Forecast>>,
    }
    impl LegalVisitor for Limited {
        const ACTIONS: OrderMask = OrderMask::from_untargeted(&[
            OrderKind::Wait,
            OrderKind::Capture,
            OrderKind::Supply,
            OrderKind::Join,
            OrderKind::Load,
        ])
        .with_targets(TargetKind::Attack);
        const ATTACK_CONTEXT: bool = true;
        fn visit_unit(&mut self, _: UnitIdx) -> bool {
            self.count = 0;
            true
        }
        fn continue_unit(&mut self, unit: UnitIdx) -> bool {
            self.checks.push(unit);
            self.count += 1;
            self.count <= self.limit
        }
        fn order(&mut self, order: Order) {
            self.orders.push(order);
        }
        fn attack(&mut self, candidate: AttackCandidate<'_>) {
            self.orders.push(candidate.order);
            self.forecasts.push(candidate.forecast);
        }
    }
    for state in corpus() {
        let warm = Session::new(state.clone());
        warm.legal().visit_orders(&mut Limited {
            limit: usize::MAX,
            ..Limited::default()
        });
        for limit in [0, 1, 2, usize::MAX] {
            let fresh = Session::new(state.clone());
            let mut actual = Limited {
                limit,
                ..Limited::default()
            };
            let mut expected = Limited {
                limit,
                ..Limited::default()
            };
            warm.legal().visit_orders(&mut actual);
            fresh.legal().visit_orders(&mut expected);
            assert_eq!(actual, expected);
        }
        let mut retained = warm;
        let orders = all_orders(&retained.legal());
        for order in orders
            .into_iter()
            .filter(|order| order.unit().is_some() && !matches!(order.kind(), OrderKind::Delete))
            .take(4)
        {
            let Ok(mark) = retained.apply(order, &mut RandomTape::new(&[]), &mut ()) else {
                continue;
            };
            for rewind in [false, true] {
                if rewind {
                    retained.rewind(mark);
                }
                let fresh = Session::new(retained.state().clone());
                let mut actual = Limited {
                    limit: usize::MAX,
                    ..Limited::default()
                };
                let mut expected = Limited {
                    limit: usize::MAX,
                    ..Limited::default()
                };
                retained.legal().visit_orders(&mut actual);
                fresh.legal().visit_orders(&mut expected);
                assert_eq!(actual, expected, "order {order:?}, rewind {rewind}");
            }
        }
    }
}
