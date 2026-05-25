//! Eval harness for Mneme scenario replay.
//!
//! This crate exposes the `mneme-eval` binary and the [`run_cli`] entry point
//! for driving validation, replay, acceptance, baseline, baseline-gate, and
//! baseline-summary commands from local tooling. Scenario behavior and report
//! contracts are the stable surface; internal target adapters remain
//! implementation details while Mneme is pre-1.0.

mod cli;
mod error;
mod fake;
mod mneme_v1;
mod report;
mod runtime;
mod scenario;
mod target;

pub use cli::run_cli;
pub use error::EvalError;
