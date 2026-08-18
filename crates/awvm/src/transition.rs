//! Small authoritative reducer surface used by the conformance protocol.

use std::borrow::Borrow;
use std::cell::OnceCell;

use serde::{Deserialize, Serialize};

use crate::commander::{self, PowerLevel};
use crate::event::{AttackTarget, Event};
use crate::random::{Entropy, Luck, RandomError, RandomTape, RandomToken, RandomTokenKind};
use crate::ruleset::{self, Domain, UnitKind};
use crate::semantic::{
    AwbwView, KnownReason, Location, Match, Outcome, Phase, PlayerId, PlayerIdx, Pos, State,
    TerrainId, Unit, UnitId, UnitKindId, Viewpoint, WeatherKind,
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

/// One command, as `spec/schema/command.schema.json` describes it.
///
/// Every branch restates `player`, which every branch of the schema also
/// requires. Holding it once in a `Command { player, action }` pair was tried
/// and reverted: the flat wire shape then needs `#[serde(flatten)]`, which
/// buffers the request map a second time before the tag can be dispatched, and
/// that measured ~300 ns per decode — around a fifth of what `execute` costs on
/// the movement benchmark. [`Command::player`] is exhaustive, so the repetition
/// is a declaration cost and not a correctness one.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
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
    /// A `type` this adapter does not implement. Reaching the reducer with one
    /// is [`ExecuteError::UnsupportedCommand`], not a rules violation.
    #[serde(other)]
    Unsupported,
}

impl Command {
    /// The player acting, which every branch but [`Command::Unsupported`] names.
    pub const fn player(&self) -> Option<&PlayerId> {
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

/// A movement that was resolved against one state but has no selected action.
///
/// The borrowed state binds the plan to the state that produced it. A caller
/// can inspect several actions without validating the path again.
#[derive(Clone, Debug)]
pub struct PreparedMovement<'a> {
    state: &'a State,
    unit: UnitId,
    movement: MovedUnit,
}

/// A ready unit that belongs to the active player and is on the board.
///
/// The borrowed state binds later movement and delete preparation to the
/// checks that produced this value.
#[derive(Clone, Debug)]
pub struct PreparedActiveUnit<'a> {
    state: &'a State,
    unit: UnitId,
    unit_index: usize,
    origin: Pos,
}

/// A production position bound to one active turn.
///
/// Construct this once and use it to inspect every unit kind. Shared board and
/// roster facts are computed once, while each kind keeps the violation order
/// required by the reducer.
#[derive(Clone, Debug)]
pub struct PreparedProductionSite<'a> {
    state: &'a State,
    position: Pos,
    player_index: PlayerIdx,
    occupied: bool,
    owned_units: u64,
    owns_lab: bool,
}

/// A valid transport bound to one active turn.
#[derive(Clone, Debug)]
pub struct PreparedUnloadTransport<'a> {
    state: &'a State,
    transport: UnitId,
    position: Pos,
}

/// Cargo that is carried by a prepared transport.
#[derive(Clone, Debug)]
pub struct PreparedUnloadCargo<'a> {
    transport: PreparedUnloadTransport<'a>,
    cargo: UnitId,
    cargo_index: usize,
    cargo_slot: usize,
}

/// A command that was resolved against one state but was not applied.
#[derive(Debug)]
pub struct PreparedCommand<'a> {
    command: PreparedCommandKind<'a>,
}

#[derive(Debug)]
struct Prepared<'a, A> {
    movement: PreparedMovement<'a>,
    action: A,
}

