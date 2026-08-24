//! Small authoritative reducer surface used by the conformance protocol.

use std::borrow::Borrow;
use std::cell::OnceCell;

use serde::{Deserialize, Serialize};

use crate::commander::Holdings;
use crate::commander::{self, PowerLevel};
use crate::event::{AttackTarget, Event};
use crate::query::{TurnMaps, TurnTables};
use crate::random::{Entropy, Luck, RandomError, RandomTape, RandomToken, RandomTokenKind};
use crate::ruleset::{self, Domain, UnitKind};
use crate::semantic::{
    AwbwView, Concealment, KnownReason, Location, Match, Outcome, Phase, PlayerId, PlayerIdx, Pos,
    State, TerrainId, Unit, UnitId, UnitKindId, Viewpoint, WeatherKind,
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
pub(crate) use property::*;
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
pub(crate) struct PreparedMovement<'a> {
    state: &'a State,
    unit: UnitId,
    movement: MovedUnit<'a>,
}

/// A ready unit that belongs to the active player and is on the board.
///
/// The borrowed state binds later movement and delete preparation to the
/// checks that produced this value.
#[derive(Clone, Debug)]
pub(crate) struct PreparedActiveUnit<'a> {
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
pub(crate) struct PreparedProductionSite<'a> {
    state: &'a State,
    position: Pos,
    player_index: PlayerIdx,
    occupied: bool,
    owned_units: u64,
    owns_lab: bool,
}

/// A valid transport bound to one active turn.
#[derive(Clone, Debug)]
pub(crate) struct PreparedUnloadTransport<'a> {
    state: &'a State,
    transport: UnitId,
    position: Pos,
}

/// Cargo that is carried by a prepared transport.
#[derive(Clone, Debug)]
pub(crate) struct PreparedUnloadCargo<'a> {
    transport: PreparedUnloadTransport<'a>,
    cargo: UnitId,
    cargo_index: usize,
    cargo_slot: usize,
}

#[derive(Debug)]
struct MovementAction<'a, A> {
    movement: PreparedMovement<'a>,
    action: A,
    /// The hidden unit that will stop this movement short, resolved while
    /// preparing. Execution used to build a second view of the same state to
    /// ask this again.
    trap: Option<(usize, Pos, UnitId)>,
}

