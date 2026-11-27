//! Opening one of the operator's input files: the grants file (`[safety] grants_file`) and the
//! ticket file of `attach` (`--ticket-file`). Both are the operator's own files, chosen by the
//! operator, on a local fixed disk, and trusted for integrity; they are opened with an ordinary
//! open, never with the MCP adapter's identification-level pipe client, which is for the
//! connection to the engine only.
//!
//! [`open_regular`] opens the path once and, **before anything is read**, refuses a handle that
//! is not a regular file: a directory, a device such as `NUL` or the console, or a named pipe
//! (which would let whoever serves it feed the reader, and could block it). On Windows with the
//! `broker-pipe` feature, the build that reads these files, the kind of the handle is asked of the
//! system (`GetFileType`, through the pinned `winapi-util`: a disk file) **and** the standard
//! library's metadata must say a file; a failed query is a refusal. The standard library's
//! `is_file` alone does not establish the kind on Windows (it means only "neither a directory nor
//! a symbolic link": a named pipe's handle passes it). Elsewhere the standard library's own test
//! of a regular file is used.
//!
//! A stated limit: the open, the two queries and the bounded read that follows can each block
//! for as long as the operator's filesystem does (a network share, a suspended or removed
//! volume). Nothing here bounds that time; the file must be on a local fixed disk.

use std::fs::File;
use std::io;
use std::path::Path;

/// Why [`open_regular`] refused a path. Nothing of the file's content is carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// Nothing at the path.
    NotFound,
    /// The path could not be opened, with the kind of the error.
    Unopenable(io::ErrorKind),
    /// The path opened, but not as a regular file on a disk, or its kind could not be read.
    NotARegularFile,
}

/// Opens `path` for reading and returns the file only when its handle is a regular file. Nothing
/// is read; nothing is written or created.
///
/// # Errors
///
/// [`OpenError`].
pub fn open_regular(path: &Path) -> Result<File, OpenError> {
    let file = File::open(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => OpenError::NotFound,
        kind => OpenError::Unopenable(kind),
    })?;
    if is_regular(&file) {
        Ok(file)
    } else {
        Err(OpenError::NotARegularFile)
    }
}

/// Whether the opened handle is a regular file on a disk: the system's file type of the handle,
/// then the standard library's metadata; any failed query answers no.
#[cfg(all(windows, feature = "broker-pipe"))]
fn is_regular(file: &File) -> bool {
    winapi_util::file::typ(file).is_ok_and(|kind| kind.is_disk())
        && file.metadata().is_ok_and(|metadata| metadata.is_file())
}

/// Whether the opened handle is a regular file. On Windows without `broker-pipe` this is only
/// the standard library's "neither a directory nor a symbolic link"; that build's executable never
/// reads these files (the configuration refuses the pipe switch first): only tests reach it.
#[cfg(not(all(windows, feature = "broker-pipe")))]
fn is_regular(file: &File) -> bool {
    file.metadata().is_ok_and(|metadata| metadata.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ommatix-operator-file-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ut_76_a_regular_file_opens_and_a_missing_one_is_named() {
        let dir = scratch("regular");
        let path = dir.join("grants.toml");
        std::fs::write(&path, "x").unwrap();
        assert!(open_regular(&path).is_ok());
        assert_eq!(
            open_regular(&dir.join("absent")).unwrap_err(),
            OpenError::NotFound
        );
        // A directory is refused, whether at the open (Windows) or by its kind.
        assert!(open_regular(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `NUL` opens on Windows and is a character device: refused before any read.
    #[cfg(windows)]
    #[test]
    fn ut_76_a_device_that_opens_is_refused_before_any_read() {
        assert!(File::open("NUL").is_ok(), "the device opens");
        assert_eq!(
            open_regular(Path::new("NUL")).unwrap_err(),
            OpenError::NotARegularFile
        );
    }
}
