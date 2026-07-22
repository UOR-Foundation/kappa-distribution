//! Independent edge canonical form encoder for L3-04 verification (impl spec §3.12).
//!
//! This is NOT the standard crate's `edge_canonical` - it is an independent reimplementation
//! (~20 lines) that the conformance binary uses to verify the registry's edge blob content
//! matches the Appendix A layout byte-for-byte.

use crate::kappa::KappaLabel;

/// Encode an edge in the Appendix A canonical form.
///
/// `source ‖ 0x00 ‖ relation ‖ 0x00 ‖ target ‖ 0x00 ‖ u32-BE(metadata_len) ‖ metadata`
pub fn encode_edge(
    source: &KappaLabel,
    relation: &str,
    target: &KappaLabel,
    metadata: &[u8],
) -> Vec<u8> {
    let src = source.as_str().as_bytes();
    let rel = relation.as_bytes();
    let tgt = target.as_str().as_bytes();
    let meta_len = (metadata.len() as u32).to_be_bytes();
    let mut out =
        Vec::with_capacity(src.len() + 1 + rel.len() + 1 + tgt.len() + 1 + 4 + metadata.len());
    out.extend_from_slice(src);
    out.push(0x00);
    out.extend_from_slice(rel);
    out.push(0x00);
    out.extend_from_slice(tgt);
    out.push(0x00);
    out.extend_from_slice(&meta_len);
    out.extend_from_slice(metadata);
    out
}