#[derive(Debug)]
enum PreparedCommandKind<'a> {
    Wait(MovementAction<'a, movement::WaitProof>),
    Capture(MovementAction<'a, property::CaptureProof>),
    Supply(MovementAction<'a, transport::SupplyProof>),
    Concealment(MovementAction<'a, movement::ConcealProof>),
    Join(MovementAction<'a, transport::JoinProof>),
    Load(MovementAction<'a, transport::LoadProof>),
    Attack(MovementAction<'a, attack::AttackProof>),
    Repair(MovementAction<'a, transport::RepairProof>),
    Launch(MovementAction<'a, special::LaunchProof>),
    Explode(MovementAction<'a, special::ExplodeProof>),
    Produce(property::PreparedProduction<'a>),
    Delete(special::PreparedDelete<'a>),
    Unload(transport::PreparedUnload<'a>),
    /// End-of-turn, tag, and resignation share one boundary reducer, and none
    /// of them has a check to make past the four [`ActiveTurn::open`] already made.
    Boundary(turn::PreparedBoundary<'a>),
    Power(powers::PreparedPower<'a>),
}

/// The semantic result of preparing something against a state.
///
/// Preparation has two failure modes and they are not alike. A [`Violation`] is
/// an answer: the command is illegal, and a caller enumerating what is legal
/// wants it back. An [`ExecuteError`] is a fault, and no answer exists. Every
/// preparation entry point therefore returns `Result<Prepared<T>, ExecuteError>`
/// — the outer `Result` carries the fault, this alias the answer.
///
/// Each level of the chain composes with `?` on both:
///
/// ```text
/// let Ok(unit) = prepare_active_unit(&state, &player, unit)? else { return };
/// let Ok(movement) = unit.prepare_movement(path)? else { return };
/// ```
pub(crate) type Prepared<T> = Result<T, Violation>;

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

/// Resolve movement without choosing the action at its destination.
pub(crate) fn prepare_movement<'a>(
    state: &'a State,
    player: &PlayerId,
    unit: UnitId,
    path: Vec<Pos>,
) -> Result<Prepared<PreparedMovement<'a>>, ExecuteError> {
    prepared(prepare_movement_inner(state, player, unit, path))
}

/// Resolve the checks shared by movement and deletion.
///
/// This opens a turn for the one question. A caller asking several, such as
/// every unit of a turn or a unit and a production site, wants
/// [`ActiveTurn::opened`] once and its methods after, which share the acting
/// team's view.
pub(crate) fn prepare_active_unit<'a>(
    state: &'a State,
    player: &PlayerId,
    unit: UnitId,
) -> Result<Prepared<PreparedActiveUnit<'a>>, ExecuteError> {
    prepared(ActiveTurn::opened(state, player).and_then(|turn| turn.prepare_unit(unit)))
}

/// Whether the reducer would accept `command` against `state`.
///
/// The same deterministic checks [`execute`] makes, and from one shared
/// implementation of each, without the state clone, the mutation or the random
/// draws. [`crate::session::Legal`] asks this about the orders that belong to
/// no unit and no destination, where there is no cheaper question.
pub(crate) fn accepts(state: &State, command: Command) -> Result<bool, ExecuteError> {
    match prepare(state, command) {
        Ok(_) => Ok(true),
        Err(ReducerError::Violation(_)) => Ok(false),
        Err(error) => Err(execute_error(error)),
    }
}

impl<'a> PreparedMovement<'a> {
    /// Bind the movement to facts shared by all actions at its destination.
    pub fn prepare_destination(self) -> PreparedDestination<'a> {
        let owner = self.state.units[self.movement.unit_index()].owner;
        let maps = TurnMaps::for_seat(self.state, owner)
            .expect("a validated movement names a player on the roster");
        self.prepare_destination_with(maps)
    }

    pub(crate) fn prepare_destination_with<M>(self, maps: M) -> PreparedDestination<'a, M>
    where
        M: Borrow<TurnMaps<'a>>,
    {
        PreparedDestination {
            movement: self,
            maps,
            available: OnceCell::new(),
            trap: OnceCell::new(),
        }
    }

    pub(crate) const fn state(&self) -> &State {
        self.state
    }

    pub(crate) const fn unit(&self) -> UnitId {
        self.unit
    }

    pub(crate) const fn plan(&self) -> &MovedUnit<'a> {
        &self.movement
    }
}

/// A validated movement with facts shared by its destination actions.
///
/// The borrowed state prevents this proof from being applied to a different
/// state. Destination occupancy, visibility, and hidden movement traps are
/// resolved once when an action needs them. The default form owns the turn's
/// tables. A move field supplies a form that borrows the tables it shares.
#[derive(Debug)]
pub(crate) struct PreparedDestination<'a, M = TurnMaps<'a>> {
    movement: PreparedMovement<'a>,
    maps: M,
    available: OnceCell<Result<AvailableDestination, Violation>>,
    trap: OnceCell<Option<(usize, Pos, UnitId)>>,
}

/// One action a mover may take at a validated destination.
///
/// The implementing type carries the action's arguments; validating it produces
/// the proof that executing it consumes. Every destination action differs only
/// in those two things, so [`PreparedDestination::prepare_action`] and
/// [`PreparedDestination::can_action`] are written once and serve all of them.
///
/// This is the module's own vocabulary, not the caller's. The public surface
/// stays the named `prepare_*` and `can_*` methods, which name their arguments
/// and need no import.
trait DestinationAction<'a>: Sized {
    /// What validation established, and what execution consumes.
    type Proof;

    /// Decide whether this action is legal at `at`.
    fn validate<M>(&self, at: &PreparedDestination<'a, M>) -> Result<Self::Proof, ReducerError>
    where
        M: Borrow<TurnMaps<'a>>;

    /// Name the prepared-command variant this action produces.
    fn into_kind(bound: MovementAction<'a, Self::Proof>) -> PreparedCommandKind<'a>;
}

impl<'a, M> PreparedDestination<'a, M>
where
    M: Borrow<TurnMaps<'a>>,
{
    /// Validate `action` here and keep the proof bound to this movement.
    ///
    /// The reducer path wants a rejection as an error, which is what separates
    /// this from [`Self::prepare_action`].
    fn validated<A>(self, action: A) -> Result<MovementAction<'a, A::Proof>, ReducerError>
    where
        A: DestinationAction<'a>,
    {
        let proof = action.validate(&self)?;
        let trap = self.trap();
        Ok(MovementAction {
            movement: self.into_movement(),
            action: proof,
            trap,
        })
    }

    /// Validate `action` here and erase which action it was.
    fn prepare_kind<A>(self, action: A) -> Result<PreparedCommandKind<'a>, ReducerError>
    where
        A: DestinationAction<'a>,
    {
        self.validated(action).map(A::into_kind)
    }

    /// Whether `action` would be accepted here, without preparing it.
    fn can_action<A>(&self, action: A) -> Result<bool, ExecuteError>
    where
        A: DestinationAction<'a>,
    {
        prepared(action.validate(self)).map(|outcome| outcome.is_ok())
    }

    pub(crate) fn can_wait(&self) -> Result<bool, ExecuteError> {
        self.can_action(movement::Wait)
    }

    pub(crate) fn can_capture(&self) -> Result<bool, ExecuteError> {
        self.can_action(property::Capture)
    }

    pub(crate) fn can_supply(&self) -> Result<bool, ExecuteError> {
        self.can_action(transport::Supply)
    }

    pub(crate) fn can_hide(&self) -> Result<bool, ExecuteError> {
        self.can_action(movement::Conceal(Concealment::Hidden))
    }

    pub(crate) fn can_reveal(&self) -> Result<bool, ExecuteError> {
        self.can_action(movement::Conceal(Concealment::Exposed))
    }

    pub(crate) fn can_join(&self, target: UnitId) -> Result<bool, ExecuteError> {
        self.can_action(transport::Join(target))
    }

    pub(crate) fn can_load(&self, transport: UnitId) -> Result<bool, ExecuteError> {
        self.can_action(transport::Load(transport))
    }

    pub(crate) fn can_attack(&self, target: AttackTarget) -> Result<bool, ExecuteError> {
        self.can_action(attack::Attack(target))
    }

    pub(crate) fn can_repair(&self, target: UnitId) -> Result<bool, ExecuteError> {
        self.can_action(transport::Repair(target))
    }

    pub(crate) fn can_launch(&self, target: Pos) -> Result<bool, ExecuteError> {
        self.can_action(special::Launch(target))
    }

    /// Whether a launch from here could be accepted at any target at all.
    ///
    /// Everything [`can_launch`] checks except the target's own bounds. A
    /// caller walking the board for launch targets asks this first, because a
    /// spent silo or a mover that cannot fire one refuses every tile, and
    /// finding that out tile by tile is a board-sized answer to a question
    /// about the tile underfoot.
    ///
    /// [`can_launch`]: Self::can_launch
    pub(crate) fn can_launch_anywhere(&self) -> Result<bool, ExecuteError> {
        prepared(special::launch_preflight(self)).map(|outcome| outcome.is_ok())
    }

    pub(crate) fn can_explode(&self) -> Result<bool, ExecuteError> {
        self.can_action(special::Explode)
    }

    pub(crate) const fn movement(&self) -> &PreparedMovement<'a> {
        &self.movement
    }

    fn into_movement(self) -> PreparedMovement<'a> {
        self.movement
    }

    /// Drop this destination and give its route buffers back, emptied.
    pub(crate) fn recycle(self) -> (Vec<Pos>, Vec<u64>) {
        self.movement.movement.recycle()
    }

    pub(crate) fn view(&self) -> &AwbwView<'a> {
        self.maps.borrow().view()
    }

    /// What every player holds, for the commander rules that score a strike.
    pub(crate) fn holdings(&self) -> &Holdings<'a> {
        self.maps.borrow().holdings()
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
                self.movement.plan(),
                self.movement.unit(),
                self.view(),
            )
        })
    }
}

impl<'a> PreparedActiveUnit<'a> {
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

    /// Whether the reducer would accept deleting this unit.
    pub(crate) fn can_delete(&self) -> Result<bool, ExecuteError> {
        prepared(special::prepare_delete(self.clone())).map(|outcome| outcome.is_ok())
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

impl PreparedProductionSite<'_> {
    /// Whether the reducer would accept building `kind` here.
    ///
    /// Binding the site counts the player's army and reads the board, so a
    /// caller asking about every unit kind binds once and probes many times.
    pub(crate) fn can_produce(&self, kind: UnitKindId) -> Result<bool, ExecuteError> {
        prepared(property::prepare_production(self.clone(), kind)).map(|outcome| outcome.is_ok())
    }

    /// What this site would charge for `kind`, or why it would refuse.
    ///
    /// A build menu shows a unit the player cannot pay for and hides one the
    /// site cannot make at all, and telling those apart means reading the
    /// refusal rather than a boolean. The order of the checks makes that
    /// readable. The price is tested last, so an
    /// [`Violation::InsufficientFunds`] means everything else about the
    /// request was accepted.
    ///
    /// This answers about the request alone. [`Self::can_produce`] answers the
    /// stronger question the action space needs, whether applying the build
    /// would also succeed, which further requires a state that can name the
    /// unit it creates.
    pub(crate) fn produce_cost(&self, kind: UnitKindId) -> Result<Prepared<u64>, ExecuteError> {
        prepared(property::production_cost(self, kind))
    }
}

impl<'a> PreparedUnloadTransport<'a> {
    /// Resolve cargo carried by this transport.
    pub(crate) fn cargo(
        &self,
        cargo: UnitId,
    ) -> Result<Prepared<PreparedUnloadCargo<'a>>, ExecuteError> {
        prepared(transport::prepare_unload_cargo(self.clone(), cargo))
    }
}

impl PreparedUnloadCargo<'_> {
    /// Whether the reducer would accept putting this cargo down there.
    pub(crate) fn can_unload(&self, destination: Pos) -> Result<bool, ExecuteError> {
        prepared(transport::prepare_unload(self.clone(), destination))
            .map(|outcome| outcome.is_ok())
    }
}

/// Split a reducer result into the semantic answer and the fault.
fn prepared<T>(result: Result<T, ReducerError>) -> Result<Prepared<T>, ExecuteError> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(ReducerError::Violation(violation)) => Ok(Err(violation)),
        Err(error) => Err(execute_error(error)),
    }
}

