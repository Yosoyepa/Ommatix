# 0043. Serve several clients over a local named pipe, through a safe wrapper crate and a synchronous loop

- **Status:** Proposed
- **Date:** 2026-11-24
- **Approval:** Pending. Draft written by an AI agent (Claude) on 2026-11-24 under the owner's
  delegation, from a spike and two experiments run on 2026-11-24, kept outside the repository.
  Acceptance is the owner's, and it must wait for the checks of Required validation that the owner
  supervises. It is implemented on local branches, behind a cargo feature and a configuration
  switch that are both off by default (see "What exists").

## Context

[ADR-0039](0039-several-agents-through-one-engine-with-an-input-lease.md) lets several agents reach
one engine through a broker inside the engine process, built and tested first on in-memory streams
(its decision 12). That broker exists on local branches (`Broker` and `BrokerConnection` in
`src/adapters/mcp/src/broker.rs`); what it lacked to serve agents in different hosts was this
channel, the `attach` mode and the switch in the executable, which now exist on local branches
stacked on it, off by default (see "What exists"). An MCP host starts a stdio server per agent, so agents in
different hosts can only reach one running engine through a local channel between processes.
ADR-0039 asks for that channel to be decided separately, and
[ADR-0040](0040-task-scoped-grants-narrower-than-the-local-policy.md) needs it to carry the ticket
that selects a client's grant. This ADR decides it, for Windows only.

The repository constrains the choice as follows. Each fact was checked in this repository at the
commit of this ADR unless it says it comes from the spike, which names its upstream source, or
from an experiment.

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
  and `dbg!` (lines 144–160). Reading a file (`fs::read`, `File::open`) is not among them. The
  check has no allowlist: any occurrence in any production file fails it (lines 162–172).
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
  for the broker: the broker's `BrokerConnection::serve`, on its branch, runs the same loop over
  the same streams and purges the engine only when the last client leaves. The connection
  itself is synchronous: `Connection::handle_message` takes one line and returns the reply to
  write, or none (`protocol.rs`, line 112), and takes the engine's lock only inside the call.
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

A spike and two experiments run on 2026-11-24, kept outside the repository, looked for a
transport under these constraints. The spike compared five families from their sources; the
first experiment built the recommended one in a scratch crate with `#![forbid(unsafe_code)]` on
this project's toolchain (Rust 1.99.0, GNU host, Windows 11, not elevated) and ran it; the second
ran the client of decision 12 in the same crate. What this ADR takes from them:

- **From the spike, upstream sources named:** `interprocess` 2.4.9 (`0BSD OR Apache-2.0`) with its
  default features off pulls in no `tokio`, `mio` or `socket2` (its `Cargo.toml`); its listener
  refuses remote clients by default (`PIPE_REJECT_REMOTE_CLIENTS`) and creates its first instance
  with `FILE_FLAG_FIRST_PIPE_INSTANCE` (`listener/create_instance.rs`); it builds a security
  descriptor from SDDL in safe code (`security_descriptor/owned.rs`); its client opens the pipe
  with `FILE_FLAG_OVERLAPPED` alone, without `SECURITY_SQOS_PRESENT`
  (`named_pipe/c_wrappers.rs`, `connect_without_waiting`; read again in the crate's published
  source for this revision); a stream can be built safely from a handle opened elsewhere, which
  the crate assumes to be overlapped (`named_pipe/stream/impl/handle.rs`, `TryFrom<OwnedHandle>`);
  and its server stream can impersonate its client through a safe call (`impersonate_client`,
  `named_pipe/stream/impl.rs`). Rust's standard library sets `SECURITY_SQOS_PRESENT` whenever
  `OpenOptionsExt::security_qos_flags` is used (`std`'s `os/windows/fs.rs`).
- **Why the client's quality of service matters.** A client that opens a pipe without
  `SECURITY_SQOS_PRESENT` lets the server impersonate it at impersonation level, at which the
  server's thread acts as the connecting user on this machine (Windows documentation of
  `CreateFile` for pipes, which the first experiment cites). So a process that created the pipe's name
  before the engine started would today receive, from an `attach` built on the crate's client,
  the ticket, all of the agent's MCP traffic, and the connecting user's identity to act with. A
  process of the same user already has that identity; another user's process, or a service, does
  not.
- **A process that holds the name is also an MCP server.** Whatever quality of service the client
  sets, the process that owns the pipe answers the relay: it can reply to the agent's requests as
  if it were the engine, with or without a ticket, hand the agent fabricated observations (whose
  labels may carry instructions written for the model) and report action results that never
  happened. The quality of service of a client limits what the server can do as the user; it says
  nothing about who the server is.
