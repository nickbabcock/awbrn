//! Lightweight diagnostic values that are safe to use in the core crate.

use serde::{Deserialize, Serialize};

/// A value that applies to an event but may not be known to the observer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "kebab-case")]
pub enum Fact<T> {
    Known(T),
    Unknown,
}

impl<T> Fact<T> {
    /// Construct a known fact.
    pub const fn known(value: T) -> Self {
        Self::Known(value)
    }
}
