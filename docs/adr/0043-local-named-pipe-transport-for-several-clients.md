# 0043. Serve several clients over a local named pipe, through a safe wrapper crate and a synchronous loop

- **Status:** Proposed
- **Date:** 2026-11-24
- **Approval:** Pending. Draft written by an AI agent (Claude) on 2026-11-24 under the owner's
  delegation, from a spike and an experiment run on 2026-11-24, kept outside the repository.
  Acceptance is the owner's, and it must wait for the two supervised checks of Required
  validation. Nothing in it is implemented.

## Context

[ADR-0039](0039-several-agents-through-one-engine-with-an-input-lease.md) lets several agents reach
one engine through a broker inside the engine process, built and tested first on in-memory streams
(its decision 12). An MCP host starts a stdio server per agent, so agents in different hosts can
only reach one running engine through a local channel between processes. ADR-0039 asks for that
channel to be decided separately, and [ADR-0040](0040-task-scoped-grants-narrower-than-the-local-policy.md)
needs it to carry the ticket that selects a client's grant. This ADR decides it, for Windows only.

The repository constrains the choice as follows. Each fact was checked in this repository at the
commit of this ADR unless it says it comes from the spike, which names its upstream source.

- **No `unsafe` of our own outside two modules.** The workspace sets `unsafe_code = "forbid"`
  (`Cargo.toml`, lines 67–68). [ADR-0016](0016-bounded-unsafe-for-windows-capture-and-input.md)
  lifts it only in the `#[cfg(windows)] mod win` of the capture and input adapters (its decision,
  lines 33–39), whose manifests replace `forbid` by `deny` (`src/adapters/capture/Cargo.toml`
  line 44, `src/adapters/input/Cargo.toml` line 39). The MCP adapter and the app keep `forbid`.
- **The SEC-04 source check.** `sec_04_no_production_code_opens_a_socket_writes_a_file_or_starts_a_process`
  (`src/acceptance/tests/repository.rs`, lines 142–173) scans the product's sources, outside
  tests and `//` comments (lines 112–133), for seventeen substrings: `std::net`, `TcpStream`,
  `TcpListener`, `UdpSocket`, `tokio::net`, `reqwest`, `hyper::`, `File::create`, `fs::write`,
  `OpenOptions`, `create_dir`, `fs::copy`, `fs::rename`, `fs::remove`, `.save(`, `Command::new`
  and `dbg!` (lines 144–160). Reading a file (`fs::read`, `File::open`) is not among them.
