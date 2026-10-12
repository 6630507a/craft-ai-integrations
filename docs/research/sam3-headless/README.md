# SAM 3 headless audit for PhotoCraft

Research and live probes: 12/10/2026. This is a capability audit, not an implementation change.

## Verdict

Keep the existing custom pack for the immediate Object Selection work. Headless ComfyUI can run its collectors, point/box segmentation, text grounding, candidate masks and refinement without a browser. The ComfyUI canvas widgets are optional input editors, not a requirement for inference. PhotoCraft can supply the same JSON itself.[2][6]

**Status 12/10/2026: fixed (sg1).** The adapter now returns every bounded mask and an explicit union; see `verification/`. The text below records the original finding.

The first change should be fixing our output adapter, not adding another model. Our grounding graph requests every detection, but `output::fetch` selects `images.first()`. Live grounding of `eye` returned two separate masks; the client presently receives only the first. This also affects any attempt to preserve multiple regions or candidate masks. Evidence: `results.json`, `ground-eyes-all-history.json`, and the local source pointers below.

After that, the highest-value additions are candidate cycling, per-object selection, detection boxes with labels/scores, optional refinement, and selection-to-layer-mask actions. Native ComfyUI SAM 3 is a promising second backend, but it is not configured or inference-tested here.

## 1. What is actually installed

Read directly from the running server and installed source:

- ComfyUI 0.22.3, Python 3.12.9, PyTorch 2.9.1+cu130.
- Headless server at `http://127.0.0.1:8000`, serving the Desktop installation's Python resources with auto-launch disabled. No browser was used for any probe.
- NVIDIA RTX 3090, nominal 24 GB VRAM. This is not a minimum-hardware claim.
- `Documents/ComfyUI/custom_nodes/comfyui-sam3`, origin `PozzettiAndrea/ComfyUI-SAM3`, commit `de0ff5d2c2ea435d29f800abfa568cffdfb94773`.
- The remote `main` commit matched that installed revision when checked.[10]
- Existing weights: `models/sam3/sam3.safetensors`, 3,449,860,632 bytes.
- `/object_info` exposed **19 SAM 3 node IDs**, 15 custom-pack IDs and four native IDs.
- The live schema incorrectly attributes the custom IDs to `custom_nodes.comfyui_fearnworksnodes`. That pack's own mapping contains token/file-count nodes, not SAM 3. Identify the implementation by class IDs plus inspected source, not this module label alone.

Full machine-readable schemas are in `evidence/node-schemas.json`. Server identity and launch arguments are in `evidence/system-stats.json`.

### Complete node inventory

Custom node behaviours and schemas were inspected in the installed Python files.[2][5][6]

| Node ID | Purpose | PhotoCraft use |
|---|---|---|
| `LoadSAM3Model` | Existing model configuration, precision and compile setting | Shared loader, not a user-facing editing tool |
| `SAM3Grounding` | Text plus optional positive/negative exemplar boxes; masks, boxes, scores | Find instances matching a phrase; parts such as eyes and mouth |
| `SAM3Segmentation` | Points/first box; refinement; best mask or all candidates; logits | Candidate cycling and a deliberate Refine action |
| `SAM3MultipromptSegmentation` | Independent geometric regions, one selected mask per region | Separate objects, per-region masks, multi-layer extraction |
| `SAM3CreatePoint` | Normalised point prompt | API graph helper when a collector is unnecessary |
| `SAM3CombinePoints` | Up to ten point-helper inputs | Small explicit graphs; collectors are better for variable counts |
| `SAM3CreateBox` | Normalised centre/width/height exemplar box | Text grounding focus/example prompts |
| `SAM3CombineBoxes` | Up to five box-helper inputs | Combine positive or negative grounding examples |
| `SAM3PointCollector` | Pixel-coordinate JSON to positive/negative point prompts | Headless bridge into `SAM3Segmentation` |
| `SAM3BBoxCollector` | Pixel-coordinate JSON to positive/negative box prompts | Headless bridge into box consumers |
| `SAM3MultiRegionCollector` | JSON regions to typed multi-prompts | Headless bridge into multiprompt segmentation |
| `SAM3InteractiveCollector` | Prepare cached image features; best mask per region; live preview route | Keep for the existing hover tool |
| `SAM3VideoSegmentation` | Video state with text/point/box prompts | Later animation or FilmCraft/EffectCraft adapter |
| `SAM3Propagate` | Propagate video prompts across a frame range/direction | Later tracked masks |
| `SAM3VideoOutput` | Combined or selected object-ID masks and visualisations | Later object-specific video output |
| `SAM3_Detect` | Native text/point/box inference, refinement, union or individual masks | Candidate native still-image backend |
| `SAM3_VideoTrack` | Native tracking, initial masks, text detection | Later shared video capability |
| `SAM3_TrackToMask` | Native tracked-object filtering to masks | Later video mask extraction |
| `SAM3_TrackPreview` | Native rendered tracking preview | Diagnostic output, not PhotoCraft's main UI |

