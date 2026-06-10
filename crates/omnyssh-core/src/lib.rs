//! OmnySSH core — the frontend-agnostic engine.
//!
//! Hosts the SSH engine, configuration handling, domain events, and the
//! self-updater. Frontends (the `omnyssh` TUI today, others later) depend on
//! this crate and never the other way around: nothing here may reference
//! `ratatui`, `crossterm`, or `clap`.

pub mod update;
pub mod utils;
