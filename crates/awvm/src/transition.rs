//! Small authoritative reducer surface used by the conformance protocol.

use serde::{Deserialize, Serialize};

use crate::commander::{self, PowerLevel};
use crate::event::{AttackTarget, Event};
use crate::random::{Luck, RandomTape, RandomToken};
use crate::ruleset::{self, Domain, UnitKind};
use crate::semantic::{
    Location, Match, Outcome, Phase, PlayerId, Pos, State, TerrainId, Unit, UnitId, UnitKindId,
    Viewpoint,
};
use crate::violation::Violation;

mod attack;
mod elimination;
mod movement;
mod powers;
mod property;
mod special;
mod transport;
mod turn;

pub(crate) use attack::*;
pub(crate) use elimination::*;
pub(crate) use movement::*;
pub(crate) use powers::*;
pub(crate) use property::*;
pub(crate) use special::*;
pub(crate) use transport::*;
pub(crate) use turn::*;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Command {
    MoveWait {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
    },
    MoveAttack {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
        target: AttackTarget,
    },
    MoveLaunch {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
        target: Pos,
    },
    MoveExplode {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
    },
    DeleteUnit {
        player: PlayerId,
        unit: UnitId,
    },
    MoveHide {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
    },
    MoveReveal {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
    },
    MoveCapture {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
    },
    ProduceUnit {
        player: PlayerId,
        position: Pos,
        kind: UnitKindId,
    },
    MoveJoin {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
        target: UnitId,
    },
    MoveSupply {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
    },
    MoveRepair {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
        target: UnitId,
    },
    MoveLoad {
        player: PlayerId,
        unit: UnitId,
        path: Vec<Pos>,
        transport: UnitId,
    },
    Unload {
        player: PlayerId,
        transport: UnitId,
        cargo: UnitId,
        destination: Pos,
    },
    ActivatePower {
        player: PlayerId,
        level: PowerLevel,
    },
    Tag {
        player: PlayerId,
    },
    EndTurn {
        player: PlayerId,
    },
    Resign {
        player: PlayerId,
    },
    #[serde(other)]
    Unsupported,
}

