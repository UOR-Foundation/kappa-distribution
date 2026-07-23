//! KappaLabel - independent kappa-label type for the conformance binary.
//!
//! This is NOT the registry's KappaLabel. It is an independent implementation that computes hashes
//! from scratch using sha2/blake3/sha1-checked crates. The independence is the point - it is what
//! makes this binary an oracle rather than a self-test.
//!
//! The parser accepts all six recognized axes (sha1, sha256, blake3, sha3-256, keccak256, sha512)
//! because a registry may return any of them. The conformance binary independently computes sha1,
//! sha256, and blake3 hashes. Tests that verify cross-axis behavior (L4-10) use parsed labels from
//! the registry, not independently computed ones.

use sha1_checked::Sha1 as Sha1Checked;
use sha2::{Digest, Sha256};
use std::fmt;

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Decode a single lowercase hex nibble. Returns None for non-hex bytes.
fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// Oversized fixed array (impl spec §3.1). Heterogeneous collections require one type, not
/// `KappaLabel<71>` vs `KappaLabel<135>`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KappaLabel {
    buf: [u8; 135],
    len: u8,
}

/// Parse/construction errors.
#[derive(Debug)]
#[non_exhaustive]
pub enum LabelError {
    BadLength { got: usize },
    NoColon,
    UnknownAxis,
    BadHex { position: usize, byte: u8 },
    WrongDigitCount { expected: usize, got: usize },
    CollisionDetected,
}

impl fmt::Display for LabelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LabelError::BadLength { got } => write!(f, "length {got} not in 45..=135"),
            LabelError::NoColon => write!(f, "no colon separator"),
            LabelError::UnknownAxis => write!(f, "unrecognized axis token"),
            LabelError::BadHex { position, byte } => {
                write!(f, "non-hex byte 0x{byte:02x} at position {position}")
            }
            LabelError::WrongDigitCount { expected, got } => {
                write!(f, "expected {expected} hex digits, got {got}")
            }
            LabelError::CollisionDetected => {
                write!(f, "SHA-1 collision detected, content rejected")
            }
        }
    }
}

impl std::error::Error for LabelError {}

impl KappaLabel {
    /// Parse and validate a κ-label string.
    pub fn parse(s: &str) -> Result<Self, LabelError> {
        let bytes = s.as_bytes();
        if !(45..=135).contains(&bytes.len()) {
            return Err(LabelError::BadLength { got: bytes.len() });
        }
        let colon = bytes
            .iter()
            .position(|&b| b == b':')
            .ok_or(LabelError::NoColon)?;
        let axis = &s[..colon];
        let expected_hex = match axis {
            "sha1" => 40,
            "sha256" => 64,
            "blake3" => 64,
            "sha3-256" => 64,
            "keccak256" => 64,
            "sha512" => 128,
            _ => return Err(LabelError::UnknownAxis),
        };
        let hex_part = &bytes[colon + 1..];
        if hex_part.len() != expected_hex {
            return Err(LabelError::WrongDigitCount {
                expected: expected_hex,
                got: hex_part.len(),
            });
        }
        for (i, &b) in hex_part.iter().enumerate() {
            if !matches!(b, b'0'..=b'9' | b'a'..=b'f') {
                return Err(LabelError::BadHex {
                    position: colon + 1 + i,
                    byte: b,
                });
            }
        }
        let mut buf = [0u8; 135];
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(KappaLabel {
            buf,
            len: bytes.len() as u8,
        })
    }

    /// Compute the SHA-256 κ-label for `content`.
    pub fn sha256(content: &[u8]) -> Self {
        let hash = Sha256::digest(content);
        let mut buf = [0u8; 135];
        buf[..7].copy_from_slice(b"sha256:");
        for (i, &byte) in hash.iter().enumerate() {
            buf[7 + 2 * i] = HEX[(byte >> 4) as usize];
            buf[7 + 2 * i + 1] = HEX[(byte & 0x0f) as usize];
        }
        KappaLabel { buf, len: 71 }
    }