Native behaviours are also present in ComfyUI's own implementation and node documentation.[3][11]

## 2. Headless integration: what we can use

### A. Interactive click/box selection

Our Rust session already uses `SAM3InteractiveCollector` and `/sam3/interactive_segment_one`. Preparing queues the graph once, then previews call the route against a cached embedding. The route returns `num_masks` and a base64 JPEG overlay. It does not return an exact mask, candidate list, detection boxes or score metadata.[6]

**Keep this for hover.** Use an exact mask output for selection commits. Do not infer a precise selection from the coloured JPEG, because it contains the source image, lossy compression and drawn annotations.

The route cache is a dictionary keyed by node ID, not inherently one global image slot. Our Rust adapter uses the same stable node ID for every session, so *our sessions* overwrite one another. A safe multi-document design needs managed session IDs and bounded cache lifetime, or explicit serial ownership. Creating endless unique IDs would retain image state without a cleanup policy.

### B. Candidate cycling

`SAM3Segmentation` with `use_multimask=true` and `output_best_mask=false` returns all candidates. No node modification or browser is needed.[2]

Live test, one point on the astronaut's face:

- Three exact masks returned.
- Candidate 0 covered the face, score 0.95703125.
- Candidate 1 covered the figure's broad extent, score 0.0888671875.
- Candidate 2 covered a narrower elongated region, score 0.421875.

**Recommendation:** let the user cycle thumbnails or overlays with Tab and retain all masks client-side. Do not name the slots permanently “detail / object / whole person”. The candidates are alternatives, not reliable semantic labels. The highest predicted score need not correspond to the scope the person intended.

The current interactive collector hardcodes multimask inference but immediately takes `argmax`, discarding the other candidates. Candidate cycling therefore needs a different graph or a small, versioned bridge extension. It cannot be enabled by adding an unknown parameter to today's hover route.[6]

### C. Per-object text selection and object finder

Grounding returns a batch of individual masks with boxes and scores, sorted by score. `max_detections=-1` requests all; a positive value caps the results.[2]

Live `eye` grounding returned two masks with measured selected-pixel bounds:

- `[193, 96, 211, 105]`, 129 selected pixels.
- `[239, 98, 259, 108]`, 143 selected pixels.

Use these to support:

- Select all objects matching a phrase.
- Select one detection under the cursor.
- Cycle same-class objects.
- Build a cached object-finder overlay, then hit-test masks locally rather than rerunning inference for every hover.
- Create a mask or extracted layer for each instance.

A complete automatic object finder still requires a chosen vocabulary or a separate scene-description step. `SAM3Grounding` is prompted segmentation, not a guaranteed “discover and name every object” operation. Avoid a large noun sweep on every pointer move.

### D. Text labels, boxes and confidence