fn prepare(state: &State, command: Command) -> Result<PreparedCommandKind<'_>, ReducerError> {
    /// Resolve the movement every destination action shares.
    macro_rules! at {
        ($player:expr, $unit:expr, $path:expr) => {
            prepare_movement_inner(state, &$player, $unit, $path)?.prepare_destination()
        };
    }
    match command {
        Command::MoveWait { player, unit, path } => {
            at!(player, unit, path).prepare_kind(movement::Wait)
        }
        Command::MoveCapture { player, unit, path } => {
            at!(player, unit, path).prepare_kind(property::Capture)
        }
        Command::MoveSupply { player, unit, path } => {
            at!(player, unit, path).prepare_kind(transport::Supply)
        }
        Command::MoveHide { player, unit, path } => {
            at!(player, unit, path).prepare_kind(movement::Conceal(Concealment::Hidden))
        }
        Command::MoveReveal { player, unit, path } => {
            at!(player, unit, path).prepare_kind(movement::Conceal(Concealment::Exposed))
        }
        Command::MoveExplode { player, unit, path } => {
            at!(player, unit, path).prepare_kind(special::Explode)
        }
        Command::MoveJoin {
            player,
            unit,
            path,
            target,
        } => at!(player, unit, path).prepare_kind(transport::Join(target)),
        Command::MoveLoad {
            player,
            unit,
            path,
            transport,
        } => at!(player, unit, path).prepare_kind(transport::Load(transport)),
        Command::MoveAttack {
            player,
            unit,
            path,
            target,
        } => at!(player, unit, path).prepare_kind(attack::Attack(target)),
        Command::MoveRepair {
            player,
            unit,
            path,
            target,
        } => at!(player, unit, path).prepare_kind(transport::Repair(target)),
        Command::MoveLaunch {
            player,
            unit,
            path,
            target,
        } => at!(player, unit, path).prepare_kind(special::Launch(target)),
        Command::ProduceUnit {
            player,
            position,
            kind,
        } => property::prepare_production_site(&ActiveTurn::opened(state, &player)?, position)
            .and_then(|site| property::prepare_production(site, kind))
            .map(PreparedCommandKind::Produce),
        Command::DeleteUnit { player, unit } => ActiveTurn::opened(state, &player)?
            .prepare_unit(unit)
            .and_then(special::prepare_delete)
            .map(PreparedCommandKind::Delete),
        Command::Unload {
            player,
            transport,
            cargo,
            destination,
        } => transport::prepare_unload_transport(&ActiveTurn::opened(state, &player)?, transport)
            .and_then(|transport| transport::prepare_unload_cargo(transport, cargo))
            .and_then(|cargo| transport::prepare_unload(cargo, destination))
            .map(PreparedCommandKind::Unload),
        Command::ActivatePower { player, level } => {
            powers::prepare_power(ActiveTurn::opened(state, &player)?, level)
                .map(PreparedCommandKind::Power)
        }
        Command::Tag { player } => turn::prepare_boundary(
            ActiveTurn::opened(state, &player)?,
            turn::BoundaryCommand::Tag,
        )
        .map(PreparedCommandKind::Boundary),
        Command::EndTurn { player } => turn::prepare_boundary(
            ActiveTurn::opened(state, &player)?,
            turn::BoundaryCommand::EndTurn,
        )
        .map(PreparedCommandKind::Boundary),
        Command::Resign { player } => turn::prepare_boundary(
            ActiveTurn::opened(state, &player)?,
            turn::BoundaryCommand::Resign,
        )
        .map(PreparedCommandKind::Boundary),
        Command::Unsupported => Err(ReducerError::UnsupportedCommand),
    }
}