    /// Compute the BLAKE3 κ-label for `content`.
    pub fn blake3(content: &[u8]) -> Self {
        let hash = blake3::hash(content);
        let mut buf = [0u8; 135];
        buf[..7].copy_from_slice(b"blake3:");
        for (i, &byte) in hash.as_bytes().iter().enumerate() {
            buf[7 + 2 * i] = HEX[(byte >> 4) as usize];
            buf[7 + 2 * i + 1] = HEX[(byte & 0x0f) as usize];
        }
        KappaLabel { buf, len: 71 }
    }

    /// Compute the SHA-1 kappa-label for `content` with collision detection.
    ///
    /// Returns `Err(LabelError::CollisionDetected)` if the content triggers
    /// the collision detection algorithm. Legitimate content will never
    /// trigger this -- only crafted collision attacks (SHAttered, shambles).
    pub fn sha1(content: &[u8]) -> Result<Self, LabelError> {
        let result = Sha1Checked::try_digest(content);
        if result.has_collision() {
            return Err(LabelError::CollisionDetected);
        }
        let hash = result.hash();
        let mut buf = [0u8; 135];
        buf[..5].copy_from_slice(b"sha1:");
        for (i, &byte) in hash.iter().enumerate() {
            buf[5 + 2 * i] = HEX[(byte >> 4) as usize];
            buf[5 + 2 * i + 1] = HEX[(byte & 0x0f) as usize];
        }
        Ok(KappaLabel { buf, len: 45 })
    }

    /// The kappa-label as a string slice.
    // SAFETY: buf is always ASCII, guaranteed by parse() and sha256()/blake3() constructors.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len as usize]).unwrap()
    }

    /// The σ-axis token.
    pub fn axis(&self) -> &str {
        let s = self.as_str();
        &s[..s.find(':').unwrap()]
    }

    /// The colon position in the buffer (axis prefix length).
    fn colon_pos(&self) -> usize {
        self.buf[..self.len as usize]
            .iter()
            .position(|&b| b == b':')
            .unwrap()
    }

    /// Bitwise complement of the digest (for F4 mirror tests, impl spec §3.1).
    ///
    /// Returns a new KappaLabel with the same axis prefix and every digest byte bitwise-NOT'd.
    /// Panics if the digest hex has odd length (cannot happen for labels constructed via `parse`,
    /// `sha256`, or `blake3`). Zero allocation - copies the axis prefix directly from `self.buf`.
    pub fn complement(&self) -> Self {
        let colon = self.colon_pos();
        let prefix_len = colon + 1; // include the colon
        let mut buf = [0u8; 135];
        // Copy axis prefix (e.g., "sha256:") directly from self - no allocation.
        buf[..prefix_len].copy_from_slice(&self.buf[..prefix_len]);
        // Complement each digest byte: decode two hex nibbles, NOT, re-encode.
        let hex_bytes = &self.buf[prefix_len..self.len as usize];
        for (i, pair) in hex_bytes.chunks_exact(2).enumerate() {
            let hi = hex_nibble(pair[0]).unwrap();
            let lo = hex_nibble(pair[1]).unwrap();
            let byte = (hi << 4) | lo;
            let comp = !byte;
            buf[prefix_len + 2 * i] = HEX[(comp >> 4) as usize];
            buf[prefix_len + 2 * i + 1] = HEX[(comp & 0x0f) as usize];
        }
        KappaLabel { buf, len: self.len }
    }
}

impl fmt::Display for KappaLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for KappaLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "κ({})", self.as_str())
    }
}