- **The SEC-04 lockfile check.** `sec_04_no_crate_in_any_lockfile_speaks_the_network` (lines
  228–262) refuses twenty-one crate names anywhere in `Cargo.lock` and in the two lockfiles of
  `tools/s4`, among them `mio` and `socket2` (lines 243–244). `tokio`'s `net` feature, where its
  named pipes live, depends on both (from the spike: `tokio`'s `Cargo.toml`).
- **REL-01.** `Cargo.lock` must be committed and must not change when the workspace resolves
  offline (`rel_01_the_lockfiles_are_committed_and_current`, lines 386–406; `cargo metadata
  --locked --offline`, line 330). Every third-party licence expression must appear in
  `docs/thesis/S4_REPRODUCIBILIDAD.md` and none may be GPL without the library exception (lines
  581–614). The comparison is by substring (line 605 onwards), so `0BSD` already passes inside
  the listed `0BSD OR MIT OR Apache-2.0` (`S4_REPRODUCIBILIDAD.md`, line 50); `0BSD OR Apache-2.0`
  does not.
- **What the server needs from a stream.** `McpServer::serve` is `async` and generic over
  `tokio`'s `AsyncRead` and `AsyncWrite` (`src/adapters/mcp/src/server.rs`, lines 68–82); it reads
  in chunks of 8 KiB (line 16), writes each reply followed by `\n` and flushes it (lines 110–114),
  and purges the engine when the stream ends (lines 79–81), which ADR-0039 decision 10 replaces
  for the broker. The connection itself is synchronous: `Connection::handle_message` takes one
  line and returns the reply to write, or none (`protocol.rs`, line 112), and takes the engine's
  lock only inside the call.
- **The message bounds of [ADR-0015](0015-mcp-transport-on-rmcp-wire-types.md).** At most
  `max_message_bytes` per line in each direction, 64 KiB by default and never below 4 096
  (`src/adapters/mcp/src/config.rs`, lines 3–10 and 43–45). The existing line reader buffers at
  most the limit and reports a longer line as oversized after discarding it
  (`src/adapters/mcp/src/transport.rs`, lines 1–21); the oversized line is answered with
  `-32600` and the connection goes on (`protocol.rs`, lines 147–156). That reader is `async`
  (`transport.rs`, line 31).
- **CI.** The job `windows` builds and tests on `windows-latest` with the pinned toolchain
  (`.github/workflows/ci.yml`, lines 106–133); this machine has only the GNU host.
- **The input adapter already reads the engine's integrity level**, inside ADR-0016's module and
  only in builds with `sendinput` (`src/adapters/input/src/sendinput/win.rs`, lines 675–678).

A spike and an experiment run on 2026-11-24, kept outside the repository, looked for a transport
under these constraints. The spike compared five families from their sources; the experiment built
the recommended one in a scratch crate with `#![forbid(unsafe_code)]` on this project's toolchain
(Rust 1.99.0, GNU host, Windows 11, not elevated) and ran it. What this ADR takes from them:

- **From the spike, upstream sources named:** `interprocess` 2.4.9 (`0BSD OR Apache-2.0`) with its
  default features off pulls in no `tokio`, `mio` or `socket2` (its `Cargo.toml`); its listener
  refuses remote clients by default (`PIPE_REJECT_REMOTE_CLIENTS`) and creates its first instance
  with `FILE_FLAG_FIRST_PIPE_INSTANCE` (`listener/create_instance.rs`); it builds a security
  descriptor from SDDL in safe code (`security_descriptor/owned.rs`); its client opens the pipe
  without security-QoS flags (`named_pipe/c_wrappers.rs`).
- **From the experiment:** its lockfile had no banned crate, and adds three crates the
  repository's lockfile lacks (`interprocess`, `doctest-file`, a proc-macro, and `recvmsg`, both
  `0BSD`), while `widestring` 1.2.1 and `windows-sys` 0.61.2 are already in it at the same
  versions (checked: `Cargo.lock`, lines 3634–3635 and 3839–3840). A second listener on the name
  was refused with OS error 5, from the same process and from a second one. Clients through the
  SMB loopback were refused. A descriptor of `D:P(A;;GA;;;OW)` was read back as one protected
  allow entry for the owner, the same user connected, and a negative control (`SY` only) refused
  that user, so the kernel enforces it. 10 000 pipelined requests with replies up to 60 000 bytes
  completed with every reply matched; a 65 536-byte line was accepted and a 70 000-byte line
  refused without closing the connection; `flush()` after each reply never blocked while the
  client read. Eight clients were served at once while one dropped in mid-line and one never
  read; the one that never read blocked only its own server thread. With the pipe's instance
  limit set, a ninth client was not refused but connected and waited. No safe call of the crate
  gives the current user's SID, and the crate does not re-export `widestring`, which its SDDL
  call takes.
- **Not tested by the experiment:** a second local user and an elevated process against the
  descriptor (both need administrator rights), a remote machine, the MSVC host, the real
  `McpServer` over the pipe, and long runs.

## Decision

1. **The transport is a Windows named pipe through `interprocess`**, pinned to an exact version
   (`=2.4.9`, the version the experiment ran), with its default features off and only its
   synchronous API. Both sides of the pipe live in `adapter-mcp`, the adapter that owns the wire
   format; the app only wires them. No `unsafe` of our own is added: the crate's own `unsafe` is
   third-party code, as that of `windows-sys` is today.
2. **One blocking thread per connection, with a synchronous line reader.** Each accepted
   connection gets its own thread with a synchronous reader that enforces the bounds of ADR-0015
   exactly as the asynchronous one does: it buffers at most `message_limit()` bytes, discards a
   longer line up to its `\n`, answers it with the same `-32600` and keeps the connection open.
   Each complete line goes to `Connection::handle_message`, and its reply is written and flushed
   after the call has returned, so the engine's lock is never held while a reply is written: a
   client that does not read blocks only its own thread.
3. **Remote clients are refused** (`PIPE_REJECT_REMOTE_CLIENTS`, the crate's default, never turned
   off).
4. **One name per user session, created first-instance; this is how a second engine is refused.**
   The pipe's name is fixed per user session and the first instance is created with
   `FILE_FLAG_FIRST_PIPE_INSTANCE`. A second `serve` with the broker on, on the same desktop,
   cannot create it and exits with an error that says so. This is what ADR-0039 decision 1 needs
   and nothing else in the code provides.
5. **A descriptor that grants the owner only: `D:P(A;;GA;;;OW)`.** Protected, one allow entry for
   the object's owner, nothing for Everyone, Anonymous, SYSTEM or Administrators. Without it, the
   default pipe descriptor lets Everyone and Anonymous read (from the experiment). The user's own
   SID would be the more direct principal, but no safe call gives it: the crate has none, an SDDL
   string takes a SID or a two-letter alias, and obtaining the SID otherwise needs `unsafe` of our
   own or a subprocess (`Command::new`), both forbidden in this adapter.
