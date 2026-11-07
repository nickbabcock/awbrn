//! The edit history of a move planner board.
//!
//! Each edit makes a new state from the current one. The history keeps every
//! state, so undo and redo only move a cursor. An edit after an undo drops the
//! states that redo could have reached.
//!
//! An order is an action from the game: a move with its action, a build, an
//! unload, a power activation or the end of a turn. The army whose turn it is
//! gives the order, and the AWVM reducer executes it as in a match, so a
//! power changes the move ranges after it, and an attack charges the power
//! meters. The other edits change the board outside the rules.

use awbrn_game::GameCommand;
use awvm::commander::Domain;
use awvm::event::Event;
use awvm::random::{Entropy, Luck, RandomError};
use awvm::semantic::{CAPTURE_REQUIRED_POINTS, Location, Pos, State, UnitId, WeatherKind};
use awvm::transition::ExecuteOutcome;
use awvm::violation::Violation;
use bevy::prelude::*;

/// A change that the player asks for on a planner board.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum PlannerEdit {
    /// An order from the army whose turn it is, in the words that a match
    /// client sends to its server.
    Order(GameCommand),
    /// Set a unit's visible HP, from 1 to 10.
    SetHp {
        unit: UnitId,
        hp: u8,
    },
    /// Remove a unit, and the units that it carries.
    Delete {
        unit: UnitId,
    },
    Undo,
    Redo,
    /// Go back to the board that the planner file recorded.
    Reset,
}

/// Where each luck roll of a planner order lands in its range.
///
/// The value applies to every roll of a battle, the attack and the counter.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlannerLuck {
    /// The least damage that luck allows.
    Low,
    /// The middle of each luck range.
    #[default]
    Average,
    /// The most damage that luck allows.
    High,
}

impl PlannerLuck {
    fn roll(self, polarity: Luck, domain: Domain) -> i64 {
        // Bad luck removes damage, so its high end is the low roll.
        let (low, high) = match polarity {
            Luck::Good => (domain.minimum, domain.maximum),
            Luck::Bad => (domain.maximum, domain.minimum),
        };
        match self {
            Self::Low => low,
            Self::Average => domain.minimum + (domain.maximum - domain.minimum) / 2,
            Self::High => high,
        }
    }
}

impl Entropy for PlannerLuck {
    fn luck(&mut self, polarity: Luck, domain: Domain) -> Result<i64, RandomError> {
        Ok(self.roll(polarity, domain))
    }

    // A plan does not guess the weather, so random weather stays clear.
    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        Ok(WeatherKind::Clear)
    }
}

/// Why the board refused an edit. A refused edit changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditRefused {
    UnknownUnit(UnitId),
    /// The order does not name a valid unit or action.
    InvalidOrder(String),
    /// The rules do not allow the order.
    Refused(Violation),
    InvalidHp(u8),
    NothingToUndo,
    NothingToRedo,
    InvalidState(String),
}

impl std::fmt::Display for EditRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownUnit(unit) => write!(f, "unit {unit} is not on the planner board"),
            Self::InvalidOrder(error) => write!(f, "the order is not valid: {error}"),
            Self::Refused(violation) => write!(f, "the rules refuse the order: {violation:?}"),
            Self::InvalidHp(hp) => write!(f, "HP {hp} is not between 1 and 10"),
            Self::NothingToUndo => f.write_str("there is no edit to undo"),
            Self::NothingToRedo => f.write_str("there is no edit to redo"),
            Self::InvalidState(error) => write!(f, "the edit makes an invalid board: {error}"),
        }
    }
}

/// How an accepted edit changed the board.
#[derive(Debug, Clone, PartialEq)]
pub enum EditOutcome {
    /// An order was played. `events` tell what happened, from the `prior`
    /// state to the current one.
    Played {
        prior: Box<State>,
        events: Vec<Event>,
    },
    /// The board changed outside the rules, with no events.
    Replaced,
}

/// The states of a planner board, from the loaded file to the latest edit.
#[derive(Resource, Debug, Clone)]
pub struct PlannerBoard {
    history: Vec<State>,
    cursor: usize,
}

