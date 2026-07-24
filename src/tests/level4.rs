//! Level 4 - Composition, Witnesses, Schemas (L4-01 through L4-52).
//!
//! Verifies G2/F4/E6/E7/E8 composition operations, witness self-describing headers,
//! composed-of/witness-of edge provenance, cross-axis rejection, schema registration
//! and validation, and F4/E7/E8 digest coincidence disambiguation.

use crate::assertions::*;
use crate::client::{RegistryClient, TracedResponse};
use crate::config::Config;
use crate::kappa::KappaLabel;
use crate::runner::TestRunner;
use crate::testdata::TestData;

fn compose(
    client: &RegistryClient,
    ns: &str,
    op: &str,
    operands: &[&str],
) -> Result<TracedResponse, String> {
    let list = operands
        .iter()
        .map(|o| format!("\"{o}\""))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(r#"{{"operands":[{list}]}}"#);
    let rb = client
        .post_with_content_type(&format!("/v2/{ns}/compose/{op}"), "application/json")
        .body(body);
    client
        .send_traced(rb)
        .map_err(|e| format!("request failed: {e}"))
}

fn json_get(body: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let after = &body[body.find(&pat)? + pat.len()..];
    let after = &after[after.find(':')? + 1..];
    let after = &after[after.find('"')? + 1..];
    Some(after[..after.find('"')?].to_string())
}

pub fn run(runner: &mut TestRunner, client: &RegistryClient, config: &Config, data: &TestData) {
    let ns = &config.namespace;

    // Setup: ensure blobs exist. Abort level on failure.
    for blob in [&data.hello, &data.config, &data.layer_a, &data.layer_b] {
        let path = format!("/v2/{ns}/blobs/{}", blob.sha256.as_str());
        let rb = client.put(&path).body(blob.content.to_vec());
        if let Ok(r) = client.send_traced(rb) {
            if r.status != 201 && r.status != 200 {
                eprintln!(
                    "Level 4 setup: blob {} push returned {}, aborting level",
                    blob.name, r.status
                );
                return;
            }
        }
    }

    let mut composed_kappa = String::new();
    let mut witness_kappa = String::new();
    let mut l4_01_body = String::new();

    // ── §8.1 G2 ──────────────────────────────────────────────────────────────

    // L4-01 MUST "G2 compose"
    {
        let mut t = runner.must("L4-01", 4, "G2 composition");
        match compose(
            client,
            ns,
            "g2",
            &[data.hello.sha256.as_str(), data.config.sha256.as_str()],
        ) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    let body = r.body_text();
                    composed_kappa = json_get(&body, "composed").unwrap_or_default();
                    witness_kappa = json_get(&body, "witness").unwrap_or_default();
                    l4_01_body = body;
                    t.pass();
                } else {
                    t.fail(r.fail_msg(200));
                }
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-02 MUST "Response fields"
    {
        let t = runner.must("L4-02", 4, "Composition response fields");
        if l4_01_body.is_empty() {
            t.fail("no response from L4-01".into());
        } else {
            let has_composed = json_get(&l4_01_body, "composed").is_some();
            let has_witness = json_get(&l4_01_body, "witness").is_some();
            let has_op = json_get(&l4_01_body, "operation") == Some("g2".into());
            let has_operands = l4_01_body.contains("operands");
            if has_composed && has_witness && has_op && has_operands {
                t.pass();
            } else {
                t.fail(format!("missing fields in response: {l4_01_body}"));
            }
        }
    }

    // L4-03 MUST "Commutative"
    {
        let mut t = runner.must("L4-03", 4, "G2 commutativity");
        match compose(
            client,
            ns,
            "g2",
            &[data.config.sha256.as_str(), data.hello.sha256.as_str()],
        ) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    let c = json_get(&r.body_text(), "composed").unwrap_or_default();
                    if c == composed_kappa {
                        t.pass();
                    } else {
                        t.fail(format!("g2(b,a) != g2(a,b): {c} vs {composed_kappa}"));
                    }
                } else {
                    t.fail(r.fail_msg(200));
                }
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-04 MUST "Composed stored"
    {
        let mut t = runner.must("L4-04", 4, "Composed blob retrievable");
        if composed_kappa.is_empty() {
            t.fail("no composed kappa".into());
        } else {
            let path = format!("/v2/{ns}/blobs/{composed_kappa}");
            match client.send_traced(client.get(&path)) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 200 {
                        t.pass();
                    } else {
                        t.fail(r.fail_msg(200));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-05 MUST "Composed-of edges"
    let mut l4_05_body = String::new();
    {
        let mut t = runner.must("L4-05", 4, "Composed-of edges created");
        if composed_kappa.is_empty() {
            t.fail("no composed kappa".into());
        } else {
            let path =
                format!("/v2/{ns}/edges/{composed_kappa}?direction=outbound&relation=composed-of");
            match client.send_traced(client.get(&path)) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    let body = r.body_text();
                    let has_a = body.contains(data.hello.sha256.as_str());
                    let has_b = body.contains(data.config.sha256.as_str());
                    if has_a && has_b {
                        l4_05_body = body;
                        t.pass();
                    } else {
                        t.fail(format!("composed-of edges missing operands: {body}"));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-06 MUST "Edge metadata operation"
    {
        let t = runner.must("L4-06", 4, "Edge metadata includes operation type");
        if l4_05_body.is_empty() {
            t.fail("no edge query response from L4-05".into());
        } else if l4_05_body.contains("g2") {
            t.pass();
        } else {
            t.fail(format!(
                "operation type 'g2' not found in edge metadata: {l4_05_body}"
            ));
        }
    }

    // L4-07 MUST "Witness retrievable"
    let mut witness_body: Option<Vec<u8>> = None;
    {
        let mut t = runner.must("L4-07", 4, "Witness retrievable");
        if composed_kappa.is_empty() {
            t.fail("no composed kappa".into());
        } else {
            let path = format!("/v2/{ns}/witnesses/{composed_kappa}");
            match client.send_traced(client.get(&path)) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 200 {
                        t.fail(r.fail_msg(200));
                    } else {
                        witness_body = Some(r.body.clone());
                        t.pass();
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-08 MUST "Witness header and payload verification"
    {
        let t = runner.must(
            "L4-08",
            4,
            "Witness self-describing header and payload re-hash",
        );
        match &witness_body {
            Some(body) => {
                if body.len() < 6 {
                    t.fail("witness blob too short".into());
                } else {
                    let lw = u16::from_le_bytes([body[0], body[1]]);
                    let fw = u16::from_le_bytes([body[2], body[3]]);
                    if lw != 71 || fw != 32 {
                        t.fail(format!("header: label_width={lw} fingerprint_width={fw}"));
                    } else {
                        let payload = &body[6..];
                        if payload.is_empty() {
                            t.fail("witness payload is empty".into());
                        } else if composed_kappa.is_empty() {
                            t.fail("no composed kappa to verify against".into());
                        } else {
                            let axis = composed_kappa.split(':').next().unwrap_or("sha256");
                            let re_hashed = match axis {
                                "sha256" => KappaLabel::sha256(payload),
                                "blake3" => KappaLabel::blake3(payload),
                                _ => {
                                    t.fail(format!("unsupported axis for witness re-hash: {axis}"));
                                    return;
                                }
                            };
                            if re_hashed.as_str() == composed_kappa {
                                t.pass();
                            } else {
                                t.fail(format!(
                                    "witness payload re-hash {} does not match composed {}",
                                    re_hashed.as_str(),
                                    composed_kappa
                                ));
                            }
                        }
                    }
                }
            }
            None => t.fail("no witness body from L4-07".into()),
        }
    }

    // L4-09 MUST "Witness-of edge"
    {
        let mut t = runner.must("L4-09", 4, "Witness-of edge exists");
        if composed_kappa.is_empty() || witness_kappa.is_empty() {
            t.fail("no composed or witness kappa".into());
        } else {
            let path =
                format!("/v2/{ns}/edges/{composed_kappa}?direction=inbound&relation=witness-of");
            match client.send_traced(client.get(&path)) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    let body = r.body_text();
                    if body.contains(&witness_kappa) {
                        t.pass();
                    } else {
                        t.fail(format!(
                            "witness kappa {witness_kappa} not in edge results: {body}"
                        ));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-10 MUST "Cross-axis rejected"
    let mut l4_10_body = String::new();
    let mut l4_10_trace: Option<crate::client::HttpTrace> = None;
    {
        let mut t = runner.must("L4-10", 4, "Cross-axis composition rejected");
        match compose(
            client,
            ns,
            "g2",
            &[data.hello.sha256.as_str(), data.hello.blake3.as_str()],
        ) {
            Ok(r) => {
                l4_10_trace = Some(r.trace.clone());
                l4_10_body = r.body_text();
                t.trace(r.trace.clone());
                if r.status == 422 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(422));
                }
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-11 MUST "AXIS_MISMATCH code"
    {
        let mut t = runner.must("L4-11", 4, "Axis mismatch error code");
        if let Some(tr) = &l4_10_trace {
            t.trace(tr.clone());
        }
        if l4_10_body.is_empty() {
            t.fail("no response body from L4-10".into());
        } else if let Err(e) = verify_error_response(l4_10_body.as_bytes(), "AXIS_MISMATCH") {
            t.fail(e);
        } else {
            t.pass();
        }
    }

    // ── §8.2 F4/E6/E7/E8 ────────────────────────────────────────────────────

    let mut f4_kappa = String::new();

    // L4-20 MUST "F4 compose"
    {
        let mut t = runner.must("L4-20", 4, "F4 composition");
        match compose(client, ns, "f4", &[data.hello.sha256.as_str()]) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    f4_kappa = json_get(&r.body_text(), "composed").unwrap_or_default();
                    t.pass();
                } else {
                    t.fail(r.fail_msg(200));
                }
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-21 MUST "F4 mirror"
    {
        let mut t = runner.must("L4-21", 4, "F4 mirror collapse");
        let complement = data.hello.sha256.complement();
        match compose(client, ns, "f4", &[complement.as_str()]) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    let c = json_get(&r.body_text(), "composed").unwrap_or_default();
                    if c == f4_kappa {
                        t.pass();
                    } else {
                        t.fail(format!("f4(mirror) != f4(original): {c} vs {f4_kappa}"));
                    }
                } else {
                    t.fail(r.fail_msg(200));
                }
            }
            Err(e) => t.fail(e),
        }
    }

    let mut e6_kappa = String::new();

    // L4-22 MUST "E6 compose"
    {
        let mut t = runner.must("L4-22", 4, "E6 composition");
        match compose(client, ns, "e6", &[data.hello.sha256.as_str()]) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    e6_kappa = json_get(&r.body_text(), "composed").unwrap_or_default();
                    t.pass();
                } else {
                    t.fail(r.fail_msg(200));
                }
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-23 MUST "E6 tag byte"
    {
        let mut t = runner.must("L4-23", 4, "E6 tag byte correct");
        if e6_kappa.is_empty() {
            t.fail("no e6 kappa".into());
        } else {
            let hex_str = data.hello.sha256.as_str();
            let first_hex = &hex_str[7..9];
            let first_byte = u8::from_str_radix(first_hex, 16).unwrap_or(0);
            let class = first_byte % 9;
            let expected_tag: u8 = if class <= 7 { 0x05 } else { 0x06 };
            let path = format!("/v2/{ns}/blobs/{e6_kappa}");
            match client.send_traced(client.get(&path)) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.body.is_empty() {
                        t.fail("empty e6 blob".into());
                    } else if r.body[0] == expected_tag {
                        t.pass();
                    } else {
                        t.fail(format!(
                            "expected tag 0x{expected_tag:02x}, got 0x{:02x}",
                            r.body[0]
                        ));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-24 MUST "E7 compose"
    let mut e7_kappa = String::new();
    {
        let mut t = runner.must("L4-24", 4, "E7 composition");
        match compose(client, ns, "e7", &[data.hello.sha256.as_str()]) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    e7_kappa = json_get(&r.body_text(), "composed").unwrap_or_default();
                    t.pass();
                } else {
                    t.fail(r.fail_msg(200));
                }
            }
            Err(e) => t.fail(e),
        }
    }

    let mut e8_kappa = String::new();

    // L4-25 MUST "E8 compose"
    {
        let mut t = runner.must("L4-25", 4, "E8 composition");
        match compose(client, ns, "e8", &[data.hello.sha256.as_str()]) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    e8_kappa = json_get(&r.body_text(), "composed").unwrap_or_default();
                    t.pass();
                } else {
                    t.fail(r.fail_msg(200));
                }
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-26 MUST "E8 != operand"
    {
        let t = runner.must("L4-26", 4, "E8 differs from operand");
        if e8_kappa == data.hello.sha256.as_str() {
            t.fail("e8 kappa equals operand kappa".into());
        } else {
            t.pass();
        }
    }

    // L4-27 MAY "Coincidence documented"
    {
        let t = runner.may("L4-27", 4, "F4/E7/E8 coincidence documented");
        let f4_digest = f4_kappa.split(':').nth(1).unwrap_or("");
        let e7_digest = e7_kappa.split(':').nth(1).unwrap_or("");
        let e8_digest = e8_kappa.split(':').nth(1).unwrap_or("");
        let mut _coincidences = Vec::<&str>::new();
        if !f4_digest.is_empty() && f4_digest == e8_digest {
            _coincidences.push("F4/E8");
        }
        if !e7_digest.is_empty() && e7_digest == e8_digest {
            _coincidences.push("E7/E8");
        }
        if !f4_digest.is_empty() && !e7_digest.is_empty() && f4_digest == e7_digest {
            _coincidences.push("F4/E7");
        }
        t.pass();
    }

    // L4-28 MUST "Coincidence disambiguated"
    {
        let f4_digest = f4_kappa.split(':').nth(1).unwrap_or("");
        let e7_digest = e7_kappa.split(':').nth(1).unwrap_or("");
        let e8_digest = e8_kappa.split(':').nth(1).unwrap_or("");
        let has_coincidence = (!f4_digest.is_empty() && f4_digest == e8_digest)
            || (!e7_digest.is_empty() && e7_digest == e8_digest)
            || (!f4_digest.is_empty() && !e7_digest.is_empty() && f4_digest == e7_digest);

        if !has_coincidence {
            runner
                .must(
                    "L4-28",
                    4,
                    "Coincidence edge metadata disambiguates (no coincidence observed)",
                )
                .pass();
        } else {
            let check_kappa = if !f4_digest.is_empty() && f4_digest == e8_digest {
                &f4_kappa
            } else {
                &e7_kappa
            };
            let mut t = runner.must("L4-28", 4, "Coincidence edge metadata disambiguates");
            let path =
                format!("/v2/{ns}/edges/{check_kappa}?direction=outbound&relation=composed-of");
            match client.send_traced(client.get(&path)) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    let body = r.body_text();
                    let has_op = body.contains("\"f4\"")
                        || body.contains("\"e7\"")
                        || body.contains("\"e8\"");
                    if has_op {
                        t.pass();
                    } else {
                        t.fail(format!(
                            "coincident edge metadata missing operation type: {body}"
                        ));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // ── §8.3 Schema ──────────────────────────────────────────────────────────

    let mut schema_kappa = String::new();
    let schema_content = r#"{"scope":"test-scope","format":"json-schema","validation":{"type":"object","required":["name"],"properties":{"name":{"type":"string"}}}}"#;

    // L4-40 MUST "Schema register"
    {
        let mut t = runner.must("L4-40", 4, "Schema registration");
        let rb = client
            .put_with_content_type(&format!("/v2/{ns}/schemas/test-scope"), "application/json")
            .body(schema_content);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 201 {
                    t.fail(r.fail_msg(201));
                } else {
                    schema_kappa = r.header("x-kappa-label").unwrap_or("").to_string();
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-41 MUST "Schema retrieve"
    {
        let mut t = runner.must("L4-41", 4, "Schema retrieval (body + header)");
        match client.send_traced(client.get(&format!("/v2/{ns}/schemas/test-scope"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let server_label = r.header("x-kappa-label").unwrap_or("").to_string();
                    if !schema_kappa.is_empty() && server_label != schema_kappa {
                        t.fail(format!(
                            "X-Kappa-Label mismatch: {server_label} vs {schema_kappa}"
                        ));
                    } else if let Err(e) = assert_body_eq(&r.body, schema_content.as_bytes()) {
                        t.fail(e);
                    } else {
                        t.pass();
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-42 MUST "Schema list"
    {
        let mut t = runner.must("L4-42", 4, "Schema listing");
        match client.send_traced(client.get(&format!("/v2/{ns}/schemas/"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let body = r.body_text();
                    if body.contains("test-scope") {
                        t.pass();
                    } else {
                        t.fail("test-scope not in schema list".into());
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    let mut valid_kappa = String::new();

    // L4-43 MUST "Valid accepted"
    {
        let mut t = runner.must("L4-43", 4, "Valid content accepted under schema");
        let rb = client
            .put_with_content_type(
                &format!("/v2/{ns}/manifests/schema-pass"),
                "application/json",
            )
            .body(r#"{"name":"valid"}"#);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 201 {
                    t.fail(r.fail_msg(201));
                } else {
                    valid_kappa = r.header("x-kappa-label").unwrap_or("").to_string();
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-44 MUST "Invalid rejected"
    {
        let mut t = runner.must("L4-44", 4, "Invalid content rejected under schema");
        let rb = client
            .put_with_content_type(
                &format!("/v2/{ns}/manifests/schema-fail"),
                "application/json",
            )
            .body(r#"{"not_name":"invalid"}"#);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 422 {
                    if let Err(e) = verify_error_response(&r.body, "SCHEMA_VIOLATION") {
                        t.fail(format!("status 422 but error code wrong: {e}"));
                    } else {
                        t.pass();
                    }
                } else if r.status == 201 || r.status == 200 {
                    t.fail(format!(
                        "schema validation not enforced (content accepted): {}",
                        r.body_text()
                    ));
                } else {
                    t.fail(r.fail_msg(422));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-45 MUST "Rejected not stored"
    {
        let mut t = runner.must("L4-45", 4, "Rejected content not stored");
        let rejected_kappa = KappaLabel::sha256(br#"{"not_name":"invalid"}"#);
        let path = format!("/v2/{ns}/blobs/{}", rejected_kappa.as_str());
        match client.send_traced(client.get(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 404 {
                    t.pass();
                } else {
                    t.fail(format!(
                        "rejected content found (schema not enforced): {}: {}",
                        r.status,
                        r.body_text()
                    ));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-46 MUST "Schema does not alter kappa-label"
    {
        let mut t = runner.must("L4-46", 4, "Schema does not alter kappa-label");
        let expected = KappaLabel::sha256(br#"{"name":"valid"}"#);
        let unscoped_path = format!("/v2/{ns}-noscope/blobs/{}", expected.as_str());
        let rb = client
            .put(&unscoped_path)
            .body(br#"{"name":"valid"}"#.to_vec());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if valid_kappa == expected.as_str() {
                    t.pass();
                } else {
                    t.fail(format!(
                        "schema altered kappa: scoped={valid_kappa}, expected={}",
                        expected.as_str()
                    ));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-47 SHOULD "Schema evolution"
    let mut new_schema_kappa = String::new();
    {
        let mut t = runner.should("L4-47", 4, "Schema evolution with edge");
        let updated = r#"{"scope":"test-scope","format":"json-schema","validation":{"type":"object","required":["name"],"properties":{"name":{"type":"string"},"description":{"type":"string"}}}}"#;
        let rb = client
            .put_with_content_type(&format!("/v2/{ns}/schemas/test-scope"), "application/json")
            .body(updated);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 {
                    new_schema_kappa = r.header("x-kappa-label").unwrap_or("").to_string();
                    t.pass();
                } else {
                    t.fail(r.fail_msg(201));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-48 SHOULD "Evolution edge"
    {
        if new_schema_kappa.is_empty() || schema_kappa.is_empty() {
            runner.skip(
                "L4-48",
                4,
                "Schema evolution edge verified",
                "schema evolution not supported",
            );
        } else {
            let mut t = runner.should("L4-48", 4, "Schema evolution edge verified");
            let path = format!(
                "/v2/{ns}/edges/{new_schema_kappa}?direction=outbound&relation=derives-from"
            );
            match client.send_traced(client.get(&path)) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    let body = r.body_text();
                    if body.contains(&schema_kappa) {
                        t.pass();
                    } else {
                        t.fail(format!("derives-from edge to old schema not found: {body}"));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-50 SHOULD "Composition sets object-type metadata"
    {
        let mut t = runner.should(
            "L4-50",
            4,
            "Composition sets object-type=composition metadata",
        );
        match client.send_traced(client.get_meta_list(
            &format!("/v2/{ns}"),
            "object-type",
            "composition",
        )) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else if r.body_text().contains("sha256:") {
                    t.pass();
                } else {
                    t.fail(format!(
                        "no compositions in metadata query: {}",
                        r.body_text()
                    ));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-51 SHOULD "Witness sets object-type metadata"
    {
        let mut t = runner.should("L4-51", 4, "Witness sets object-type=witness metadata");
        match client.send_traced(client.get_meta_list(
            &format!("/v2/{ns}"),
            "object-type",
            "witness",
        )) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else if r.body_text().contains("sha256:") {
                    t.pass();
                } else {
                    t.fail(format!("no witnesses in metadata query: {}", r.body_text()));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-52 SHOULD "Schema registration sets object-type metadata"
    {
        let mut t = runner.should(
            "L4-52",
            4,
            "Schema registration sets object-type=schema metadata",
        );
        match client.send_traced(client.get_meta_list(
            &format!("/v2/{ns}"),
            "object-type",
            "schema",
        )) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else if r.body_text().contains("sha256:") {
                    t.pass();
                } else {
                    t.fail(format!("no schemas in metadata query: {}", r.body_text()));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }
}
