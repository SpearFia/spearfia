// SPDX-License-Identifier: MIT OR Apache-2.0
//! Project-bound CLI production coordination, shared by the desktop and CLI.
mod config;
mod runner;
mod verify;
pub use config::{CONFIG, Check, Pipeline, Stage, defaults, load_pipeline, save_pipeline};
pub use runner::{Action, Run, State, Step, execute, is_running, load_run, request_stop};
pub use spearfia_ai::{Agent, Provider};
pub use verify::verify;

fn invalid(message: impl Into<String>) -> super::OpError {
    super::OpError::Invalid(message.into())
}