The custom grounding outputs already include boxes and scores as JSON strings. Wire those outputs to the built-in output node `PreviewAny`; its `text` fields appear in `/history`. This was live-verified, so there is no requirement to install a custom text-saving node.[9]

**Built in sg2a (`Sam3Session::ground_instances`).** Boxes are pixel `[x1, y1, x2, y2]`, scores are 0 to 1, both ordered by score and aligned with the saved masks. Live `eye` result: scores 0.5213 and 0.5029, boxes `[195,96,213,106]` and `[238,100,258,109]`, mask-measured bounds `[193,96,211,105]` and `[239,98,259,108]`, so the detector box is only accurate to a few pixels; the crate exposes both and picks with the mask. **Caching trap:** an output node whose inputs are unchanged since an earlier run is absent from `/history`. This hit `SaveImage` (fixed with a unique filename prefix) and `PreviewAny` (no widget to vary). Each request now routes the text through core `StringConcatenate` with a unique stamp, so only the text nodes re-run, grounding stays cached, and text without this request's stamp is refused. This needs `PreviewAny` and `StringConcatenate`, both present in the installed ComfyUI.

**Built in sg2b (`Sam3Session::candidates`).** A point prompt goes through `SAM3PointCollector` (pixel coordinates; the node normalises them) into `SAM3Segmentation` with `use_multimask` true and `output_best_mask` false. Live face click at (220,110): three candidates in server order with scores 0.9570, 0.0889 and 0.4219; adding a negative point at (100,400) changed the third candidate's extent (bottom edge 460 to 240) and nudged the others. The node's own `boxes` output is derived from the masks with inclusive maxima, so the crate ignores it and measures bounds itself. Image features are recomputed on every call (1.1 s cold, 0.4 s on a cached repeat), so this is a deliberate-action route, not a hover route. 

**Box prompts and refinement (sg2b, built).** The first box goes through `SAM3CreateBox` then `SAM3CombineBoxes` into the node's `box` input as normalised centre and size, alone or with points; a box corner must lie strictly inside the image (the shared prompt rule), so a box cannot end exactly on the far edge. Live face box `[165,70,285,195]`: all three candidates hug it (`[171,76,280,191]` and similar); a box over empty background still returns a mask, so a box prompt always yields something and the person should check it. `candidates_refined(prompt, passes)` takes 0 to 10 passes: three passes on the face click changed the second candidate's score from 0.089 to 0.574 in about 0.7 s.

PhotoCraft should draw its own overlay. The phrase is the label for that query. Assigning `eye` to both detected eye masks is valid; inventing “left eye” or “right eye” requires additional app logic. Neither the existing custom output nor native detection boxes automatically supply a complete per-instance semantic naming system.

Distinguish model-detected boxes from exact mask extents. In the eye probe the detection boxes and measured PNG bounds differed slightly. Use the mask for picking and precise bounds, and keep the returned box as detector metadata.

### E. Multiple concepts and detail passes

Our current node accepts one phrase per grounding call. The uncommitted `sam3_multi_select.rs` splits comma-separated text and unions the results of separate phrase calls. That is an app-side sequence, not one joint model pass, and it still loses all but the first instance of each phrase while the output-fetch bug exists.

Useful patterns after the bug is fixed:

- `person, bag, shoes`: union all returned instances for the requested concepts.
- `eyes, mouth`: retain separate masks for local adjustments.
- Detect a person first, then crop around that person and query details. A real crop is preferable when a user requires a strict region restriction.

The custom grounding node calls `set_image` again on execution. Reusing an uploaded filename avoids upload/render work, but does not prove the grounding graph reuses the interactive embedding. A shared-embedding multi-concept endpoint would be a separate optimisation, with its own concurrency tests.

### F. Independent regions

`SAM3MultiRegionCollector` plus `SAM3MultipromptSegmentation` returned two separate masks for two synthetic discs, from one image preparation. Their extents were disjoint and matched their respective discs. This proves the transport and region separation.

