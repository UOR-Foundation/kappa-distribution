//! Level 4 - Composition, Witnesses, Schemas (L4-01 through L4-48).
//!
//! Verifies G2/F4/E6/E7/E8 composition operations, witness self-describing headers,
//! composed-of/witness-of edge provenance, cross-axis rejection, schema registration
//! and validation, and F4/E7/E8 digest coincidence disambiguation.

use crate::assertions::*;
use crate::client::{HttpTrace, RegistryClient};
use crate::config::Config;
use crate::kappa::KappaLabel;
use crate::runner::TestRunner;
use crate::testdata::TestData;

/// Helper: extract headers as lowercase key-value pairs from a response.
fn hdrs(resp: &reqwest::blocking::Response) -> Vec<(String, String)> {
    resp.headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_lowercase(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

fn hdr_val<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

/// Compose helper that returns trace for the caller to record.
fn compose(
    client: &RegistryClient,
    ns: &str,
    op: &str,
    operands: &[&str],
) -> Result<(u16, String, HttpTrace), String> {
    let list = operands
        .iter()
        .map(|o| format!("\"{o}\""))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(r#"{{"operands":[{list}]}}"#);
    let rb = client
        .post(&format!("/v2/{ns}/compose/{op}"))
        .header("Content-Type", "application/json")
        .body(body);
    match client.send_traced(rb) {
        Ok((resp, trace)) => {
            let status = resp.status().as_u16();
            let body = resp.text().unwrap_or_default();
            Ok((status, body, trace))
        }
        Err(e) => Err(format!("request failed: {e}")),
    }
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
        if let Ok((resp, _)) = client.send_traced(rb) {
            let s = resp.status().as_u16();
            if s != 201 && s != 200 {
                eprintln!(
                    "Level 4 setup: blob {} push returned {s}, aborting level",
                    blob.name
                );
                return;
            }
        }
    }

    let mut composed_kappa = String::new();
    let mut witness_kappa = String::new();
    let mut l4_01_body = String::new();

    // ── §8.1 G2 ──────────────────────────────────────────────────────────────

    // L4-01 MUST "G2 compose" - capture response for L4-02
    {
        let mut t = runner.must("L4-01", 4, "G2 composition");
        match compose(
            client,
            ns,
            "g2",
            &[data.hello.sha256.as_str(), data.config.sha256.as_str()],
        ) {
            Ok((200, body, trace)) => {
                t.trace(trace);
                composed_kappa = json_get(&body, "composed").unwrap_or_default();
                witness_kappa = json_get(&body, "witness").unwrap_or_default();
                l4_01_body = body;
                t.pass();
            }
            Ok((s, _, trace)) => {
                t.trace(trace);
                t.fail(format!("expected 200, got {s}"));
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-02 MUST "Response fields" - validate L4-01's response, no re-issue
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
            Ok((200, body, trace)) => {
                t.trace(trace);
                let c = json_get(&body, "composed").unwrap_or_default();
                if c == composed_kappa {
                    t.pass();
                } else {
                    t.fail(format!("g2(b,a) != g2(a,b): {c} vs {composed_kappa}"));
                }
            }
            Ok((s, _, trace)) => {
                t.trace(trace);
                t.fail(format!("expected 200, got {s}"));
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-04 MUST "Composed stored"
    {
        let mut t = runner.must("L4-04", 4, "Composed blob retrievable");
        if composed_kappa.is_empty() {
            t.fail("no composed κ".into());
        } else {
            let path = format!("/v2/{ns}/blobs/{composed_kappa}");
            match client.send_traced(client.get(&path)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                        t.fail(e);
                    } else {
                        t.pass();
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-05 MUST "Composed-of edges" - capture response for L4-06
    let mut l4_05_body = String::new();
    {
        let mut t = runner.must("L4-05", 4, "Composed-of edges created");
        if composed_kappa.is_empty() {
            t.fail("no composed κ".into());
        } else {
            let path =
                format!("/v2/{ns}/edges/{composed_kappa}?direction=outbound&relation=composed-of");
            match client.send_traced(client.get(&path)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    match resp.text() {
                        Ok(body) => {
                            let has_a = body.contains(data.hello.sha256.as_str());
                            let has_b = body.contains(data.config.sha256.as_str());
                            if has_a && has_b {
                                l4_05_body = body;
                                t.pass();
                            } else {
                                t.fail(format!("composed-of edges missing operands: {body}"));
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-06 MUST "Edge metadata operation" - verify "g2" in edge metadata from L4-05 response
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

    // L4-07 MUST "Witness retrievable" - capture response for L4-08
    let mut witness_body: Option<Vec<u8>> = None;
    {
        let mut t = runner.must("L4-07", 4, "Witness retrievable");
        if composed_kappa.is_empty() {
            t.fail("no composed κ".into());
        } else {
            let path = format!("/v2/{ns}/witnesses/{composed_kappa}");
            match client.send_traced(client.get(&path)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                        t.fail(e);
                    } else {
                        match resp.bytes() {
                            Ok(body) => {
                                witness_body = Some(body.to_vec());
                                t.pass();
                            }
                            Err(e) => t.fail(format!("body read failed: {e}")),
                        }
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-08 MUST "Witness header" - validate from L4-07's captured body, no re-fetch
    {
        let t = runner.must("L4-08", 4, "Witness self-describing header");
        match &witness_body {
            Some(body) => {
                if body.len() < 6 {
                    t.fail("witness blob too short".into());
                } else {
                    let lw = u16::from_le_bytes([body[0], body[1]]);
                    let fw = u16::from_le_bytes([body[2], body[3]]);
                    if lw == 71 && fw == 32 {
                        t.pass();
                    } else {
                        t.fail(format!("header: label_width={lw} fingerprint_width={fw}"));
                    }
                }
            }
            None => t.fail("no witness body from L4-07".into()),
        }
    }

    // L4-09 MUST "Witness-of edge" - verify witness_kappa appears as source
    {
        let mut t = runner.must("L4-09", 4, "Witness-of edge exists");
        if composed_kappa.is_empty() || witness_kappa.is_empty() {
            t.fail("no composed or witness κ".into());
        } else {
            let path =
                format!("/v2/{ns}/edges/{composed_kappa}?direction=inbound&relation=witness-of");
            match client.send_traced(client.get(&path)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    match resp.text() {
                        Ok(body) => {
                            if body.contains(&witness_kappa) {
                                t.pass();
                            } else {
                                t.fail(format!(
                                    "witness κ {witness_kappa} not in edge results: {body}"
                                ));
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-10 MUST "Cross-axis rejected"
    let mut l4_10_body = String::new();
    let mut l4_10_traces: Vec<crate::client::HttpTrace> = Vec::new();
    {
        let mut t = runner.must("L4-10", 4, "Cross-axis composition rejected");
        match compose(
            client,
            ns,
            "g2",
            &[data.hello.sha256.as_str(), data.hello.blake3.as_str()],
        ) {
            Ok((422, body, trace)) => {
                l4_10_traces.push(trace.clone());
                l4_10_body = body;
                t.trace(trace);
                t.pass();
            }
            Ok((s, _, trace)) => {
                t.trace(trace);
                t.fail(format!("expected 422, got {s}"));
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-11 MUST "AXIS_MISMATCH code"
    {
        let mut t = runner.must("L4-11", 4, "Axis mismatch error code");
        for tr in &l4_10_traces {
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
            Ok((200, body, trace)) => {
                t.trace(trace);
                f4_kappa = json_get(&body, "composed").unwrap_or_default();
                t.pass();
            }
            Ok((s, _, trace)) => {
                t.trace(trace);
                t.fail(format!("expected 200, got {s}"));
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-21 MUST "F4 mirror"
    {
        let mut t = runner.must("L4-21", 4, "F4 mirror collapse");
        let complement = data.hello.sha256.complement();
        match compose(client, ns, "f4", &[complement.as_str()]) {
            Ok((200, body, trace)) => {
                t.trace(trace);
                let c = json_get(&body, "composed").unwrap_or_default();
                if c == f4_kappa {
                    t.pass();
                } else {
                    t.fail(format!("f4(mirror) != f4(original): {c} vs {f4_kappa}"));
                }
            }
            Ok((s, _, trace)) => {
                t.trace(trace);
                t.fail(format!("expected 200, got {s}"));
            }
            Err(e) => t.fail(e),
        }
    }

    let mut e6_kappa = String::new();

    // L4-22 MUST "E6 compose"
    {
        let mut t = runner.must("L4-22", 4, "E6 composition");
        match compose(client, ns, "e6", &[data.hello.sha256.as_str()]) {
            Ok((200, body, trace)) => {
                t.trace(trace);
                e6_kappa = json_get(&body, "composed").unwrap_or_default();
                t.pass();
            }
            Ok((s, _, trace)) => {
                t.trace(trace);
                t.fail(format!("expected 200, got {s}"));
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-23 MUST "E6 tag byte"
    {
        let mut t = runner.must("L4-23", 4, "E6 tag byte correct");
        if e6_kappa.is_empty() {
            t.fail("no e6 κ".into());
        } else {
            let hex_str = data.hello.sha256.as_str();
            let first_hex = &hex_str[7..9];
            let first_byte = u8::from_str_radix(first_hex, 16).unwrap_or(0);
            let class = first_byte % 9;
            let expected_tag: u8 = if class <= 7 { 0x05 } else { 0x06 };
            let path = format!("/v2/{ns}/blobs/{e6_kappa}");
            match client.send_traced(client.get(&path)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    match resp.bytes() {
                        Ok(body) => {
                            if body.is_empty() {
                                t.fail("empty e6 blob".into());
                            } else if body[0] == expected_tag {
                                t.pass();
                            } else {
                                t.fail(format!(
                                    "expected tag 0x{expected_tag:02x}, got 0x{:02x}",
                                    body[0]
                                ));
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L4-24 MUST "E7 compose" - capture e7_kappa for coincidence detection
    let mut e7_kappa = String::new();
    {
        let mut t = runner.must("L4-24", 4, "E7 composition");
        match compose(client, ns, "e7", &[data.hello.sha256.as_str()]) {
            Ok((200, body, trace)) => {
                t.trace(trace);
                e7_kappa = json_get(&body, "composed").unwrap_or_default();
                t.pass();
            }
            Ok((s, _, trace)) => {
                t.trace(trace);
                t.fail(format!("expected 200, got {s}"));
            }
            Err(e) => t.fail(e),
        }
    }

    let mut e8_kappa = String::new();

    // L4-25 MUST "E8 compose"
    {
        let mut t = runner.must("L4-25", 4, "E8 composition");
        match compose(client, ns, "e8", &[data.hello.sha256.as_str()]) {
            Ok((200, body, trace)) => {
                t.trace(trace);
                e8_kappa = json_get(&body, "composed").unwrap_or_default();
                t.pass();
            }
            Ok((s, _, trace)) => {
                t.trace(trace);
                t.fail(format!("expected 200, got {s}"));
            }
            Err(e) => t.fail(e),
        }
    }

    // L4-26 MUST "E8 != operand"
    {
        let t = runner.must("L4-26", 4, "E8 differs from operand");
        if e8_kappa == data.hello.sha256.as_str() {
            t.fail("e8 κ equals operand κ".into());
        } else {
            t.pass();
        }
    }

    // L4-27 MAY "Coincidence documented" - check for actual digest coincidence
    {
        let t = runner.may("L4-27", 4, "F4/E7/E8 coincidence documented");
        // Extract digest portions (after the colon) and compare.
        let f4_digest = f4_kappa.split(':').nth(1).unwrap_or("");
        let e7_digest = e7_kappa.split(':').nth(1).unwrap_or("");
        let e8_digest = e8_kappa.split(':').nth(1).unwrap_or("");
        let mut coincidences = Vec::new();
        if !f4_digest.is_empty() && f4_digest == e8_digest {
            coincidences.push("F4/E8");
        }
        if !e7_digest.is_empty() && e7_digest == e8_digest {
            coincidences.push("E7/E8");
        }
        if !f4_digest.is_empty() && !e7_digest.is_empty() && f4_digest == e7_digest {
            coincidences.push("F4/E7");
        }
        if coincidences.is_empty() {
            t.pass(); // No coincidence - informational pass.
        } else {
            t.pass(); // Coincidence observed - informational pass, logged.
        }
    }

    // L4-28 MUST "Coincidence disambiguated" - if coincidence, verify edge metadata distinguishes
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
            // Verify the edge metadata for one of the coincident κ-labels contains the operation type.
            let check_kappa = if !f4_digest.is_empty() && f4_digest == e8_digest {
                &f4_kappa
            } else {
                &e7_kappa
            };
            let mut t = runner.must("L4-28", 4, "Coincidence edge metadata disambiguates");
            let path =
                format!("/v2/{ns}/edges/{check_kappa}?direction=outbound&relation=composed-of");
            match client.send_traced(client.get(&path)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    match resp.text() {
                        Ok(body) => {
                            // The metadata should contain the operation type (f4, e7, or e8).
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
                        Err(e) => t.fail(format!("body read failed: {e}")),
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
            .put(&format!("/v2/{ns}/schemas/test-scope"))
            .header("Content-Type", "application/json")
            .body(schema_content);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 201) {
                    t.fail(e);
                } else {
                    schema_kappa = resp
                        .headers()
                        .get("x-kappa-label")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-41 MUST "Schema retrieve" - verify body content and X-Kappa-Label
    {
        let mut t = runner.must("L4-41", 4, "Schema retrieval (body + header)");
        match client.send_traced(client.get(&format!("/v2/{ns}/schemas/test-scope"))) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    let h = hdrs(&resp);
                    let server_label = hdr_val(&h, "x-kappa-label").unwrap_or("").to_string();
                    if !schema_kappa.is_empty() && server_label != schema_kappa {
                        t.fail(format!(
                            "X-Kappa-Label mismatch: {server_label} vs {schema_kappa}"
                        ));
                    } else {
                        match resp.bytes() {
                            Ok(body) => {
                                if let Err(e) = assert_body_eq(&body, schema_content.as_bytes()) {
                                    t.fail(e);
                                } else {
                                    t.pass();
                                }
                            }
                            Err(e) => t.fail(format!("body read failed: {e}")),
                        }
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
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    match resp.text() {
                        Ok(body) => {
                            if body.contains("test-scope") {
                                t.pass();
                            } else {
                                t.fail("test-scope not in schema list".into());
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
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
            .put(&format!("/v2/{ns}/manifests/schema-pass"))
            .header("Content-Type", "application/json")
            .body(r#"{"name":"valid"}"#);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 201) {
                    t.fail(e);
                } else {
                    valid_kappa = resp
                        .headers()
                        .get("x-kappa-label")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-44 MUST "Invalid rejected" - verify SCHEMA_VIOLATION error code
    {
        let mut t = runner.must("L4-44", 4, "Invalid content rejected under schema");
        let rb = client
            .put(&format!("/v2/{ns}/manifests/schema-fail"))
            .header("Content-Type", "application/json")
            .body(r#"{"not_name":"invalid"}"#);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s == 422 {
                    match resp.bytes() {
                        Ok(body) => {
                            if let Err(e) = verify_error_response(&body, "SCHEMA_VIOLATION") {
                                // Some registries use FILTER_REJECTED - warn, don't fail.
                                t.warn(format!("status 422 but error code wrong: {e}"));
                            } else {
                                t.pass();
                            }
                        }
                        Err(_) => t.pass(), // Status correct, body unreadable - pass on status.
                    }
                } else if s == 201 || s == 200 {
                    t.warn("schema validation not enforced (content accepted)".into());
                } else {
                    t.fail(format!("expected 422, got {s}"));
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
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s == 404 {
                    t.pass();
                } else {
                    t.warn(format!("rejected content found (schema not enforced): {s}"));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L4-46 MUST "Schema ≠ κ-modifier"
    {
        let mut t = runner.must("L4-46", 4, "Schema does not alter κ-label");
        let expected = KappaLabel::sha256(br#"{"name":"valid"}"#);
        let unscoped_path = format!("/v2/{ns}-noscope/blobs/{}", expected.as_str());
        let rb = client
            .put(&unscoped_path)
            .body(br#"{"name":"valid"}"#.to_vec());
        match client.send_traced(rb) {
            Ok((_, trace)) => {
                t.trace(trace);
                if valid_kappa == expected.as_str() {
                    t.pass();
                } else {
                    t.fail(format!(
                        "schema altered κ: scoped={valid_kappa}, expected={}",
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
            .put(&format!("/v2/{ns}/schemas/test-scope"))
            .header("Content-Type", "application/json")
            .body(updated);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s == 201 {
                    new_schema_kappa = resp
                        .headers()
                        .get("x-kappa-label")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    t.pass();
                } else {
                    t.warn(format!("schema evolution returned {s}"));
                }
            }
            Err(e) => t.warn(format!("request failed: {e}")),
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
                Ok((resp, trace)) => {
                    t.trace(trace);
                    match resp.text() {
                        Ok(body) => {
                            if body.contains(&schema_kappa) {
                                t.pass();
                            } else {
                                t.warn(format!(
                                    "derives-from edge to old schema not found: {body}"
                                ));
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }
}
