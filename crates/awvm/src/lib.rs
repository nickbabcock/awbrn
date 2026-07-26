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
pub mod random;
pub mod ruleset;
pub mod semantic;
pub mod transition;
pub mod violation;

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
