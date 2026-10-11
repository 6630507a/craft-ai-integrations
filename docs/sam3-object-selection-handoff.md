# SAM 3 Object Selection handoff (11/10/2026, end of day)

Local SAM 3 object selection for PhotoCraft, through the existing Desktop ComfyUI. Works end to end and is installed in the PhotoCraft TEST BUILD. The UI is functional but rough and is the next job. Everything is pushed to Jeremy's own repos only, on feature branches. Nothing is in the storytold org and nothing has a PR. Keep it that way until it is ready to be brought in.

## Where the code is

| Repo | Remote | Branch | What |
|---|---|---|---|
| `Apps/ArtCraft/craft-ai-model` | `Jeremy8776/craft-ai-model` | `feat/sam3-object-selection` | `crates/ai-comfyui`: `Sam3Session`, `sam3::Prompt`, `Sam3Point`, tests. Only the SAM 3 files are on this branch. The rest of that working tree (ai-artcraft, ai-features, ai-media, ai-comfy-env, ai-client tools, README, Cargo.lock) is other uncommitted work and was deliberately left out. |
| `Apps/ArtCraft/photocraft-wt-ai` | `Jeremy8776/photocraft` | `feat/sam3-object-selection` (from `feat/ai-port`) | engine `select.sam3Mask`, ui-egui runtime, panel, prompts, server launcher, i18n rows. Includes the earlier local-only generative fill commit as its parent. |

`upstream` (storytold) is never pushed to. PhotoCraft finds the library by relative path (`ui-egui/Cargo.toml`: `../../../craft-ai-model/crates/ai-comfyui`), so on another machine check both repos out as siblings. craft-ai-model's own Cargo.lock is not on the branch; cargo regenerates it.

`photocraft-wt-devbuild` is the throwaway combined tree (upstream/main plus listed branches) the test exe is built from. It is not pushed. `update-dev-build.py` resets it, so SAM 3 only survives there once the branch is committed and listed in `dev-additions.txt` (add `feat/sam3-object-selection`). Until then, port by hand: copy the `sam3_*.rs` files, re-apply the `canvas.rs`, `lib.rs`, `state.rs` edits (CRLF files, use an exact-count replace, `git apply` fails on canvas.rs), and APPEND the five i18n rows to each catalog. Never copy whole `.tsv` files across, devbuild has newer translations.

## What it does now

- Choose the Object Selection tool. A floating panel opens, the image is prepared once (about 1 s at 1536 px) and the status bar says "Click an object to select it".
- Click adds a positive point, Alt-click a negative point, drag sets one box. The preview follows the cursor (one request in flight, newest position wins, about 0.1 s each). Alt held previews an exclusion.
- Enter or panel Apply commits (about 0.55 s). Backspace undoes the last element, Escape clears. After a commit the runtime re-keys to the new revision so the next click reuses the embedding.
- Mode (replace, add, subtract, intersect) feeds `select.sam3Mask`.
- Select Subject runs text grounding with the panel text; Enter in the text field does the same. Empty text tries person, animal, clothing, bag, shoe, car, plant, furniture in order and reports the word that matched ("Selected “person”"). A word that matches nothing is an error ("Nothing matched ..."), not an empty selection.
- `select.subject` (options bar button, Select > Subject, automation) is routed to SAM 3 while the Object Selection tool is active (`sam3_panel::select_subject`, called at the top of `PhotocraftApp::run`). If SAM 3 has failed, or another tool is active, the older colour-based algorithm still runs.

## Facts learned the hard way

- SAM 3 grounding understands concrete nouns only. "person", "people", "astronaut", "woman", "face", "flag" work. "subject", "object", "main subject", "foreground", "thing", and "animal" (when none is present) return an empty mask. Comma lists ("person, animal") return nothing; it is one phrase.
- A single click is ambiguous between part and whole (a click on a face selected the face only). Multiple points or a box resolve it. A candidate-cycling control would help.
- Server latency is not the problem. Measured at 1536 px: prepare 0.96 s, hover preview 0.1 s, commit 0.55 s, object_info 0.24 s (grounding no longer refetches it). Cold start of ComfyUI on first use is slower and has not been measured or addressed.
- The interactive node's embedding cache is one shared slot: a second session replaces the first.
- ComfyUI headless: `python start_comfy_headless.py` (venv python in `C:/Users/jerem/AppData/Local/hermes/hermes-agent/venv`), listens on 127.0.0.1:8000 only (another app owns IPv6 ::1:8000). The app also starts it via `sam3_server.rs`. A harmless alembic `0008_drop_asset_meta` warning prints at startup.
- Nodes used: `SAM3InteractiveCollector` (points, boxes, preview endpoint `/sam3/interactive_segment_one`) and `SAM3Grounding` (`text_prompt`, `confidence_threshold` 0.2). Edge refinement (`refinement_iterations`) exists only in the Multiprompt node, not the interactive collector.
- The i18n catalogs are also deflated by `build.rs`; if `deflated_catalogs_match_their_files` fails after editing a `.tsv`, touch `crates/ui-egui/build.rs` and rerun.
- `sam3_selection.rs` is about 510 lines (soft limit 500). Split the worker or the hover logic out when next touched.

