# Review: the formal-model tests of PR #114 (independent QA)

Reviewed on 2026-11-24 at `5c3e363`, the head of PR #114 (`chore/formal-model-tests-and-proposals`),
by an independent QA run on Gemini 4 Neon. The four commits of the PR are authored by Claude, so the QA vendor (Gemini) differs from the
author's. The verdict is `evidencias/2026-11-24/qa/pr114-gemini-verdict.json` in the control folder
(outside this repository), and its spec is `qa/pr114.md` there.

The verdict passed eight of nine criteria. It failed one, C3: the generated closure test asserted
only one of its two outcomes. The finding and its resolution are below. The fix adds no production
code and changes only the test.

## Findings

| # | P | Finding | Resolution | Regression test |
|---|---|---|---|---|
| R114-1 | P3 | `generated_closures_are_the_least_closed_region_whatever_the_order` (`src/domain/src/incremental.rs`) ended with `assert!(grew > 900)` only. It proved that the generator reaches closures that grow, and not that it also reaches closures that stay as they started. The other two generated tests of the PR assert both of their outcomes. | The test now counts the closures that stay as they started (`closed == region`) and asserts both counts in one assertion, with a message that gives both: `grew > 900 && stayed > 900`. Nothing else in the test or the file changed. | The same test, `incremental::tests::generated_closures_are_the_least_closed_region_whatever_the_order`. On the seed in the test the counts are 952 grew and 1,048 stayed, so it passes. It fails when fewer than 901 cases stay as they started. Checked by a scratch mutation that raised the bound for the staying cases to 1,100: the test failed with `the generator must exercise both kinds of closure: 952 grew and 1048 stayed as they started (more than 900 of each expected)`. The mutation was reverted. |

## Evidence

Run in this worktree on 2026-11-24, on the pinned toolchain `1.99.0`
(`rustc 1.99.0`, `CARGO_TARGET_DIR=C:\Users\User\Ommatix\target`). The cargo commands went through
`cargo-wt.ps1`.

Run for this review:

- `cargo fmt --all -- --check`: exit 0, no diff.
- `clippy --workspace --all-targets -v -- -D warnings`: exit 0, no warnings.
- `test -p ommatix-domain`: 136 passed (unit), 3 passed and 1 ignored (`candidate_index`), 0 failed.
  The first run was refused by Windows Smart App Control (`os error 4551`) on the test binary
  `ommatix_domain-d3b6b74d16a942b7.exe`. After `cargo clean -p ommatix-domain` through the wrapper,
  the rebuilt binary ran and passed.
- `test -p ommatix-acceptance --test repository`: 21 passed, 0 failed.
- `test --workspace --no-fail-fast`:
  - `ommatix-application`: 246 passed, 2 ignored.
  - Workspace total: 695 passed, 4 ignored.
  - `serve_process`: 5 tests did not start, because Windows Smart App Control refused `ommatix.exe`
    (`os error 4551`). They are not counted as passed. Together with the 695 passed, that makes the
    700 non-ignored tests of the PR's record, counted on 1.97.0 there.
  - Gherkin (`ommatix-acceptance --test thesis`): 62 scenarios passed, 209 steps passed.
- Mutation: the bound for the staying cases raised to 1,100 in a scratch edit; the test failed with
  the message above; reverted.

Taken from the verdict file, not re-run here:

- The verdict's own runs of the five named tests (C2: four in `ommatix-domain`, one in
  `ommatix-application`) and of the `repository` suite (C7) agree with the counts above.
- The form of the four ADR drafts (C8) and the statements the record of 2026-11-22 makes about the
  code (C9) were checked by the QA run and were not re-run for this commit. The Lean model that
  record mentions (12 modules, 186 theorems) and `MultiAgente.lean` are outside the repository, and
  neither QA run read or verified them, so nothing in this review vouches for the model.

Not run: the MSVC and Windows-feature jobs (no MSVC Build Tools on that machine, per `AGENTS.md`).
Not repeated here: the four mutation checks that the 2026-11-22 record describes. This review ran only the one mutation of R114-1 above.