impl PlannerBoard {
    pub fn new(initial: State) -> Self {
        Self {
            history: vec![initial],
            cursor: 0,
        }
    }

    /// The board as the player sees it now.
    pub fn current(&self) -> &State {
        &self.history[self.cursor]
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.history.len()
    }

    /// Apply `edit`, and roll each luck value of an order at `luck`. On
    /// success, the current state is the result.
    pub fn apply(
        &mut self,
        edit: &PlannerEdit,
        luck: PlannerLuck,
    ) -> Result<EditOutcome, EditRefused> {
        match edit {
            PlannerEdit::Undo => {
                if !self.can_undo() {
                    return Err(EditRefused::NothingToUndo);
                }
                self.cursor -= 1;
            }
            PlannerEdit::Redo => {
                if !self.can_redo() {
                    return Err(EditRefused::NothingToRedo);
                }
                self.cursor += 1;
            }
            PlannerEdit::Reset => {
                // A reset is an edit like the others, so undo can return
                // to the board from before the reset.
                let initial = self.history[0].clone();
                self.push(initial);
            }
            PlannerEdit::Order(order) => {
                let (next, events) = ordered(self.current(), order, luck)?;
                let prior = self.current().clone();
                self.push(next);
                return Ok(EditOutcome::Played {
                    prior: Box::new(prior),
                    events,
                });
            }
            PlannerEdit::SetHp { unit, hp } => {
                self.push(with_hp(self.current(), *unit, *hp)?);
            }
            PlannerEdit::Delete { unit } => self.push(without(self.current(), *unit)?),
        }
        Ok(EditOutcome::Replaced)
    }

    fn push(&mut self, state: State) {
        self.history.truncate(self.cursor + 1);
        self.history.push(state);
        self.cursor += 1;
    }
}

/// The result of `order` from the active army on `state`, with each luck value
/// at `luck`, and the events of each AWVM command in the order.
fn ordered(
    state: &State,
    order: &GameCommand,
    luck: PlannerLuck,
) -> Result<(State, Vec<Event>), EditRefused> {
    let commands = awbrn_game::awvm_commands(state.turn.active_player.clone(), order, state)
        .map_err(|error| EditRefused::InvalidOrder(error.to_string()))?;
    let mut next = state.clone();
    let mut events = Vec::new();
    for command in commands {
        match awvm::transition::execute_with(&next, command, &mut { luck }) {
            Ok(ExecuteOutcome::Accepted(execution)) => {
                next = execution.state;
                events.extend(execution.events);
            }
            Ok(ExecuteOutcome::Rejected(violation)) => return Err(EditRefused::Refused(violation)),
            Err(error) => return Err(EditRefused::InvalidState(error.to_string())),
        }
    }
    Ok((next, events))
}

/// `state` with the visible HP of `unit` set to `hp`.
///
/// The planner file records visible HP only, so `hp` becomes the whole exact
/// HP `hp * 10`. HP does not change capture progress.
fn with_hp(state: &State, unit: UnitId, hp: u8) -> Result<State, EditRefused> {
    if !(1..=10).contains(&hp) {
        return Err(EditRefused::InvalidHp(hp));
    }
    let mut next = state.clone();
    next.units
        .get_mut(unit)
        .ok_or(EditRefused::UnknownUnit(unit))?
        .hp = hp * 10;
    validated(next)
}

/// `state` without `unit` and the units that it carries.
fn without(state: &State, unit: UnitId) -> Result<State, EditRefused> {
    let subject = state
        .units
        .get(unit)
        .ok_or(EditRefused::UnknownUnit(unit))?;
    let position = match subject.location {
        Location::Board { position } => Some(position),
        Location::Cargo { .. } => None,
    };
    let mut next = state.clone();
    next.units.retain(|candidate| {
        candidate.id != unit
            && !matches!(candidate.location, Location::Cargo { transport, .. } if transport == unit)
    });
    // A removed capturer stops its capture.
    if let Some(position) = position {
        reset_capture(&mut next, position);
    }
    validated(next)
}