impl AsRef<str> for KappaLabel {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<[u8]> for KappaLabel {
    fn as_ref(&self) -> &[u8] {
        &self.buf[..self.len as usize]
    }
}

impl std::ops::Deref for KappaLabel {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

// ─────────────────────────────── test vector constants (§14) ─────────────

#[cfg(test)]
pub const EMPTY_SHA1: &str = "sha1:da39a3ee5e6b4b0d3255bfef95601890afd80709";

#[cfg(test)]
pub const HELLO_SHA1: &str = "sha1:aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

#[cfg(test)]
pub const EMPTY_SHA256: &str =
    "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[cfg(test)]
pub const HELLO_SHA256: &str =
    "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

#[cfg(test)]
pub const EMPTY_BLAKE3: &str =
    "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

#[cfg(test)]
pub const HELLO_BLAKE3: &str =
    "blake3:ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f";

// Verified at test time: KappaLabel::sha256(br#"{"type":"config"}"#) == CONFIG_SHA256.
#[cfg(test)]
pub const CONFIG_SHA256: &str =
    "sha256:77b1c7603b8bd61b7e1abee27dfd93a43403c9b4398abfe07f75365e8f2603d4";

// Verified against: echo -n '{"no_content_type_test":true}' | sha256sum
#[cfg(test)]
pub const NO_CT_SHA256: &str =
    "sha256:f110d6a44d9bdf3bb7d798901a3aa3de8b01cd41742333d6dcc7530e224df909";

// Verified against: echo -n 'partial-upload-data-for-cancel-test' | sha256sum
#[cfg(test)]
pub const CANCEL_PARTIAL_SHA256: &str =
    "sha256:1b450ee476f1128140208e9e6ed102a79185d1a72fb0a104d7ac6e3d00b1a181";

#[cfg(test)]
mod tests {
    use super::*;

    // ── sha1 hash vector tests ────────────────────────────────────────────

    #[test]
    fn sha1_empty() {
        assert_eq!(KappaLabel::sha1(b"").unwrap().as_str(), EMPTY_SHA1);
    }

    #[test]
    fn sha1_hello() {
        assert_eq!(KappaLabel::sha1(b"hello").unwrap().as_str(), HELLO_SHA1);
    }

    #[test]
    fn sha1_label_length() {
        assert_eq!(KappaLabel::sha1(b"x").unwrap().as_str().len(), 45);
    }

    #[test]
    fn parse_roundtrip_sha1() {
        let k = KappaLabel::sha1(b"test").unwrap();
        let parsed = KappaLabel::parse(k.as_str()).unwrap();
        assert_eq!(k, parsed);
    }

    #[test]
    fn parse_sha1_wrong_digits() {
        // 41 hex digits (46 bytes total, within 45..=135 range) but sha1 expects 40.
        let s = format!("sha1:{}", "a".repeat(41));
        assert_eq!(s.len(), 46);
        assert!(matches!(
            KappaLabel::parse(&s),
            Err(LabelError::WrongDigitCount {
                expected: 40,
                got: 41
            })
        ));
    }

    #[test]
    fn complement_sha1_roundtrip() {
        let k = KappaLabel::sha1(b"involution test").unwrap();
        assert_eq!(k.complement().complement(), k);
    }

    // ── sha256/blake3 hash vector tests ─────────────────────────────────────

    #[test]
    fn sha256_empty() {
        assert_eq!(KappaLabel::sha256(b"").as_str(), EMPTY_SHA256);
    }

    #[test]
    fn sha256_hello() {
        assert_eq!(KappaLabel::sha256(b"hello").as_str(), HELLO_SHA256);
    }

    #[test]
    fn blake3_empty() {
        assert_eq!(KappaLabel::blake3(b"").as_str(), EMPTY_BLAKE3);
    }

    #[test]
    fn blake3_hello() {
        assert_eq!(KappaLabel::blake3(b"hello").as_str(), HELLO_BLAKE3);
    }

