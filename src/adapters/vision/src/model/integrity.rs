//! Supply-chain checks (thesis chapter 08 §8.2): a file is only trusted when its SHA-256
//! matches the digest the operator pinned. Files are read once into memory, bounded, and
//! the bytes that were hashed are the bytes that are used.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::ModelError;

/// A SHA-256 digest pinned by the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Parses 64 hexadecimal characters (either case). Anything else is rejected.
    pub fn parse(hex: &str) -> Result<Self, ModelError> {
        let bad = || ModelError::InvalidDigest(hex.chars().take(80).collect());
        if hex.len() != 64 || !hex.is_ascii() {
            return Err(bad());
        }
        let mut out = [0; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| bad())?;
        }
        Ok(Self(out))
    }

    /// The digest of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Lower-case hexadecimal.
    pub fn to_hex(&self) -> String {
        use std::fmt::Write;
        self.0.iter().fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }
}

/// Fails unless `bytes` hash to `expected`.
pub fn verify(what: &str, bytes: &[u8], expected: &Sha256Digest) -> Result<(), ModelError> {
    let actual = Sha256Digest::of(bytes);
    if actual == *expected {
        Ok(())
    } else {
        Err(ModelError::DigestMismatch {
            what: what.to_owned(),
            expected: expected.to_hex(),
            actual: actual.to_hex(),
        })
    }
}

/// Reads at most `max_bytes` of `path` and checks them against `expected`. A larger file
/// is refused without reading it all.
pub fn read_verified(
    what: &str,
    path: &Path,
    expected: &Sha256Digest,
    max_bytes: u64,
) -> Result<Vec<u8>, ModelError> {
    let io = |source| ModelError::Io {
        what: what.to_owned(),
        path: path.display().to_string(),
        source,
    };
    let file = std::fs::File::open(path).map_err(io)?;
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() as u64 > max_bytes {
        return Err(ModelError::TooLarge {
            what: what.to_owned(),
            max_bytes,
        });
    }
    verify(what, &bytes, expected)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SHA-256 of the empty string and of "abc" (FIPS 180-2 test vectors).
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC: &str = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";

    #[test]
    fn digests_round_trip_and_match_the_test_vectors() {
        assert_eq!(Sha256Digest::of(b"").to_hex(), EMPTY);
        let abc = Sha256Digest::parse(ABC).unwrap();
        assert_eq!(abc, Sha256Digest::of(b"abc"));
        assert_eq!(abc.to_hex(), ABC.to_lowercase());
    }

    #[test]
    fn malformed_digests_are_rejected() {
        for bad in [
            "",
            "abc",
            &EMPTY[1..],
            &format!("{EMPTY}0"),
            &EMPTY.replace('e', "g"),
        ] {
            assert!(Sha256Digest::parse(bad).is_err(), "{bad}");
        }
        // Multi-byte characters never split a byte pair.
        assert!(Sha256Digest::parse(&"é".repeat(32)).is_err());
    }

    #[test]
    fn a_tampered_artifact_is_refused() {
        let pinned = Sha256Digest::of(b"weights v1");
        assert!(verify("detector", b"weights v1", &pinned).is_ok());
        let err = verify("detector", b"weights v2", &pinned).unwrap_err();
        assert!(matches!(err, ModelError::DigestMismatch { .. }));
        assert!(err.to_string().contains(&pinned.to_hex()));
    }

    #[test]
    fn files_are_bounded_and_verified() {
        let dir = std::env::temp_dir().join(format!("ommatix-integrity-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("artifact.bin");
        std::fs::write(&path, b"abc").unwrap();
        let abc = Sha256Digest::parse(ABC).unwrap();
        assert_eq!(read_verified("x", &path, &abc, 3).unwrap(), b"abc");
        assert!(matches!(
            read_verified("x", &path, &abc, 2),
            Err(ModelError::TooLarge { .. })
        ));
        assert!(matches!(
            read_verified("x", &path, &Sha256Digest::of(b""), 10),
            Err(ModelError::DigestMismatch { .. })
        ));
        assert!(matches!(
            read_verified("x", &dir.join("missing"), &abc, 10),
            Err(ModelError::Io { .. })
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
