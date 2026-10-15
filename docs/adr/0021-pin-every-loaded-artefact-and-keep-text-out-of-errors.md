# 0021. Pin every artefact the engine loads before loading anything, and keep screen and agent text out of errors

- **Status:** Proposed
- **Date:** 2026-10-14
- **Approval:** Pending (the project owner)

## Context

Chapter 08 §8.2 asks for "hash y versión exactos de modelos, librerías y caché", and §8.2
(privacy) for logs with local IDs, codes and times only. SEC-06 and SEC-04 test both
(`matrices/plan_pruebas.csv`). Reading the code against them (CHANGELOG 1.0.35) found three
gaps:

1. The detector's files were pinned (manifest, ONNX Runtime library, DirectML library, weights
   by the manifest), but the weights were hashed *after* the runtime library had been loaded
   into the process, and the order could only be tested with the real library.
2. The curated-icon prototypes (`[cache] prototypes`, Algorithm 3A) were not pinned at all:
   a line added to the file after the operator reviewed it would have been loaded and used to
   label icons. That is the cache poisoning of SEC-06, and chapter 08 names the cache among
   the artefacts to hash.
3. Two protocol errors repeated what the sender wrote: `Unknown tool` carried the tool name
   and `Method not found` the method, up to the 64 KiB message limit. The agent's text may be
   text it read on a screen (SEC-03), and the module that parses arguments already commits to
   "nothing the agent sent is ever echoed".

## Decision

1. **Verify, then load.** `ommatix_adapter_vision::model::artifacts::verify` reads every file
   of the detector once, bounded, and checks it: the manifest against the operator's pin, the
   manifest's detector entry (providers included), `DirectML.dll` and the ONNX Runtime library
   against their pins, and the weights against the digest in the manifest. It returns the
   verified bytes and absolute paths and loads nothing. Only then does the detector load a
   library or build a session. The module is always compiled, so the whole gate is tested with
   temporary files and no model.
2. **The curated-icon file is pinned like the manifest.** `[cache] prototypes_sha256` (64
   hexadecimal characters) is required whenever the resolver is configured, with no default;
   the file is read once, bounded, hashed and parsed from those bytes, and any mismatch stops
   the server before it serves. Nothing at run time adds a prototype.
3. **Errors never echo free text.** `Unknown tool` lists the tools that exist instead of the
   name it was given; `Method not found` repeats the method only when it is shaped like a
   method name (1–64 characters of letters, digits, `_`, `.`, `/`). The protocol-version error
   keeps `requested`, which the MCP specification requires.

## Alternatives considered

- **Leave the order as it was.** The library is hashed before it is loaded; only the weights
  came late. Rejected: weights that fail their digest after a library has already run is the
  wrong order for a gate, and it made the weights check untestable without the library.
- **Pin the prototypes optionally.** An unpinned cache is the gap; an optional pin is a gap
  that a configuration silently keeps. The resolver is not deployed anywhere yet, so a
  required key costs nothing now.
- **Sign the artefacts.** A signature moves trust to a key nobody has set up. The pin in the
  operator's own configuration is the trust root, like the rest of `[models]`. S0 checked the
  Authenticode signatures of the Microsoft libraries by hand.
- **Strip the tool name from every error.** Errors of tools that exist name them, which helps
  and echoes nothing the agent invented.

## Consequences and cost

- A configuration with the icon resolver needs a sixth key; the operator computes the digest of
  the file once. Editing the file means pinning it again, on purpose.
- The weights stay in memory while the libraries load, as before; the check just runs first.
- An agent that sent a misspelt tool name no longer sees it back, only the tools there are.

## Required validation

SEC-06 (`sec_06_*` in `ommatix-adapter-vision`, `ommatix` and its process test), IT-06 and
SEC-04 (`it_06_rejection_reasons_are_codes_and_fixed_sentences`), REL-01.

## Reversibility

Both pins and the errors are local to their modules. The order of verify-then-load could only
be undone by loading before hashing, which the tests forbid.

## Provenance

Refines ADR-0009 (models and licences), ADR-0004 (the cache) and ADR-0012 (data
minimisation); implements chapter 08 §8.2 for the cache.

## Resumen (ES)

- **Contexto:** el capítulo 08 pide fijar por hash modelos, bibliotecas y caché y no registrar
  texto de pantalla. El código fijaba el detector pero comprobaba los pesos después de cargar la
  biblioteca, no fijaba el archivo de íconos curados (envenenamiento del caché) y repetía en dos
  errores el nombre de herramienta o de método que mandó el agente.
- **Decisión:** `verify` comprueba todos los archivos del detector antes de cargar nada;
  `[cache] prototypes_sha256` es obligatorio con el resolvedor de íconos y el archivo se lee,
  se hashea y se interpreta desde los mismos bytes; los errores no repiten texto libre
  (`Unknown tool` lista las herramientas; `Method not found` repite sólo nombres con forma de
  método).
- **Costo:** una clave más para el operador, que calcula el digest una vez.
- **Estado:** propuesta, a la espera del responsable.