/// Stop the capture on `position`, if there is one.
///
/// Only a unit on the tile can capture it, so the caller calls this when the
/// unit on `position` leaves.
fn reset_capture(state: &mut State, position: Pos) {
    if let Some(tile) = state.board.get_mut(position)
        && tile
            .capture_points
            .is_some_and(|points| points < CAPTURE_REQUIRED_POINTS)
    {
        tile.capture_points = Some(CAPTURE_REQUIRED_POINTS);
    }
}

fn validated(state: State) -> Result<State, EditRefused> {
    state
        .validate()
        .map_err(|error| EditRefused::InvalidState(error.to_string()))?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modes::planner::fixture::{ADDER, RACHEL, cowboy};
    use awbrn_game::{PostMoveAction, ServerUnitId};
    use awvm::commander::PowerLevel;
    use awvm::semantic::{PlayerId, PowerState, UnitAction};

    struct Fixture {
        board: PlannerBoard,
        player: PlayerId,
    }

    /// The cowboy board on Blue Moon's turn, with Blue Moon as `co_id` and
    /// `stars` power stars.
    fn fixture_as(co_id: awbrn_types::AwbwCoId, stars: u64) -> Fixture {
        let cowboy = cowboy(co_id, stars);
        let board = PlannerBoard::new(cowboy.state());
        Fixture {
            player: PlayerId::from(cowboy.player.as_u32().to_string()),
            board,
        }
    }

    fn fixture() -> Fixture {
        fixture_as(RACHEL, 0)
    }

    impl Fixture {
        fn state(&self) -> &State {
            self.board.current()
        }

        fn order(&mut self, order: GameCommand) -> Result<EditOutcome, EditRefused> {
            self.board
                .apply(&PlannerEdit::Order(order), PlannerLuck::Average)
        }

        fn edit(&mut self, edit: PlannerEdit) -> Result<EditOutcome, EditRefused> {
            self.board.apply(&edit, PlannerLuck::Average)
        }

        /// Where the infantry, funds and HP missiles aim on the current board.
        fn centers(&self) -> Vec<Pos> {
            awvm::query::area_strikes(self.state(), &self.player, PowerLevel::Scop)
                .unwrap()
                .iter()
                .map(|strike| strike.center)
                .collect()
        }

        fn unit_at(&self, position: Pos) -> UnitId {
            self.state()
                .units
                .iter()
                .find(|unit| unit.location == Location::Board { position })
                .unwrap()
                .id
        }

        fn capture_points(&self, position: Pos) -> Option<u8> {
            self.state().board.tile(position).capture_points
        }

        fn charge(&self) -> u64 {
            let seat = self.state().player_index(&self.player).unwrap();
            self.state().player(seat).commanders[0].power_charge
        }

        /// The reachable tile furthest from where `unit` stands.
        fn far_tile(&self, unit: UnitId) -> Pos {
            let Location::Board { position } = self.state().units.get(unit).unwrap().location
            else {
                panic!("unit {unit} is cargo");
            };
            self.destinations(unit)
                .into_iter()
                .max_by_key(|tile| (tile.distance(position), *tile))
                .unwrap()
        }

        fn destinations(&self, unit: UnitId) -> Vec<Pos> {
            awvm::query::reachable(self.state(), unit)
                .unwrap()
                .destinations()
                .map(|(position, _)| position)
                .collect()
        }

        /// The order that moves `unit` to `to` and does `action` there.
        fn move_order(&self, unit: UnitId, to: Pos, action: PostMoveAction) -> GameCommand {
            let path = awvm::query::reachable(self.state(), unit)
                .unwrap()
                .path_to(to)
                .unwrap();
            GameCommand::MoveUnit {
                unit_id: ServerUnitId(u64::from(unit.get())),
                path,
                action: Some(action),
            }
        }

        /// A ready unit of the active army, a tile that it can move to, and
        /// an enemy that it can attack from there.
        fn move_and_attack(&self) -> (UnitId, Pos, Pos) {
            let state = self.state();
            let seat = state.player_index(&state.turn.active_player).unwrap();
            state
                .units
                .iter()
                .filter(|unit| unit.owner == seat && unit.action == UnitAction::Ready)
                .filter(|unit| matches!(unit.location, Location::Board { .. }))
                .find_map(|unit| {
                    let field = awvm::query::reachable(state, unit.id).ok()?;
                    field.destinations().find_map(|(to, _)| {
                        let path = field.path_to(to)?;
                        state.units.iter().find_map(|target| {
                            let Location::Board { position } = target.location else {
                                return None;
                            };
                            let command = awvm::transition::Command::MoveAttack {
                                player: state.turn.active_player.clone(),
                                unit: unit.id,
                                path: path.clone(),
                                target: awvm::event::AttackTarget::Unit { unit: target.id },
                            };
                            awvm::transition::validate(state, command)
                                .ok()?
                                .is_ok()
                                .then_some((unit.id, to, position))
                        })
                    })
                })
                .expect("the active army has a unit that can move and attack")
        }
    }

    /// An Orange Star rocket in the funds missile's area.
    const ROCKET: Pos = Pos::new(12, 14);
    /// A Blue Moon tank.
    const TANK: Pos = Pos::new(15, 10);
    const BASE_CENTERS: [Pos; 3] = [Pos::new(11, 12), Pos::new(12, 14), Pos::new(12, 13)];

    #[test]
    fn a_move_order_waits_the_unit_until_undo() {
        let mut fixture = fixture();
        let tank = fixture.unit_at(TANK);
        let to = fixture.far_tile(tank);

        let order = fixture.move_order(tank, to, PostMoveAction::Wait);
        let outcome = fixture.order(order).unwrap();

        assert!(matches!(outcome, EditOutcome::Played { ref events, .. } if !events.is_empty()));
        assert_eq!(fixture.unit_at(to), tank);
        let unit = fixture.state().units.get(tank).unwrap();
        assert_eq!(unit.action, UnitAction::Spent);
        assert!(fixture.board.can_undo());

        // A unit that waited cannot move again until an undo.
        let order = GameCommand::MoveUnit {
            unit_id: ServerUnitId(u64::from(tank.get())),
            path: vec![to],
            action: Some(PostMoveAction::Wait),
        };
        assert!(matches!(fixture.order(order), Err(EditRefused::Refused(_))));
        fixture.edit(PlannerEdit::Undo).unwrap();
        let unit = fixture.state().units.get(tank).unwrap();
        assert_eq!(unit.action, UnitAction::Ready);
        assert_eq!(fixture.unit_at(TANK), tank);
    }

    #[test]
    fn only_the_army_whose_turn_it_is_can_order() {
        let mut fixture = fixture();
        let initial = fixture.state().clone();
        // An Orange Star tank, while Blue Moon has the turn.
        let tank = fixture.unit_at(Pos::new(11, 14));
        let order = GameCommand::MoveUnit {
            unit_id: ServerUnitId(u64::from(tank.get())),
            path: vec![Pos::new(11, 14)],
            action: Some(PostMoveAction::Wait),
        };

        assert!(matches!(fixture.order(order), Err(EditRefused::Refused(_))));
        assert_eq!(fixture.state(), &initial);
        assert!(!fixture.board.can_undo());
    }

    #[test]
    fn a_power_with_more_movement_reaches_further() {
        let cost = awvm::commander::power_activation_cost(
            awvm::ruleset::CommanderKind::Adder,
            PowerLevel::Cop,
            0,
        )
        .unwrap()
        .unwrap();
        let star = awvm::commander::power_star_charge(0).unwrap();
        let mut fixture = fixture_as(ADDER, cost / star);
        assert_eq!(fixture.charge(), cost);
        let tank = fixture.unit_at(TANK);
        let before = fixture.destinations(tank);

        fixture
            .order(GameCommand::ActivatePower {
                level: PowerLevel::Cop,
            })
            .unwrap();

        let seat = fixture.state().player_index(&fixture.player).unwrap();
        let player = fixture.state().player(seat);
        assert_eq!(player.power_state, PowerState::Cop { commander_slot: 0 });
        assert_eq!(player.commanders[0].power_charge, 0);
        let after = fixture.destinations(tank);
        let further = after
            .iter()
            .copied()
            .find(|tile| !before.contains(tile))
            .expect("Sideslip gives the tank more tiles");
        assert!(before.iter().all(|tile| after.contains(tile)));

        // The unit can stop on a tile that only the power reaches.
        let order = fixture.move_order(tank, further, PostMoveAction::Wait);
        fixture.order(order).unwrap();
        assert_eq!(fixture.unit_at(further), tank);

        // A second activation in the same turn is refused.
        assert!(matches!(
            fixture.order(GameCommand::ActivatePower {
                level: PowerLevel::Cop
            }),
            Err(EditRefused::Refused(_))
        ));
    }

    #[test]
    fn a_power_without_enough_charge_is_refused() {
        let mut fixture = fixture_as(ADDER, 1);
        let initial = fixture.state().clone();

        assert!(matches!(
            fixture.order(GameCommand::ActivatePower {
                level: PowerLevel::Cop,
            }),
            Err(EditRefused::Refused(Violation::InsufficientPower { .. }))
        ));
        assert_eq!(fixture.state(), &initial);
    }

    #[test]
    fn an_attack_order_charges_the_power_meter_and_luck_sets_the_damage() {
        let fixture = fixture();
        let (unit, to, target_tile) = fixture.move_and_attack();
        let target = fixture.unit_at(target_tile);
        let target_hp = fixture.state().units.get(target).unwrap().hp;

        let battle = |luck| {
            let mut board = fixture.board.clone();
            let order = fixture.move_order(
                unit,
                to,
                PostMoveAction::Attack {
                    target: target_tile,
                },
            );
            board.apply(&PlannerEdit::Order(order), luck).unwrap();
            let state = board.current();
            let attacker = state.units.get(unit).unwrap();
            assert_eq!(attacker.location, Location::Board { position: to });
            assert_eq!(attacker.action, UnitAction::Spent);
            let seat = state.player_index(&fixture.player).unwrap();
            assert!(state.player(seat).commanders[0].power_charge > fixture.charge());
            state.units.get(target).map_or(0, |target| target.hp)
        };
        let (low, average, high) = (
            battle(PlannerLuck::Low),
            battle(PlannerLuck::Average),
            battle(PlannerLuck::High),
        );
        assert!(low < target_hp);
        assert!(low >= average && average >= high, "{low} {average} {high}");
        assert!(low > high);
    }

    #[test]
    fn end_turn_gives_the_turn_to_the_next_army() {
        let mut fixture = fixture();
        let order = fixture.state().turn.order.clone();
        assert_eq!(fixture.state().turn.active_player, fixture.player);

        fixture.order(GameCommand::EndTurn).unwrap();

        assert_eq!(fixture.state().turn.active_player, order[1]);
        fixture.edit(PlannerEdit::Undo).unwrap();
        assert_eq!(fixture.state().turn.active_player, fixture.player);
    }

    #[test]
    fn undo_redo_and_reset_return_exact_states() {
        let mut fixture = fixture();
        let initial = fixture.state().clone();
        let tank = fixture.unit_at(TANK);
        let order = fixture.move_order(tank, fixture.far_tile(tank), PostMoveAction::Wait);
        fixture.order(order).unwrap();
        let moved = fixture.state().clone();
        fixture
            .edit(PlannerEdit::SetHp { unit: tank, hp: 4 })
            .unwrap();
        let damaged = fixture.state().clone();

        fixture.edit(PlannerEdit::Undo).unwrap();
        assert_eq!(fixture.state(), &moved);
        fixture.edit(PlannerEdit::Undo).unwrap();
        assert_eq!(fixture.state(), &initial);
        assert_eq!(
            fixture.edit(PlannerEdit::Undo),
            Err(EditRefused::NothingToUndo)
        );
        fixture.edit(PlannerEdit::Redo).unwrap();
        fixture.edit(PlannerEdit::Redo).unwrap();
        assert_eq!(fixture.state(), &damaged);
        assert_eq!(
            fixture.edit(PlannerEdit::Redo),
            Err(EditRefused::NothingToRedo)
        );

        // A reset can be undone too.
        fixture.edit(PlannerEdit::Reset).unwrap();
        assert_eq!(fixture.state(), &initial);
        fixture.edit(PlannerEdit::Undo).unwrap();
        assert_eq!(fixture.state(), &damaged);

        // An edit after an undo drops the redo branch.
        fixture.edit(PlannerEdit::Undo).unwrap();
        fixture.edit(PlannerEdit::Delete { unit: tank }).unwrap();
        assert!(!fixture.board.can_redo());
    }

    #[test]
    fn hp_and_removal_change_where_the_strikes_aim() {
        let mut fixture = fixture();
        let rocket = fixture.unit_at(ROCKET);

        // A missile removes at most 3 HP, so the funds missile scores a unit
        // by the HP it can remove. Leave every unit in its area with 1 HP,
        // and it aims somewhere else.
        let struck = fixture
            .state()
            .units
            .iter()
            .filter(|unit| {
                matches!(unit.location, Location::Board { position }
                    if position.distance(BASE_CENTERS[1]) <= 2)
            })
            .map(|unit| unit.id)
            .collect::<Vec<_>>();
        for unit in struck {
            fixture.edit(PlannerEdit::SetHp { unit, hp: 1 }).unwrap();
        }
        assert_eq!(fixture.state().units.get(rocket).unwrap().hp, 10);
        assert_ne!(fixture.centers()[1], BASE_CENTERS[1]);
        assert_eq!(
            fixture.edit(PlannerEdit::SetHp {
                unit: rocket,
                hp: 11
            }),
            Err(EditRefused::InvalidHp(11))
        );

        fixture.edit(PlannerEdit::Reset).unwrap();
        fixture.edit(PlannerEdit::Delete { unit: rocket }).unwrap();
        assert!(fixture.state().units.get(rocket).is_none());
        assert_ne!(fixture.centers(), BASE_CENTERS);
    }

    #[test]
    fn removing_a_transport_removes_its_cargo() {
        let mut fixture = fixture();
        let (transport, cargo) = fixture
            .state()
            .units
            .iter()
            .find_map(|unit| match unit.location {
                Location::Cargo { transport, .. } => Some((transport, unit.id)),
                Location::Board { .. } => None,
            })
            .unwrap();

        fixture
            .edit(PlannerEdit::Delete { unit: transport })
            .unwrap();

        let units = &fixture.state().units;
        assert!(units.get(transport).is_none());
        assert!(units.get(cargo).is_none());
    }

    #[test]
    fn a_capturer_that_leaves_or_is_removed_resets_the_capture() {
        let mut fixture = fixture();
        // Two Blue Moon infantry are capturing neutral cities.
        let (moving, removed) = (Pos::new(12, 9), Pos::new(15, 15));
        assert_eq!(fixture.capture_points(moving), Some(10));
        assert_eq!(fixture.capture_points(removed), Some(1));

        let infantry = fixture.unit_at(moving);
        fixture
            .edit(PlannerEdit::SetHp {
                unit: infantry,
                hp: 5,
            })
            .unwrap();
        assert_eq!(fixture.capture_points(moving), Some(10));

        let to = fixture
            .destinations(infantry)
            .into_iter()
            .find(|position| *position != moving)
            .unwrap();
        let order = fixture.move_order(infantry, to, PostMoveAction::Wait);
        fixture.order(order).unwrap();
        assert_eq!(
            fixture.capture_points(moving),
            Some(CAPTURE_REQUIRED_POINTS)
        );

        let infantry = fixture.unit_at(removed);
        fixture
            .edit(PlannerEdit::Delete { unit: infantry })
            .unwrap();
        assert_eq!(
            fixture.capture_points(removed),
            Some(CAPTURE_REQUIRED_POINTS)
        );
    }
}
