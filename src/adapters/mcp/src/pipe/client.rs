//! The client end of the pipe (ADR-0043, decision 12): the one file of the product that opens a
//! handle with `std::fs`, under the one exception to SEC-04's source check. That check pins this
//! file by its SHA-256 and bounds what it may contain, so it holds one function and nothing
//! else. The function opens the existing pipe at identification level, so that whoever serves
//! the name may learn who connected but cannot act as that user; overlapped, because the stream
//! the handle is wrapped into assumes it. The retry of a busy pipe is the caller's
//! ([`super::connect`]); every open of the pipe in this crate goes through this function.

use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::OwnedHandle;

use super::{PipeName, PipeStream};

/// `FILE_FLAG_OVERLAPPED`: the stream the handle is wrapped into assumes overlapped I/O.
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;

/// `SECURITY_IDENTIFICATION`: the server may identify the client, not impersonate it.
/// `security_qos_flags` adds `SECURITY_SQOS_PRESENT` itself.
const SECURITY_IDENTIFICATION: u32 = 0x0001_0000;

/// Opens the existing pipe `name` once, for reading and writing, overlapped, at identification
/// level, and wraps the handle into the pipe's stream.
pub(super) fn open(name: &PipeName) -> io::Result<PipeStream> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED)
        .security_qos_flags(SECURITY_IDENTIFICATION)
        .open(name.path())?;
    Ok(PipeStream::try_from(OwnedHandle::from(file))?)
}
