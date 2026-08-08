//! SHA-256, via the OS.
//!
//! Used to verify a downloaded installer against the checksum published with the
//! release before it is ever executed (see [`crate::updates::verify_and_launch`]).
//!
//! This calls Windows' own CNG implementation rather than carrying a hashing
//! crate: it is FIPS-validated, always present, and keeps a security-critical
//! primitive out of code we would otherwise have to review ourselves.

use crate::error::{AppError, AppResult};
use windows::core::w;
use windows::Win32::Security::Cryptography::{
    BCryptCloseAlgorithmProvider, BCryptCreateHash, BCryptDestroyHash, BCryptFinishHash,
    BCryptHashData, BCryptOpenAlgorithmProvider, BCRYPT_ALG_HANDLE, BCRYPT_HASH_HANDLE,
    BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS,
};

const SHA256_LEN: usize = 32;

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    match sha256(bytes) {
        Ok(digest) => digest.iter().map(|b| format!("{b:02x}")).collect(),
        // A hash we could not compute must never compare equal to anything, so
        // return a value that cannot match a real 64-character digest.
        Err(_) => String::new(),
    }
}

fn sha256(bytes: &[u8]) -> AppResult<[u8; SHA256_LEN]> {
    let mut algorithm = BCRYPT_ALG_HANDLE::default();
    let mut hash = BCRYPT_HASH_HANDLE::default();
    let mut digest = [0u8; SHA256_LEN];

    // SAFETY: handles are opened, used and closed in order on this thread, and
    // every buffer passed is a live local that outlives its call. Each failure
    // path closes whatever was already opened.
    unsafe {
        BCryptOpenAlgorithmProvider(
            &mut algorithm,
            w!("SHA256"),
            None,
            BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0),
        )
        .ok()
        .map_err(|e| AppError::Win32(format!("SHA-256 is unavailable: {}", e.message())))?;

        let result = (|| -> AppResult<()> {
            BCryptCreateHash(algorithm, &mut hash, None, None, 0)
                .ok()
                .map_err(|e| AppError::Win32(e.message()))?;
            BCryptHashData(hash, bytes, 0)
                .ok()
                .map_err(|e| AppError::Win32(e.message()))?;
            BCryptFinishHash(hash, &mut digest, 0)
                .ok()
                .map_err(|e| AppError::Win32(e.message()))?;
            Ok(())
        })();

        if !hash.is_invalid() {
            let _ = BCryptDestroyHash(hash);
        }
        let _ = BCryptCloseAlgorithmProvider(algorithm, 0);
        result?;
    }

    Ok(digest)
}

/// Constant-time comparison of two hex digests.
///
/// Overkill for comparing a public checksum, but comparing secrets with `==`
/// is a habit worth not having.
pub fn hex_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_published_test_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"The quick brown fox jumps over the lazy dog"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }

    #[test]
    fn a_large_input_hashes_without_truncation() {
        let big = vec![0xABu8; 3 * 1024 * 1024];
        let digest = sha256_hex(&big);
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_single_changed_byte_changes_the_digest() {
        let a = sha256_hex(b"cursorforge-installer");
        let b = sha256_hex(b"cursorforge-installes");
        assert_ne!(a, b);
    }

    #[test]
    fn comparison_rejects_empty_and_mismatched_digests() {
        let real = sha256_hex(b"abc");
        assert!(hex_eq(&real, &real));
        assert!(!hex_eq(&real, ""));
        assert!(!hex_eq("", ""), "an empty digest must never compare equal");
        assert!(!hex_eq(&real, "ba7816bf"));
    }
}