fn prepare_movement_inner<'a>(
    state: &'a State,
    player: &PlayerId,
    unit: UnitId,
    path: Vec<Pos>,
) -> Result<PreparedMovement<'a>, ReducerError> {
    let turn = ActiveTurn::opened(state, player)?;
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

/// Apply a resolved command. The one place a command changes anything.
///
/// [`execute_with`] is the only way in, so there is one reducer path and no
/// second one to keep equivalent to it.
fn apply(
    command: PreparedCommandKind<'_>,
    draws: &mut Draws<'_>,
) -> Result<Execution, ReducerError> {
    match command {
        PreparedCommandKind::Wait(prepared) => Ok(movement::execute_prepared_wait(prepared)),
        PreparedCommandKind::Capture(prepared) => property::execute_prepared_capture(prepared),
        PreparedCommandKind::Supply(prepared) => Ok(transport::execute_prepared_supply(prepared)),
        PreparedCommandKind::Concealment(prepared) => {
            Ok(movement::execute_prepared_concealment(prepared))
        }
        PreparedCommandKind::Join(prepared) => transport::execute_prepared_join(prepared),
        PreparedCommandKind::Load(prepared) => Ok(transport::execute_prepared_load(prepared)),
        PreparedCommandKind::Attack(prepared) => attack::execute_prepared_attack(prepared, draws),
        PreparedCommandKind::Repair(prepared) => transport::execute_prepared_repair(prepared),
        PreparedCommandKind::Launch(prepared) => special::execute_prepared_launch(prepared),
        PreparedCommandKind::Explode(prepared) => special::execute_prepared_explode(prepared),
        PreparedCommandKind::Produce(prepared) => {
            Ok(property::execute_prepared_production(prepared))
        }
        PreparedCommandKind::Delete(prepared) => special::execute_prepared_delete(prepared),
        PreparedCommandKind::Unload(prepared) => Ok(transport::execute_prepared_unload(prepared)),
        PreparedCommandKind::Boundary(prepared) => turn::execute_prepared_boundary(prepared, draws),
        PreparedCommandKind::Power(prepared) => powers::execute_prepared_power(prepared),
    }
}

