//! Integration suites for the AWBW adapter.
//!
//! One test target so every suite shares [`common`] instead of restating the
//! archive paths and hashing helpers. Run a single suite by name, for example
//! `cargo test --test integration -- recorded_outcomes`.

mod common;

mod command_coverage;
mod compatibility_corpus;
mod initial_state;
mod local_compatibility;
mod recorded_outcomes;