6. **The engine never runs elevated for this.** The owner of an object created by an elevated
   process is, by default, the Administrators group, not the user, so `OW` would then grant the
   pipe to every administrator of the machine and not to the user alone. The operator starts the
   engine with the broker on from an unelevated session. How the engine refuses to start the
   broker when it is elevated is an open point.
7. **The bound on clients is enforced in the accept loop.** ADR-0039 allows eight clients at once.
   The pipe's own instance limit cannot enforce that: it makes a further client connect and wait,
   not fail (from the experiment). The crate's limit is left at its default, and the accept loop
   counts live connections: a connection beyond the bound is accepted, told in one line that the
   engine is full, and closed. The loop does no other work between two accepts, because the
   listener keeps one waiting instance and a client arriving while it is re-created sees the pipe
   as busy.
8. **`attach` only relays, and never starts an engine.** The executable gains an `attach` mode
   that connects to the pipe of the current user session and relays its standard input and output
   to it, so an MCP host keeps starting a stdio server. When no engine listens, `attach` fails with
   a clear error on stderr and starts nothing. The operator, or an agent host's launcher acting
   with the operator's authority, starts the one engine process with the broker on.
9. **The ticket handshake.** A ticket reaches `attach` as the path of a file (`--ticket-file`),
   never as an argument value or an environment variable: an argument is visible in the process
   list and the environment is inherited by children, while a file can be restricted, and removed
   by the operator after use. `attach` reads it (a read is not a call SEC-04 forbids) and sends it
   as the first line on the pipe, before any MCP message; with no ticket file the line is empty.
   The broker hashes the line with SHA-256 (`sha2`, already a workspace dependency, added to
   `adapter-mcp`) and compares it with the grants file of ADR-0040. A connection whose handshake
   names no listed, unspent ticket has no grant and falls under ADR-0040's fail-closed rule. The
   ticket is never written to a log, never echoed on the wire and never reaches the model.
10. **Off by default.** The pipe exists only with the broker of ADR-0039 switched on. With the
    switch off, `serve` serves stdio exactly as today, creates no pipe and links nothing new into
    the path it runs.
11. **Windows only.** No other platform gets a transport in this ADR.

### Open points

- **A name taken before the engine starts.** The first-instance flag protects only the server's
  own creation. If another process creates the name first, the engine refuses to start, which
  fails closed; but an `attach` would then connect to that process, send it the ticket and relay
  the agent's traffic to it. The crate's client sets no security-QoS flags, and the spike's remedy
  (opening the client handle with `OpenOptions` and `security_qos_flags`) trips the SEC-04 source
  check. How `attach` verifies that it reached the engine, and how the name is derived so that
  another user cannot predict and take it, is not decided. A process of the same user is outside
  what the grant defends against (ADR-0040); another user's is not.
- **How the engine refuses to start the broker when elevated.** The only integrity read in the
  code is the input adapter's, under ADR-0016 and only with `sendinput`. Reusing it through the
  composition root, or another way, is not decided; until then decision 6 is the operator's rule.

## Alternatives considered

From the spike, each with its reason:

- **`std::fs::OpenOptions` on the client side:** it trips the SEC-04 source check; it does not wait
  on a busy pipe; and a handle opened without overlapped I/O cannot read and write at the same
  time, so a two-thread relay over it deadlocks.
- **`winpipe`:** refuses remote clients, but passes no security descriptor (the default, readable
  by Everyone) and does not create the first instance exclusively.
- **`named_pipe`:** creates the first instance exclusively, but does not refuse remote clients,
  passes no security descriptor and has been unmaintained since 2019.
- **`tokio::net` and `ipc-channel`:** both bring `mio`, which the lockfile check refuses, and
  `tokio::net` also trips the source check.
- **Our own `unsafe` over the `windows` crate, under a third ADR-0016 exception:** it adds no crate
  and could build a descriptor for the user's exact SID, at the cost of some fifteen to twenty
  `unsafe` blocks to review. It is the fallback if the required validation below refutes the
  owner-only descriptor or the flushing under load, and it would need its own ADR.
- **Inherited stdio handles or anonymous pipes:** they work only for children of one parent; an
  MCP host starts its own server per agent and cannot hand a handle to a running engine.
- **`AF_UNIX` sockets, mailslots, shared memory, ALPC:** an `AF_UNIX` socket is a socket and links
  WinSock; mailslots are one-way datagrams; shared memory needs framing in `unsafe` of our own;
  ALPC is undocumented.
- **A TCP port on the loopback interface:** a network socket, which SEC-04 forbids, reachable by
  any local user.