fn reduce(
    state: &State,
    command: Command,
    draws: &mut Draws<'_>,
) -> Result<Execution, ReducerError> {
    apply(prepare(state, command)?, draws)
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
///
/// This names one *state*, not a player's whole turn. A player takes many
/// actions in a turn and each accepted one produces a new [`State`]; the
/// borrow here is what makes that safe. Nothing can change the state while
/// this value lives, and an [`Execution`] hands back a different `State`
/// altogether, so the next action opens a new `ActiveTurn` and everything
/// cached below is rebuilt against the state it actually describes. The cache
/// cannot go stale because it cannot outlive its subject.
#[derive(Debug)]
pub(crate) struct ActiveTurn<'a> {
    state: &'a State,
    /// The active player's seat, resolved once when the turn opened. Units name
    /// their owner this way, so every ownership check reads it.
    seat: PlayerIdx,
    /// The acting team's board tables, shared by everything prepared from it.
    ///
    /// A team's sightings, where every unit stands, what each tile blocks and
    /// what it costs to enter are facts about the state, not about one unit or
    /// one destination, and enumerating an action space asks for them
    /// thousands of times over. A caller that enumerates through this value
    /// pays for them once; one that opens a turn to run a single command never
    /// touches them.
    maps: OnceCell<TurnMaps<'a>>,
    /// The board-sized half of those maps, which the opener may have kept from
    /// an earlier turn on this same position. Rebuilding an entry-cost map per
    /// movement class and a blocking map is most of what opening a turn costs.
    /// A caller that opens two turns on one position, offering an order and
    /// then spelling its route, would otherwise pay twice.
    tables: TurnTables,
}