- **From the first experiment:** its lockfile had no banned crate, and adds three crates the
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
- **From the second experiment, on the client's quality of service** (same machine, same scratch
  crate, the client code with `#![forbid(unsafe_code)]`): a handle opened by one
  `std::fs::OpenOptions` call with read, write, `FILE_FLAG_OVERLAPPED` as a custom flag and
  `security_qos_flags(SECURITY_IDENTIFICATION)` converts into the crate's duplex stream with no
  `unsafe`; the crate's conversion error converts into `io::Error` with `?`. Over that stream,
  six runs of 10 000 pipelined requests with replies up to 60 000 bytes finished with every reply
  matched and the oversized line refused, at the speed of the crate's own client. A test server
  that impersonated this client got a token at identification level (level 1): opening a file
  readable by the user alone failed with `ERROR_BAD_IMPERSONATION_LEVEL` (1346) and opening a key
  of the user's registry hive failed with error 5; with the crate's default client it got
  impersonation level (level 2) and both opens succeeded. With no server, the open fails with OS
  error 2 (`NotFound`); with every instance busy it fails at once with OS error 231
  (`ERROR_PIPE_BUSY`, which has no `ErrorKind`), and a bounded retry on 231 alone connected once
  an instance was free. The whole client fits in one file of about thirty lines whose only SEC-04
  substring is `OpenOptions`, on three lines: two `use` lines and the one call.
- **Not tested by either experiment:** a second local user and an elevated process against the
  descriptor and the client (both need administrator rights; the impersonation test ran as one
  user, so it shows the level rule, not the effect on another account), a pipe created by another
  user before the engine, a client handle opened without `FILE_FLAG_OVERLAPPED` (the crate
  documents that it then may deadlock), a remote machine, the MSVC host, the real broker over
  the pipe, and long runs.

## Decision

1. **The transport is a Windows named pipe through `interprocess`**, pinned to an exact version
   (`=2.4.9`, the version both experiments ran), with its default features off and only its
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
   and nothing else in the code provides. The name is `[mcp] pipe_name` when the operator sets
   one; otherwise it is `ommatix-` followed by the user's name as the process receives it (the
   `USERNAME` variable), lower-cased, with every character the pipe does not accept left out
   (the pipe takes 1 to 128 ASCII letters, digits, `-`, `_` and `.`), and cut to 128 characters.
   `serve` and `attach` derive it the same way from the same configuration, and when nothing of
   the user's name is left the start is refused. Pipe names are not scoped to a logon session,
   so two sessions of one user share the name, and one engine serves them. The name is
   predictable, and whoever starts the process can set `USERNAME`: the name is not a defence
   (first open point). The descriptor gives the owner every right, so any process of the same
   user can also add instances to the engine's pipe after it is created. A client reaches the
   oldest free instance first, so such instances can receive clients, an agent's `attach`
   included, and the shutdown's own wake-up connections (decision 7): a second way, beside
   taking the name first, for another process of the same user to receive `attach`'s traffic.
   A second engine of the same user is still refused, because it cannot create the first
   instance.
5. **A descriptor that grants the owner only: `D:P(A;;GA;;;OW)`.** Protected, one allow entry for
   the object's owner, nothing for Everyone, Anonymous, SYSTEM or Administrators. Without it, the
   default pipe descriptor lets Everyone and Anonymous read (from the first experiment). The user's own
   SID would be the more direct principal, but no safe call gives it: the crate has none, an SDDL
   string takes a SID or a two-letter alias, and obtaining the SID otherwise needs `unsafe` of our
   own or a subprocess (`Command::new`), both forbidden in this adapter.
