//! Content-addressed identity for the toolkit kernel. Every downstream layer
//! (impact, policy, merge, the orchestrator) caches off these so that the same
//! input always yields the same verdict — the property the whole system calls
//! "deterministic". Hashing is FNV-1a (64-bit): dependency-free and stable
//! across runs and Rust versions, unlike [`std::hash::DefaultHasher`], whose
//! output is explicitly not guaranteed stable.

use std::fmt;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Stable 64-bit FNV-1a hash of a byte slice.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// The content hash of a source file. Equal hashes ⇒ byte-identical sources, so
/// a file whose `ContentHash` is unchanged needs no re-parse on an incremental
/// update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash(pub u64);

impl ContentHash {
    /// Hash a source string.
    pub fn of(source: &str) -> ContentHash {
        ContentHash(fnv1a(source.as_bytes()))
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

/// A repo-relative path used as a file's stable identity in the index. Paths are
/// normalised to forward slashes so a `FileId` is platform-independent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub String);

impl FileId {
    /// Build a `FileId` from a path, normalising separators to `/`.
    pub fn new(path: &str) -> FileId {
        FileId(path.replace('\\', "/"))
    }

    /// The underlying relative path string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_source_same_hash() {
        assert_eq!(ContentHash::of("const x = 1"), ContentHash::of("const x = 1"));
    }

    #[test]
    fn different_source_different_hash() {
        assert_ne!(ContentHash::of("const x = 1"), ContentHash::of("const x = 2"));
    }

    #[test]
    fn whitespace_changes_content_hash() {
        // ContentHash is byte-exact (that is its job); structural equivalence is
        // the separate concern of `structural::Shape`.
        assert_ne!(ContentHash::of("const x=1"), ContentHash::of("const x = 1"));
    }

    #[test]
    fn fnv1a_is_stable() {
        // Pinned vector — guards against an accidental change to the constants.
        assert_eq!(fnv1a(b""), FNV_OFFSET);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn file_id_normalises_separators() {
        assert_eq!(FileId::new("src\\a\\b.ts").as_str(), "src/a/b.ts");
    }
}