#[derive(Debug)]
enum PreparedCommandKind<'a> {
    Wait(Prepared<'a, movement::Wait>),
    Capture(Prepared<'a, property::Capture>),
    Supply(Prepared<'a, transport::Supply>),
    Concealment(Prepared<'a, movement::ConcealmentAction>),
    Join(Prepared<'a, transport::Join>),
    Load(Prepared<'a, transport::Load>),
    Attack(Prepared<'a, attack::Attack>),
    Repair(Prepared<'a, transport::Repair>),
    Launch(Prepared<'a, special::Launch>),
    Explode(Prepared<'a, special::Explode>),
    Produce(property::PreparedProduction<'a>),
    Delete(special::PreparedDelete<'a>),
    Unload(transport::PreparedUnload<'a>),
}

/// The semantic result of preparing a supported command.
#[derive(Debug)]
pub enum PrepareOutcome<'a> {
    Prepared(PreparedCommand<'a>),
    Rejected(Violation),
}

/// The semantic result of preparing the movement shared by several commands.
#[derive(Debug)]
pub enum PrepareMovementOutcome<'a> {
    Prepared(PreparedMovement<'a>),
    Rejected(Violation),
}

/// The semantic result of preparing an active unit.
#[derive(Debug)]
pub enum PrepareActiveUnitOutcome<'a> {
    Prepared(PreparedActiveUnit<'a>),
    Rejected(Violation),
}

/// The semantic result of preparing a production position.
#[derive(Debug)]
pub enum PrepareProductionSiteOutcome<'a> {
    Prepared(PreparedProductionSite<'a>),
    Rejected(Violation),
}

/// The semantic result of preparing a transport for unload commands.
#[derive(Debug)]
pub enum PrepareUnloadTransportOutcome<'a> {
    Prepared(PreparedUnloadTransport<'a>),
    Rejected(Violation),
}

/// The semantic result of selecting cargo from a prepared transport.
#[derive(Debug)]
pub enum PrepareUnloadCargoOutcome<'a> {
    Prepared(PreparedUnloadCargo<'a>),
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

/// Evaluate one command against a recorded tape.
///
/// This is the protocol's entry point and the one a fixture or a replay wants:
/// both already know every outcome, and supplying them is how a run is checked
/// token-for-token. An authority that rolls instead wants
/// [`execute_with`].
pub fn execute(
    state: &State,
    command: Command,
    random: &[RandomToken],
) -> Result<ExecuteOutcome, ExecuteError> {
    execute_with(state, command, &mut RandomTape::new(random))
}

/// Evaluate one command, asking `entropy` for each value as the reducer reaches
/// it.
///
/// The reducer knows the acting commander's luck domain at the moment it draws;
/// a caller pre-rolling a tape does not, and would have to rebuild the combat
/// context to find out. Wrap `entropy` in [`crate::random::Recording`] to keep
/// the tape the run produced, which is what makes a live game replayable
/// through [`execute`].
pub fn execute_with(
    state: &State,
    command: Command,
    entropy: &mut impl Entropy,
) -> Result<ExecuteOutcome, ExecuteError> {
    let mut draws = Draws::new(entropy);
    match reduce(state, command, &mut draws) {
        Ok(execution) => Ok(ExecuteOutcome::Accepted(execution)),
        Err(ReducerError::Violation(violation)) => Ok(ExecuteOutcome::Rejected(violation)),
        Err(ReducerError::UnsupportedCommand) => Err(ExecuteError::UnsupportedCommand),
        Err(ReducerError::UnsupportedRuleset) => Err(ExecuteError::UnsupportedRuleset),
        Err(ReducerError::InvalidState(error)) => Err(ExecuteError::InvalidState(error)),
        Err(ReducerError::InvalidRandom(error)) => Err(ExecuteError::InvalidRandom(error)),
    }
}

/// Resolve a command without cloning or changing its input state.
///
/// Preparation supports movement, production, delete, and unload commands.
/// Other commands return [`ExecuteError::UnsupportedCommand`]. Preparation
/// performs the same deterministic checks as [`execute`], but it delays the
/// state clone, mutation, and random draws until [`execute_prepared`] is
/// called.
pub fn prepare_command(
    state: &State,
    command: Command,
) -> Result<PrepareOutcome<'_>, ExecuteError> {
    prepare_outcome(prepare(state, command))
}

/// Resolve movement without choosing the action at its destination.
pub fn prepare_movement<'a>(
    state: &'a State,
    player: &PlayerId,
    unit: UnitId,
    path: Vec<Pos>,
) -> Result<PrepareMovementOutcome<'a>, ExecuteError> {
    match prepare_movement_inner(state, player, unit, path) {
        Ok(prepared) => Ok(PrepareMovementOutcome::Prepared(prepared)),
        Err(ReducerError::Violation(violation)) => Ok(PrepareMovementOutcome::Rejected(violation)),
        Err(error) => Err(execute_error(error)),
    }
}

/// Resolve the checks shared by movement and deletion.
pub fn prepare_active_unit<'a>(
    state: &'a State,
    player: &PlayerId,
    unit: UnitId,
) -> Result<PrepareActiveUnitOutcome<'a>, ExecuteError> {
    match ActiveTurn::open(state, player).and_then(|turn| turn.prepare_unit(unit)) {
        Ok(prepared) => Ok(PrepareActiveUnitOutcome::Prepared(prepared)),
        Err(ReducerError::Violation(violation)) => {
            Ok(PrepareActiveUnitOutcome::Rejected(violation))
        }
        Err(error) => Err(execute_error(error)),
    }
}

/// Bind a production position to the active turn.
pub fn prepare_production_site<'a>(
    state: &'a State,
    player: &PlayerId,
    position: Pos,
) -> Result<PrepareProductionSiteOutcome<'a>, ExecuteError> {
    match ActiveTurn::open(state, player)
        .and_then(|turn| property::prepare_production_site(&turn, position))
    {
        Ok(prepared) => Ok(PrepareProductionSiteOutcome::Prepared(prepared)),
        Err(ReducerError::Violation(violation)) => {
            Ok(PrepareProductionSiteOutcome::Rejected(violation))
        }
        Err(error) => Err(execute_error(error)),
    }
}

/// Bind an unload-capable transport to the active turn.
pub fn prepare_unload_transport<'a>(
    state: &'a State,
    player: &PlayerId,
    transport: UnitId,
) -> Result<PrepareUnloadTransportOutcome<'a>, ExecuteError> {
    match ActiveTurn::open(state, player)
        .and_then(|turn| transport::prepare_unload_transport(&turn, transport))
    {
        Ok(prepared) => Ok(PrepareUnloadTransportOutcome::Prepared(prepared)),
        Err(ReducerError::Violation(violation)) => {
            Ok(PrepareUnloadTransportOutcome::Rejected(violation))
        }
        Err(error) => Err(execute_error(error)),
    }
}

impl<'a> PreparedMovement<'a> {
    /// Bind the movement to facts shared by all actions at its destination.
    pub fn prepare_destination(self) -> PreparedDestination<'a> {
        let view = AwbwView::new(self.state, self.movement.actor_team());
        self.prepare_destination_with(view)
    }

    pub(crate) fn prepare_destination_with<V>(self, view: V) -> PreparedDestination<'a, V>
    where
        V: Borrow<AwbwView<'a>>,
    {
        PreparedDestination {
            movement: self,
            view,
            available: OnceCell::new(),
            trap: OnceCell::new(),
        }
    }

    /// Resolve waiting at this movement's destination.
    pub fn prepare_wait(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_wait()
    }

    /// Resolve capture at this movement's destination.
    pub fn prepare_capture(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_capture()
    }

    /// Resolve supplying from this movement's destination.
    pub fn prepare_supply(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_supply()
    }

    /// Resolve entering hidden state at this movement's destination.
    pub fn prepare_hide(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_hide()
    }

    /// Resolve entering exposed state at this movement's destination.
    pub fn prepare_reveal(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_reveal()
    }

    /// Resolve joining another unit at this movement's destination.
    pub fn prepare_join(self, target: UnitId) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_join(target)
    }

    /// Resolve loading into a transport at this movement's destination.
    pub fn prepare_load(self, transport: UnitId) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_load(transport)
    }

    /// Resolve attacking a unit or tile from this movement's destination.
    pub fn prepare_attack(self, target: AttackTarget) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_attack(target)
    }

    /// Resolve repairing another unit from this movement's destination.
    pub fn prepare_repair(self, target: UnitId) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_repair(target)
    }

    /// Resolve launching a silo at a target tile.
    pub fn prepare_launch(self, target: Pos) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_launch(target)
    }

    /// Resolve exploding at this movement's destination.
    pub fn prepare_explode(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        self.prepare_destination().prepare_explode()
    }

    pub(crate) const fn state(&self) -> &State {
        self.state
    }

    pub(crate) const fn unit(&self) -> UnitId {
        self.unit
    }

    pub(crate) const fn plan(&self) -> &MovedUnit {
        &self.movement
    }
}