6. **The engine never runs elevated for this.** The owner of an object created by an elevated
   process is, by default, the Administrators group, not the user, so `OW` would then grant the
   pipe to every administrator of the machine and not to the user alone. The operator starts the
   engine with the broker on from an unelevated session. Measured, not inferred: in CI run
   45814291197 (`feat/broker-pipe` at `37c0ea0`, GitHub's Windows runner, the process elevated:
   integrity `S-1-16-12288`, Administrators enabled, the token's default owner `S-1-5-32-544`)
   the pipe's descriptor, read through a connected client handle, is
   `O:BA G:<the user's primary group> D:P(A;;FA;;;OW)`: the same protected DACL, one entry of full
   access to OWNER RIGHTS, as when not elevated, with `BUILTIN\Administrators` as its owner
   instead of the user. Not elevated, on the development machine, it reads `D:P(A;;FA;;;OW)` with
   the user as owner. So "the owner only" means, when the engine runs elevated, every process
   whose token holds the Administrators group as owner, that is any elevated process of any
   administrator of the machine, not only the user's; when it is not elevated, it means the
   user. The descriptor is the same in both cases; who its owner is differs. The engine does not
   refuse to start elevated; whether it should is the owner's decision (second open point).
7. **The bound on clients is enforced in the accept loop.** ADR-0039 allows eight clients at once.
   The pipe's own instance limit cannot enforce that: it makes a further client connect and wait,
   not fail (from the first experiment). The crate's limit is left at its default, and the accept loop
   counts live connections: a connection beyond the bound is accepted, told in one line that the
   engine is full, and closed. The accept thread never flushes or waits on a refused client: a
   flush waits until the peer has read, so a refused client that never reads would stall
   accepting. The refusal line is written and flushed, and the connection closed, on a short-lived
   thread of its own; those threads are bounded too (four), and past that bound a refused
   connection is closed with no line, which `attach` reports as a refusal; a connection is also
   closed with no line when the broker is unavailable after a failed call. The loop does no
   other work between two accepts, because the listener keeps one waiting instance and a client arriving while it is
   re-created sees the pipe as busy. A connection takes its place among the clients when it is
   accepted, before a byte of it is read, and holds it while its handshake is read; the
   handshake's size is bounded (decision 9), so peers that connect and send nothing, or send a
   handshake without end, hold at most the bound's places and threads. The listener counts its
   own places, taken at accept before the broker is asked; the broker counts its places
   (connected clients and reservations) with atomic operations, apart from its servicing lock,
   so a reservation never waits on a call in progress.

   **The bound on threads, and who enforces it.** At any moment the listener's threads not yet
   joined number at most `1 + 1 + P`: the accept thread, one reaper, and `P` workers, with
   `P = connections + refusals <= n + 4`, where `n` is the broker's bound on clients (eight in
   the executable). That makes `n + 6` at most. A place is given back only after
   `JoinHandle::join` has returned for its worker, by the reaper, never by the worker. The accept
   thread joins nobody and waits on nobody: when every place is held, ending workers included,
   it refuses. The cost: one reaper joins one worker at a time, so a slow exit delays the places
   behind it, and admission refuses meanwhile; a worker's thread-local teardown can be such a
   slow exit (accepted).

   **The reaper accounts first and never runs a worker's panic payload.** The join's result is
   held in `ManuallyDrop`; the entry and the place go first, and only then is the result looked
   at. A worker's panic payload is forgotten, never dropped, because its destructor is foreign
   code that could panic or block on the one thread that frees places. The cost: such a payload,
   with whatever it owns, stays allocated for the life of the process, once per worker that
   panicked. A worker builds its notice inside its own thread, so a thread that could not start
   sends none, and every notice has its entry.

   **Foreign teardown stays off the helper threads.** The accept thread and the reaper never run
   a destructor of a value the user supplied (the broker, its ports, its grant store). The server
   keeps its own reference to the broker for its whole life. Each helper's completion signal is
   the drop of the last thing it owns, a channel's sender, after everything else it captured is
   dropped (for the reaper: its receiver of notices and its reference to the shared state). The
   server learns of a helper's end from that channel; after the signal only the thread's exit
   remains, and shutdown joins it only after the signal. The listener's reference is released on
   the thread that drops the server. **Limit:** this holds only for a server dropped after
   `Complete`, and only when nothing else owns the broker. A server dropped earlier detaches its
   accept thread, its reaper or its workers, each of which may still own a clone of the broker,
   and the final teardown of the broker's ports may then run on one of them. The advice: reach
   `Complete` before dropping, and keep no other owner of the broker if the teardown must run on
   the caller's thread. Process-wide hooks, and thread-locals that a user's port installs on a
   worker thread, are outside this guarantee.

   **Errors of accepting, and the dependency's own retry (a limit).** An error of accepting is
   either transient (OS errors 8, 14, 1450 and 1816, memory, resources or quota short for a
   moment; 232 and 233, a client that left while it was being connected), retried with a pause
   that doubles up to one second and that the stop interrupts, or fatal: any other error ends
   the listener visibly (it is kept as the listener's failure, the listener says it no longer
   accepts, and shutdown reports it). The handling of errors 232 and 233 is a defence; whether
   Windows returns them through the paths that reach the listener was not established. Inside
   `interprocess` 2.4.9, a client that connects and leaves before the accept returns makes the
   accept disconnect that instance and wait again, without the listener's back-off or stop. Each
   such iteration costs the peer one connection (no amplification), and only a process the
   descriptor admits can cause it (the same user, with an engine that is not elevated; an
   elevated engine is the second open point, decision 6); the listener's retry counter does not
   count these iterations. When the listener stops accepting, `serve` shuts it down, says why and
   where the shutdown left it on stderr, and exits with a code for each state (3, 4 or 5), never
   lingering as if it served; it ends its process right after, without dropping the listener.

   **What the listener cannot do.** The crate's safe API has no timeout and no cancel for a
   blocked read, write or flush, and ending such a thread needs `CancelIoEx` or
   `DisconnectNamedPipe`, which are `unsafe`. So a client that connects and then neither sends
   nor reads holds one place, one thread and the pipe's name until it acts, leaves or the engine
   process ends; the handshake has a bound on its size and none on its time; and the bound on
   clients, counted from accept, is the only limit. Shutdown stops accepting at once, but cannot
   end a connection blocked on a silent client: the real end of an engine is the end of its
   process, which closes every handle.

   **What the stop guarantees, and its limit.** There is one stop check per request, immediately
   before the request is handed to the broker (the handshake's admission, and every message
   after it), and no request passes it after the stop is set. A request that passed it earlier
   runs to its end: engine work, a delivered action and its reply may happen after the stop, and
   its connection counts as remaining until its worker is joined. A request begun but not ended
   at the stop is discarded: the stop reaches a connection's reader as a distinct outcome, not as
   an end of input. Nothing more is promised. The operator's stop of actuation (ADR-0007 and its
   successors) is independent and unchanged.

   **The wake-up, and what shutdown reports.** Shutdown wakes its accept thread by connecting to
   its own pipe through the identification-level client of decision 12, and waits for that
   thread a bounded time (twenty tries of 100 ms), never with a bare join: the descriptor gives
   the pipe's owner every right, so any process of the same user can add instances under the
   name (decision 4), and the wake-up connection, like any other connection, may land on one of
   those instead of the engine's. The wake-up ends the accept's wait only when it reaches this
   listener's instance and the accept returns; otherwise the stop takes effect at the next
   connection that reaches the listener, so the stop is not always observed. Shutdown's report
   names one of three states:
   - complete: the accept thread, every worker and the reaper are joined;
   - remaining: workers not yet joined, or only the reaper's end awaited (`only_reaper_left`);
     the server keeps them, and a later call can complete;
   - the accept thread still alive: the wake-up did not reach the listener's instance; the thread
     is kept, and the next shutdown tries again.

   Shutdown's deadline covers the protocol's waits. The two final joins, of the accept thread and
   the reaper, follow each thread's end signal, after which no foreign teardown owned by the
   listener is left, and are bounded only by the thread's own exit. A time too large to
   represent means no deadline. Dropping the server without `shutdown` sets the stop, spends at
   most a bounded wake-up interval (20 connections, 100 ms after each, about 2 s), and joins
   nothing (see above for where the teardown then runs). A caller's wait on the listener's counts
   (`wait_until`) returns once its predicate holds on a copy of the counts; the predicate runs
   outside the listener's lock, no change between the copy and the wait is missed, and the time
   is checked before every wait, so counts that keep changing do not keep it from giving up; it
   is not bounded against a predicate that itself takes time. `serve` calls shutdown only once
   its listener has stopped accepting, and then exits; otherwise the engine ends with its
   process.
