# craft-ai-model

Interim data model for AI in the Craft apps: connections, models, capabilities, roles, requests,
jobs, consent and the launcher's shared store, plus a `Provider` trait, a scripted mock and
reusable conformance checks.

Small on purpose: serde only, no I/O, no async runtime, compiles for wasm. Adopt or replace it when
the suite's AI core is chosen (craft-libs issue #4). See `crates/ai-model/src/lib.rs` for the
module map.

A second crate, `craft-ai-native`, holds the desktop-only half: secret stores (OS keychain,
passphrase-encrypted file, environment, layered) and the reader and locked atomic writer for the
launcher's shared store file. The model never depends on it, so the model stays wasm-safe. Real
keychain test: `cargo test -p craft-ai-native -- --ignored keychain_round_trip`.

A third crate, `craft-ai-openai`, is the first real provider: an OpenAI-compatible adapter (text with
streaming, tools and structured output; image generate, edit and inpaint). It passes the shared
conformance checks against a scripted localhost server. Not yet run against a live server or TLS.

A fourth crate, `craft-ai-client`, is what an app holds: `AiClient` with `status`, `plan`, consent,
`start` and `Job`, plus a `ProviderFactory` that new adapters register into.

Connections are added, edited, tested and removed through `ConnectionDraft` (rules, in the model crate) and
`ConnectionManager` plus `TestRunner` (in the client crate); the launcher's Connections screen and the
`ConnectionPanel` an app shows when there is no launcher both sit on them.

A fifth crate, `craft-ai-egui`, has the shared egui widgets: model status and picker, consent prompt,
and job progress with cancel. Needs Rust 1.95 (egui 0.36).

```sh
cargo test -j 8
cargo clippy -j 8 --all-targets -- -D warnings
cargo check -j 8 --target wasm32-unknown-unknown
```

Layout and rules mirror craft-libs (workspace, edition 2024, MIT OR Apache-2.0, never-crash lints,
no unsafe) so it can move there unchanged. Not in any git repo yet; nothing has been pushed.

## Status

A first draft, built outside the Craft repos so it can be read, tried and replaced. Nothing here has been
run against a live server, over TLS, or looked at on screen. The shared store location is a proposal.

## Licence

MIT or Apache-2.0, at your option (`LICENSE-MIT`, `LICENSE-APACHE`).