## NEXT: make the UI a lot better (Jeremy: "we need to make this ui a lot better")

Reference: the current panel is a plain egui window titled "Select Subject" with an "x", a hint line, a Mode combo, a Text field and four small buttons (Select Subject, Apply, Clear, Undo). Reviewed from a screenshot on 11/10/2026. Problems, in rough priority order:

1. **The prompt is invisible on the canvas.** Only the mask overlay is drawn. Draw the clicks as green (positive) and red (negative) dots and the box as an outlined rectangle, so the user can see what the model was told. Allow dragging a dot to move it and clicking one to remove it.
2. **No state feedback.** Show "Preparing SAM 3", a spinner while a preview, commit or grounding request is in flight, disabled buttons while busy, and the matched word or the error in the panel (today it is only in the status bar). First-use ComfyUI cold start needs real progress, not a silent wait.
3. **The panel does not look like the rest of the app.** The title duplicates the first button; the "x" is tiny; the hint text is very low contrast; the Mode combo is heavy; the buttons are small and equal weight, so the primary action (Apply) is not obvious; the layout is cramped with uneven padding. Use `theme::Tokens` and the app's panel and button widgets (`crate::widgets`), not raw egui defaults.
4. **Put controls where Photoshop users expect them.** Selection mode should be the four icon buttons in the options bar (new, add, subtract, intersect) like the other selection tools, not a combo in a floating card. The options bar still says "Drag a rectangle around the object" for this tool; that text is out of date and must describe click, Alt-click, drag and Enter.
5. **Text prompt as a proper control.** A search style field with a Find button, a few suggestion chips (person, car, bag, clothing, ...) because the model needs nouns, and a clear message when nothing matched. Consider a history dropdown.
6. **Show shortcuts on the controls** (Enter applies, Backspace undoes, Esc clears) and make Apply and Undo icon buttons with tooltips.
7. **Candidate masks.** The model can return several; let Tab or a small strip cycle them, which also fixes part versus whole.
8. **Output polish.** Optional feather and smooth, edge refinement (needs the Multiprompt node), and marching ants instead of a flat overlay once committed.
9. **Panel behaviour.** Remember position, do not cover the subject by default, make sure keyboard focus does not fight the text field (global Enter and Backspace handling is skipped while a text field has focus), and give a clear way to reopen it after closing.
10. **Status messages are plain English literals**, not `tl!`; move them into the catalog when the wording settles. New `tl!` strings need rows in all 16 catalogs (context TAB source TAB translation, CRLF) and the `every_tl_literal_is_translated` test.

Consider `claude-design` or `impeccable` first. Jeremy tests UI himself, so build, install into the test build and let him look rather than screenshotting your own edits (one screenshot to find button coordinates for a scripted test is fine).

## Other open items

- Wire SAM 3 into Select > Subject from tools other than Object Selection (needs the runtime to start without the tool).
- Edge refinement; hosted Nano Banana and FLUX 3 editing; update the `build.py` handoff generator.
- Alt-click exclude and the Mode dropdown are unit tested only, not driven through the real UI.
- Text grounding quality on more real photos (only one, the NASA astronaut, public domain, has been tried).
- If this is ever upstreamed: AGENTS.md asks for a `log/devlog.md` entry and parity doc updates; skipped because the work is kept private.

## How to test

```
# library (does not need ComfyUI)
cd craft-ai-model && cargo test -p craft-ai-comfyui --test sam3_session --no-default-features -j 8
# app (one cargo at a time, -j 8, shared target dir)
cd photocraft-wt-ai && CARGO_TARGET_DIR=C:/Users/jerem/Apps/ArtCraft/photocraft-src/target \
  cargo test -p photocraft-ui-egui --lib -j 8 -- sam3_ i18n
# live tests (ComfyUI running on 127.0.0.1:8000)
... -- --ignored runtime_tests
cargo clippy -p photocraft-ui-egui --all-targets -j 8 -- -D warnings
```

Install into the test build: build in `photocraft-wt-devbuild` (`cargo build -p photocraft -j 8`), back up `PhotoCraft-dev\photocraft.exe` to `PhotoCraft-dev\backup\`, copy the new exe over it, hash check. If it is running, write `photocraft-new.exe` beside it instead of killing it.

Drive the installed app: launch with `--control 7878 --control-token-file <token> --automation-read-root <dir> --automation-write-root <dir>`. Scripts are in `sam3-scripts/` next to this file (`test_sam3_multi.py` points and box, `timing_sam3.py <size>` server timings, `start_comfy_headless.py`); they expect the hermes scratch directory for the token and test images. Control facts: `ui.click` and `ui.move` take screen points (equal to screenshot pixels here), `ui.pointer` takes document coordinates but does not set a real hover position, `engine.execute {command:"document.inspect"}` returns `selectionBounds` as [x, y, w, h].

## Rules from Jeremy that apply

No commit or push unasked (this push was asked: his own repos only, feature branches). No em dashes. One cargo process at a time with `-j 8` (heavy builds have crashed his PC). Test builds go into the test build without being asked. Minimal tool use, short sessions.