8. **`attach` only relays, and never starts an engine.** The executable gains an `attach` mode
   that connects to the pipe of the current user session and relays its standard input and output
   to it, so an MCP host keeps starting a stdio server. When no engine listens, `attach` fails with
   a clear error on stderr and starts nothing. The operator, or an agent host's launcher acting
   with the operator's authority, starts the one engine process with the broker on. The relay
   starts both of its threads before it writes the handshake, so a thread that cannot start
   leaves the pipe unwritten and the input unread. The crate hands a dropped stream it counts as
   unflushed (a client stream made from a handle, or one written since its last flush) to a
   process-wide pool that flushes it before closing it, and a flush waits until the peer has
   read everything: a dropped stream is not closed at once, and one whose peer never reads keeps
   a pool thread blocked until the process ends. So every stream of the pipe, on both ends, is
   marked flushed before it is dropped, the server's on every exit, unwinding included, and
   `attach`'s by whichever relay thread drops it last; the direction still running when the
   relay returns holds the stream until its source ends, and `attach` exits at once, which
   closes everything.
9. **The ticket handshake.** A ticket reaches `attach` as the path of a file (`--ticket-file`),
   never as an argument value or an environment variable: an argument is visible in the process
   list and the environment is inherited by children, while a file can be restricted, and removed
   by the operator after use. `attach` reads it (a read is not a call SEC-04 forbids) and sends it
   as the first line on the pipe, before any MCP message; with no ticket file the line is empty.
   A ticket is 32 to 128 characters of the base64url alphabet (`A`–`Z`, `a`–`z`, `0`–`9`, `-`,
   `_`), without padding; the ticket file holds exactly one and nothing else. The broker hashes
   the line with SHA-256 (`sha2`, a workspace dependency, added to `adapter-mcp`) and asks the
   operator's grants file of ADR-0040 for the grant listed under that hash, at every tool call
   of that client, never once at connect. The grants file (`[safety] grants_file`) is TOML of at
   most 64 KiB: a list of at most 64 `[[grant]]` tables, each with exactly `ticket_sha256` (64
   hexadecimal characters), `applications`, `deliver_input` and `read_values`, and optionally
   `window_classes` and `window_pin`; a file that is missing, unreadable, over the bound, not of
   that form (an unknown key included), with a malformed or repeated hash, too many grants or an
   impossible name or pin grants nothing to anyone until it is fixed. A connection whose
   handshake is empty, or names a ticket the file does not list, has no grant and falls under
   ADR-0040's fail-closed rule. Every well-formed ticket presented is spent for the life of the
   engine process, listed or not, so a ticket admits one connection once; one already spent
   closes the connection like a malformed line. The set of spent tickets is bounded (4 096
   hashes) and, once full, admits nobody new by ticket until the process restarts. The ticket is
   never written to a log, never echoed on the wire and never reaches the model. Both ends bound
   it:
   - on the broker, the handshake is read by the same bounded reader as every other line; a
     handshake longer than the bound, or one that is neither empty nor of the ticket's form,
     closes the connection at once and grants nothing, and no MCP message is read from it;
   - in `attach`, the read of the ticket file is bounded and a larger file is refused, and a
     ticket that contains a line break (`\n` or `\r`, anywhere, a trailing one included) is
     refused before anything is sent, so a ticket file can never put a second line on the pipe.
10. **Off by default.** The pipe exists only with the broker of ADR-0039 switched on, behind two
    gates that are both off by default: the cargo feature `broker-pipe`, which default builds do
    not compile, and the configuration switch `[mcp] transport = "pipe"`, which a build without
    the feature, or off Windows, refuses at start. With the switch off, `serve` serves stdio
    exactly as today, creates no pipe and links nothing new into the path it runs.
