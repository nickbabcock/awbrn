//! AWVM: an implementation of the executable specification under `spec/`.
//!
//! The specification defines whether a command is legal and, when it is, the
//! exact state and observable events produced by executing it. The Rust
//! implementation exposes:
//!
//! ```text
//! execute(state, command, random) -> accepted | rejected | error
//! observe(visibility, state, recipient) -> observation | error
//! observe-events(visibility, state, next-state, events, recipient)
//!   -> observed-events | error
//! ```
//!
//! [`semantic`] holds the state and observation values, [`transition`] the
//! validation and reduction, [`combat`] the damage arithmetic, and
//! [`commander`] the revisioned effective-value operators.
//!
//! Those three operations answer questions a caller already knows how to ask.
//! A user interface has the opposite problem — it has to offer the questions —
//! so [`query`] answers *what is legal* from the same rules the reducer
//! enforces: where a unit may move, what it may attack, and which command
//! families are available where. A client that asks [`query`] cannot draw a
//! move range the reducer will refuse, which is the failure mode of computing
//! one alongside.
//!
//! An authority that rolls its own dice, rather than replaying a recorded tape,
//! drives the reducer through [`transition::execute_with`] and
//! [`random::Entropy`].
//!
//! [`prelude`] re-exports what driving all of this needs.
//!
//! This crate depends only on serialization and error-derivation support. It
//! has no engine, ECS, rendering, or AWBW replay dependency, and MUST keep it
//! that way: the
//! specification's whole premise is that the model does not depend on a
//! programming language, engine architecture, serialization library, database
//! schema, replay format, or presentation system. Adapters from replay or ECS
//! identifiers belong in the crates that consume this one.

pub mod combat;
pub mod commander;
pub mod conformance;
pub mod event;
pub mod protocol;
pub mod query;
pub mod random;
pub mod ruleset;
pub mod semantic;
pub mod transition;
pub mod violation;

/// What a consumer needs to drive the machine, in one import.
///
/// The modules are the reference; this is the working set. Nothing is
/// re-exported here that a caller would not reach for while wiring a client, a
/// server, or a replay viewer to the reducer.
pub mod prelude {
    pub use crate::event::{AttackTarget, Event};
    pub use crate::query::{ActionSet, MoveField, QueryError, Step};
    pub use crate::random::{Entropy, Luck, RandomError, RandomTape, RandomToken, Recording};
    pub use crate::ruleset::{CommanderKind, Terrain, UnitKind, WeatherKind};
    pub use crate::semantic::{
        AwbwVisibility, Observation, ObservedEvent, ObservedTransition, ObservedUnit,
        ObservedUnitRef, PlayerId, Pos, State, TeamId, TerrainId, Unit, UnitId, UnitKindId,
        Visibility, observe, observe_events, observe_transition,
    };
    pub use crate::transition::{
        Command, ExecuteError, ExecuteOutcome, Execution, execute, execute_with,
    };
    pub use crate::violation::Violation;
}

#[cfg(test)]
mod error_tests {
    use std::error::Error;

    use crate::commander::PowerActivationError;
    use crate::conformance::ConformanceError;
    use crate::random::RandomError;
    use crate::semantic::{BoardShapeError, DuplicateUnitId, ObserveError};
    use crate::transition::{ExecuteError, InvalidStateError, ReducerError};

    fn assert_error<T: Error>() {}

    #[test]
    fn public_failure_types_implement_error() {
        assert_error::<PowerActivationError>();
        assert_error::<ConformanceError>();
        assert_error::<RandomError>();
        assert_error::<BoardShapeError>();
        assert_error::<DuplicateUnitId>();
        assert_error::<ObserveError>();
        assert_error::<ExecuteError>();
        assert_error::<InvalidStateError>();
        assert_error::<ReducerError>();
    }
}