impl Command {
    fn player(&self) -> Option<&PlayerId> {
        match self {
            Self::MoveWait { player, .. }
            | Self::MoveAttack { player, .. }
            | Self::MoveLaunch { player, .. }
            | Self::MoveExplode { player, .. }
            | Self::DeleteUnit { player, .. }
            | Self::MoveHide { player, .. }
            | Self::MoveReveal { player, .. }
            | Self::MoveCapture { player, .. }
            | Self::ProduceUnit { player, .. }
            | Self::MoveJoin { player, .. }
            | Self::MoveSupply { player, .. }
            | Self::MoveRepair { player, .. }
            | Self::MoveLoad { player, .. }
            | Self::Unload { player, .. }
            | Self::ActivatePower { player, .. }
            | Self::Tag { player }
            | Self::EndTurn { player }
            | Self::Resign { player } => Some(player),
            Self::Unsupported => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Execution {
    pub state: State,
    pub events: Vec<Event>,
    pub random_consumed: usize,
}

/// The semantic result of evaluating a supported command.
///
/// `Accepted` deliberately stays inline: every successful transition already
/// returns an `Execution`, and boxing it would add an allocation to the hot
/// reducer path solely to shrink the rejection representation.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecuteOutcome {
    Accepted(Execution),
    Rejected(Violation),
}

/// Adapter-owned diagnostic detail for an invalid authoritative state.
///
/// The stable category is [`ExecuteError::InvalidState`]; this prose is not a
/// protocol code and may become more structured as invariant checking grows.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct InvalidStateError(String);

impl From<&str> for InvalidStateError {
    fn from(message: &str) -> Self {
        Self(message.into())
    }
}

impl From<String> for InvalidStateError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// A fault that prevented a command from producing a semantic outcome.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ExecuteError {
    #[error("command is not implemented by this adapter")]
    UnsupportedCommand,
    #[error("only awbw/2026-07-10 is implemented")]
    UnsupportedRuleset,
    #[error("invalid authoritative state: {0}")]
    InvalidState(#[from] InvalidStateError),
    #[error("invalid random input: {0}")]
    InvalidRandom(#[from] crate::random::RandomError),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ReducerError {
    #[error("command is not implemented by this adapter")]
    UnsupportedCommand,
    #[error("command rejected: {0:?}")]
    Violation(Violation),
    #[error("only awbw/2026-07-10 is implemented")]
    UnsupportedRuleset,
    #[error("invalid authoritative state: {0}")]
    InvalidState(#[from] InvalidStateError),
    #[error("invalid random input: {0}")]
    InvalidRandom(#[from] crate::random::RandomError),
}

impl From<Violation> for ReducerError {
    fn from(violation: Violation) -> Self {
        Self::Violation(violation)
    }
}

pub fn execute(
    state: &State,
    command: Command,
    random: &[RandomToken],
) -> Result<ExecuteOutcome, ExecuteError> {
    match reduce(state, command, random) {
        Ok(execution) => Ok(ExecuteOutcome::Accepted(execution)),
        Err(ReducerError::Violation(violation)) => Ok(ExecuteOutcome::Rejected(violation)),
        Err(ReducerError::UnsupportedCommand) => Err(ExecuteError::UnsupportedCommand),
        Err(ReducerError::UnsupportedRuleset) => Err(ExecuteError::UnsupportedRuleset),
        Err(ReducerError::InvalidState(error)) => Err(ExecuteError::InvalidState(error)),
        Err(ReducerError::InvalidRandom(error)) => Err(ExecuteError::InvalidRandom(error)),
    }
}

fn reduce(
    state: &State,
    command: Command,
    random: &[RandomToken],
) -> Result<Execution, ReducerError> {
    let Some(player) = command.player() else {
        return Err(ReducerError::UnsupportedCommand);
    };
    let turn = ActiveTurn::open(state, player)?;
    match command {
        Command::MoveWait { unit, path, .. } => execute_move_wait(&turn, unit, path),
        Command::MoveAttack {
            unit, path, target, ..
        } => execute_move_attack(&turn, unit, path, target, random),
        Command::MoveLaunch {
            unit, path, target, ..
        } => execute_move_launch(&turn, unit, path, target),
        Command::MoveExplode { unit, path, .. } => execute_move_explode(&turn, unit, path),
        Command::DeleteUnit { unit, .. } => execute_delete_unit(&turn, unit),
        Command::MoveHide { unit, path, .. } => execute_move_concealment(&turn, unit, path, true),
        Command::MoveReveal { unit, path, .. } => {
            execute_move_concealment(&turn, unit, path, false)
        }
        Command::MoveCapture { unit, path, .. } => execute_move_capture(&turn, unit, path),
        Command::ProduceUnit { position, kind, .. } => execute_produce_unit(&turn, position, kind),
        Command::MoveJoin {
            unit, path, target, ..
        } => execute_move_join(&turn, unit, path, target),
        Command::MoveSupply { unit, path, .. } => execute_move_supply(&turn, unit, path),
        Command::MoveRepair {
            unit, path, target, ..
        } => execute_move_repair(&turn, unit, path, target),
        Command::MoveLoad {
            unit,
            path,
            transport,
            ..
        } => execute_move_load(&turn, unit, path, transport),
        Command::Unload {
            transport,
            cargo,
            destination,
            ..
        } => execute_unload(&turn, transport, cargo, destination),
        Command::ActivatePower { level, .. } => execute_activate_power(&turn, level),
        Command::Tag { .. } => execute_tag(&turn, random),
        Command::EndTurn { .. } => execute_end_turn(&turn, random),
        Command::Resign { .. } => execute_resign(&turn, random),
        Command::Unsupported => unreachable!("unsupported commands returned before validation"),
    }
}

/// Whether `unit`'s occupancy of its tile is disclosed to the acting team, which
/// is what decides whether it blocks movement.
///
/// This used to test "owner is an ally, or the unit is visible". The first
/// disjunct was redundant: a viewpoint reports a unit of the viewing team as
/// visible wherever it is, which is the same predicate. The disclosure rule is
/// worth naming, so this stays as the name for it.
pub(crate) fn occupancy_is_disclosed(view: &impl Viewpoint, unit: &Unit) -> bool {
    view.unit(unit)
}

pub(crate) fn board_position(unit: &Unit) -> Option<Pos> {
    match unit.location {
        Location::Board { position } => Some(position),
        Location::Cargo { .. } => None,
    }
}

pub(crate) fn complete_match(state: &mut State, outcome: Outcome, events: &mut Vec<Event>) {
    state.match_state = Match::Finished {
        outcome: outcome.clone(),
    };
    state.turn.phase = Phase::Finished;
    events.push(Event::MatchCompleted { outcome });
}

/// The domain a unit presents to commander combat predicates.
///
/// `commander-combat.json` discriminates more finely than `units.json` does:
/// it separates foot soldiers from other ground units and transports from
/// combatants. Only the transport half is derivable from a table, so the foot
/// kinds are named.
pub(crate) fn combat_domain(profile: &ruleset::UnitProfile) -> commander::CombatDomain {
    match profile.kind {
        UnitKind::Infantry | UnitKind::Mech => commander::CombatDomain::Foot,
        _ if profile.transport.is_some() => commander::CombatDomain::Transport,
        _ => match profile.domain {
            Domain::Ground => commander::CombatDomain::GroundVehicle,
            Domain::Air => commander::CombatDomain::Air,
            Domain::Sea => commander::CombatDomain::Naval,
        },
    }
}

pub(crate) fn terrain_repairs_unit(terrain: TerrainId, kind: UnitKindId) -> bool {
    ruleset::terrain_has(terrain, ruleset::profile(kind).domain.repairs())
}

pub(crate) fn refill_unit(unit: &mut Unit) -> bool {
    let profile = ruleset::profile(unit.kind);
    let changed = unit.fuel != profile.max_fuel || unit.ammo != profile.max_ammo;
    unit.fuel = profile.max_fuel;
    unit.ammo = profile.max_ammo;
    changed
}

/// Draw one combat luck roll.
///
/// A malformed combat tape has always reported `UNSUPPORTED_COMMAND` rather than
/// the execution failure `spec/model/violations.md` describes for "missing,
/// wrong-type, or out-of-domain random input". Preserved rather than corrected,
/// so typing the tape stays invisible on the wire; see handoff.md.
pub(crate) fn draw(
    tape: &mut RandomTape<'_>,
    polarity: Luck,
    domain: commander::Domain,
) -> Result<i64, ReducerError> {
    tape.luck(polarity, domain)
        .map_err(|_| ReducerError::UnsupportedCommand)
}

/// Proof that a command got past the checks every unit action shares.
///
/// Holding one means the ruleset is implemented here, the match is live, it is
/// the unit-action phase, and `player` is the player whose turn it is. Those
/// four checks were restated at the top of nine reducers; a reducer that forgot
/// one still compiled and still ran.
///
/// A reducer cannot construct this — [`ActiveTurn::open`] is the only way to
/// get one, and it does the checks.
#[derive(Debug)]
pub(crate) struct ActiveTurn<'a> {
    state: &'a State,
}

impl<'a> ActiveTurn<'a> {
    /// Run the shared checks, in the order `spec/model/violations.md` fixes:
    /// ruleset, then terminal match, then phase, then actor.
    pub(crate) fn open(state: &'a State, player: &PlayerId) -> Result<Self, ReducerError> {
        if !ruleset::supports(&state.ruleset) {
            return Err(ReducerError::UnsupportedRuleset);
        }
        if matches!(state.match_state, Match::Finished { .. }) {
            return Err(violation(Violation::MatchFinished));
        }
        if state.turn.phase != Phase::UnitAction {
            return Err(violation(Violation::WrongPhase {
                expected: Phase::UnitAction,
                actual: state.turn.phase,
            }));
        }
        if state.turn.active_player != player {
            return Err(violation(Violation::NotActivePlayer {
                player: player.clone(),
            }));
        }
        Ok(Self { state })
    }

    pub(crate) const fn state(&self) -> &'a State {
        self.state
    }

    pub(crate) const fn player(&self) -> &'a PlayerId {
        &self.state.turn.active_player
    }

    /// Validate a movement and yield the proof that it was validated.
    ///
    /// Forwards to [`movement::plan`], which is the only constructor of a
    /// [`MovedUnit`], so no reducer can act on an unvalidated path.
    pub(crate) fn plan_move(
        &self,
        unit: UnitId,
        path: Vec<Pos>,
    ) -> Result<MovedUnit, ReducerError> {
        movement::plan(self, unit, path)
    }
}

pub(crate) fn violation(violation: Violation) -> ReducerError {
    ReducerError::Violation(violation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Weapon;
    use crate::event::{EventKind, SupplySource};
    use crate::semantic::{
        AwbwVisibility, Board, Concealment, KnownReason, ObservedEvent, ObservedUnitRef,
        PlayerStatus, ReasonId, Silo, Tile, TileOwner, UnitAction, VictoryReason, Visibility,
    };
    use crate::violation::Action;
    use serde_json::{Value, json};

    fn execute(
        state: &State,
        command: Command,
        random: &[RandomToken],
    ) -> Result<Execution, ReducerError> {
        reduce(state, command, random)
    }

    /// Replace the board with a single row of `tiles`.
    ///
    /// Every fixture these tests reshape is one row high. Going through
    /// [`Board::new`] means a test cannot leave `width` disagreeing with the
    /// tiles it supplied, which the old `width = n; tiles[0] = …` pair could.
    fn set_row(state: &mut State, tiles: Vec<Tile>) {
        let width = u8::try_from(tiles.len()).expect("a test board fits in a byte");
        state.board = Board::new(width, 1, tiles).expect("a single row is a rectangle");
    }

    /// The board's tiles, for tests that grow or shrink the row.
    fn row(state: &State) -> Vec<Tile> {
        state.board.tiles().cloned().collect()
    }

    /// The unit an event is about, for tests that assert which units an
    /// operation touched and in what order.
    fn event_unit(event: &Event) -> UnitId {
        match event {
            Event::UnitDamaged { unit, .. }
            | Event::UnitRemoved { unit, .. }
            | Event::UnitRepaired { unit, .. }
            | Event::UnitResourced { unit, .. } => *unit,
            other => panic!("{} names no single unit", other.kind()),
        }
    }

    /// The weapon an `attack-resolved` or ammo-spending event reports.
    fn event_weapon(event: &Event) -> Weapon {
        match event {
            Event::AttackResolved { weapon, .. } => *weapon,
            other => panic!("{} reports no weapon", other.kind()),
        }
    }

    /// The `(before, after)` ammo an event records.
    fn event_ammo(event: &Event) -> (u64, u64) {
        match event {
            Event::UnitResourced {
                ammo_before,
                ammo_after,
                ..
            } => (*ammo_before, *ammo_after),
            other => panic!("{} records no ammo", other.kind()),
        }
    }

    /// A state with one ready red infantry at the origin of a `width`-wide row.
    fn movement_state(width: usize) -> State {
        let mut state = direct_combat_state(width);
        state.units[0].id = UnitId::new(0);
        state
    }

    /// The four checks `ActiveTurn::open` folds together, each in isolation.
    ///
    /// These were restated at the top of nine reducers and only ever reached
    /// through `execute`; `open` is the single place they live now, so this is
    /// the first time they can be exercised directly.
    #[test]
    fn opening_a_turn_checks_ruleset_then_match_then_phase_then_actor() {
        let base = movement_state(3);
        let red = PlayerId::from("red");
        let blue = PlayerId::from("blue");
        assert!(ActiveTurn::open(&base, &red).is_ok());

        let mut wrong_ruleset = base.clone();
        wrong_ruleset.ruleset.revision = "1999-01-01".into();
        assert_eq!(
            ActiveTurn::open(&wrong_ruleset, &red).unwrap_err(),
            ReducerError::UnsupportedRuleset
        );

        let mut finished = base.clone();
        finished.match_state = Match::Finished {
            outcome: Outcome::Cancelled {
                reason: ReasonId::from("aborted"),
            },
        };
        assert_eq!(
            ActiveTurn::open(&finished, &red).unwrap_err(),
            violation(Violation::MatchFinished)
        );

        let mut wrong_phase = base.clone();
        wrong_phase.turn.phase = Phase::TurnEnd;
        assert_eq!(
            ActiveTurn::open(&wrong_phase, &red).unwrap_err(),
            violation(Violation::WrongPhase {
                expected: Phase::UnitAction,
                actual: Phase::TurnEnd,
            })
        );

        assert_eq!(
            ActiveTurn::open(&base, &blue).unwrap_err(),
            violation(Violation::NotActivePlayer {
                player: PlayerId::from("blue"),
            })
        );
    }

    /// A terminal match is refused before the phase is even considered, which
    /// is the precedence `spec/model/violations.md` fixes.
    #[test]
    fn a_finished_match_outranks_a_wrong_phase() {
        let mut state = movement_state(3);
        state.turn.phase = Phase::TurnEnd;
        state.match_state = Match::Finished {
            outcome: Outcome::Cancelled {
                reason: ReasonId::from("aborted"),
            },
        };
        assert_eq!(
            ActiveTurn::open(&state, &PlayerId::from("red")).unwrap_err(),
            violation(Violation::MatchFinished)
        );
    }

    fn plan_for(state: &State, path: Vec<Pos>) -> Result<(), ReducerError> {
        ActiveTurn::open(state, &PlayerId::from("red"))?.plan_move(UnitId::new(0), path)?;
        Ok(())
    }

    /// Path validation used to exist in three verbatim copies reachable only
    /// through `execute`. Several of these codes have no fixture at all, so
    /// these assertions are their only coverage.
    #[test]
    fn planning_a_move_rejects_every_malformed_path() {
        let state = movement_state(4);
        let origin = Pos::new(0, 0);
        assert!(plan_for(&state, vec![origin, Pos::new(1, 0)]).is_ok());

        assert_eq!(
            plan_for(&state, vec![Pos::new(1, 0), Pos::new(2, 0)]).unwrap_err(),
            violation(Violation::PathOriginMismatch {
                expected: origin,
                actual: Pos::new(1, 0),
            })
        );
        assert_eq!(
            plan_for(&state, vec![origin, Pos::new(2, 0)]).unwrap_err(),
            violation(Violation::PathNonAdjacent {
                index: 1,
                from: origin,
                to: Pos::new(2, 0),
            })
        );
        assert_eq!(
            plan_for(&state, vec![origin, Pos::new(1, 0), origin]).unwrap_err(),
            violation(Violation::PathRepeatedPosition {
                index: 2,
                position: origin,
                first_index: 0,
            })
        );
        assert_eq!(
            plan_for(&state, vec![origin, Pos::new(0, 1)]).unwrap_err(),
            violation(Violation::PathOutOfBounds {
                index: 1,
                position: Pos::new(0, 1),
            })
        );
    }

    /// An empty path names no origin, so it fails the origin check rather than
    /// being treated as a move to nowhere.
    #[test]
    fn planning_an_empty_move_fails_the_origin_check() {
        let state = movement_state(3);
        assert_eq!(
            plan_for(&state, Vec::new()).unwrap_err(),
            violation(Violation::PathOriginMismatch {
                expected: Pos::new(0, 0),
                actual: Pos::new(0, 0),
            })
        );
    }

    /// Movement points and fuel are checked against the whole route, and
    /// movement is checked first.
    #[test]
    fn planning_a_move_checks_movement_before_fuel() {
        let state = movement_state(6);
        let long: Vec<Pos> = (0..6).map(|x| Pos::new(x, 0)).collect();
        assert_eq!(
            plan_for(&state, long.clone()).unwrap_err(),
            violation(Violation::InsufficientMovement {
                required: 5,
                available: 3,
            })
        );

        let mut thirsty = movement_state(6);
        thirsty.units[0].fuel = 1;
        // Within the move allowance, so fuel is what runs out.
        let short: Vec<Pos> = (0..4).map(|x| Pos::new(x, 0)).collect();
        assert_eq!(
            plan_for(&thirsty, short).unwrap_err(),
            violation(Violation::InsufficientFuel {
                required: 3,
                available: 1,
            })
        );
    }

    #[test]
    fn teleporters_cross_at_zero_cost_but_cannot_be_destinations() {
        let mut state = movement_state(6);
        state.players[0].commanders[0].id = crate::semantic::CommanderId::Sturm;
        let plain = state.board.tile(Pos::new(0, 0)).clone();
        let mut teleporter = plain.clone();
        teleporter.terrain = TerrainId::Teleporter;
        set_row(
            &mut state,
            vec![
                plain.clone(),
                teleporter.clone(),
                teleporter.clone(),
                teleporter.clone(),
                teleporter,
                plain,
            ],
        );

        let destination = Pos::new(5, 0);
        let path: Vec<_> = (0..=destination.x).map(|x| Pos::new(x, 0)).collect();
        let command: Command = serde_json::from_value(json!({
            "type":"move-wait", "player":"red", "unit":0, "path":path
        }))
        .unwrap();
        let result = execute(&state, command, &[]).unwrap();

        assert_eq!(
            board_position(result.state.units.get(UnitId::new(0)).unwrap()),
            Some(destination)
        );
        assert_eq!(result.state.units[0].fuel, state.units[0].fuel - 1);
        assert!(matches!(
            result.events.as_slice(),
            [Event::UnitMoved { fuel_spent: 1, .. }]
        ));

        assert_eq!(
            plan_for(&state, vec![Pos::new(0, 0), Pos::new(1, 0), Pos::new(2, 0)]).unwrap_err(),
            violation(Violation::TerrainImpassable {
                index: Some(2),
                position: Pos::new(2, 0),
            })
        );
    }

    #[test]
    fn a_hidden_trap_rolls_back_over_trailing_teleporters() {
        let mut state = direct_combat_state(5);
        state.settings.fog = true;
        let plain = state.board.tile(Pos::new(0, 0)).clone();
        let mut teleporter = plain.clone();
        teleporter.terrain = TerrainId::Teleporter;
        set_row(
            &mut state,
            vec![
                plain.clone(),
                teleporter.clone(),
                teleporter,
                plain.clone(),
                plain,
            ],
        );
        let mut blocker = state.units[0].clone();
        blocker.id = UnitId::new(1);
        blocker.owner = "blue".into();
        blocker.location = Location::Board {
            position: Pos::new(3, 0),
        };
        state.units.push(blocker);
        let path: Vec<_> = (0..5).map(|x| Pos::new(x, 0)).collect();
        let command: Command = serde_json::from_value(json!({
            "type":"move-wait", "player":"red", "unit":0, "path":path
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert_eq!(
            board_position(result.state.units.get(UnitId::new(0)).unwrap()),
            Some(Pos::new(0, 0))
        );
        assert_eq!(result.state.units[0].fuel, state.units[0].fuel);
        assert!(matches!(
            result.events.as_slice(),
            [
                Event::UnitMoved {
                    to,
                    fuel_spent: 0,
                    ..
                },
                Event::MovementTrapped {
                    position,
                    ..
                }
            ] if *to == Pos::new(0, 0) && *position == Pos::new(3, 0)
        ));
    }

    #[test]
    fn cargo_cannot_be_unloaded_onto_a_teleporter() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/transport/unload-infantry-from-apc.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.board.tile_mut(Pos::new(0, 0)).terrain = TerrainId::Teleporter;
        let command: Command = serde_json::from_value(case["steps"][0]["command"].clone()).unwrap();

        assert_eq!(
            execute(&state, command, &[]).unwrap_err(),
            violation(Violation::TerrainImpassable {
                index: None,
                position: Pos::new(0, 0),
            })
        );
    }

    /// A unit that has already acted cannot move, and that is checked before
    /// the path is looked at.
    #[test]
    fn planning_a_move_refuses_a_spent_unit() {
        let mut state = movement_state(3);
        state.units[0].action = UnitAction::Spent;
        assert_eq!(
            plan_for(&state, vec![Pos::new(9, 9)]).unwrap_err(),
            violation(Violation::UnitAlreadyActed {
                unit: UnitId::new(0)
            })
        );
    }

    fn direct_combat_state(width: usize) -> State {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/movement/infantry-plain-move.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        let plain = state.board.tile(Pos::new(0, 0)).clone();
        set_row(&mut state, vec![plain; width]);
        state.teams.push(crate::semantic::Team {
            id: "blue-team".into(),
            status: crate::semantic::TeamStatus::Active,
        });
        let mut blue = state.players[0].clone();
        blue.id = "blue".into();
        blue.team = "blue-team".into();
        blue.commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.players[0].commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.players.push(blue);
        state.units[0].id = UnitId::new(0);
        state
    }

    #[test]
    fn scalar_power_activation_validates_availability_and_scaled_cost() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/commander/adder-power-activation.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.players[0].commanders[0].power_uses = 1;
        state.players[0].commanders[0].power_charge = 21_599;
        let activate = || {
            serde_json::from_value(json!({
                "type":"activate-power", "player":"red", "level":"cop"
            }))
            .unwrap()
        };

        assert_eq!(
            execute(&state, activate(), &[]),
            Err(violation(Violation::InsufficientPower {
                required: 21600,
                available: 21599
            }))
        );

        state.settings.powers = crate::semantic::Toggle::Disabled;
        assert_eq!(
            execute(&state, activate(), &[]),
            Err(violation(Violation::ActionNotSupported {
                action: Action::ActivatePower
            }))
        );
    }

    #[test]
    fn random_weather_rejects_missing_and_wrong_tokens() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/turn-hooks/random-weather-outcomes.json"
        ))
        .unwrap();
        let state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        let command: Command = serde_json::from_value(case["steps"][0]["command"].clone()).unwrap();

        for random in [vec![], vec![RandomToken::CombatGoodLuck(0)]] {
            assert!(
                matches!(
                    execute(&state, command.clone(), &random),
                    Err(ReducerError::InvalidRandom(_))
                ),
                "unexpectedly accepted random input {random:?}"
            );
        }
    }

    #[test]
    fn cargo_is_supplied_before_crashing_transport_removes_it() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/turn-hooks/fuel-upkeep-and-crash.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.units[0].kind = UnitKindId::Carrier;
        state.units[0].fuel = 0;
        state.units[0].ammo = 9;
        state.units[1].kind = UnitKindId::Fighter;
        state.units[1].fuel = 30;
        state.units[1].ammo = 2;
        state.units[1].location = Location::Cargo {
            transport: UnitId::new(0),
            slot: 0,
        };
        let command: Command = serde_json::from_value(case["steps"][0]["command"].clone()).unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert!(!result.state.units.contains(UnitId::new(0)));
        assert!(!result.state.units.contains(UnitId::new(1)));
        assert_eq!(
            result.events[4..7],
            [
                Event::AutomaticSupply {
                    source: SupplySource::Unit(UnitId::new(0)),
                    units: vec![UnitId::new(1)]
                },
                Event::UnitRemoved {
                    unit: UnitId::new(0),
                    reason: KnownReason::FuelDepleted.into()
                },
                Event::UnitRemoved {
                    unit: UnitId::new(1),
                    reason: KnownReason::CarrierLost.into()
                },
            ]
        );
    }