## Consequences and cost

- **Dependencies.** Three crates enter `Cargo.lock` (`interprocess`, `doctest-file`, `recvmsg`),
  with two licence expressions, `0BSD` and `0BSD OR Apache-2.0`, that the implementing change adds
  to the table of `docs/thesis/S4_REPRODUCIBILIDAD.md` with a CHANGELOG entry. `widestring` becomes
  a direct dependency of `adapter-mcp` (same version, licence already listed), because the SDDL
  call takes its type and the crate does not re-export it. `doctest-file` is a proc-macro crate,
  the kind of artefact Smart App Control has refused on the development machine.
- **Speed.** Flushing after every reply, as `serve` does today, cost up to 1.9 times the time of
  the same run without it on large replies, and never blocked while the client read.
- **A new local attack surface:** a pipe any process of the user can open. What bounds it: remote
  clients refused; one owner-only descriptor; one engine per name; a bound on clients and on every
  line; no authority without a ticket, which travels in a file and never through the model; the
  grant's meet with the operator's policy (ADR-0040); and the switch that is off by default. What
  does not: a process that takes the name first (open point).
- **Two serving loops.** The pipe uses a synchronous reader next to the asynchronous one of stdio;
  both must keep the same bounds, and the tests compare them.

## Required validation

New planned tests, with identifiers to be assigned in the matrix when the ADR is accepted.

- On in-memory streams: the synchronous line reader against the bounds of ADR-0015 (a line at the
  limit accepted, one byte over it answered with `-32600`, the connection still up), and the same
  frames as the asynchronous reader on generated input; the handshake (a valid ticket selects its
  grant, an empty, unknown or spent one gives none, the ticket never appears in a reply or a log).
- On Windows, a loopback pipe test with the real `McpServer` and the simulated desktop: a session
  over the pipe answers as the same session over stdio.
- A second listener on the same name is refused, and the second `serve` exits with its error.
- A client beyond the bound is told so and closed; the others are not disturbed.
- An oversized line is refused without ending the connection.
- A client that disconnects in mid-message, or never reads, does not disturb the others.
- `attach` with no engine listening fails and starts no process.
- The repository's SEC-04 source and lockfile checks and the REL-01 checks pass with the new crates.
- The `windows` job of CI (MSVC host) builds and tests the pipe.
- **Supervised by the owner, because it needs administrator rights:** a second local user cannot
  open the pipe, and the descriptor created by an elevated process is checked for what `OW` then
  grants. The experiment tested neither. Until both have run, the claim that the pipe is restricted
  to the current user rests on one read-back descriptor and a negative control, and this ADR must
  not be accepted on it.

## Reversibility

The transport exists only with the broker switched on, which is off by default; with it off the
executable serves stdio as today. Removing it removes a module of `adapter-mcp`, the `attach` mode
and three crates from the lockfile. A failure of the supervised checks, or of flushing under load,
moves the transport to the fallback above under its own ADR; a defect in the descriptor or in the
first-instance refusal withdraws it until fixed.

## Provenance

Decided on 2026-11-24 under the owner's delegation, after a read-only spike and an experiment in a
scratch crate, both run on 2026-11-24 and kept outside the repository. Serves
[ADR-0039](0039-several-agents-through-one-engine-with-an-input-lease.md) (decisions 1 and 12) and
[ADR-0040](0040-task-scoped-grants-narrower-than-the-local-policy.md) (decision 3); keeps the
bounds of [ADR-0015](0015-mcp-transport-on-rmcp-wire-types.md) and the scope of
[ADR-0016](0016-bounded-unsafe-for-windows-capture-and-input.md); meets the SEC-04 and REL-01
checks of `src/acceptance/tests/repository.rs` as they stand.

## Resumen (ES)

El transporte local de varios clientes es un tubo con nombre de Windows a través de la crate
`interprocess`, fijada a una versión exacta, sin funciones por defecto y con su API síncrona: un
hilo por conexión, un lector de líneas que respeta los límites de ADR-0015 y nada de `unsafe`
propio. Se rechazan los clientes remotos; el nombre es fijo por sesión de usuario y se crea como
primera instancia, lo que impide un segundo motor; el descriptor `D:P(A;;GA;;;OW)` sólo da acceso
al propietario, y el motor no se ejecuta elevado. El límite de clientes se aplica al aceptar.
`attach` sólo retransmite y nunca arranca un motor; el tique llega en un archivo y viaja en la
primera línea. Cuesta tres crates y dos licencias. Quedan abiertos un nombre ocupado antes de
arrancar y cómo se niega el motor a arrancar elevado. No se acepta sin probar antes, con el
responsable, un segundo usuario y un proceso elevado.