    #[test]
    fn sha256_config_blob() {
        assert_eq!(
            KappaLabel::sha256(br#"{"type":"config"}"#).as_str(),
            CONFIG_SHA256
        );
    }

    #[test]
    fn sha256_no_content_type_blob() {
        assert_eq!(
            KappaLabel::sha256(b"{\"no_content_type_test\":true}").as_str(),
            NO_CT_SHA256
        );
    }

    #[test]
    fn sha256_cancel_partial_blob() {
        assert_eq!(
            KappaLabel::sha256(b"partial-upload-data-for-cancel-test").as_str(),
            CANCEL_PARTIAL_SHA256
        );
    }

    // ── parse tests ──────────────────────────────────────────────────────

    #[test]
    fn parse_roundtrip_sha256() {
        let k = KappaLabel::sha256(b"test");
        let parsed = KappaLabel::parse(k.as_str()).unwrap();
        assert_eq!(k, parsed);
    }

    #[test]
    fn parse_roundtrip_blake3() {
        let k = KappaLabel::blake3(b"test");
        let parsed = KappaLabel::parse(k.as_str()).unwrap();
        assert_eq!(k, parsed);
    }

    // Verify the length table for sha3-256 (73 bytes: "sha3-256:" = 9 chars + 64 hex).
    #[test]
    fn parse_valid_sha3_256_length() {
        let s = format!("sha3-256:{}", "a".repeat(64));
        assert_eq!(s.len(), 73);
        let k = KappaLabel::parse(&s).unwrap();
        assert_eq!(k.as_str(), s);
        assert_eq!(k.axis(), "sha3-256");
    }

    // Verify the length table for keccak256 (74 bytes: "keccak256:" = 10 chars + 64 hex).
    #[test]
    fn parse_valid_keccak256_length() {
        let s = format!("keccak256:{}", "b".repeat(64));
        assert_eq!(s.len(), 74);
        let k = KappaLabel::parse(&s).unwrap();
        assert_eq!(k.axis(), "keccak256");
    }

    // Verify the length table for sha512 (135 bytes: "sha512:" = 7 chars + 128 hex).
    #[test]
    fn parse_valid_sha512_length() {
        let s = format!("sha512:{}", "c".repeat(128));
        assert_eq!(s.len(), 135);
        let k = KappaLabel::parse(&s).unwrap();
        assert_eq!(k.axis(), "sha512");
    }

    // ── rejection tests ──────────────────────────────────────────────────

    // Unknown axis at a valid total length (72 bytes: "unknown:" = 8 + 64 hex).
    #[test]
    fn reject_unknown_axis_valid_length() {
        let s = format!("unknown:{}", "a".repeat(64));
        assert_eq!(s.len(), 72);
        assert!(matches!(
            KappaLabel::parse(&s),
            Err(LabelError::UnknownAxis)
        ));
    }

    // Unknown axis at a length within 45..=135 -- axis check fires (not length).
    #[test]
    fn reject_short_unknown_axis_is_unknown_axis() {
        let s = format!("md5:{}", "a".repeat(64));
        assert_eq!(s.len(), 68);
        assert!(matches!(
            KappaLabel::parse(&s),
            Err(LabelError::UnknownAxis)
        ));
    }

    // Unknown axis below the 45-byte minimum -- length check fires first.
    #[test]
    fn reject_very_short_is_bad_length() {
        let s = format!("md5:{}", "a".repeat(10));
        assert_eq!(s.len(), 14);
        assert!(matches!(
            KappaLabel::parse(&s),
            Err(LabelError::BadLength { got: 14 })
        ));
    }

    #[test]
    fn reject_uppercase() {
        let s = format!("sha256:{}", "A".repeat(64));
        assert!(matches!(
            KappaLabel::parse(&s),
            Err(LabelError::BadHex { .. })
        ));
    }

    #[test]
    fn reject_wrong_digit_count() {
        // sha256 expects 64 hex digits; give it 63.
        let s = format!("sha3-256:{}", "a".repeat(63));
        assert_eq!(s.len(), 72); // within 71-135 range
        assert!(matches!(
            KappaLabel::parse(&s),
            Err(LabelError::WrongDigitCount {
                expected: 64,
                got: 63
            })
        ));
    }

    // ── complement tests ─────────────────────────────────────────────────

    // complement(complement(x)) == x - involution property.
    #[test]
    fn complement_roundtrip() {
        let k = KappaLabel::sha256(b"involution test");
        assert_eq!(k.complement().complement(), k);
    }

    // Known vector: sha256:00...00 complements to sha256:ff...ff
    #[test]
    fn complement_known_vector() {
        let zeros = format!("sha256:{}", "0".repeat(64));
        let k = KappaLabel::parse(&zeros).unwrap();
        let comp = k.complement();
        let expected = format!("sha256:{}", "f".repeat(64));
        assert_eq!(comp.as_str(), expected);
    }

    // complement preserves the axis prefix.
    #[test]
    fn complement_preserves_axis() {
        let k = KappaLabel::blake3(b"axis preservation");
        let comp = k.complement();
        assert_eq!(comp.axis(), "blake3");
        assert_eq!(comp.complement().complement(), comp);
    }
}
