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
- ComfyUI headless: `python start_comfy_headless.py` (venv python in `%USERPROFILE%/AppData/Local/hermes/hermes-agent/venv`), listens on 127.0.0.1:8000 only (another app owns IPv6 ::1:8000). The app also starts it via `sam3_server.rs`. A harmless alembic `0008_drop_asset_meta` warning prints at startup.
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
cd photocraft-wt-ai && CARGO_TARGET_DIR=%USERPROFILE%/Apps/ArtCraft/photocraft-src/target \
  cargo test -p photocraft-ui-egui --lib -j 8 -- sam3_ i18n
# live tests (ComfyUI running on 127.0.0.1:8000)
... -- --ignored runtime_tests
cargo clippy -p photocraft-ui-egui --all-targets -j 8 -- -D warnings
```

Install into the test build: build in `photocraft-wt-devbuild` (`cargo build -p photocraft -j 8`), back up `PhotoCraft-dev\photocraft.exe` to `PhotoCraft-dev\backup\`, copy the new exe over it, hash check. If it is running, write `photocraft-new.exe` beside it instead of killing it.

Drive the installed app: launch with `--control 7878 --control-token-file <token> --automation-read-root <dir> --automation-write-root <dir>`. Scripts are in `sam3-scripts/` next to this file (`test_sam3_multi.py` points and box, `timing_sam3.py <size>` server timings, `start_comfy_headless.py`); they expect the hermes scratch directory for the token and test images. Control facts: `ui.click` and `ui.move` take screen points (equal to screenshot pixels here), `ui.pointer` takes document coordinates but does not set a real hover position, `engine.execute {command:"document.inspect"}` returns `selectionBounds` as [x, y, w, h].

## Rules from Jeremy that apply

No commit or push unasked (this push was asked: his own repos only, feature branches). No em dashes. One cargo process at a time with `-j 8` (heavy builds have crashed his PC). Test builds go into the test build without being asked. Minimal tool use, short sessions.

## SAM 3 parameter inventory and part-concept test (12/10/2026, Jeremy asked)

Verified live on the installed ComfyUI node pack (`comfyui-sam3` in `Documents/ComfyUI/custom_nodes`, wrapping Meta's sam3 code).

Grounding (`SAM3Grounding`): `text_prompt` (one phrase, concrete nouns, parts included), `confidence_threshold` 0-1 default 0.2 (score = presence x class prob), `positive_boxes`/`negative_boxes` (focus/exclude regions), `max_detections` -1..100. Outputs masks, boxes, scores JSON sorted by score; no per-detection label, with one prompt the label is the prompt word. Interactive collector: positive/negative points, one box, `multimask_output` hardcoded on, best of 3 (subpart/part/whole) taken; `predict_inst` also accepts `mask_input` (feed previous low-res logits back, native refinement, unused today) and `return_logits`. Unused nodes: `SAM3Segmentation` (`refinement_iterations` 0-10, `output_best_mask=false` gives all 3 candidates), `SAM3MultipromptSegmentation`, `LoadSAM3Model` (`precision`, `compile`). Wrapper hardcodes resolution 1008 and mask NMS IoU 0.5. Model level `forward_text` accepts a caption LIST: multi-concept detection with per-detection concept labels is possible, node passes one caption.

Live test (astronaut.png 512 px, threshold 0.2, scratch script `sam3_masks_test.py`, masks saved then measured): `eye` 2 detections, the two eyes side by side at y 96-108; `mouth` 1 detection between and below them; `face` 1 big box containing both plus 1 small false positive; `person` whole figure. Part concepts work today; each grounding pass about 1 s. Eye boxes are ~20 px on 512 px, so part detections are small, mind hit targets in the UI.

Jeremy's three asks mapped: refinement = candidate cycling (free), mask_input feedback (one extra 0.1 s call), refinement_iterations (needs SAM3Segmentation routing or a sam3_server endpoint extension). Bbox labels = app-side drawing of boxes+scores already returned; multi-concept labels need the bridge to pass a caption list. Details in global detection = sequential part passes after the main one, or the multi-caption extension.

## End-to-end app test of part concepts (12/10/2026, "reboot and test")

PhotoCraft test build restarted with the control channel and driven through the real panel (scratch script `sam3_app_ground_test2.py`; panel text field focused by ui.click, text via ui.type, Enter to ground). Results on astronaut.png 512 px, exact match with the node-level test:

- `eye` -> status "Selected \"eye\"", selectionBounds [193, 96, 18, 9]
- `mouth` -> "Selected \"mouth\"", [206, 141, 37, 10]
- `person` -> "Selected \"person\"", [20, 16, 341, 496]

Bug found while testing: the panel was INVISIBLE on a fresh start. egui memory (PhotoCraftData/ui.ron, portable) had restored its position at pivot x=2051 from a session where the window was wider, but today's window geometry is 1707 px, so the floating panel rendered outside the viewport while `sam3_panel` was true; the tool looked dead ("Click an object to select it" but no panel). Workaround applied by hand: with the app closed, every `pivot_pos` x>1700 in ui.ron was moved to 900. Code fix needed with NEXT item 9: clamp the restored window position into the current viewport (or reset it when out of bounds). Also note ui.set alone does not reopen a closed panel if the tool was already active; switch tools to trigger `follow_panel`.

## The popup is now a Properties section (12/10/2026, Jeremy: "the pop up should be in properties panel along with any settings or parameters we add")

The floating Select Subject card is gone; its controls are a collapsible "Select Subject" section at the top of Properties, shown while the Object Selection tool is active. Docked Properties (Pro) and the floating Studio card both render it (`sam3_panel::properties`); Studio keeps the card up even with no layer content. This is where model settings (refinement, labels, part concepts) land as they are added. It also removes the off-screen panel class of bug: nothing floats, so nothing restores off-viewport.

Changes in `photocraft-wt-ai` (uncommitted, on top of the f1d196da merge):
- `sam3_panel.rs` rewritten: `properties(app, ui)` instead of the floating `show`; uses `props_layout::section`, the app widgets, and only pre-existing tl! strings (no new catalog rows needed).
- `panels.rs`: the section at the top of `properties_body_inner` and inside `properties_window` (both calls `#[cfg(not(wasm32))]`; on wasm `sam3 = false`); the Studio card's layer part moved behind `Option<(Layer, bool, bool)>`.
- `sam3_selection.rs`: `follow_panel`/`panel_closed` and the `sam3_panel`/`sam3_panel_closed` state management removed; `state.rs` fields removed (old preferences.json files still load: unknown fields are ignored).
- `lib.rs`: the `sam3_panel::show` call removed.
- New runtime test `the_properties_section_selects_the_subject_from_its_button` (kittest: renders the section, clicks the button by role - the section header shares the label, take the LAST match - and asserts one Job::Ground goes out; also asserts no section with another tool). Replaces the closed-panel test.
- `vi.tsv`: 5 rows appended (hint, Intersect, Replace, Text, person-red-car hint); the other 16 catalogs already had them. Touch `crates/ui-egui/build.rs` after tsv edits.

