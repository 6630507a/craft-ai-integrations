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
mod sam3_point;
mod sam3_session;
mod workflow;
pub use library::{WorkflowLibrary, WorkflowSpec};
pub use provider::{ComfyConfig, ComfyProvider};
pub use sam3_point::Sam3Point;
pub use sam3_session::Sam3Session;
pub use sam3_session::sam3;
pub use workflow::{Binding, Workflow};