/// A validated movement with facts shared by its destination actions.
///
/// The borrowed state prevents this proof from being applied to a different
/// state. Destination occupancy, visibility, and hidden movement traps are
/// resolved once when an action needs them. The default form owns its view.
/// A move field supplies a form that borrows its shared view.
#[derive(Debug)]
pub struct PreparedDestination<'a, V = AwbwView<'a>> {
    movement: PreparedMovement<'a>,
    view: V,
    available: OnceCell<Result<AvailableDestination, Violation>>,
    trap: OnceCell<Option<(usize, Pos, UnitId)>>,
}

impl<'a, V> PreparedDestination<'a, V>
where
    V: Borrow<AwbwView<'a>>,
{
    /// Resolve waiting at this destination.
    pub fn prepare_wait(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(movement::prepare_wait(self).map(PreparedCommandKind::Wait))
    }

    /// Resolve capture at this destination.
    pub fn prepare_capture(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(property::prepare_capture(self).map(PreparedCommandKind::Capture))
    }

    /// Resolve supplying from this destination.
    pub fn prepare_supply(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(transport::prepare_supply(self).map(PreparedCommandKind::Supply))
    }

    /// Resolve entering hidden state at this destination.
    pub fn prepare_hide(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(
            movement::prepare_concealment(self, true).map(PreparedCommandKind::Concealment),
        )
    }

    /// Resolve entering exposed state at this destination.
    pub fn prepare_reveal(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(
            movement::prepare_concealment(self, false).map(PreparedCommandKind::Concealment),
        )
    }

    /// Resolve joining another unit at this destination.
    pub fn prepare_join(self, target: UnitId) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(transport::prepare_join(self, target).map(PreparedCommandKind::Join))
    }

    /// Resolve loading into a transport at this destination.
    pub fn prepare_load(self, transport: UnitId) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(transport::prepare_load(self, transport).map(PreparedCommandKind::Load))
    }

    /// Resolve attacking a unit or tile from this destination.
    pub fn prepare_attack(self, target: AttackTarget) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(attack::prepare_attack(self, target).map(PreparedCommandKind::Attack))
    }

    /// Resolve repairing another unit from this destination.
    pub fn prepare_repair(self, target: UnitId) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(transport::prepare_repair(self, target).map(PreparedCommandKind::Repair))
    }

    /// Resolve launching a silo at a target tile.
    pub fn prepare_launch(self, target: Pos) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(special::prepare_launch(self, target).map(PreparedCommandKind::Launch))
    }

    /// Resolve exploding at this destination.
    pub fn prepare_explode(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(special::prepare_explode(self).map(PreparedCommandKind::Explode))
    }

    pub(crate) const fn movement(&self) -> &PreparedMovement<'a> {
        &self.movement
    }

    fn into_movement(self) -> PreparedMovement<'a> {
        self.movement
    }

    fn view(&self) -> &AwbwView<'a> {
        self.view.borrow()
    }

    fn available_destination(&self) -> Result<AvailableDestination, ReducerError> {
        self.available
            .get_or_init(|| movement::available_destination(&self.movement, self.view()))
            .clone()
            .map_err(Into::into)
    }

    fn trap(&self) -> Option<(usize, Pos, UnitId)> {
        *self.trap.get_or_init(|| {
            movement::planned_movement_trap_with_view(
                self.movement.state(),
                self.movement.unit(),
                self.movement.plan(),
                self.view(),
            )
        })
    }

    pub(crate) fn can_wait(&self) -> Result<bool, ExecuteError> {
        preparation_is_valid(movement::validate_wait(self))
    }

    pub(crate) fn can_capture(&self) -> Result<bool, ExecuteError> {
        preparation_is_valid(property::validate_capture(self))
    }

    pub(crate) fn can_supply(&self) -> Result<bool, ExecuteError> {
        preparation_is_valid(transport::validate_supply(self))
    }

    pub(crate) fn can_hide(&self) -> Result<bool, ExecuteError> {
        preparation_is_valid(movement::validate_concealment(self, true))
    }

    pub(crate) fn can_reveal(&self) -> Result<bool, ExecuteError> {
        preparation_is_valid(movement::validate_concealment(self, false))
    }

    pub(crate) fn can_join(&self, target: UnitId) -> Result<bool, ExecuteError> {
        preparation_is_valid(transport::validate_join(self, target))
    }

    pub(crate) fn can_load(&self, transport: UnitId) -> Result<bool, ExecuteError> {
        preparation_is_valid(transport::validate_load(self, transport))
    }

    pub(crate) fn can_attack(&self, target: AttackTarget) -> Result<bool, ExecuteError> {
        preparation_is_valid(attack::validate_attack(self, target))
    }

    pub(crate) fn can_repair(&self, target: UnitId) -> Result<bool, ExecuteError> {
        preparation_is_valid(transport::validate_repair(self, target))
    }

    pub(crate) fn can_launch(&self, target: Pos) -> Result<bool, ExecuteError> {
        preparation_is_valid(special::validate_launch(self, target))
    }

    pub(crate) fn can_explode(&self) -> Result<bool, ExecuteError> {
        preparation_is_valid(special::validate_explode(self))
    }
}