On the astronaut, two points placed on different eyes both returned face masks. Independent prompts do not guarantee semantic uniqueness. The app needs to preview, deduplicate where appropriate, and allow correction rather than treating each click as a new, distinct object automatically.

### G. Refinement

The still-image segmentation node supports `refinement_iterations` from 0 to 10. The multiprompt node supports the same range. They feed the selected low-resolution logits into another prediction, not simply blur the edges.[2]

Live runs with one and three iterations completed and changed the masks. They did **not** prove a general accuracy improvement. The broad candidate shrank substantially after repeated feedback, while the face candidate changed much less.

**Recommendation:** default to zero additional passes for responsiveness. Offer an explicit Refine action, initially one or two passes, with undo and before/after preview. Retain the user's chosen candidate rather than silently selecting a different scope. Keep smoothing, feathering and grow/shrink in PhotoCraft's deterministic mask operations.

The node outputs `mask_logits`, but has no external `mask_input` socket. Persisting logits between separate requests needs an adapter extension. Saving logits through normal image output would clip their negative/unbounded values and is not a valid state-transfer format.

### H. Layer masks, cutouts and targeted edits

These are product uses of the same masks, not new SAM 3 operations:

1. Exact SAM mask becomes an undoable selection.
2. The selection can feed an existing layer-mask or extraction command.
3. Its inverse isolates the background.
4. An inpaint/edit workflow consumes that mask to remove or replace the selected object.

The current engine already exposes `select.sam3Mask` with document/revision validation and replace/add/subtract/intersect modes. It rescales a prepared-image mask across the whole document. A cropped high-resolution refinement result therefore needs explicit crop origin/extent handling; sending a crop mask into that command unchanged would stretch it across the document.

SAM 3 segmentation alone is not a generative remover or a soft-alpha matting model. A mask can identify what to edit, but background synthesis and fine transparent hair/veil edges need separate algorithms/models. Current node outputs threshold masks; a full-size returned PNG does not recover detail lost at model resolution.[2][3]

## 3. Confirmed traps in this installation

### First-result-only output adapter (fixed in sg1)

`craft-ai-model/crates/ai-comfyui/src/output.rs:21-27` searches the output file array, then takes `list.first()`. `Sam3Session::mask_from` uses it. The grounding graph requests all detections, but receives one PNG.

**Required correction:** collect the full bounded output array, validate each mask, and deliberately choose one mask, union all, or preserve instances. For a simple selection, union with pixelwise maximum. For per-object UI, keep metadata and masks aligned. The comma-phrase helper does not fix this by itself.

### Negative boxes are parsed but ignored in point inference

`SAM3InteractiveCollector._parse_raw_prompts` parses negative boxes. `_run_prompts` consumes positive/negative points and only the first positive box. It never passes negative boxes to the predictor.[6]

Live proof: adding a negative box over the entire face produced a pixel-identical mask to the face-only point prompt. Different saved PNG hashes were due to metadata; pixel equality was checked with `ImageChops.difference`.

Do not expose an “exclude rectangle” control through this path. Use negative points, deterministic mask subtraction, or test a separately implemented grounding/bridge route. Positive and negative grounding boxes are model prompts, not guaranteed hard geometric clipping.

### A grounding exemplar is not a hard ROI

A positive grounding box near one eye produced five masks, including detections around the other eye and box-related alternatives. That was a functioning endpoint, but unsuitable as proof of strict “search only inside this rectangle”. Crop first or explicitly clip the final mask if that is the required behaviour.

### Still-image custom nodes process only the first input batch image

`comfy_image_to_pil` takes the zeroth batch image. A live graph supplying two copies of the astronaut returned two eye masks, not four. Batch-photo processing must iterate per image through this wrapper, rather than feeding an IMAGE batch and assuming every photo was processed.

### Caches and concurrency

