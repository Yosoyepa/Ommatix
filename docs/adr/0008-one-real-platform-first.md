# 0008. One real platform before promising portability

- **Status:** Accepted
- **Date:** 2026-10-13 (proposed), 2026-10-14 (accepted)
- **Approval:** Project owner, 2026-10-14

## Context

The source conversation lists Windows, macOS and Linux, but delivers no real adapter for any
of them [F0, pp. 3–5, 43–45].

## Decision

The first MVP targets Windows 11 x64, with a selected monitor and defined DPI scales. The
ports stay portable, and what is out of scope is stated explicitly.

The approved scope, from [thesis chapter 02 §2.1](../thesis/02_SDD_ARQUITECTURA_MVP.md):
the local interactive desktop of Windows 11 x64, one selected monitor, SDR/BGRA, test
applications without elevated privileges, a base resolution of 1920×1080 and validation at
100 %, 125 % and 150 % scaling. Capture uses DXGI Desktop Duplication and input uses
`SendInput`. The choice of test applications is still pending approval.

## Alternatives considered

- Several platforms at once.
- Offline replay only.
- The browser only.

## Consequences and cost

Resources are concentrated and a real end-to-end test becomes possible. macOS, Linux, HDR, UAC
and multi-monitor setups are not covered.

In this repository, the capture and input adapters written during the bootstrap on
cross-platform libraries (macOS, Linux X11, and Windows at 100 %) are retired when the
Windows adapters land in S3. Those adapters call Win32 APIs that need `unsafe`, so the
exception to the workspace lint `unsafe_code = "forbid"` gets its own ADR at that point.

## Required validation

IT-01..IT-07 and a corpus of three controlled applications.

## Reversibility

The initial platform can change in S0 through an explicit decision, before the dependencies
are pinned.

## Provenance

S06–S08. New scoping [P], approved by the project owner on 2026-10-14.

## Resumen (ES)

Texto original del dossier (13 de octubre de 2026), seguido de la aprobación.

- **Contexto:** El PDF enumera Windows/macOS/Linux, pero no entrega adaptadores reales
  [F0, pp. 3–5, 43–45].
- **Decisión:** Primer MVP Windows 11 x64, monitor seleccionado y DPI definidos. Mantener
  puertos portables y especificar fuera de alcance.
- **Alternativas consideradas:** Multiplataforma simultánea; sólo replay offline; sólo
  navegador.
- **Consecuencias y coste:** Concentra recursos y permite E2E real. No acredita macOS, Linux,
  HDR, UAC ni multimonitor. Supuesto pendiente de aprobación.
- **Validación exigida:** IT-01..IT-07 y corpus de tres aplicaciones controladas.
- **Reversibilidad:** Cambiar plataforma inicial en S0 mediante decisión explícita antes de
  fijar dependencias.
- **Procedencia:** S06–S08; nueva delimitación [P].

**Aprobación (14 de octubre de 2026):** el responsable del proyecto aprobó Windows 11 x64 como
primera plataforma, con un monitor y DPI de 100, 125 y 150 %. Los adaptadores multiplataforma
del arranque (macOS, Linux X11 y Windows al 100 %) se retiran cuando entren los de Windows en
S3. Las aplicaciones de prueba siguen pendientes de aprobación.
