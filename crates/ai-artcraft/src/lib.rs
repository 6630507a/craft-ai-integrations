//! ArtCraft Omni API-key adapter, separate from account/subscription sign-in.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod http;
mod provider;
pub use provider::{ArtCraftConfig, ArtCraftProvider};
