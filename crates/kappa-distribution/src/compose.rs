//! Composition canonical byte forms (spec §5.4 / Appendix B) for the five categorical operations
//! `g2`/`f4`/`e6`/`e7`/`e8`, plus the self-describing witness header (§3.6).
//!
//! The composed κ-label is the hash of [`compose_canonical`] under the operands' shared σ-axis
//! (§5.4 homogeneity). Operand κ-labels are passed as their on-the-wire bytes (`axis:hexdigest`).

use alloc::vec::Vec;

/// A categorical composition operation (spec §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Commutative binary product - lex-min-first concatenation (B.1).
    G2,
    /// Involution quotient - lex-min of the digest and its bitwise complement (B.2).
    F4,
    /// Degree partition - a tag byte from `first_byte % 9` (B.3).
    E6,
    /// S₄ orbit - lex-min over the 24 quarter-permutations of the digest (B.4).
    E7,
    /// Identity embedding - the operand's canonical bytes (B.5).
    E8,
}

impl Op {
    /// Parse a `POST /v2/{path}/compose/{op}` operation token.
    pub fn parse(s: &str) -> Option<Op> {
        match s {
            "g2" => Some(Op::G2),
            "f4" => Some(Op::F4),
            "e6" => Some(Op::E6),
            "e7" => Some(Op::E7),
            "e8" => Some(Op::E8),
            _ => None,
        }
    }

    /// The operation token.
    pub fn as_str(self) -> &'static str {
        match self {
            Op::G2 => "g2",
            Op::F4 => "f4",
            Op::E6 => "e6",
            Op::E7 => "e7",
            Op::E8 => "e8",
        }
    }

    /// The number of operands the operation takes (`g2` is binary; the rest are unary).
    pub fn arity(self) -> usize {
        if matches!(self, Op::G2) {
            2
        } else {
            1
        }
    }
}

/// Why a composition's canonical form could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeError {
    /// The operand count does not match the operation's arity.
    BadArity,
    /// Operands do not all share one σ-axis (§5.4 homogeneity).
    AxisMismatch,
    /// An operand is not a well-formed `axis:hexdigest` κ-label.
    MalformedOperand,
}

/// The canonical byte form of a composition (spec Appendix B). All operands MUST share a σ-axis.
/// The composed κ is `hash(compose_canonical(..))` under that axis.
pub fn compose_canonical(op: Op, operands: &[&[u8]]) -> Result<Vec<u8>, ComposeError> {
    if operands.len() != op.arity() {
        return Err(ComposeError::BadArity);
    }
    let axis0 = axis_of(operands[0]).ok_or(ComposeError::MalformedOperand)?;
    for o in operands.iter() {
        if axis_of(o) != Some(axis0) {
            return Err(ComposeError::AxisMismatch);
        }
    }
    match op {
        Op::G2 => Ok(g2(operands[0], operands[1])),
        Op::F4 => f4(operands[0]),
        Op::E6 => e6(operands[0]),
        Op::E7 => e7(operands[0]),
        Op::E8 => Ok(operands[0].to_vec()),
    }
}

/// A §3.6 self-describing witness blob: `[label_width u16le][fingerprint_width u16le][trace_count
/// u16le][trace]`.
pub fn witness_blob(label_width: u16, fingerprint_width: u16, trace: &[u8]) -> Vec<u8> {
    let mut w = Vec::with_capacity(6 + trace.len());
    w.extend_from_slice(&label_width.to_le_bytes());
    w.extend_from_slice(&fingerprint_width.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(trace);
    w
}

/// The trace bytes of a §3.6 witness blob (everything after the 6-byte header).
pub fn witness_trace(witness: &[u8]) -> Option<&[u8]> {
    witness.get(6..)
}

// ─────────────────────────────── canonical form internals ───────────────────────────────

const HEX_ENC: &[u8; 16] = b"0123456789abcdef";

fn axis_of(label: &[u8]) -> Option<&str> {
    let colon = label.iter().position(|&b| b == b':')?;
    core::str::from_utf8(&label[..colon]).ok()
}

/// The hex portion of a κ-label as a byte slice (everything after the colon).
fn hex_of(label: &[u8]) -> Option<&[u8]> {
    let colon = label.iter().position(|&b| b == b':')?;
    let hex = &label[colon + 1..];
    if hex.is_empty() {
        return None;
    }
    Some(hex)
}

/// Decode the hex portion of a κ-label into raw digest bytes.
fn digest_of(label: &[u8]) -> Option<Vec<u8>> {
    let hex = hex_of(label)?;
    let mut out = Vec::with_capacity(hex.len() / 2);
    let mut pairs = hex.chunks_exact(2);
    for pair in pairs.by_ref() {
        out.push((hexval(pair[0])? << 4) | hexval(pair[1])?);
    }
    if !pairs.remainder().is_empty() {
        return None;
    }
    Some(out)
}

/// Re-encode raw digest bytes as lowercase hex.
fn to_hex(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() * 2);
    for &b in raw {
        out.push(HEX_ENC[(b >> 4) as usize]);
        out.push(HEX_ENC[(b & 0x0f) as usize]);
    }
    out
}

