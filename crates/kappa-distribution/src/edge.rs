//! The edge blob canonical byte form (spec Appendix A / §3.4).
//!
//! An edge is a typed directed relationship between two κ-labels and is itself a content-addressed
//! blob. Its κ-label is computed by hashing **these bytes** under the **source's σ-axis** (§3.4).
//!
//! Layout (Appendix A):
//!
//! ```text
//! source ‖ 0x00 ‖ NFC(relation) ‖ 0x00 ‖ target ‖ 0x00 ‖ u32-BE(metadata_len) ‖ metadata
//! ```
//!
//! Null byte separators are unambiguous - κ-labels and relation strings contain no null bytes.
//! The caller supplies already-canonical metadata bytes (or none). Full Unicode NFC normalization
//! of non-ASCII relations is a tracked follow-on; the defined relation types are ASCII, for which
//! NFC is the identity.

use alloc::vec::Vec;

/// The canonical byte form of an edge blob (spec Appendix A).
///
/// - `source` / `target`: κ-labels as their on-the-wire UTF-8 bytes.
/// - `relation`: the relationship type (ASCII relations are NFC-canonical as-is).
/// - `metadata`: opaque, already-deterministic bytes (RFC 8949 §4.2 CBOR); may be empty.
///
/// The returned bytes are hashed under the source's σ-axis to obtain the edge κ-label (§3.4).
pub fn edge_canonical(source: &[u8], relation: &str, target: &[u8], metadata: &[u8]) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(source.len() + relation.len() + target.len() + metadata.len() + 7);
    out.extend_from_slice(source);
    out.push(0x00);
    out.extend_from_slice(relation.as_bytes());
    out.push(0x00);
    out.extend_from_slice(target);
    out.push(0x00);
    out.extend_from_slice(&(metadata.len() as u32).to_be_bytes());
    out.extend_from_slice(metadata);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn canonical_matches_appendix_a_layout() {
        let source = b"blake3:aa";
        let target = b"blake3:bb";
        let relation = "derives-from";

        let mut expected = Vec::new();
        expected.extend_from_slice(source);
        expected.push(0x00);
        expected.extend_from_slice(relation.as_bytes());
        expected.push(0x00);
        expected.extend_from_slice(target);
        expected.push(0x00);
        expected.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(edge_canonical(source, relation, target, b""), expected);

        // Metadata length is u32 big-endian (258 = 0x0000_0102).
        let metadata = alloc::vec![0x5a_u8; 258];
        let out = edge_canonical(source, relation, target, &metadata);
        let len_off = source.len() + 1 + relation.len() + 1 + target.len() + 1;
        assert_eq!(&out[len_off..len_off + 4], &[0x00, 0x00, 0x01, 0x02]);
        assert_eq!(&out[len_off + 4..], &metadata[..]);
    }

    #[test]
    fn deterministic_and_sensitive() {
        let s = b"sha256:aaaa";
        let t = b"sha256:bbbb";
        assert_eq!(
            edge_canonical(s, "owns", t, b""),
            edge_canonical(s, "owns", t, b"")
        );
        assert_ne!(
            edge_canonical(s, "owns", t, b""),
            edge_canonical(s, "derives-from", t, b"")
        );
        assert_ne!(
            edge_canonical(s, "owns", t, b""),
            edge_canonical(t, "owns", s, b"")
        );
    }
}