    #[test]
    fn capture_move_resets_origin_before_attempting_destination() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-partial.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        let mut destination = state.board.tile(Pos::new(0, 0)).clone();
        destination.capture_points = Some(20);
        state.board.tile_mut(Pos::new(0, 0)).capture_points = Some(10);
        let mut tiles = row(&state);
        tiles.push(destination);
        set_row(&mut state, tiles);
        let command: Command = serde_json::from_value(json!({
            "type":"move-capture", "player":"red", "unit":0,
            "path":[Pos::new(0, 0),Pos::new(1, 0)]
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert_eq!(
            result.state.board.tile(Pos::new(0, 0)).capture_points,
            Some(20)
        );
        assert_eq!(
            result.state.board.tile(Pos::new(1, 0)).capture_points,
            Some(10)
        );
        assert_eq!(
            result.events,
            vec![
                Event::CaptureChanged {
                    position: Pos::new(0, 0),
                    from: 10,
                    to: 20
                },
                Event::UnitMoved {
                    unit: UnitId::new(0),
                    from: Pos::new(0, 0),
                    to: Pos::new(1, 0),
                    path: vec![Pos::new(0, 0), Pos::new(1, 0)],
                    fuel_spent: 1
                },
                Event::CaptureChanged {
                    position: Pos::new(1, 0),
                    from: 20,
                    to: 10
                },
            ]
        );
    }

    #[test]
    fn hidden_destination_occupant_traps_and_suppresses_capture() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-complete.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.settings.fog = true;
        let mut plain = state.board.tile(Pos::new(0, 0)).clone();
        plain.terrain = TerrainId::Plain;
        plain.owner = TileOwner::NotOwnable;
        plain.capture_points = None;
        let destination = state.board.tile(Pos::new(0, 0)).clone();
        set_row(
            &mut state,
            vec![plain.clone(), plain.clone(), plain, destination],
        );
        let mut blocker = state.units[0].clone();
        blocker.id = UnitId::new(1);
        blocker.kind = UnitKindId::Tank;
        blocker.owner = "blue".into();
        blocker.location = Location::Board {
            position: Pos::new(3, 0),
        };
        state.units[0].location = Location::Board {
            position: Pos::new(0, 0),
        };
        state.units.push(blocker);
        let command: Command = serde_json::from_value(json!({
            "type":"move-capture", "player":"red", "unit":0,
            "path":[Pos::new(0, 0),Pos::new(1, 0),Pos::new(2, 0),Pos::new(3, 0)]
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert_eq!(
            board_position(result.state.units.get(UnitId::new(0)).unwrap()),
            Some(Pos::new(2, 0))
        );
        assert_eq!(
            result.state.board.tile(Pos::new(3, 0)).capture_points,
            Some(10)
        );
        assert_eq!(
            result.events,
            vec![
                Event::UnitMoved {
                    unit: UnitId::new(0),
                    from: Pos::new(0, 0),
                    to: Pos::new(2, 0),
                    path: vec![Pos::new(0, 0), Pos::new(1, 0), Pos::new(2, 0)],
                    fuel_spent: 2
                },
                Event::MovementTrapped {
                    unit: UnitId::new(0),
                    blocker: UnitId::new(1),
                    position: Pos::new(3, 0)
                },
            ]
        );
    }

    #[test]
    fn hidden_destination_occupant_traps_and_suppresses_concealment() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-complete.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.settings.fog = true;
        let mut plain = state.board.tile(Pos::new(0, 0)).clone();
        plain.terrain = TerrainId::Plain;
        plain.owner = TileOwner::NotOwnable;
        plain.capture_points = None;
        set_row(&mut state, vec![plain; 7]);
        state.units[0].kind = UnitKindId::Stealth;
        state.units[0].fuel = 55;
        state.units[0].ammo = 6;
        state.units[0].location = Location::Board {
            position: Pos::new(0, 0),
        };
        let mut blocker = state.units[0].clone();
        blocker.id = UnitId::new(1);
        blocker.kind = UnitKindId::Tank;
        blocker.owner = "blue".into();
        blocker.concealment = Concealment::Exposed;
        blocker.location = Location::Board {
            position: Pos::new(6, 0),
        };
        state.units.push(blocker);
        let command: Command = serde_json::from_value(json!({
            "type":"move-hide", "player":"red", "unit":0,
            "path":[Pos::new(0, 0),Pos::new(1, 0),Pos::new(2, 0),Pos::new(3, 0),Pos::new(4, 0),Pos::new(5, 0),Pos::new(6, 0)]
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();
        let stealth = result.state.units.get(UnitId::new(0)).unwrap();

        assert_eq!(board_position(stealth), Some(Pos::new(5, 0)));
        assert_eq!(stealth.concealment, Concealment::Exposed);
        assert_eq!(
            result.events,
            vec![
                Event::UnitMoved {
                    unit: UnitId::new(0),
                    from: Pos::new(0, 0),
                    to: Pos::new(5, 0),
                    path: vec![
                        Pos::new(0, 0),
                        Pos::new(1, 0),
                        Pos::new(2, 0),
                        Pos::new(3, 0),
                        Pos::new(4, 0),
                        Pos::new(5, 0)
                    ],
                    fuel_spent: 5
                },
                Event::MovementTrapped {
                    unit: UnitId::new(0),
                    blocker: UnitId::new(1),
                    position: Pos::new(6, 0)
                },
            ]
        );
    }

    #[test]
    fn move_launch_damages_all_board_units_in_stable_order_without_charge() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-complete.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        let mut plain = state.board.tile(Pos::new(0, 0)).clone();
        plain.terrain = TerrainId::Plain;
        plain.owner = TileOwner::NotOwnable;
        plain.capture_points = None;
        plain.silo = None;
        let mut silo = plain.clone();
        silo.terrain = TerrainId::MissileSilo;
        silo.silo = Some(Silo::Ready);
        let base = state.board.tile(Pos::new(0, 0)).clone();
        set_row(
            &mut state,
            vec![plain, silo, base.clone(), base.clone(), base.clone(), base],
        );
        state.units[0].location = Location::Board {
            position: Pos::new(0, 0),
        };
        state.units[0].hp = 20;
        let mut ally = state.units[0].clone();
        ally.id = UnitId::new(1);
        ally.location = Location::Board {
            position: Pos::new(2, 0),
        };
        ally.hp = 100;
        let mut enemy = ally.clone();
        enemy.id = UnitId::new(2);
        enemy.owner = "blue".into();
        enemy.location = Location::Board {
            position: Pos::new(5, 0),
        };
        enemy.hp = 10;
        state.units.extend([ally, enemy]);
        state.settings.fog = true;
        let command: Command = serde_json::from_value(json!({
            "type":"move-launch", "player":"red", "unit":0,
            "path":[Pos::new(0, 0),Pos::new(1, 0)], "target":Pos::new(4, 0)
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert_eq!(
            result.state.board.tile(Pos::new(1, 0)).silo,
            Some(Silo::Spent)
        );
        assert_eq!(result.state.players[0].commanders[0].power_charge, 0);
        assert_eq!(result.state.units.get(UnitId::new(0)).unwrap().hp, 1);
        assert_eq!(result.state.units.get(UnitId::new(1)).unwrap().hp, 70);
        assert_eq!(result.state.units.get(UnitId::new(2)).unwrap().hp, 1);
        let types: Vec<_> = result.events.iter().map(Event::kind).collect();
        assert_eq!(
            types,
            vec![
                EventKind::UnitMoved,
                EventKind::AreaStrikeResolved,
                EventKind::UnitDamaged,
                EventKind::UnitDamaged,
                EventKind::UnitDamaged,
                EventKind::SiloChanged
            ]
        );
        assert_eq!(event_unit(&result.events[2]), UnitId::new(0));
        assert_eq!(event_unit(&result.events[3]), UnitId::new(1));
        assert_eq!(event_unit(&result.events[4]), UnitId::new(2));
        let observed = crate::semantic::observe_events(
            &AwbwVisibility,
            &state,
            &result.state,
            &result.events,
            &PlayerId::from("red"),
        )
        .unwrap();
        let hidden_ally = ObservedUnitRef::Friendly {
            unit: UnitId::new(2),
        };
        assert!(observed.iter().all(|event| !matches!(
            event,
            ObservedEvent::UnitChanged { unit, .. }
                | ObservedEvent::UnitMoved { unit, .. }
                | ObservedEvent::UnitRemoved { unit, .. }
                | ObservedEvent::UnitDisappeared { unit, .. }
                | ObservedEvent::MovementStopped { unit }
            if *unit == hidden_ally
        )));
    }

    #[test]
    fn move_explode_damages_other_units_then_removes_bomb_without_charge() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-complete.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        let mut plain = state.board.tile(Pos::new(0, 0)).clone();
        plain.terrain = TerrainId::Plain;
        plain.owner = TileOwner::NotOwnable;
        plain.capture_points = None;
        plain.silo = None;
        set_row(&mut state, vec![plain; 7]);
        state.units[0].id = UnitId::new(0);
        state.units[0].kind = UnitKindId::BlackBomb;
        state.units[0].location = Location::Board {
            position: Pos::new(0, 0),
        };
        let mut ally = state.units[0].clone();
        ally.id = UnitId::new(1);
        ally.kind = UnitKindId::Infantry;
        ally.location = Location::Board {
            position: Pos::new(2, 0),
        };
        ally.hp = 100;
        let mut enemy = ally.clone();
        enemy.id = UnitId::new(2);
        enemy.owner = "blue".into();
        enemy.location = Location::Board {
            position: Pos::new(3, 0),
        };
        enemy.hp = 10;
        let mut reserve = ally.clone();
        reserve.id = UnitId::new(3);
        reserve.location = Location::Board {
            position: Pos::new(6, 0),
        };
        state.units.extend([ally, enemy, reserve]);
        let command: Command = serde_json::from_value(json!({
            "type":"move-explode", "player":"red", "unit":0,
            "path":[Pos::new(0, 0)]
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert!(!result.state.units.contains(UnitId::new(0)));
        assert_eq!(result.state.units.get(UnitId::new(1)).unwrap().hp, 50);
        assert_eq!(result.state.units.get(UnitId::new(2)).unwrap().hp, 1);
        assert_eq!(result.state.units.get(UnitId::new(3)).unwrap().hp, 100);
        assert_eq!(result.state.players[0].commanders[0].power_charge, 0);
        let types: Vec<_> = result.events.iter().map(Event::kind).collect();
        assert_eq!(
            types,
            vec![
                EventKind::UnitMoved,
                EventKind::AreaStrikeResolved,
                EventKind::UnitDamaged,
                EventKind::UnitDamaged,
                EventKind::UnitRemoved
            ]
        );
        assert_eq!(event_unit(&result.events[2]), UnitId::new(1));
        assert_eq!(event_unit(&result.events[3]), UnitId::new(2));
    }

    #[test]
    fn delete_unit_resets_capture_before_removal_without_charge() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/capture/capture-city-complete.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state.board.tile_mut(Pos::new(0, 0)).capture_points = Some(10);
        let mut plain = state.board.tile(Pos::new(0, 0)).clone();
        plain.capture_points = None;
        plain.owner = TileOwner::NotOwnable;
        let mut tiles = row(&state);
        tiles.push(plain);
        set_row(&mut state, tiles);
        let mut reserve = state.units[0].clone();
        reserve.id = UnitId::new(1);
        reserve.location = Location::Board {
            position: Pos::new(1, 0),
        };
        state.units.push(reserve);
        let command: Command = serde_json::from_value(json!({
            "type":"delete-unit", "player":"red", "unit":0
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert!(!result.state.units.contains(UnitId::new(0)));
        assert_eq!(
            result.state.board.tile(Pos::new(0, 0)).capture_points,
            Some(20)
        );
        assert_eq!(result.state.players[0].commanders[0].power_charge, 0);
        assert_eq!(
            result.events,
            vec![
                Event::CaptureChanged {
                    position: Pos::new(0, 0),
                    from: 10,
                    to: 20
                },
                Event::UnitRemoved {
                    unit: UnitId::new(0),
                    reason: KnownReason::Delete.into()
                },
            ]
        );
    }

    #[test]
    fn direct_unit_moves_then_attacks_from_resolved_destination() {
        let mut state = direct_combat_state(3);
        state.board.tile_mut(Pos::new(0, 0)).capture_points = Some(10);
        let mut defender = state.units[0].clone();
        defender.id = UnitId::new(1);
        defender.owner = "blue".into();
        defender.location = Location::Board {
            position: Pos::new(2, 0),
        };
        state.units.push(defender);
        let command: Command = serde_json::from_value(json!({
            "type":"move-attack", "player":"red", "unit":0,
            "path":[Pos::new(0, 0),Pos::new(1, 0)],
            "target":{"type":"unit","unit":1}
        }))
        .unwrap();
        let random = vec![
            RandomToken::CombatGoodLuck(0),
            RandomToken::CombatBadLuck(0),
            RandomToken::CombatGoodLuck(0),
            RandomToken::CombatBadLuck(0),
        ];

        let result = execute(&state, command, &random).unwrap();
        let attacker = result.state.units.get(UnitId::new(0)).unwrap();

        assert_eq!(board_position(attacker), Some(Pos::new(1, 0)));
        assert_eq!(attacker.fuel, 98);
        assert_eq!(attacker.action, UnitAction::Spent);
        assert_eq!(
            result.state.board.tile(Pos::new(0, 0)).capture_points,
            Some(20)
        );
        assert_eq!(result.random_consumed, 4);
        assert_eq!(
            result.events[..3],
            [
                Event::CaptureChanged {
                    position: Pos::new(0, 0),
                    from: 10,
                    to: 20
                },
                Event::UnitMoved {
                    unit: UnitId::new(0),
                    from: Pos::new(0, 0),
                    to: Pos::new(1, 0),
                    path: vec![Pos::new(0, 0), Pos::new(1, 0)],
                    fuel_spent: 1
                },
                Event::AttackResolved {
                    attacker: UnitId::new(0),
                    weapon: Weapon::Unlimited,
                    target: AttackTarget::Unit {
                        unit: UnitId::new(1)
                    }
                },
            ]
        );
    }

    #[test]
    fn movement_can_reveal_attack_target_under_fog() {
        let mut state = direct_combat_state(3);
        state.settings.fog = true;
        state.board.tile_mut(Pos::new(2, 0)).terrain = TerrainId::Wood;
        let mut defender = state.units[0].clone();
        defender.id = UnitId::new(1);
        defender.owner = "blue".into();
        defender.location = Location::Board {
            position: Pos::new(2, 0),
        };
        state.units.push(defender);
        assert!(
            !AwbwVisibility
                .view(&state, &crate::semantic::TeamId::from("red-team"))
                .unit(&state.units[1])
        );
        let command: Command = serde_json::from_value(json!({
            "type":"move-attack", "player":"red", "unit":0,
            "path":[Pos::new(0, 0),Pos::new(1, 0)],
            "target":{"type":"unit","unit":1}
        }))
        .unwrap();
        let random = vec![
            RandomToken::CombatGoodLuck(0),
            RandomToken::CombatBadLuck(0),
            RandomToken::CombatGoodLuck(0),
            RandomToken::CombatBadLuck(0),
        ];

        let result = execute(&state, command, &random).unwrap();

        assert_eq!(result.events[0].kind(), EventKind::UnitMoved);
        assert_eq!(result.events[1].kind(), EventKind::AttackResolved);
    }

    #[test]
    fn hidden_blocker_truncates_combat_movement_and_suppresses_attack() {
        let mut state = direct_combat_state(5);
        state.settings.fog = true;
        let mut blocker = state.units[0].clone();
        blocker.id = UnitId::new(1);
        blocker.kind = UnitKindId::Tank;
        blocker.owner = "blue".into();
        blocker.location = Location::Board {
            position: Pos::new(3, 0),
        };
        let mut target = state.units[0].clone();
        target.id = UnitId::new(2);
        target.owner = "blue".into();
        target.location = Location::Board {
            position: Pos::new(4, 0),
        };
        state.units.extend([blocker, target]);
        let command: Command = serde_json::from_value(json!({
            "type":"move-attack", "player":"red", "unit":0,
            "path":[Pos::new(0, 0),Pos::new(1, 0),Pos::new(2, 0),Pos::new(3, 0)],
            "target":{"type":"unit","unit":2}
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();
        let attacker = result.state.units.get(UnitId::new(0)).unwrap();

        assert_eq!(board_position(attacker), Some(Pos::new(2, 0)));
        assert_eq!(attacker.action, UnitAction::Spent);
        assert_eq!(result.random_consumed, 0);
        assert_eq!(
            result.events,
            [
                Event::UnitMoved {
                    unit: UnitId::new(0),
                    from: Pos::new(0, 0),
                    to: Pos::new(2, 0),
                    path: vec![Pos::new(0, 0), Pos::new(1, 0), Pos::new(2, 0)],
                    fuel_spent: 2
                },
                Event::MovementTrapped {
                    unit: UnitId::new(0),
                    blocker: UnitId::new(1),
                    position: Pos::new(3, 0)
                },
            ]
        );
    }

    #[test]
    fn indirect_units_cannot_move_and_fire() {
        let mut state = direct_combat_state(4);
        state.units[0].kind = UnitKindId::Artillery;
        let mut defender = state.units[0].clone();
        defender.id = UnitId::new(1);
        defender.kind = UnitKindId::Infantry;
        defender.owner = "blue".into();
        defender.location = Location::Board {
            position: Pos::new(3, 0),
        };
        state.units.push(defender);
        let command: Command = serde_json::from_value(json!({
            "type":"move-attack", "player":"red", "unit":0,
            "path":[Pos::new(0, 0),Pos::new(1, 0)],
            "target":{"type":"unit","unit":1}
        }))
        .unwrap();

        assert_eq!(
            execute(&state, command, &[]),
            Err(ReducerError::Violation(Violation::ActionNotSupported {
                action: Action::MoveAndFire
            }))
        );
    }

    #[test]
    fn neutral_infantry_combat_consumes_counter_luck() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/movement/infantry-plain-move.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        let mut tiles = row(&state);
        tiles.truncate(2);
        set_row(&mut state, tiles);
        state.teams.push(crate::semantic::Team {
            id: "blue-team".into(),
            status: crate::semantic::TeamStatus::Active,
        });
        let mut blue = state.players[0].clone();
        blue.id = "blue".into();
        blue.team = "blue-team".into();
        blue.commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.players[0].commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.players.push(blue);
        let mut defender = state.units[0].clone();
        defender.id = UnitId::new(1);
        defender.owner = "blue".into();
        defender.location = Location::Board {
            position: Pos::new(1, 0),
        };
        state.units[0].id = UnitId::new(0);
        state.units.push(defender);
        let command: Command = serde_json::from_value(json!({"type":"move-attack","player":"red","unit":0,"path":[Pos::new(0, 0)],"target":{"type":"unit","unit":1}})).unwrap();
        let random = vec![
            RandomToken::CombatGoodLuck(0),
            RandomToken::CombatBadLuck(0),
            RandomToken::CombatGoodLuck(0),
            RandomToken::CombatBadLuck(0),
        ];
        let result = execute(&state, command, &random).unwrap();
        assert_eq!(result.state.units[0].hp, 75);
        assert_eq!(result.state.units[1].hp, 51);
        assert_eq!(result.random_consumed, 4);
        assert_eq!(event_weapon(&result.events[0]), Weapon::Unlimited);
        assert_eq!(event_weapon(&result.events[2]), Weapon::Unlimited);

        let attack = |state: &State| {
            let command: Command = serde_json::from_value(json!({"type":"move-attack","player":"red","unit":0,"path":[Pos::new(0, 0)],"target":{"type":"unit","unit":1}})).unwrap();
            execute(state, command, &random).unwrap()
        };

        let mut tank_vs_tank = state.clone();
        tank_vs_tank.units[0].kind = UnitKindId::Tank;
        tank_vs_tank.units[0].ammo = 9;
        tank_vs_tank.units[1].kind = UnitKindId::Tank;
        tank_vs_tank.units[1].ammo = 9;
        let result = attack(&tank_vs_tank);
        assert_eq!(event_ammo(&result.events[0]), (9, 8));
        assert_eq!(event_weapon(&result.events[1]), Weapon::Ammo);

        let mut tank_vs_infantry = state.clone();
        tank_vs_infantry.units[0].kind = UnitKindId::Tank;
        tank_vs_infantry.units[0].ammo = 9;
        let result = attack(&tank_vs_infantry);
        assert_eq!(event_weapon(&result.events[0]), Weapon::Unlimited);
        assert_eq!(result.state.units.get(UnitId::new(0)).unwrap().ammo, 9);

        let mut empty_tank_vs_tank = tank_vs_tank;
        empty_tank_vs_tank.units[0].ammo = 0;
        let result = attack(&empty_tank_vs_tank);
        assert_eq!(event_weapon(&result.events[0]), Weapon::Unlimited);
    }

    #[test]
    fn lethal_combat_routes_last_unit_owner() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/combat/neutral-infantry-counter.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        state
            .units
            .iter_mut()
            .find(|unit| unit.id == UnitId::new(1))
            .unwrap()
            .hp = 1;
        let command: Command = serde_json::from_value(case["steps"][2]["command"].clone()).unwrap();
        let random = vec![
            RandomToken::CombatGoodLuck(0),
            RandomToken::CombatBadLuck(0),
        ];

        let result = execute(&state, command, &random).unwrap();

        assert!(matches!(result.state.match_state, Match::Finished { .. }));
        assert_eq!(result.state.turn.phase, Phase::Finished);
        assert_eq!(
            result.events[result.events.len() - 3..],
            [
                Event::PlayerStatusChanged {
                    player: PlayerId::from("blue"),
                    from: PlayerStatus::Active,
                    to: PlayerStatus::Eliminated
                },
                Event::TeamEliminated {
                    team: crate::semantic::TeamId::from("blue-team"),
                    reason: KnownReason::Rout.into()
                },
                Event::MatchCompleted {
                    outcome: Outcome::Victory {
                        winners: vec![crate::semantic::TeamId::from("red-team")],
                        reason: VictoryReason::Rout
                    }
                },
            ]
        );
    }
}