11. **Windows only.** No other platform gets a transport in this ADR.
12. **`attach` connects at identification level, and nothing higher.** The client side of the pipe
    opens it with `SECURITY_SQOS_PRESENT` and `SECURITY_IDENTIFICATION`, so the server it reaches
    may learn who connected but cannot act as that user. It does not make the server the engine: a
    process that holds the name can still answer as one (see Context and the first open point).
    This is a requirement of the transport, not optional hardening. The crate's own client cannot set it, so `attach` opens the handle with
    one `std::fs::OpenOptions` call (read, write, `FILE_FLAG_OVERLAPPED` as a custom flag,
    `security_qos_flags(SECURITY_IDENTIFICATION)`) and wraps it, safely, into the crate's stream.
    `FILE_FLAG_OVERLAPPED` is mandatory in that call: the crate's conversion assumes an
    overlapped handle. The two flags are written as literal constants, so the file needs no
    `unsafe` and no new dependency. When the open fails with OS error 231 (every instance busy,
    which also happens for a moment while the listener re-creates its waiting instance), `attach`
    retries a bounded number of times with a short pause, on that error alone; OS error 2 means
    that no engine listens, and `attach` fails as decision 8 says, without retrying. That call
    trips the SEC-04 source check, so the check gains one exception, bounded as follows:
    - it is a named allowlist entry in the test, the pair of one file and one substring
      (`src/adapters/mcp/src/pipe/client.rs`, `OpenOptions`), not a relaxed pattern: every other
      production file keeps failing on `OpenOptions`, and that file keeps failing on every other
      forbidden substring; a list of exceptions other than that one pair is itself a finding;
    - the digest pin is the guarantee: the test pins the file by its SHA-256, with its line ends
      read as `\n`, so the exception covers exactly the bytes a reviewer read, and any change to
      the client is a change to the pin in the same commit. The lexical rules below are a second,
      best-effort tripwire that records what the reviewed file may contain; they read the file
      whole (a test-module marker hides nothing), but they are not claimed to be independent of
      formatting or to tie each setter to the open when the file and the pin change together. A
      change to the file, the pin or the rules is visible in the diff of one test file and one
      source file, and that review is the control;
    - in that file the substring appears exactly three times, in its two `use` lines, which may
      not rename it (`as`), and in one `OpenOptions::new()` call, and the file has one `.open(`,
      of the pipe's path;
    - the file contains only the function that opens the client handle and wraps it (the retry of
      a busy pipe is beside it, in the module); it opens a path it builds itself from the fixed
      prefix `\\.\pipe\` and the per-session name; its one custom flag is the
      `FILE_FLAG_OVERLAPPED` constant and its one quality of service `SECURITY_IDENTIFICATION`,
      each defined once with its value; the test refuses in it, as a word anywhere, comments
      included, `create`, `create_new`, `truncate`, `append`, `creation_disposition` and
      `attributes` (which can carry `FILE_FLAG_DELETE_ON_CLOSE`), so the exception cannot create,
      truncate, extend or delete a file, and `unsafe`, `cfg`, `extern`, `mod`, `include`,
      `include_str`, `include_bytes`, `macro_rules` and `asm`; and it refuses in it any string
      literal, block comment, attribute or macro;
    - this client is the one way the product opens the pipe, the server's own wake-up at
      shutdown included: a second test refuses, in every production source, the crate's own ways
      of connecting.

    The second experiment (see Context) showed what this decision needs: the wrapped handle
    carries traffic in both directions at once with no `unsafe` of our own, and a server that
    impersonates such a client gets identification level and cannot open what the user can,
    while with the crate's default client it can. Identification level still lets that server
    learn who connected, read what the client sends, the ticket included, and answer the agent as
    if it were the engine, which is why verifying the server before anything is relayed remains an
    open point. If a later check
    refutes this decision, the fallback is our own `unsafe` under an exception in the manner of
    ADR-0016, under its own ADR.

### Open points

- **Verifying who owns the pipe before anything is relayed.** Decision 12 keeps a process that
  took the name first from acting as the user, and the real engine then refuses to start, with an
  error that says the name is taken. It does not keep that process from learning who connected,
  from reading the ticket and the agent's requests, or from answering them as if it were the
  engine, with or without a ticket: fabricated observations, whose labels may carry instructions
  for the model, and action results that never happened. What limits the damage as the design
  stands: that process holds no engine, so nothing it reports is delivered to the desktop through
  this relay, and the relay gives the agent no other authority. What it does not limit: an agent
  misled by what it was told, and whatever else that agent's host lets it do with other tools.
  How `attach` could check, before it relays any MCP traffic, the ticket included, that the server
  is this user's engine (by the server's process or the pipe's owner), and how the name is
  derived so that another user cannot predict and take it, are not decided; the calls that tell
  who owns a pipe are not in the crate's safe API. A process of the same user is outside what the
  grant defends against (ADR-0040); another user's is not.
- **Whether, and how, the engine refuses to start the broker when elevated.** The engine does
  not refuse today. Elevated, the pipe's owner is `BUILTIN\Administrators` (measured in CI run
  45814291197, decision 6), so any elevated process of any administrator of the machine can open
  the pipe, not only the user's. Whether the engine should refuse is the owner's decision. The
  only integrity read in the code is the input adapter's, under ADR-0016 and only with
  `sendinput`. Reusing it through the composition root, or another way, is not decided; until
  then decision 6 is the operator's rule.

## Alternatives considered

From the spike, each with its reason:

- **`std::fs::OpenOptions` as the whole client, with its own reads and writes:** a handle opened
  without overlapped I/O cannot read and write at the same time, so a two-thread relay over it
  deadlocks, and it does not wait on a busy pipe. Decision 12 keeps `OpenOptions` for one thing
  only, opening an overlapped handle with the quality of service, and hands it to the crate.
- **The crate's own client, as the first draft had it:** it sets no quality of service, so a
  process that took the name first could act as the connecting user. Rejected as failing open.
- **A broader SEC-04 exception (a relaxed pattern, or `OpenOptions` allowed in a crate):** it
  would let any later file open or create files unseen. One file, one substring, one call.
- **`winpipe`:** refuses remote clients, but passes no security descriptor (the default, readable
  by Everyone) and does not create the first instance exclusively.
- **`named_pipe`:** creates the first instance exclusively, but does not refuse remote clients,
  passes no security descriptor and has been unmaintained since 2019.
- **`tokio::net` and `ipc-channel`:** both bring `mio`, which the lockfile check refuses, and
  `tokio::net` also trips the source check.
- **Our own `unsafe` over the `windows` crate, under a third ADR-0016 exception:** it adds no crate
  and could build a descriptor for the user's exact SID, at the cost of some fifteen to twenty
  `unsafe` blocks to review. It is the fallback if the required validation below refutes the
  owner-only descriptor, the flushing under load or the identification-level client (whose
  wrapped handle carried traffic both ways in the second experiment), and it would need its own
  ADR.
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
  the kind of artefact Smart App Control has refused on the development machine. REL-01's
  licence test reads the default features only, so the crates behind `broker-pipe`
  (`interprocess`, `recvmsg`, `doctest-file`) are not checked by it; their licences were checked
  by hand and are listed in `S4_REPRODUCIBILIDAD.md` §3. With all features, `ISC` and
  `MIT OR Apache-2.0 OR Zlib` are missing from that document; they belong to other features'
  crates, a gap from before this transport.
- **Speed.** Flushing after every reply, as `serve` does today, cost up to 1.9 times the time of
  the same run without it on large replies, and never blocked while the client read. The client's
  quality of service did not change throughput measurably. Because `OpenOptions` does not wait on
  a busy pipe, `attach` may pay one retry pause when it connects while the listener re-creates
  its waiting instance.
- **A new local attack surface:** a pipe any process of the user can open. What bounds it: remote
  clients refused; one owner-only descriptor; one engine per name; a client that grants the
  server identification and nothing more; a bound on clients, on every line and on the
  handshake; no authority without a ticket, which travels in a file, on one line, and never
  through the model; the grant's meet with the operator's policy (ADR-0040); and the switch that
  is off by default. What does not: a process that takes the name first can still read the ticket
  and the traffic of an `attach` that connects to it, and answer the agent as if it were the
  engine, with fabricated observations and invented action results; it holds no engine, so it
  delivers nothing to the desktop through this relay, but it can mislead the agent (open point);
  and a process of the same user can add instances to the engine's pipe and receive some of
  `attach`'s connections in the same way (decision 4).
- **A residual denial of service, from processes of the same user.** A client that stays
  connected without sending or reading holds a place, a thread and the name with no time bound,
  and shutdown cannot end it (decision 7); a peer that presents enough distinct tickets fills
  the set of spent tickets, after which nobody new is admitted by ticket until the engine
  restarts (decision 9). With an engine that is not elevated, only the user's own processes can
  open the pipe (an elevated engine is the second open point, decision 6), and ADR-0040 places
  them outside what a grant defends against; the bound on clients and the set's bound keep both
  to memory and threads that are counted in advance (decision 7 states the bound on the
  listener's threads and who enforces it).
- **Kept until the process ends.** A listener worker's panic payload is forgotten, not dropped,
  so it stays allocated, with whatever it owns, for the life of the process, once per worker
  that panicked (decision 7). The broker's fail-closed poisoning, unchanged, stops service for
  every client after a panic under its lock.
- **One exception to the SEC-04 source check**, the first: one file pinned by its digest, one
  substring, one call, with the calls that could create, change or delete a file refused in that
  file. The test that enforces SEC-04
  changes with the implementation to carry it.
- **Two serving loops.** The pipe uses a synchronous reader next to the asynchronous one of stdio;
  both must keep the same bounds, and the tests compare them.

## What exists (2026-11-26)

On local branches stacked on the broker of ADR-0039, all behind the cargo feature `broker-pipe`
and, in the executable, the switch `[mcp] transport = "pipe"`, both off by default:

- the synchronous loop, the grants asked of a `GrantStore` at every call and the ticket
  handshake, in `adapter-mcp` (`feat/transport-core`);
- the listener and the identification-level client of decisions 2 to 7 and 12, the relay, the
  one bounded SEC-04 exception, pinned to the reviewed file, and the CI step, in `adapter-mcp`,
  with the resolutions of its four reviews (places counted at accept and given back only after
  the reaper has joined their worker, the reservation outside the broker's lock, one stop check
  at the dispatch, the three states of shutdown, the errors of accepting and the dependency's
  own retry as a limit, foreign teardown kept off the helper threads, a worker's panic payload
  never dropped, the digest pin as the guarantee of the exception) (`feat/broker-pipe`);
- the switch, the derivation of the name, the grants file, `attach`, `serve`'s exit when the
  listener stops accepting and the process tests, in the executable, with requirement RF-20, its
  planned tests and scenarios, and the dossier (`feat/broker-attach`).

Run on the development machine (Windows 11, GNU host, not elevated), through real pipes between
threads and between processes. Not run: a second local user, an elevated engine, a real MCP
host, the MSVC host and its CI job (Required validation below). The two open points are open.

## Required validation

New planned tests, with identifiers to be assigned in the matrix when the ADR is accepted.

- On in-memory streams: the synchronous line reader against the bounds of ADR-0015 (a line at the
  limit accepted, one byte over it answered with `-32600`, the connection still up), and the same
  frames as the asynchronous reader on generated input; the handshake (a valid ticket selects its
  grant, an empty, unknown or spent one gives none, the ticket never appears in a reply or a log).
- On Windows, a loopback pipe test with the real broker and the simulated desktop: a session over
  the pipe answers as the same session over stdio.
- A second listener on the same name is refused, and the second `serve` exits with its error.
- A client beyond the bound is told so and closed; the others are not disturbed. A client beyond
  the bound that never reads does not stall accepting: the next client is accepted at once.
- An oversized line is refused without ending the connection; an oversized handshake, or one
  that is not of the ticket's form, closes the connection with no grant and no MCP message read.
- `attach` refuses a ticket file larger than its bound and a ticket with a line break, and sends
  nothing.
- A remote client is refused: a test on Windows connects through the SMB loopback path
  (`\\localhost\pipe\…`), as the first experiment did, and is refused. It needs the Server and
  Workstation services; where they are not running it is reported as skipped with that reason,
  never as passed.
- The client grants identification and nothing more. A test on Windows plays the squatter: it
  creates a pipe server under the name, lets `attach`'s client connect and send a line,
  impersonates it through the crate's safe `impersonate_client`, and tries to open a file the
  test created: the open must fail with `ERROR_BAD_IMPERSONATION_LEVEL` (1346). A positive
  control repeats it with the crate's default client, where the open must succeed, so the test
  cannot pass vacuously. Both are safe Rust and live with the transport's tests in `adapter-mcp`.
  Reading the impersonation level itself needs Win32 calls (`OpenThreadToken`,
  `GetTokenInformation`), hence `unsafe`, which the repository allows only in the two modules of
  ADR-0016 and which the workspace lint forbids in test code too (`src/acceptance/Cargo.toml`,
  lines 59–60); the test does not need it, because the second experiment showed that the safe
  observable separates the two levels.
- `attach` retries a busy pipe (231) a bounded number of times and then fails; with no engine (2)
  it fails at once.
- The SEC-04 source check fails if `OpenOptions` appears in any other production file, in the
  allowed file other than in its two `use` lines and one `OpenOptions::new()` call, if a `use`
  line renames it, or with `create`, `create_new`, `truncate`, `append`, `creation_disposition`
  or `attributes` in that file (a best-effort tripwire), and in every case when that file's
  digest is not the pinned one (the guarantee).
- A client that disconnects in mid-message, or never reads, does not disturb the others.
- `attach` with no engine listening fails and starts no process.
- The repository's SEC-04 source and lockfile checks and the REL-01 checks pass with the new crates
  (REL-01's licence test reads the default features only; see Consequences).
- The `windows` job of CI (MSVC host) builds and tests the pipe.
- **Supervised by the owner, because it needs administrator rights:** a second local user cannot
  open the pipe; a pipe created first by a second local user, playing the squatter, cannot act as
  the `attach` user; and the descriptor created by an elevated process is checked for what `OW`
  then grants. Neither experiment tested these; the last one has a first measurement in CI (run
  45814291197, decision 6: the owner is `BUILTIN\Administrators`), not yet a supervised check.
  Until they have run, the claim that the pipe is restricted to the current user, with an engine
  that is not elevated, rests on one read-back descriptor and a negative control, and this ADR
  must not be accepted on it.

## Reversibility

The transport exists only with the broker switched on, which is off by default; with it off the
executable serves stdio as today. Removing it removes a module of `adapter-mcp`, the `attach` mode
and three crates from the lockfile. A failure of the supervised checks, or of flushing under load,
moves the transport to the fallback above under its own ADR, and so does a failure of the
identification-level client on the MSVC host or with the real broker; a defect in the
descriptor, in the first-instance refusal or in the client's quality of service withdraws it until
fixed. Removing the transport removes the
SEC-04 exception with it.

## Provenance

Decided on 2026-11-24 under the owner's delegation, after a read-only spike and two experiments
in a scratch crate, all run on 2026-11-24 and kept outside the repository, and an independent
review of the first draft that asked for the client's quality of service, the accept loop's
refusal and the handshake's bounds; the second experiment answered the first of these. Serves
[ADR-0039](0039-several-agents-through-one-engine-with-an-input-lease.md) (decisions 1 and 12) and
[ADR-0040](0040-task-scoped-grants-narrower-than-the-local-policy.md) (decision 3); keeps the
bounds of [ADR-0015](0015-mcp-transport-on-rmcp-wire-types.md) and the scope of
[ADR-0016](0016-bounded-unsafe-for-windows-capture-and-input.md); meets the SEC-04 lockfile check
and the REL-01 checks of `src/acceptance/tests/repository.rs` as they stand, and the SEC-04 source
check with the one named exception of decision 12.

## Resumen (ES)

El transporte local de varios clientes es un tubo con nombre de Windows a través de la crate
`interprocess`, fijada a una versión exacta, sin funciones por defecto y con su API síncrona: un
hilo por conexión, un lector de líneas que respeta los límites de ADR-0015 y nada de `unsafe`
propio. Se rechazan los clientes remotos; el nombre es fijo por sesión de usuario y se crea como
primera instancia, lo que impide un segundo motor; el descriptor `D:P(A;;GA;;;OW)` sólo da acceso
al propietario, y el motor no se ejecuta elevado. El límite de clientes se aplica al aceptar.
`attach` sólo retransmite y nunca arranca un motor; se conecta con calidad de servicio de
identificación, para que quien haya ocupado antes el nombre no pueda actuar como el usuario, con
una sola llamada a `OpenOptions` en un archivo, fijado por su SHA-256, que es la única excepción,
acotada, a la comprobación SEC-04, abierta con `FILE_FLAG_OVERLAPPED`; un segundo experimento mostró que ese
manejador envuelto transmite en ambos sentidos a la vez sin `unsafe` propio y que el servidor sólo
obtiene nivel de identificación. Un tubo ocupado se reintenta un número acotado de veces y uno
ausente significa que no hay motor. Un cliente rechazado no bloquea la
aceptación de otros. El tique llega en un archivo acotado, sin saltos de línea, y viaja en una
primera línea acotada; una primera línea demasiado larga o mal formada cierra la conexión sin
concesión. Cuesta tres crates y dos licencias. Un proceso que ocupe antes el nombre no puede
actuar como el usuario, pero sí leer el tique y las peticiones y responder como si fuera el motor:
observaciones inventadas, que pueden llevar instrucciones para el modelo, y resultados de acciones
que no ocurrieron; no entrega nada al escritorio por este canal, pero puede engañar al agente.
Quedan abiertos cómo comprobar quién es dueño del tubo antes de retransmitir cualquier mensaje y
si el motor debe negarse a arrancar elevado, lo que decide el responsable: hoy no se niega, y
elevado el dueño del tubo es el grupo `BUILTIN\Administrators` (medido en la ejecución de CI
45814291197), así que cualquier proceso elevado de cualquier administrador de la máquina lo
puede abrir; sin elevar, el dueño es el usuario. No se acepta sin probar
antes, con el responsable, un segundo usuario (también como ocupante del nombre) y un proceso
elevado; tampoco están probados el anfitrión MSVC ni un anfitrión MCP real. El nombre del tubo es
`[mcp] pipe_name` o se deriva del nombre del usuario; el tique son 32 a 128 caracteres base64url
en un archivo, de un solo uso por proceso, y su concesión sale del archivo de concesiones,
leído en cada llamada. Un cliente callado retiene una plaza, un hilo y el nombre sin límite de
tiempo, y la parada no lo puede terminar. Una plaza sólo se devuelve después de que el hilo
segador ha unido (join) el hilo que la tenía, así que el oyente tiene a lo sumo n + 6 hilos sin
unir (el de aceptación, el segador y n + 4 plazas); el segador no ejecuta nunca la carga del
pánico de un hilo de trabajo, que queda en memoria mientras viva el proceso. Hay una
comprobación de parada por petición, justo antes de entregarla al intermediario: ninguna la pasa
después de la parada, una que ya la pasó llega a su fin y una a medio leer se descarta. La
parada dice en qué estado deja el oyente: completo (todos sus hilos unidos, el segador
incluido), con hilos que quedan o con su hilo de aceptación vivo, porque el despertar no siempre
lo alcanza; su espera no acota las dos uniones finales, que siguen a la señal de fin de cada
hilo. Un servidor soltado antes de completo puede ejecutar la destrucción final del
intermediario en un hilo desligado. Dentro de la dependencia, un cliente que se conecta y se va
antes de que vuelva la aceptación hace que ésta vuelva a esperar sin la pausa ni la parada del
oyente. `serve`, si el oyente deja de aceptar por un error que no es pasajero, lo para y sale
con 3, 4 o 5; un proceso del mismo usuario puede añadir
instancias del nombre y recibir conexiones de `attach`; el conjunto de tiques gastados se puede
llenar. La garantía de la excepción a SEC-04 es el resumen fijado; sus reglas de texto son un
aviso de buena fe. El
transporte, `attach` y el interruptor existen en ramas locales, detrás de una función de
compilación y de un interruptor apagados por defecto.
