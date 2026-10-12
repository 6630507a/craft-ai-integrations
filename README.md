# craft-ai-model

Interim data model for AI in the Craft apps: connections, models, capabilities, roles, requests,
jobs, consent and the launcher's shared store, plus a `Provider` trait, a scripted mock and
reusable conformance checks.

Small on purpose: the model uses serde only, no I/O, no async runtime, and compiles for wasm.
Jeremy is driving the shared layer implementation in this interim workspace. We will
propose the smallest common contract to craft-libs for technical review; neither this API nor
its placement there has been accepted yet. See `crates/ai-model/src/lib.rs` for the module map.

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

`craft-ai-comfyui` is an offline-tested, **local-only workflow adapter**. Its shared
`WorkflowLibrary` stores API-format JSON graphs for all apps that use `craft-ai-client`.
It maps a prompt field, and for `image.inpaint` requires explicitly mapped `LoadImage` source
and `LoadImageMask` mask fields connected to the output. Source PNG/JPEG and mask PNG are
uploaded separately to the loopback server using `/upload/image`; the returned safe filenames
are substituted before `/prompt`. Image/video generation workflows remain supported. The adapter
polls `/history/{id}` and retrieves one mapped output through `/view`. Cancellation is off by
default. A caller may opt into targeted cancellation only after confirming that server supports
prompt-scoped interrupts, since an older server might treat it as a global interrupt.
Each configured workflow is offered as one model only after `/object_info` confirms its node
classes and static selector choices, including checkpoint names. Submission repeats the preflight;
it does not guarantee execution will succeed. By default outputs stay inline with a 32 MiB
limit. An app can opt in through `ProviderFactory::with_comfyui_output_dir` to stream output
into its own existing directory, with a 1 GiB maximum and a local `file:` payload handle.
The app owns retention and cleanup. Partial, oversize and redirected transfers leave no asset.
No input file handles, arbitrary image formats, general parameter mapping, MIME/content
verification, real-server execution or app mounting yet.

```sh
cargo test -p craft-ai-comfyui -j 8
cargo test -p craft-ai-client -j 8
cargo clippy -p craft-ai-comfyui -p craft-ai-client -j 8 --all-targets -- -D warnings
cargo fmt --all -- --check
```

Layout and rules mirror craft-libs (workspace, edition 2024, MIT OR Apache-2.0, never-crash lints,
no unsafe). This is a public first-draft repository on `Jeremy8776/craft-ai-model`, not merged into
craft-libs or an app.

## Tools and the local runtime

`craft-ai-features` (wasm-safe) is what a launcher or app shows a person: AI tools such as select subject
or generative fill, where each would appear, how it can run on this computer (model files with size, SHA-256,
source and licence; ComfyUI node packs), and hosted options that have an adapter. `plan` compares a tool with
an `Inventory` and says what is left: ready, a download of a stated size, a licence to accept, or a reason it
cannot run. `craft-ai-comfy-env` (native) finds an existing ComfyUI Desktop install, inventories its model files
and node packs, and downloads missing model files into it: resumable, checked against the recipe's size and
SHA-256 before it takes its real name, token sent only to its own host. It reads an existing install and never
starts, installs or repairs ComfyUI. Only select subject (SAM 3) has a local recipe so far. `ToolDesk` (craft-ai-client) lets a launcher choose which connected model does each tool, stored as the launcher default binding that apps inherit, and run a small generated sample to prove it; hosted samples ask first. Nothing has run
against Hugging Face or a real ComfyUI workflow; node pack installation and the segmentation workflow are not built.

## Media-provider coverage

The provider contract accepts open capability IDs, typed prompts, sources, masks and references,
large-media handles, and asynchronous jobs. Scripted-server adapters now cover OpenAI-compatible
text/image, BFL FLUX image/video, Volcengine Ark Seedance video, and Vertex express-mode Gemini
image generation. Local SAM 3 text-guided image detection has a loopback bridge tested with a fake
predictor; a real checkpoint and Python/CUDA runtime are not set up. Local ComfyUI workflows
are registered in `ProviderFactory` through the shared library. Vertex express-mode keys are
not full-project Vertex credentials and have no verified free key probe. None of these adapters
has been exercised against its real service or a real SAM 3 checkpoint.

Hosted providers return links that expire. `craft-ai-media` is an opt-in wrapper (`ProviderFactory::with_media_dir`)
that copies such a link into an app-owned directory as the job finishes and returns a `file:` handle. It sends no
credentials, accepts https only (loopback only for a connection the user marked local), re-checks every redirect,
refuses private addresses, bounds the size, and takes the file type from the bytes, not a header. A failed copy keeps
the original link and adds a notice rather than failing the job. Scripted-server tested only: no real delivery host,
TLS, retry, resume or expiry tracking, and no input-side upload cache.

There is no production-ready input-side media upload or subscription-entitlement
route. `craft-ai-artcraft` covers only the ArtCraft Omni API-key route (prompt-only image and
video, scripted-server tested); ArtCraft account or subscription access has no adapter because no
official programmatic route is documented. The ComfyUI adapter's opt-in local output persistence is a narrow offline-tested exception,
not a hosted-asset transfer solution. A saved connection does not mean its image or video operations
are proven live. For a media-capable V1, remaining work includes ArtCraft's separate Omni API route,
ComfyUI live execution and large-media input handles, hosted image-edit inputs, hosted durable asset
transfer, and real-service compatibility checks when authorised. API-key access to ArtCraft's
Omni API is not evidence that a user's ArtCraft subscription covers those requests. Standalone apps
must mount the shared ConnectionPanel in Preferences; the panel alone does not integrate an app.
The living acceptance checklist is `../ai-framework-handoff/index.html` (generated from `build.py`).

## Status

A first draft, built outside the Craft repos so it can be read, tried and replaced. Nothing here has been
run against a live server, over TLS, or looked at on screen. The shared store location is a proposal.

## Licence

MIT or Apache-2.0, at your option (`LICENSE-MIT`, `LICENSE-APACHE`).