The interactive fingerprint uses image shape and prompt-store text. The adapter's per-image nonce is important for forcing preparation when a different image has the same dimensions. Keep a document/revision/generation check on both server work and result application.

The custom route has a segmentation lock, but this is not proof that graph execution and hover calls are coordinated safely against every other workflow. Serialise conflicting model/session operations initially. Do not issue a server-wide interrupt to cancel a PhotoCraft preview when the same ComfyUI instance may have unrelated work queued.

## 4. Native ComfyUI SAM 3 / SAM 3.1

Meta released SAM 3.1 Object Multiplex on **27 March 2026**. The current official ComfyUI image-segmentation blueprint uses `sam3.1_multiplex_fp16.safetensors` through `CheckpointLoaderSimple`, whose MODEL and CLIP outputs feed native detection and text encoding.[7][8]

Our live server already exposes the native nodes, but its checkpoint loader lists only an LTX checkpoint. The installed SAM weights are in the custom pack's `models/sam3` location and are not currently selectable through the checkpoint loader. No model-folder registration, copying, download or native inference was performed. Reuse/compatibility of the existing file remains to be tested.

Native `SAM3_Detect` exposes text, points, boxes, per-object masks, per-image batch processing and 0-5 refinement passes. Unlike the custom still-image helper, its implementation loops over input images.[3][11]

The installed native SAM text encoder parses comma-separated categories with `:N` detection caps. Example: `person:10, eye:20, bag:5`. **The suffix is a maximum count, not a prompt weight.** Its default is one detection per phrase, so native migration without explicit counts could reproduce our “first object only” symptom differently.[12]

Important differences:

- Native points/boxes do not expose the custom three-candidate picker interface.
- The native graph has no equivalent of the custom hover-preview HTTP route inspected here.
- Native detection returns masks and bounding-box dictionaries with scores, not custom string outputs or automatic concept labels.
- Native detection handles text and point paths separately, then combines their masks. Supplying a negative point alongside text should not be assumed to refine/subtract from the text detection.
- Native refinement operates differently from the custom feedback loop; “two passes” is not a portable setting with identical behaviour.

**Recommendation:** preserve the existing backend for today's interactive tool. Evaluate native detection as a second provider for image batches, concept lists and future shared video tracking. Benchmark quality, cold start, memory and interactive behaviour before replacing the working route. SAM 3.1's advertised video-tracking gains are not evidence of a still-image speedup on this PC.

## 5. Other packs reviewed, not installed or tested

- **wouterverweirder/comfyui_sam3:** its README exposes both individual masks and a combined mask, size filters and Impact-Pack `SEGS` output. This provides useful design precedent for separating instance results from a union. Adding its dependencies is unnecessary just to fix our output adapter.[14]
- **1038lab/ComfyUI-RMBG:** its changelog dated **30 September 2026** and README describe SAM3 Multiplex, merged/separate segmentation, picking, model unloading and separate matting/background-removal models. It is a useful candidate for a later matting audit, not proof that SAM 3 itself produces fine alpha mattes. Its latest bundle is not installed here, and no advertised quality or performance was measured.[15][16]

Avoid introducing another broad pack solely for blur, grow/shrink or combining masks. Those controls belong in PhotoCraft unless a genuinely new, tested model capability is needed. The existing wrapper warns that its isolation mechanism uses experimental `comfy-env`, which is another reason to evaluate native ComfyUI before adding more wrapper infrastructure.[1]

## 6. Product order I recommend

1. **Fix output completeness.** All returned masks plus aligned metadata, bounded inputs/outputs, explicit union versus individual-instance behaviour.
2. **Candidate cycling.** Keep all three candidates from the noninteractive segmentation graph and switch them locally. Preserve exact masks.
3. **Object results UI.** Prompt label, instance index, score, mask-derived bounds; pick under cursor or select all. Redraw overlays natively.
4. **Refine action.** Small optional pass count, explicit candidate choice, undo and before/after.
5. **Mask destination.** Apply as selection, layer mask, extracted layer, or edit/inpaint target through normal engine commands.
6. **Detail/ROI workflow.** Parent detection, bounded crop, child concept, explicit coordinate mapping. Validate on high-resolution real photographs.
7. **Native backend evaluation.** After the image tool is correct, assess native SAM 3.1 rather than installing another wrapper by default.

