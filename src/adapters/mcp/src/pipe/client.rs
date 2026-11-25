//! The client end of the pipe (ADR-0043, decision 12): the one file of the product that opens a
//! handle with `std::fs`, under the one exception to SEC-04's source check, which bounds what
//! this file may contain. It opens the pipe at identification level, so that whoever serves the
//! name may learn who connected but cannot act as that user; overlapped, because the stream the
//! handle is wrapped into assumes it; and it retries a busy pipe a bounded number of times.
//! Nothing else belongs in this file.

use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::OwnedHandle;
use std::thread;
use std::time::Duration;

use super::{ConnectError, ERROR_PIPE_BUSY, PipeName, PipeStream};

/// `FILE_FLAG_OVERLAPPED`: the stream the handle is wrapped into assumes overlapped I/O.
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;

/// `SECURITY_IDENTIFICATION`: the server may identify the client, not impersonate it.
/// `security_qos_flags` adds `SECURITY_SQOS_PRESENT` itself.
const SECURITY_IDENTIFICATION: u32 = 0x0001_0000;

/// Opens the existing pipe `name` once, for reading and writing, overlapped, at identification
/// level, and wraps the handle into the pipe's stream.
fn open(name: &PipeName) -> io::Result<PipeStream> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED)
        .security_qos_flags(SECURITY_IDENTIFICATION)
        .open(name.path())?;
    Ok(PipeStream::try_from(OwnedHandle::from(file))?)
}

/// Connects to the engine's pipe `name` (ADR-0043, decision 12). A pipe whose every instance is
/// busy (os error 231, also seen for a moment while the server re-creates its waiting instance)
/// is opened again after `pause`, at most `attempts` times in all; any other failure ends at
/// once. [`super::CONNECT_ATTEMPTS`] and [`super::CONNECT_PAUSE`] are the defaults.
///
/// # Errors
///
/// [`ConnectError::NoEngine`] when no pipe of that name exists, [`ConnectError::Busy`] when
/// every try found it busy, [`ConnectError::Os`] otherwise.
pub fn connect(
    name: &PipeName,
    attempts: u32,
    pause: Duration,
) -> Result<PipeStream, ConnectError> {
    let mut tries: u32 = 0;
    loop {
        tries = tries.saturating_add(1);
        match open(name) {
            Ok(stream) => return Ok(stream),
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && tries < attempts => {
                thread::sleep(pause);
            }
            Err(error) => return Err(ConnectError::of_open(&error)),
        }
    }
}