fn hexval(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// B.1 - commutative binary product.
fn g2(a: &[u8], b: &[u8]) -> Vec<u8> {
    let ab = [a, b].concat();
    let ba = [b, a].concat();
    if ab <= ba {
        ab
    } else {
        ba
    }
}

/// B.2 - involution quotient. Operates on decoded digest bytes (bitwise complement requires
/// raw bytes), then re-encodes the lex-min result as hex so the canonical form output is
/// wire-format (`axis:hexdigest`), consistent with G2/E6/E8.
fn f4(a: &[u8]) -> Result<Vec<u8>, ComposeError> {
    let axis = axis_of(a).ok_or(ComposeError::MalformedOperand)?;
    let digest = digest_of(a).ok_or(ComposeError::MalformedOperand)?;
    let complement: Vec<u8> = digest.iter().map(|b| !b).collect();
    let m = core::cmp::min(digest, complement);
    Ok(prefixed(axis, &to_hex(&m)))
}

/// B.3 - degree partition.
fn e6(a: &[u8]) -> Result<Vec<u8>, ComposeError> {
    let digest = digest_of(a).ok_or(ComposeError::MalformedOperand)?;
    let first = *digest.first().ok_or(ComposeError::MalformedOperand)?;
    let tag: u8 = if first % 9 <= 7 { 0x05 } else { 0x06 };
    let mut out = Vec::with_capacity(1 + a.len());
    out.push(tag);
    out.extend_from_slice(a);
    Ok(out)
}

/// B.4 - S₄ orbit. Operates on decoded digest bytes (quarter-permutation requires raw bytes),
/// then re-encodes the lex-min result as hex so the canonical form output is wire-format
/// (`axis:hexdigest`), consistent with G2/E6/E8.
fn e7(a: &[u8]) -> Result<Vec<u8>, ComposeError> {
    let axis = axis_of(a).ok_or(ComposeError::MalformedOperand)?;
    let digest = digest_of(a).ok_or(ComposeError::MalformedOperand)?;
    if digest.is_empty() || digest.len() & 3 != 0 {
        return Err(ComposeError::MalformedOperand);
    }
    let q = digest.len() / 4;
    let quarter = |i: usize| &digest[i * q..(i + 1) * q];
    let mut best: Option<Vec<u8>> = None;
    for perm in PERMS4 {
        let mut cand = Vec::with_capacity(digest.len());
        for &idx in &perm {
            cand.extend_from_slice(quarter(idx));
        }
        if best.as_ref().map_or(true, |b| cand < *b) {
            best = Some(cand);
        }
    }
    Ok(prefixed(axis, &to_hex(&best.unwrap())))
}

fn prefixed(axis: &str, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(axis.len() + 1 + body.len());
    out.extend_from_slice(axis.as_bytes());
    out.push(b':');
    out.extend_from_slice(body);
    out
}

const PERMS4: [[usize; 4]; 24] = [
    [0, 1, 2, 3],
    [0, 1, 3, 2],
    [0, 2, 1, 3],
    [0, 2, 3, 1],
    [0, 3, 1, 2],
    [0, 3, 2, 1],
    [1, 0, 2, 3],
    [1, 0, 3, 2],
    [1, 2, 0, 3],
    [1, 2, 3, 0],
    [1, 3, 0, 2],
    [1, 3, 2, 0],
    [2, 0, 1, 3],
    [2, 0, 3, 1],
    [2, 1, 0, 3],
    [2, 1, 3, 0],
    [2, 3, 0, 1],
    [2, 3, 1, 0],
    [3, 0, 1, 2],
    [3, 0, 2, 1],
    [3, 1, 0, 2],
    [3, 1, 2, 0],
    [3, 2, 0, 1],
    [3, 2, 1, 0],
];

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::String;

    fn hex(bytes: &[u8]) -> String {
        let mut s = String::new();
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    fn label(axis: &str, digest: &[u8]) -> Vec<u8> {
        prefixed(axis, hex(digest).as_bytes())
    }

    #[test]
    fn composition_canonical_forms_and_witness_header() {
        let da: [u8; 32] = [0x11; 32];
        let db: [u8; 32] = [0x22; 32];
        let a = label("blake3", &da);
        let b = label("blake3", &db);

        // g2 is commutative.
        let ab = compose_canonical(Op::G2, &[&a, &b]).unwrap();
        assert_eq!(ab, compose_canonical(Op::G2, &[&b, &a]).unwrap());

        // f4 is involution-invariant.
        let mirror_digest: Vec<u8> = da.iter().map(|b| !b).collect();
        let a_mirror = label("blake3", &mirror_digest);
        assert_eq!(
            compose_canonical(Op::F4, &[&a]).unwrap(),
            compose_canonical(Op::F4, &[&a_mirror]).unwrap()
        );

        // e6 prepends a degree tag byte.
        let e6 = compose_canonical(Op::E6, &[&a]).unwrap();
        assert_eq!(e6[0], 0x06); // 0x11 = 17, 17 % 9 = 8, tag = 0x06
        assert_eq!(&e6[1..], a.as_slice());

        // e7 is quarter-permutation-invariant.
        let mut swapped = Vec::new();
        swapped.extend_from_slice(&da[8..16]);
        swapped.extend_from_slice(&da[0..8]);
        swapped.extend_from_slice(&da[16..24]);
        swapped.extend_from_slice(&da[24..32]);
        let a_perm = label("blake3", &swapped);
        assert_eq!(
            compose_canonical(Op::E7, &[&a]).unwrap(),
            compose_canonical(Op::E7, &[&a_perm]).unwrap()
        );

        // e8 is identity.
        assert_eq!(compose_canonical(Op::E8, &[&a]).unwrap(), a);

        // σ-axis homogeneity.
        let sa = label("sha256", &da);
        assert_eq!(
            compose_canonical(Op::G2, &[&a, &sa]),
            Err(ComposeError::AxisMismatch)
        );

        // Arity enforcement.
        assert_eq!(
            compose_canonical(Op::G2, &[&a]),
            Err(ComposeError::BadArity)
        );
        assert_eq!(
            compose_canonical(Op::E8, &[&a, &b]),
            Err(ComposeError::BadArity)
        );

        // Witness §3.6 header round-trip.
        let w = witness_blob(71, 32, &ab);
        assert_eq!(&w[0..2], &71u16.to_le_bytes());
        assert_eq!(&w[2..4], &32u16.to_le_bytes());
        assert_eq!(witness_trace(&w), Some(ab.as_slice()));
    }

    /// Verify each canonical form contains only wire-format bytes (no raw binary).
    /// Each operation has a different output structure:
    ///   G2: concatenation of two wire-format labels
    ///   E6: tag byte + wire-format label
    ///   E8: identity (the wire-format label itself)
    ///   F4/E7: axis:hex (decoded, operated, re-encoded to hex)
    #[test]
    fn canonical_forms_output_wire_format() {
        let da: [u8; 32] = [0x11; 32];
        let db: [u8; 32] = [0x22; 32];
        let a = label("blake3", &da);
        let b = label("blake3", &db);

        fn is_wire_label(s: &[u8]) -> bool {
            let Some(colon) = s.iter().position(|&c| c == b':') else {
                return false;
            };
            s[colon + 1..]
                .iter()
                .all(|&c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
        }

        // G2: lex-min concatenation of two wire-format operands
        let g2_out = compose_canonical(Op::G2, &[&a, &b]).unwrap();
        assert!(g2_out.starts_with(b"blake3:"));
        assert_eq!(g2_out.len(), a.len() + b.len());

        // F4: axis:hex (re-encoded from raw digest)
        let f4_out = compose_canonical(Op::F4, &[&a]).unwrap();
        assert!(is_wire_label(&f4_out), "f4 output must be axis:hex");

        // E6: tag byte + wire-format operand
        let e6_out = compose_canonical(Op::E6, &[&a]).unwrap();
        assert!(e6_out[0] == 0x05 || e6_out[0] == 0x06);
        assert!(is_wire_label(&e6_out[1..]));

        // E7: axis:hex (re-encoded from raw digest)
        let e7_out = compose_canonical(Op::E7, &[&a]).unwrap();
        assert!(is_wire_label(&e7_out), "e7 output must be axis:hex");

        // E8: identity on wire-format operand
        let e8_out = compose_canonical(Op::E8, &[&a]).unwrap();
        assert!(is_wire_label(&e8_out), "e8 output must be axis:hex");
        assert_eq!(e8_out, a);
    }
}