Gates: sam3+i18n tests 76 passed; clippy -D warnings clean; fmt clean; wasm check clean. Full ui-egui lib has 3 failures that PRE-EXIST this branch's upstream merge (verified on clean HEAD f1d196da): `retouch_ui::object_selection_click_falls_back_to_quick_selection`, `canvas_menu_tests::secondary_taps_keep_selection_menus_open`, `file_dialog::tests::export_as_with_several_artboards_selected_writes_one_file_each`. Not caused by SAM 3 work; the next session should look at them.

Ported by hand into `photocraft-wt-devbuild` (same edits; branch NOT committed so update-dev-build.py would lose them). Test build installed 14:21, backup `backup/photocraft-pre-props-section-20261012-1421.exe`, hash verified. Live-verified through the control channel: typed `eye` -> "Selected \"eye\"" bounds [193,96,18,9]; `mouth` -> [206,141,37,10]; screenshots `sam3-props-*.png` in the hermes scratch.

## Follow-up same day: the section now follows the Properties panel's own conventions (Jeremy: "why is it jumbled with doc properties look at how properties panel works!!!")

First cut was wrong: it drew the section ABOVE the panel's header row with its own egui grid style, so it sat on top of the Document properties and read as one jumble. Fixed to how the panel actually composes:

- Every Properties view draws its header first (layer header via `props_layout::header`, or the Document row inside `doc_props_ui::properties`), then sections. The Select Subject section is now drawn IMMEDIATELY AFTER that header in every view: `sam3_props()` call sites after the header in the mask, vector-mask and main layer branches of `properties_body_inner` (panels.rs), inside `doc_props_ui::properties` after the Document row, and in the Studio card after its layer header (or right after the title when no layer content matches). Each section's own separator (props_layout::section) provides the hairline, like Transform/Canvas.
- Content rows now use the panel's field style: a right-aligned 44px dim label column ("Mode", "Text") then the control filling the rest (matches `doc_props_ui::field_label`); 12px text; four equal buttons with the accent-filled primary Apply.
- `sam3_panel::properties` keeps only pre-existing tl! strings, so no catalog changes.

Gates re-run after the restyle: sam3 tests 17 passed, clippy -D warnings clean, wasm check clean. Ported to devbuild, exe installed 15:1x (backup `photocraft-props-jumbled-20261012-*.exe`), live-verified: typing `eye` in the relocated field and pressing the button selects the eye, bounds [193, 96, 18, 9]. Note for future tests: the "Selected ..." status is cleared quickly by the selection-change event, so poll status at <=0.2s or assert on `document.inspect` selectionBounds instead.