fn preparation_is_valid<T>(result: Result<T, ReducerError>) -> Result<bool, ExecuteError> {
    match result {
        Ok(_) => Ok(true),
        Err(ReducerError::Violation(_)) => Ok(false),
        Err(error) => Err(execute_error(error)),
    }
}

impl<'a> PreparedActiveUnit<'a> {
    /// Resolve movement for this unit without repeating active-unit checks.
    pub fn prepare_movement(
        self,
        path: Vec<Pos>,
    ) -> Result<PrepareMovementOutcome<'a>, ExecuteError> {
        match movement::plan(&self, path) {
            Ok(movement) => Ok(PrepareMovementOutcome::Prepared(PreparedMovement {
                state: self.state,
                unit: self.unit,
                movement,
            })),
            Err(ReducerError::Violation(violation)) => {
                Ok(PrepareMovementOutcome::Rejected(violation))
            }
            Err(error) => Err(execute_error(error)),
        }
    }

    pub(crate) fn movement_from_field(
        &self,
        path: Vec<Pos>,
        entry_costs: Vec<u64>,
    ) -> PreparedMovement<'a> {
        PreparedMovement {
            state: self.state,
            unit: self.unit,
            movement: movement::from_field(self, path, entry_costs),
        }
    }

    /// Resolve deletion for this unit.
    pub fn prepare_delete(self) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(special::prepare_delete(self).map(PreparedCommandKind::Delete))
    }

    pub(crate) const fn state(&self) -> &'a State {
        self.state
    }

    pub(crate) const fn unit(&self) -> UnitId {
        self.unit
    }

    pub(crate) const fn unit_index(&self) -> usize {
        self.unit_index
    }

    pub(crate) const fn origin(&self) -> Pos {
        self.origin
    }
}

