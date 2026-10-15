# Serve process harness: a closed pipe failed a passing test (2026-10-15)

The CI job `fmt · clippy · test` failed once on PR #32 (a documentation-only change) in
`sec_06_a_tampered_prototypes_file_stops_serve_before_it_serves` (`src/app/tests/serve_process.rs`,
added in PR #25). The PR was merged before the failure was noticed; `main` carried the race.

## Finding and resolution

| # | Severity | Finding | Resolution | Regression test |
|---|---|---|---|---|
| 1 | Minor (test harness, CI) | `Sandbox::run` wrote the request to stdin with `expect`. When `serve` refuses to start (here, a tampered curated-icon file) it exits before reading stdin; if it exits before the write, the write fails with a broken pipe and the harness panics, although the binary behaved as the test expects. Timing-dependent: it passed locally and on most CI runs | `Sandbox::run` treats a broken pipe while writing as the binary's answer and still panics on any other write error; the exit status and outputs are asserted as before | `a_refusal_before_reading_stdin_is_reported_as_the_binarys_exit`: `serve` with a missing configuration and a 4 MiB request (larger than any pipe buffer), which makes the closed pipe deterministic; it fails without the change (checked) |

No product code changed.
