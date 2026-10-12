# Phase 2d handoff (12/10/2026)

You are continuing Phase 2d of the Craft suite's shared AI layer for Jeremy. Load the skills craft-suite-shared-work, test-driven-development and systematic-debugging first.

Repo: %USERPROFILE%/Apps/ArtCraft/craft-ai-model, branch feat/sam3-object-selection, HEAD 6f98c64 at session start. The working tree also holds earlier uncommitted model-pick work in ai-client, ai-features, ai-model and ai-openai. That is not this work. Run git status before any commit and stage explicit paths only.

## Status (12/10/2026, after working the to-do list)

All six items are done; the record is in `ai-framework-handoff/NEXT.md`. u4 is 33 of 33 mutants
caught (the three survivors were triaged: one bad mutant fixed, one equivalent mutant sharpened,
one real test gap closed). The u4 tests were split into `nodepack.rs`, `nodepack_archive.rs` and
`nodepack_rig/`. Nothing is committed. Remaining: build u2 if wanted, u5 launcher screens, and the
commit if Jeremy asks.

## Rules that still apply

- Commit and push only when Jeremy asks, every time.
- No real download, node-pack install, hosted call or change to his ComfyUI. Scripted localhost servers only.
- Cargo: targeted with -p, one process at a time, -j 8. No workspace-wide builds (they have crashed his PC).
- Files under 500 lines. No em dashes. UK English, dates dd/mm/yyyy.
- Anything a script saves goes through docs/research/sam3-headless/scrub.py.
- Every mutation survivor is a test gap (fix the test) or a bad mutant (fix the mutant). Never ignore one.

## State

| Task | State |
|---|---|
| u1 custom connections | Built test-first. Mutation run: 40 of 40 caught. |
| u3 model-source registry and installer | Built test-first. 64 mutants: 63 caught, 1 equivalent (`directory-taken-as-local-file`, recorded in `EQUIVALENT` in mutation_check.py with the reason; not proven on Linux or macOS), 0 survivors. |
| u4 node packs from GitHub (`ai-comfy-env/src/source/pack/`, tests in `tests/nodepack.rs`) | Code and tests written, ai-comfy-env suite passed. 33 mutants: 30 caught, 3 survivors not yet triaged. |
| u2 declarative adapter | Design only: docs/design/u2-declarative-adapter.md. |
| u5 launcher screens | Out of scope. |

Tools: `python docs/research/phase2d/mutation_check.py u1|u3|u4 [name-substring]`, and `--dry` to check each mutant has exactly one target. Filtered reruns merge into docs/research/phase2d/verification/<group>-mutations.json.

## To do, in order

1. Triage the three u4 survivors:
   - `pack-archive-unpinned`: archive address uses the branch name instead of the commit. Likely a test gap: the test server answers any archive path. Make it answer only `/owner/Pack/archive/<COMMIT>.tar.gz` and fail otherwise.
   - `archive-dot-dot-accepted`: `..` check removed but the archive is still refused, so another check catches it first. Decide whether the mutant is equivalent or the test case is too weak.
   - `archive-case-duplicates-accepted`: case-insensitive duplicate check removed. Check the test archive really produces a collision (two names differing only in case in the same folder).
2. Check whether the symlink test `a_folder_that_is_a_link_is_not_followed_when_removing` ran or printed SKIPPED (Windows may refuse symlink creation). If it skipped, say link handling is unverified.
3. Run clippy `--all-targets -- -D warnings` for craft-ai-comfy-env after the u4 code, then `cargo fmt --all -- --check`, then `cargo check -p craft-ai-model --target wasm32-unknown-unknown` if craft-ai-model was touched (it was, in u1; the last output was not seen).
4. Run the saved logs in docs/research/phase2d/verification/ through scrub.py. Grep the new docs and source for em and en dashes.
5. Update the rows for u1, u2, u3 and u4 in ai-framework-handoff/handoff_tasks.py, the top section of NEXT.md and the craft-ai-model README roadmap. Then run `python build.py` in ai-framework-handoff. Claim only what was run, and list what was not run.
6. Report to Jeremy. Offer the commit only if he asks.

## Decisions to report to Jeremy

- Checksums (u3): size and SHA-256 come from the host when a source is added. Where the host publishes none (GitHub archives, Civitai sizes), the checksum is recorded as observed, not verified, and only with consent.
- t8 (remembered licences): u3 carries a minimal record per source kind, pinned version, and licence name and link. No general ledger.
- u1 models: a custom connection never guesses abilities. Models come only from what the person declares. The Test button runs a keyless control request, so a key counts as accepted only if the server refused the same request without it.
- Node packs: install needs consent naming the exact commit, nothing is run, and requirements.txt or install.py are only reported as present. An existing folder is never replaced, and only packs this app installed can be removed.
- Security rules built in: tokens go only to their own host and never follow a redirect off it; hosts are matched exactly or by dot-prefixed subdomain; pickle formats need explicit consent; a file's first bytes must match its extension; archives refuse links, device files, `..`, absolute paths, Windows device names, case duplicates and over-limit sizes.

## Key files

- u1: ai-model/src/{connection.rs,draft/custom.rs}, ai-openai/src/{probe.rs,catalog.rs,http.rs}, ai-client/src/manage/probe.rs. Tests: ai-model/tests/custom_connection.rs, ai-openai/tests/custom.rs, ai-client/tests/custom.rs.
- u3: ai-comfy-env/src/source/**, download.rs and verify.rs changes. Tests: source_basics, download_sources, source_resolve, source_install.
- u4: ai-comfy-env/src/source/pack/{mod,archive,resolve,install}.rs, tests/nodepack.rs.
- Scratch patch scripts: the Hermes scratch folder (p2d_*.py) (pruned after 24h, not needed to continue).