impl<'a> PreparedProductionSite<'a> {
    /// Resolve one unit kind at this position.
    pub fn prepare_kind(self, kind: UnitKindId) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(property::prepare_production(self, kind).map(PreparedCommandKind::Produce))
    }
}

impl<'a> PreparedUnloadTransport<'a> {
    /// Resolve cargo carried by this transport.
    pub fn prepare_cargo(
        self,
        cargo: UnitId,
    ) -> Result<PrepareUnloadCargoOutcome<'a>, ExecuteError> {
        match transport::prepare_unload_cargo(self, cargo) {
            Ok(prepared) => Ok(PrepareUnloadCargoOutcome::Prepared(prepared)),
            Err(ReducerError::Violation(violation)) => {
                Ok(PrepareUnloadCargoOutcome::Rejected(violation))
            }
            Err(error) => Err(execute_error(error)),
        }
    }
}

impl<'a> PreparedUnloadCargo<'a> {
    /// Resolve one destination for this cargo.
    pub fn prepare_destination(self, destination: Pos) -> Result<PrepareOutcome<'a>, ExecuteError> {
        prepare_outcome(
            transport::prepare_unload(self, destination).map(PreparedCommandKind::Unload),
        )
    }
}

fn prepare_outcome(
    result: Result<PreparedCommandKind<'_>, ReducerError>,
) -> Result<PrepareOutcome<'_>, ExecuteError> {
    match result {
        Ok(command) => Ok(PrepareOutcome::Prepared(PreparedCommand { command })),
        Err(ReducerError::Violation(violation)) => Ok(PrepareOutcome::Rejected(violation)),
        Err(error) => Err(execute_error(error)),
    }
}

