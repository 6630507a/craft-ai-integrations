//! Offline-testable local ComfyUI workflow adapter. Image/video generation and image inpaint
//! via API-format graphs. No guessed model catalog, large-media handles, or global interruption.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
mod http;
mod library;
mod output;
mod provider;
mod workflow;
pub use library::{WorkflowLibrary, WorkflowSpec};
pub use provider::{ComfyConfig, ComfyProvider};
pub use workflow::{Binding, Workflow};
