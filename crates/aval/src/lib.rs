//! The binary's internals, exposed as a library so the test suite exercises
//! the code the CLI runs rather than a copy of it.
//!
//! This exists because a copy had already drifted. `tests/conformance.rs`
//! reimplemented corpus discovery — the directory walk, the `.md` filter and
//! the numeric-prefix rule — so the battery was asserting semantics that
//! happened to match the binary's rather than semantics the binary has. A
//! second source of records would have separated them silently.

pub mod add;
pub mod fetch;
pub mod heads;
pub mod hook;
pub mod links;
pub mod load;
pub mod mcp;
pub mod migrate;
pub mod packfile;
pub mod provenance;
pub mod relevant;
pub mod render;
pub mod status;