fn prepare(state: &State, command: Command) -> Result<PreparedCommandKind<'_>, ReducerError> {
    match command {
        Command::MoveWait { player, unit, path } => movement::prepare_wait(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
        )
        .map(PreparedCommandKind::Wait),
        Command::MoveCapture { player, unit, path } => property::prepare_capture(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
        )
        .map(PreparedCommandKind::Capture),
        Command::MoveSupply { player, unit, path } => transport::prepare_supply(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
        )
        .map(PreparedCommandKind::Supply),
        Command::MoveHide { player, unit, path } => movement::prepare_concealment(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
            true,
        )
        .map(PreparedCommandKind::Concealment),
        Command::MoveReveal { player, unit, path } => movement::prepare_concealment(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
            false,
        )
        .map(PreparedCommandKind::Concealment),
        Command::MoveExplode { player, unit, path } => special::prepare_explode(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
        )
        .map(PreparedCommandKind::Explode),
        Command::MoveJoin {
            player,
            unit,
            path,
            target,
        } => transport::prepare_join(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
            target,
        )
        .map(PreparedCommandKind::Join),
        Command::MoveLoad {
            player,
            unit,
            path,
            transport,
        } => transport::prepare_load(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
            transport,
        )
        .map(PreparedCommandKind::Load),
        Command::MoveAttack {
            player,
            unit,
            path,
            target,
        } => attack::prepare_attack(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
            target,
        )
        .map(PreparedCommandKind::Attack),
        Command::MoveRepair {
            player,
            unit,
            path,
            target,
        } => transport::prepare_repair(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
            target,
        )
        .map(PreparedCommandKind::Repair),
        Command::MoveLaunch {
            player,
            unit,
            path,
            target,
        } => special::prepare_launch(
            prepare_movement_inner(state, &player, unit, path)?.prepare_destination(),
            target,
        )
        .map(PreparedCommandKind::Launch),
        Command::ProduceUnit {
            player,
            position,
            kind,
        } => property::prepare_production_site(&ActiveTurn::open(state, &player)?, position)
            .and_then(|site| property::prepare_production(site, kind))
            .map(PreparedCommandKind::Produce),
        Command::DeleteUnit { player, unit } => ActiveTurn::open(state, &player)?
            .prepare_unit(unit)
            .and_then(special::prepare_delete)
            .map(PreparedCommandKind::Delete),
        Command::Unload {
            player,
            transport,
            cargo,
            destination,
        } => transport::prepare_unload_transport(&ActiveTurn::open(state, &player)?, transport)
            .and_then(|transport| transport::prepare_unload_cargo(transport, cargo))
            .and_then(|cargo| transport::prepare_unload(cargo, destination))
            .map(PreparedCommandKind::Unload),
        _ => Err(ReducerError::UnsupportedCommand),
    }
}

fn prepare_movement_inner<'a>(
    state: &'a State,
    player: &PlayerId,
    unit: UnitId,
    path: Vec<Pos>,
) -> Result<PreparedMovement<'a>, ReducerError> {
    let turn = ActiveTurn::open(state, player)?;
    turn.prepare_move(unit, path)
}

fn execute_error(error: ReducerError) -> ExecuteError {
    match error {
        ReducerError::UnsupportedCommand => ExecuteError::UnsupportedCommand,
        ReducerError::UnsupportedRuleset => ExecuteError::UnsupportedRuleset,
        ReducerError::InvalidState(error) => ExecuteError::InvalidState(error),
        ReducerError::InvalidRandom(error) => ExecuteError::InvalidRandom(error),
        ReducerError::Violation(_) => {
            unreachable!("violations are converted to preparation outcomes")
        }
    }
}

/// Apply a prepared command to the state that produced it.
///
/// Only combat consumes tokens from `random`. Deterministic actions report
/// zero `random_consumed`. Do not reconcile these actions against tape offsets.
pub fn execute_prepared(
    prepared: PreparedCommand<'_>,
    random: &[RandomToken],
) -> Result<Execution, ExecuteError> {
    execute_prepared_with(prepared, &mut RandomTape::new(random))
}

/// Apply a prepared command and ask `entropy` for values when it needs them.
///
/// Only combat asks `entropy` for values. Deterministic actions report zero
/// `random_consumed`. Do not reconcile these actions against tape offsets. Attack
/// preparation delays its combat draws until this application step.
pub fn execute_prepared_with(
    prepared: PreparedCommand<'_>,
    entropy: &mut impl Entropy,
) -> Result<Execution, ExecuteError> {
    match prepared.command {
        PreparedCommandKind::Wait(prepared) => Ok(movement::execute_prepared_wait(prepared)),
        PreparedCommandKind::Capture(prepared) => {
            property::execute_prepared_capture(prepared).map_err(execute_error)
        }
        PreparedCommandKind::Supply(prepared) => Ok(transport::execute_prepared_supply(prepared)),
        PreparedCommandKind::Concealment(prepared) => {
            Ok(movement::execute_prepared_concealment(prepared))
        }
        PreparedCommandKind::Join(prepared) => {
            transport::execute_prepared_join(prepared).map_err(execute_error)
        }
        PreparedCommandKind::Load(prepared) => Ok(transport::execute_prepared_load(prepared)),
        PreparedCommandKind::Attack(prepared) => {
            let mut draws = Draws::new(entropy);
            attack::execute_prepared_attack(prepared, &mut draws).map_err(execute_error)
        }
        PreparedCommandKind::Repair(prepared) => {
            transport::execute_prepared_repair(prepared).map_err(execute_error)
        }
        PreparedCommandKind::Launch(prepared) => {
            special::execute_prepared_launch(prepared).map_err(execute_error)
        }
        PreparedCommandKind::Explode(prepared) => {
            special::execute_prepared_explode(prepared).map_err(execute_error)
        }
        PreparedCommandKind::Produce(prepared) => {
            Ok(property::execute_prepared_production(prepared))
        }
        PreparedCommandKind::Delete(prepared) => {
            special::execute_prepared_delete(prepared).map_err(execute_error)
        }
        PreparedCommandKind::Unload(prepared) => Ok(transport::execute_prepared_unload(prepared)),
    }
}