This order is an engineering recommendation, not an approved implementation queue.

## 7. Live evidence and limits

Executed **17 probe cases**, producing **28 exact mask PNGs** and three hover JPEGs. Queue state was read back as empty after completion. All exact-mask probe dimensions were verified at 512×512. Evidence includes submitted graph, prompt ID, complete history, returned boxes/scores and decoded mask measurements.

Representative wall times, including queue/history polling but not startup or upload:

| Probe | Result | Seconds |
|---|---|---:|
| Face point, all candidates | Three masks | 1.0212 |
| Same point, one refinement pass | Three changed masks | 0.6060 |
| Same point, three refinement passes | Three changed masks | 0.5042 |
| Text `eye`, all detections | Two masks, boxes and scores | 0.8043 |
| Text `eye`, maximum one | One mask | 1.1978 |
| Two independent disc regions, one refinement pass | Two disjoint masks | 1.3036 |
| Interactive preparation | Cached image state, empty initial mask | 0.6670 |
| Hover requests | One lossy overlay each | 0.2101-0.2375 |
| Exact interactive face commit | One exact mask | 1.1071 |

These are individual warm-server observations, not a statistically controlled benchmark. The lower elapsed time for three refinement passes does not prove more passes are faster. Cached nodes, warm-up, IO and polling affect comparisons. Cold server/model loading was not measured.

Real-photo accuracy was checked only on the NASA astronaut sample. Synthetic discs establish transport and independent-region behaviour, not photographic quality. Video inference, native SAM 3.1 inference, torch.compile, alternative packs, soft matting and app-side UI adoption of the unused features remain unverified.

### Local source pointers

Relative to `Apps/ArtCraft`:

- `craft-ai-model/crates/ai-comfyui/src/output.rs`, output file selection.
- `craft-ai-model/crates/ai-comfyui/src/sam3_session.rs`, prepare/preview/commit/ground graphs and stable cache ID.
- `photocraft-wt-ai/crates/ui-egui/src/sam3_multi_select.rs`, uncommitted comma-phrase orchestration.
- `photocraft-wt-ai/crates/engine/src/sam3_select_cmds.rs`, mask import, scaling, stale-result rejection and undo.

Installed ComfyUI source:

- `custom_nodes/comfyui-sam3/nodes/segmentation.py`.
- `custom_nodes/comfyui-sam3/nodes/sam3_interactive.py`.
- `custom_nodes/comfyui-sam3/nodes/utils.py`.
- Desktop resources `comfy_extras/nodes_sam3.py`, `nodes_preview_any.py` and `comfy/text_encoders/sam3_clip.py`.

### Licence boundary

The installed wrapper identifies its code as GPL-3.0. Meta's model/code materials use the separate SAM License. Do not describe both as MIT/Apache, or copy wrapper implementation into Rust without a licence review. Distribution decisions require checking both actual licences and the project's attribution policy.[4][13]

### Scope kept intact

No installed nodes, packages, models, application source, UI state or binaries were changed. No commits, pushes, hosted jobs or downloads of model weights. Probe uploads and generated ComfyUI output files were written locally. Existing dirty feature work was read, not overwritten. Research files and reusable procedure notes were saved separately.

## Sources