impl<'a> ActiveTurn<'a> {
    /// Run the shared checks, in the order `spec/model/violations.md` fixes:
    /// ruleset, then terminal match, then phase, then actor.
    ///
    /// `tables` is the board tables the caller kept from an earlier turn on
    /// this same position, or [`TurnTables::default`] to have this turn build
    /// and drop its own. See [`TurnTables`].
    pub(crate) fn open(
        state: &'a State,
        player: &PlayerId,
        tables: TurnTables,
    ) -> Result<Prepared<Self>, ExecuteError> {
        prepared(Self::opened_with(state, player, tables))
    }

    /// The board tables every unit of this turn shares.
    pub(crate) fn maps(&self) -> &TurnMaps<'a> {
        self.maps.get_or_init(|| {
            TurnMaps::with_tables(self.state, self.seat, self.tables.clone())
                .expect("an open turn names a player on the roster")
        })
    }

    /// Resolve the checks shared by movement and deletion.
    pub fn unit(&self, unit: UnitId) -> Result<Prepared<PreparedActiveUnit<'a>>, ExecuteError> {
        prepared(self.prepare_unit(unit))
    }

    /// Bind a production position to this turn.
    pub fn production_site(
        &self,
        position: Pos,
    ) -> Result<Prepared<PreparedProductionSite<'a>>, ExecuteError> {
        prepared(property::prepare_production_site(self, position))
    }

    /// Bind an unload-capable transport to this turn.
    pub fn unload(
        &self,
        transport: UnitId,
    ) -> Result<Prepared<PreparedUnloadTransport<'a>>, ExecuteError> {
        prepared(transport::prepare_unload_transport(self, transport))
    }

    pub(crate) fn opened(state: &'a State, player: &PlayerId) -> Result<Self, ReducerError> {
        Self::opened_with(state, player, TurnTables::default())
    }

    /// The same turn, reusing board tables the caller kept from this position.
    ///
    /// The caller vouches that `tables` names this position and this turn's
    /// seat. See [`TurnTables`]. Reusing tables from another position would
    /// answer about a board that is gone.
    pub(crate) fn opened_with(
        state: &'a State,
        player: &PlayerId,
        tables: TurnTables,
    ) -> Result<Self, ReducerError> {
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
        // The name matched the turn, so a seat that does not resolve means the
        // roster disagrees with the turn — a broken state, not a refused move.
        let seat = state.player_index(player).ok_or_else(|| {
            ReducerError::InvalidState(InvalidStateError::from(
                "the active player is not on the roster",
            ))
        })?;
        Ok(Self {
            state,
            seat,
            maps: OnceCell::new(),
            tables,
        })
    }

    pub(crate) const fn state(&self) -> &'a State {
        self.state
    }

    pub(crate) const fn player(&self) -> &'a PlayerId {
        &self.state.turn.active_player
    }

    /// The same player, as the units name them.
    pub(crate) const fn seat(&self) -> PlayerIdx {
        self.seat
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
        if subject.owner != self.seat {
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
        AwbwVisibility, Board, ObservedEvent, ObservedUnitRef, PlayerStatus, ReasonId, Silo, Tile,
        TileOwner, UnitAction, VictoryReason, Visibility,
    };
    use crate::violation::Action;
    use serde_json::{Value, json};

    /// The seat every single-player fixture below seats its player in.
    const SEAT_ZERO: crate::semantic::PlayerIdx = crate::semantic::PlayerIdx::from_seat(0);

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

    /// The four checks `Turn::opened` folds together, each in isolation.
    ///
    /// These were restated at the top of nine reducers and only ever reached
    /// through `execute`; `open` is the single place they live now, so this is
    /// the first time they can be exercised directly.
    #[test]
    fn opening_a_turn_checks_ruleset_then_match_then_phase_then_actor() {
        let base = movement_state(3);
        let red = PlayerId::from("red");
        let blue = PlayerId::from("blue");
        ActiveTurn::opened(&base, &red).unwrap();

        let mut wrong_ruleset = base.clone();
        wrong_ruleset.ruleset.revision = "1999-01-01".into();
        assert_eq!(
            ActiveTurn::opened(&wrong_ruleset, &red).unwrap_err(),
            ReducerError::UnsupportedRuleset
        );

        let mut finished = base.clone();
        finished.match_state = Match::Finished {
            outcome: Outcome::Cancelled {
                reason: ReasonId::from("aborted"),
            },
        };
        assert_eq!(
            ActiveTurn::opened(&finished, &red).unwrap_err(),
            violation(Violation::MatchFinished)
        );

        let mut wrong_phase = base.clone();
        wrong_phase.turn.phase = Phase::TurnEnd;
        assert_eq!(
            ActiveTurn::opened(&wrong_phase, &red).unwrap_err(),
            violation(Violation::WrongPhase {
                expected: Phase::UnitAction,
                actual: Phase::TurnEnd,
            })
        );

        assert_eq!(
            ActiveTurn::opened(&base, &blue).unwrap_err(),
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
            ActiveTurn::opened(&state, &PlayerId::from("red")).unwrap_err(),
            violation(Violation::MatchFinished)
        );
    }

    fn plan_for(state: &State, path: Vec<Pos>) -> Result<(), ReducerError> {
        ActiveTurn::opened(state, &PlayerId::from("red"))?.prepare_move(UnitId::new(0), path)?;
        Ok(())
    }

    /// Path validation used to exist in three verbatim copies reachable only
    /// through `execute`. Several of these codes have no fixture at all, so
    /// these assertions are their only coverage.
    #[test]
    fn planning_a_move_rejects_every_malformed_path() {
        let state = movement_state(4);
        let origin = Pos::new(0, 0);
        plan_for(&state, vec![origin, Pos::new(1, 0)]).unwrap();

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
        state.player_mut(SEAT_ZERO).commanders[0].id = crate::semantic::CommanderId::Sturm;
        let plain = *state.board.tile(Pos::new(0, 0));
        let mut teleporter = plain;
        teleporter.terrain = TerrainId::Teleporter;
        set_row(
            &mut state,
            vec![plain, teleporter, teleporter, teleporter, teleporter, plain],
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
        let plain = *state.board.tile(Pos::new(0, 0));
        let mut teleporter = plain;
        teleporter.terrain = TerrainId::Teleporter;
        set_row(
            &mut state,
            vec![plain, teleporter, teleporter, plain, plain],
        );
        let mut blocker = state.units[0];
        blocker.id = UnitId::new(1);
        blocker.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
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
        let plain = *state.board.tile(Pos::new(0, 0));
        set_row(&mut state, vec![plain; width]);
        state.teams.push(crate::semantic::Team {
            id: "blue-team".into(),
            status: crate::semantic::TeamStatus::Active,
        });
        let mut blue = state.players[0].renamed("blue".into());
        blue.team = "blue-team".into();
        blue.commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.player_mut(SEAT_ZERO).commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.players =
            crate::semantic::Roster::new(state.players.iter().cloned().chain([blue]).collect())
                .expect("two players fit a roster");
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
        state.player_mut(SEAT_ZERO).commanders[0].power_uses = 1;
        state.player_mut(SEAT_ZERO).commanders[0].power_charge = 21_599;
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
        let mut destination = *state.board.tile(Pos::new(0, 0));
        destination.capture_points = Some(crate::semantic::CAPTURE_REQUIRED_POINTS);
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
        let mut plain = *state.board.tile(Pos::new(0, 0));
        plain.terrain = TerrainId::Plain;
        plain.owner = TileOwner::NotOwnable;
        plain.capture_points = None;
        let destination = *state.board.tile(Pos::new(0, 0));
        set_row(&mut state, vec![plain, plain, plain, destination]);
        let mut blocker = state.units[0];
        blocker.id = UnitId::new(1);
        blocker.kind = UnitKindId::Tank;
        blocker.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
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
        let mut plain = *state.board.tile(Pos::new(0, 0));
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
        let mut blocker = state.units[0];
        blocker.id = UnitId::new(1);
        blocker.kind = UnitKindId::Tank;
        blocker.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
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
        let mut plain = *state.board.tile(Pos::new(0, 0));
        plain.terrain = TerrainId::Plain;
        plain.owner = TileOwner::NotOwnable;
        plain.capture_points = None;
        plain.silo = None;
        let mut silo = plain;
        silo.terrain = TerrainId::MissileSilo;
        silo.silo = Some(Silo::Ready);
        let base = *state.board.tile(Pos::new(0, 0));
        set_row(&mut state, vec![plain, silo, base, base, base, base]);
        state.units[0].location = Location::Board {
            position: Pos::new(0, 0),
        };
        state.units[0].hp = 20;
        let mut ally = state.units[0];
        ally.id = UnitId::new(1);
        ally.location = Location::Board {
            position: Pos::new(2, 0),
        };
        ally.hp = 100;
        let mut enemy = ally;
        enemy.id = UnitId::new(2);
        enemy.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
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
        let mut plain = *state.board.tile(Pos::new(0, 0));
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
        let mut ally = state.units[0];
        ally.id = UnitId::new(1);
        ally.kind = UnitKindId::Infantry;
        ally.location = Location::Board {
            position: Pos::new(2, 0),
        };
        ally.hp = 100;
        let mut enemy = ally;
        enemy.id = UnitId::new(2);
        enemy.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
        enemy.location = Location::Board {
            position: Pos::new(3, 0),
        };
        enemy.hp = 10;
        let mut reserve = ally;
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
        let mut plain = *state.board.tile(Pos::new(0, 0));
        plain.capture_points = None;
        plain.owner = TileOwner::NotOwnable;
        let mut tiles = row(&state);
        tiles.push(plain);
        set_row(&mut state, tiles);
        let mut reserve = state.units[0];
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
        let mut defender = state.units[0];
        defender.id = UnitId::new(1);
        defender.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
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
        let mut defender = state.units[0];
        defender.id = UnitId::new(1);
        defender.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
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
        let session = crate::session::Session::from_observation(&observation).unwrap();
        let dimensions = session.state().board.dimensions();
        assert_eq!(
            session.legal().forecast(
                session.index_of(UnitId::new(0)).unwrap(),
                dimensions.cell_index(Pos::new(1, 0)).unwrap(),
                dimensions.cell_index(Pos::new(2, 0)).unwrap(),
            ),
            None
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
        property.capture_points = Some(crate::semantic::CAPTURE_REQUIRED_POINTS);
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
        target_tile.owner = TileOwner::Owned(PlayerIdx::from_seat(0));
        target_tile.capture_points = Some(crate::semantic::CAPTURE_REQUIRED_POINTS);
        let mut blocker = state.units[0];
        blocker.id = UnitId::new(1);
        blocker.kind = UnitKindId::Tank;
        blocker.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
        blocker.location = Location::Board {
            position: Pos::new(3, 0),
        };
        let mut target = state.units[0];
        target.id = UnitId::new(2);
        target.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
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
        let mut defender = state.units[0];
        defender.id = UnitId::new(1);
        defender.kind = UnitKindId::Infantry;
        defender.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
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
        let mut blue = state.players[0].renamed("blue".into());
        blue.team = "blue-team".into();
        blue.commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.player_mut(SEAT_ZERO).commanders[0].id = crate::semantic::CommanderId::Neutral;
        state.players =
            crate::semantic::Roster::new(state.players.iter().cloned().chain([blue]).collect())
                .expect("two players fit a roster");
        let mut defender = state.units[0];
        defender.id = UnitId::new(1);
        defender.owner = state
            .player_index(&PlayerId::from("blue"))
            .expect("blue is on the roster");
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
                    to: PlayerStatus::Eliminated,
                    reason: KnownReason::Rout.into()
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

    #[test]
    fn timeout_sets_timed_out_status_and_reason() {
        let case: Value = serde_json::from_str(include_str!(
            "../../../spec/fixtures/elimination/resign-ends-match.json"
        ))
        .unwrap();
        let mut state: State = serde_json::from_value(case["initial_state"].clone()).unwrap();
        let mut events = Vec::new();

        let completed = eliminate_player(
            &mut state,
            &PlayerId::from("blue"),
            VictoryReason::Timeout,
            None,
            None,
            &mut events,
        )
        .unwrap();

        assert!(completed);
        assert_eq!(
            state.find_player(&PlayerId::from("blue")).unwrap().status,
            PlayerStatus::TimedOut
        );
        assert_eq!(
            events,
            [
                Event::PlayerStatusChanged {
                    player: PlayerId::from("blue"),
                    from: PlayerStatus::Active,
                    to: PlayerStatus::TimedOut,
                    reason: KnownReason::Timeout.into(),
                },
                Event::TeamEliminated {
                    team: crate::semantic::TeamId::from("blue-team"),
                    reason: KnownReason::Timeout.into(),
                },
                Event::MatchCompleted {
                    outcome: Outcome::Victory {
                        winners: vec![crate::semantic::TeamId::from("red-team")],
                        reason: VictoryReason::Timeout,
                    },
                },
            ]
        );
    }
}