fn reduce(
    state: &State,
    command: Command,
    draws: &mut Draws<'_>,
) -> Result<Execution, ReducerError> {
    let Some(player) = command.player() else {
        return Err(ReducerError::UnsupportedCommand);
    };
    let turn = ActiveTurn::open(state, player)?;
    match command {
        Command::MoveWait { unit, path, .. } => execute_move_wait(&turn, unit, path),
        Command::MoveAttack {
            unit, path, target, ..
        } => execute_move_attack(&turn, unit, path, target, draws),
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
        Command::Tag { .. } => execute_tag(&turn, draws),
        Command::EndTurn { .. } => execute_end_turn(&turn, draws),
        Command::Resign { .. } => execute_resign(&turn, draws),
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

/// Remove a board unit and any units it carried.
///
/// Cargo has no board position of its own, so it cannot outlive its carrier.
/// The loss is a consequence of losing the carrier, not a separate strike, so
/// the cargo removals carry `carrier-lost` whatever took the carrier out.
/// Cargo is reported in slot order to keep the event sequence deterministic.
pub(crate) fn remove_unit_and_cargo(
    state: &mut State,
    unit: UnitId,
    reason: KnownReason,
    events: &mut Vec<Event>,
) {
    let mut cargo: Vec<_> = state
        .units
        .iter()
        .filter_map(|candidate| match candidate.location {
            Location::Cargo { transport, slot } if transport == unit => Some((slot, candidate.id)),
            _ => None,
        })
        .collect();
    cargo.sort();
    state.units.retain(|candidate| {
        candidate.id != unit
            && !matches!(
                candidate.location,
                Location::Cargo { transport, .. } if transport == unit
            )
    });
    events.push(Event::UnitRemoved {
        unit,
        reason: reason.into(),
    });
    for (_, cargo) in cargo {
        events.push(Event::UnitRemoved {
            unit: cargo,
            reason: KnownReason::CarrierLost.into(),
        });
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

/// The reducer's view of the caller's entropy, with the draw count attached.
///
/// `random_consumed` used to be `RandomTape::consumed`, which tied the reported
/// count to one implementation of the source. Counting here instead keeps the
/// count a fact about the run — the property that doc comment was defending —
/// while letting the source be a tape, an RNG, or anything else.
pub(crate) struct Draws<'a> {
    source: &'a mut dyn Entropy,
    drawn: usize,
}

impl<'a> Draws<'a> {
    pub(crate) fn new(source: &'a mut dyn Entropy) -> Self {
        Self { source, drawn: 0 }
    }

    /// How many values the reducer asked for.
    pub(crate) const fn drawn(&self) -> usize {
        self.drawn
    }

    pub(crate) fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        self.drawn += 1;
        self.source.weather()
    }

    /// Draw a luck value and hold the source to the domain it was given.
    ///
    /// [`RandomTape`] checks its own tokens, but a caller-supplied source is
    /// just as able to hand back a number outside the range — and an
    /// unchecked one reaches the damage formula, where it silently changes the
    /// result instead of failing. Checking here means the bound is a property
    /// of the reducer rather than of one implementation of [`Entropy`].
    fn luck(&mut self, polarity: Luck, domain: commander::Domain) -> Result<i64, RandomError> {
        self.drawn += 1;
        let value = self.source.luck(polarity, domain)?;
        if !(domain.minimum..=domain.maximum).contains(&value) {
            return Err(RandomError::OutOfDomain {
                kind: match polarity {
                    Luck::Good => RandomTokenKind::CombatGoodLuck,
                    Luck::Bad => RandomTokenKind::CombatBadLuck,
                },
                value,
                minimum: domain.minimum,
                maximum: domain.maximum,
            });
        }
        Ok(value)
    }
}

/// Draw one combat luck roll.
///
/// A malformed tape is an *execution error* — `spec/model/violations.md` item 5
/// puts "missing, wrong-type, or out-of-domain random input" there explicitly,
/// alongside stale state binding, and apart from both violations and
/// unsupported commands. This path used to report `UNSUPPORTED_COMMAND`, which
/// told a caller its command was not implemented when the command was fine and
/// the tape was not. The weather draw (`turn::advance_weather`) always reported
/// the execution failure; the two agree now.
pub(crate) fn draw(
    draws: &mut Draws<'_>,
    polarity: Luck,
    domain: commander::Domain,
) -> Result<i64, ReducerError> {
    Ok(draws.luck(polarity, domain)?)
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

    /// Validate the checks shared by movement and deletion.
    pub(crate) fn prepare_unit(
        &self,
        unit: UnitId,
    ) -> Result<PreparedActiveUnit<'a>, ReducerError> {
        let unit_index = self
            .state
            .units
            .index_of(unit)
            .ok_or_else(|| violation(Violation::UnitNotFound { unit }))?;
        let subject = &self.state.units[unit_index];
        if subject.owner != self.player() {
            return Err(violation(Violation::UnitNotOwned {
                unit,
                player: self.player().clone(),
            }));
        }
        let Location::Board { position: origin } = subject.location else {
            return Err(violation(Violation::UnitNotOnBoard { unit }));
        };
        if subject.action != crate::semantic::UnitAction::Ready {
            return Err(violation(Violation::UnitAlreadyActed { unit }));
        }
        Ok(PreparedActiveUnit {
            state: self.state,
            unit,
            unit_index,
            origin,
        })
    }

    /// Validate movement and bind it to this turn's state.
    pub(crate) fn prepare_move(
        &self,
        unit: UnitId,
        path: Vec<Pos>,
    ) -> Result<PreparedMovement<'a>, ReducerError> {
        let active = self.prepare_unit(unit)?;
        let movement = movement::plan(&active, path)?;
        Ok(PreparedMovement {
            state: self.state,
            unit,
            movement,
        })
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
        let mut tape = RandomTape::new(random);
        reduce(state, command, &mut Draws::new(&mut tape))
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
        ActiveTurn::open(state, &PlayerId::from("red"))?.prepare_move(UnitId::new(0), path)?;
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
    fn move_launch_damages_units_in_radius_in_stable_order_without_charge() {
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
        assert_eq!(result.state.units.get(UnitId::new(0)).unwrap().hp, 20);
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
                EventKind::SiloChanged
            ]
        );
        assert_eq!(event_unit(&result.events[2]), UnitId::new(1));
        assert_eq!(event_unit(&result.events[3]), UnitId::new(2));
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
        state.settings.fog = true;
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
    fn movement_cannot_make_an_undisclosed_unit_an_attack_target() {
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
        assert!(
            !crate::query::actions_at(&state, UnitId::new(0), Pos::new(1, 0))
                .unwrap()
                .attack
                .contains(&AttackTarget::Unit {
                    unit: UnitId::new(1)
                })
        );
        let observation = crate::semantic::observe(&AwbwVisibility, &state, &"red".into()).unwrap();
        assert_eq!(
            crate::query::observed_forecasts(
                &observation,
                UnitId::new(0),
                Pos::new(1, 0),
                &[Pos::new(2, 0)]
            )
            .unwrap(),
            vec![None]
        );
        assert_eq!(
            execute(&state, command, &[]),
            Err(violation(Violation::InvalidTarget {
                target: Some(UnitId::new(1).into())
            }))
        );
    }

    #[test]
    fn capture_does_not_require_precommand_property_visibility() {
        let mut state = direct_combat_state(4);
        state.settings.fog = true;
        let destination = Pos::new(3, 0);
        let property = state.board.tile_mut(destination);
        property.terrain = TerrainId::City;
        property.owner = TileOwner::Neutral;
        property.capture_points = Some(20);
        assert!(
            !AwbwVisibility
                .view(&state, &crate::semantic::TeamId::from("red-team"))
                .position(destination)
        );
        let command: Command = serde_json::from_value(json!({
            "type":"move-capture", "player":"red", "unit":0,
            "path":[
                Pos::new(0, 0), Pos::new(1, 0), Pos::new(2, 0), destination
            ]
        }))
        .unwrap();

        let result = execute(&state, command, &[]).unwrap();

        assert_eq!(board_position(&result.state.units[0]), Some(destination));
        assert_eq!(
            result.state.board.tile(destination).capture_points,
            Some(10)
        );
    }

    #[test]
    fn hidden_blocker_truncates_combat_movement_and_suppresses_attack() {
        let mut state = direct_combat_state(5);
        state.settings.fog = true;
        let target_tile = state.board.tile_mut(Pos::new(4, 0));
        target_tile.terrain = TerrainId::City;
        target_tile.owner = TileOwner::Owned("red".into());
        target_tile.capture_points = Some(20);
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