[1] https://github.com/PozzettiAndrea/ComfyUI-SAM3
[2] https://github.com/PozzettiAndrea/ComfyUI-SAM3/blob/main/nodes/segmentation.py
[3] https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_sam3.py
[4] https://github.com/facebookresearch/sam3/blob/main/LICENSE
[5] https://github.com/PozzettiAndrea/ComfyUI-SAM3/blob/main/nodes/sam3_video_nodes.py
[6] https://github.com/PozzettiAndrea/ComfyUI-SAM3/blob/main/nodes/sam3_interactive.py
[7] https://github.com/Comfy-Org/workflow_templates/blob/main/blueprints/image_segmentation_sam3.json
[8] https://github.com/facebookresearch/sam3/blob/main/README.md
[9] https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_preview_any.py
[10] https://api.github.com/repos/PozzettiAndrea/ComfyUI-SAM3/commits/main
[11] https://docs.comfy.org/built-in-nodes/SAM3_Detect
[12] https://github.com/Comfy-Org/ComfyUI/blob/master/comfy/text_encoders/sam3_clip.py
[13] https://github.com/PozzettiAndrea/ComfyUI-SAM3/blob/main/LICENSE
[14] https://github.com/wouterverweirder/comfyui_sam3
[15] https://github.com/1038lab/ComfyUI-RMBG/blob/main/update.md
[16] https://github.com/1038lab/ComfyUI-RMBG/blob/main/README.md

## Session ownership, revisions and cancellation (sg3, built)

**The hover cache.** `SAM3InteractiveCollector.execute` stores the prepared image in a module-level dict, `_INTERACTIVE_CACHE[str(unique_id)]`, where `unique_id` is the graph node id; `/sam3/interactive_segment_one` reads it back by the `node_id` in the request and checks nothing about which image it holds. There is no eviction. A fixed node id shared by every client therefore lets a second prepare silently replace the first image, and the first client's hover then returns an overlay of the wrong image. A fresh id per session isolates them but never frees server memory.

**Live control.** With both sessions forced onto one slot, preparing a mirrored image in session B changed session A's hover overlay (`verification/sg3-live-control-shared-slot.txt`). With one slot each, A's hover bytes were identical before and after B prepared, B's differed, and committing in either did not disturb the other (`verification/sg3-live-sessions.txt`).

**Design.** Four slots, graph node ids `craft_ai_sam3_0` to `3`. A slot is an open file with an exclusive operating-system lock in `<temp>/craft-ai-sam3`, one file per server address and slot. The lock is exclusive across processes, so several Craft apps sharing one ComfyUI do not collide, and the operating system releases it if the holder crashes (tested with a second process that is killed). A fifth prepared image is refused until a session is released or dropped. The server's cached tensors cannot be freed from outside, so the bound on slots is the bound on what a client leaves behind until ComfyUI restarts. Measured on the installed server (`measure_slot_footprint.py`, `verification/sg3-slot-footprint.json`): each entry holds about 144 MB of GPU memory and about 150 MB of process memory, so four slots cost at most about 575 MB of VRAM. Clients that still use the old shared `craft_ai_sam3` id take no slot and are not covered; PhotoCraft, the only one, uses this crate by path, so it moves to slots when it is next built, and its old entry stays as one more stale entry until restart.

**Revisions.** Every `prepare`, failed `prepare` and `release` starts a new `Revision`. Typed results carry the revision they were computed under, and `Sam3Session::is_current(revision)` says whether it is still the held image, so an app drops a result that arrives after the document changed.

**Cancellation.** A `Sam3Canceller` cancels the request a session has in flight when `cancel` is called. It deletes that prompt from the queue (`POST /queue` with `delete`), and with `with_targeted_interrupt(true)` also posts `/interrupt` with that `prompt_id`. The installed `server.py` interrupts only when the id matches the running prompt; an older server without that check would interrupt whatever is running, so the interrupt is opt-in. Live, a cancel returned in about 0.8 s while a concurrent ten-pass refinement request in another session finished with all three candidates.

> Evidence note: the person's Windows profile folder in logs, server stats and scripts is written as `%USERPROFILE%` (scripts use `Path.home()`). Nothing else in the evidence was changed.
