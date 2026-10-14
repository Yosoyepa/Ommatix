# 0016. Bounded unsafe for the Windows capture and input adapters

- **Status:** Accepted (2026-10-14)
- **Date:** 2026-10-14
- **Approval:** Project owner, 2026-10-14

## Context

The workspace forbids `unsafe` (`unsafe_code = "forbid"`, AGENTS.md rule 3) and says the
Windows adapters of S3 will need an ADR to lift it. S3 replaces the interim `xcap` capture and
`enigo` input with the platform the dossier names for Windows 11 (ADR-0008): DXGI Desktop
Duplication for `FrameCapturer` [S06] and `SendInput` for `OsActuator` [S07], with a declared
DPI policy and tested conversions to the input API's normalised coordinates [S08] (SDD
[§2.4 and §2.8](../thesis/02_SDD_ARQUITECTURA_MVP.md)). S3 also needs evidence the interim
adapters cannot give: the foreground process (so that `[safety] allowed_applications` is
enforced, chapter 08 §8.2) and its integrity level (UIPI drops input to an elevated window
silently while `SendInput` still counts it as inserted).

In the `windows` crate (pinned at `=0.62.2`) every COM method of DXGI and Direct3D 11 and
every Win32 function used here (`SendInput`, `GetForegroundWindow`, `OpenProcess`,
`QueryFullProcessImageNameW`, `OpenProcessToken`, `GetTokenInformation`,
`GetAsyncKeyState`, ...) is an `unsafe fn`. Reading a mapped texture means building a slice
from the pointer `Map` returns. There is no safe wrapper of comparable maturity that covers
Desktop Duplication, `SendInput` and token queries together.

A `forbid` cannot be relaxed by an inner `allow`, so the crates that need `unsafe` cannot keep
`[lints] workspace = true`.

## Decision

Allow `unsafe` only where the Win32/COM boundary is, and make every use reviewable:

- **Scope.** Only the `#[cfg(windows)] mod win` of `ommatix-adapter-capture` (feature `dxgi`,
  `src/dxgi/win.rs`) and of `ommatix-adapter-input` (feature `sendinput`,
  `src/sendinput/win.rs`). If `ommatix-adapter-vision` needs WinRT OCR
  (`Windows.Media.Ocr`) through the same bindings, it follows this ADR in the same way, in its
  own `#[cfg(windows)] mod win`, and lists its file here when it lands. Nothing else: the
  domain, the application, the MCP adapter, the app and every non-Windows module keep
  `forbid`.
- **Lints.** Those crates replace `[lints] workspace = true` with a crate-level table that
  mirrors the workspace lints except `unsafe_code = "deny"`, and adds
  `clippy::undocumented_unsafe_blocks = "deny"`. The `win` module alone carries
  `#[allow(unsafe_code, reason = "ADR-0016: ...")]`. Any other `unsafe` fails the build.
- **One call per block.** Each `unsafe` block wraps one binding (or the immediate read of
  what it just returned) and has a `// SAFETY:` comment saying why the call is sound: which
  pointers it receives and how long they live, which handle or interface it relies on.
- **Raw pointers never leave the module.** The public surface is safe Rust:
  `DxgiCapturer` (a `FrameCapturer`) and `SendInputActuator` (an `OsActuator`). Handles are
  owned by drop guards (`OwnedHandle`, `HeldFrame`, `MappedTexture`) so every `CloseHandle`,
  `ReleaseFrame` and `Unmap` runs exactly once.
- **Pure logic outside.** Coordinate normalisation, batch building, row packing, the geometry
  epoch, executable-name extraction and the integrity and modifier checks are safe Rust in the
  parent module, unit-tested on every OS.
- **No `unsafe impl`.** The COM interfaces used are already `Send` in `windows` 0.62.
- **The binding is pinned** (`windows = "=0.62.2"`); an upgrade re-reads every `SAFETY:`
  comment against the new signatures.

At the time of writing the surface is 22 unsafe blocks in `capture/src/dxgi/win.rs` and 20 in
`input/src/sendinput/win.rs`, each with its `SAFETY:` comment.

## Alternatives considered

- **Keep `xcap` and `enigo`.** They hide their own `unsafe`, cannot attest the foreground
  application or its integrity level, give no access to DXGI's dirty/move rectangles or to
  `SendInput`'s inserted count, and on Windows leave the DPI policy implicit (ADR-0008 and
  SDD §2.8 ask for it to be declared and tested).
- **`windows-sys` or hand-written FFI.** The same `unsafe` with fewer types: COM reference
  counting by hand instead of `windows`' smart interfaces. More unsafe, not less.
- **A separate crate for the FFI.** One more crate with no second implementation in sight
  (AGENTS.md §3); the `cfg(windows)` module gives the same boundary.
- **A helper process.** Moves the problem rather than solving it, and adds IPC on the hot
  path (ADR-0010).
- **`unsafe_code = "allow"` crate-wide.** Loses the compiler's guarantee that `unsafe` stays
  in the reviewed module.

## Consequences and cost

Memory safety in those two modules rests on review, not on the compiler. The cost is bounded
by keeping them small, one binding per block, and documented. Reviewers of a PR that touches
a `win` module check every changed `SAFETY:` comment against the binding's documentation.
The crate-level lint tables must be kept in sync with `[workspace.lints]` by hand.

What becomes possible: DXGI capture in physical pixels with an honest "no change" path,
`SendInput` in one batch per action with a real inserted count, and the foreground-process and
integrity checks that make `allowed_applications` and UIPI enforceable.

## Required validation

- `cargo clippy` with `-D warnings` on Windows with the features `dxgi,sendinput` (CI job on
  `windows-latest`): `undocumented_unsafe_blocks` is denied.
- The unit tests of the pure helpers (normalisation round-trips every pixel of the reference
  virtual desktop, including its negative origin; batches; row packing; epochs).
- The manual test `dxgi_captures_one_frame_of_output_0` on the reference machine.
- Before the decision is trusted: IT/SEC evidence of S3 (thesis chapter 06 §6.5): capture
  loss, denied injection, elevated foreground, stuck modifiers, on the reference hardware.

## Reversibility

Easy to tighten, hard to need to: if a safe, maintained wrapper covers Desktop Duplication,
`SendInput` and token queries, replace the `win` modules with it and restore
`[lints] workspace = true` (and `forbid`) in both crates; the public types do not change. A
memory-safety defect traced to one of these modules is a reason to review all their blocks
before the next release.

## Provenance

S3 of the thesis roadmap (chapter 06 §6.5). Applies SDD §2.4 [S06–S07] and §2.8 [S08] and
chapter 08 §8.2; refines AGENTS.md rule 3 and [ADR-0008](0008-one-real-platform-first.md).
Keeps [ADR-0007](0007-fail-closed-actuation-and-honest-simulation.md) (every refusal is an
error before any event is sent) and [ADR-0012](0012-data-minimisation-and-evidence-traceability.md)
(frames stay in memory; nothing is saved or logged).

## Resumen (ES)

Se permite `unsafe` sólo en los módulos `#[cfg(windows)] mod win` de los adaptadores de
captura (DXGI Desktop Duplication) y de entrada (`SendInput`), y en el de visión si llega a
usar OCR de WinRT, porque en `windows` 0.62 los métodos COM y las funciones Win32 son
`unsafe fn`. Cada bloque envuelve una llamada y lleva un comentario `// SAFETY:` exigido por
`clippy::undocumented_unsafe_blocks = "deny"`; los punteros crudos no salen del módulo y la
lógica pura queda en Rust seguro con pruebas. Se revierte si aparece un envoltorio seguro y
mantenido que cubra esas APIs.
