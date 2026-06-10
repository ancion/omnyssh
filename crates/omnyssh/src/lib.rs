//! OmnySSH library — public API exposed for integration tests.
//!
//! The binary entry point lives in `main.rs`. This lib target re-exports the
//! internal modules so that files under `tests/` can reach them.

pub mod app;
pub mod config;
pub mod event;
pub mod keybindings;
pub mod ssh;
pub mod term_input;
pub mod ui;
pub mod update;
pub mod utils;
