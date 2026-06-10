//! OmnySSH core — the frontend-agnostic engine.
//!
//! Hosts the SSH engine, configuration handling, domain events, and the
//! self-updater. Frontends (the `omnyssh` TUI today, others later) depend on
//! this crate and never the other way around: nothing here may depend on
//! terminal-rendering, input, or CLI crates.

pub mod config;
pub mod event;
pub mod ssh;
pub mod update;
pub mod utils;
