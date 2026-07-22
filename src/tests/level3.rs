//! Level 3 - Edge Operations (L3-01 through L3-22).
//!
//! Verifies edge canonical form, axis inheritance, direction queries, and deletion semantics.

use crate::assertions::*;
use crate::client::RegistryClient;
use crate::config::Config;
use crate::edge_canon;
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
                    "Level 3 setup: blob {} push returned {s}, aborting level",
                    blob.name
                );
                return;
            }
        }
    }
    let sm_path = format!("/v2/{ns}/blobs/{}", data.simple_manifest_sha256.as_str());
    let rb = client.put(&sm_path).body(data.simple_manifest.clone());
    let _ = client.send_traced(rb);
    let mm_path = format!("/v2/{ns}/blobs/{}", data.multi_manifest_sha256.as_str());
    let rb = client.put(&mm_path).body(data.multi_manifest.clone());
    let _ = client.send_traced(rb);

    // Independently compute the expected edge κ-label for derives-from edge.
    // Edge canonical form: source ‖ 0x00 ‖ relation ‖ 0x00 ‖ target ‖ 0x00 ‖ u32-BE(1) ‖ 0xA0
    // 0xA0 is CBOR empty map (RFC 8949 §4.2 deterministic encoding of {}).
    let expected_edge_bytes = edge_canon::encode_edge(
        &data.simple_manifest_sha256,
        "derives-from",
        &data.config.sha256,
        &[0xA0],
    );
    let expected_edge_kappa = KappaLabel::sha256(&expected_edge_bytes);

    let mut edge_kappa = String::new();

    // L3-01 MUST "Create edge"
    {
        let mut t = runner.must("L3-01", 3, "Create edge");
        let body = format!(
            r#"{{"source":"{}","relation":"derives-from","target":"{}","metadata":{{}}}}"#,
            data.simple_manifest_sha256.as_str(),
            data.config.sha256.as_str(),
        );
        let rb = client
            .put(&format!("/v2/{ns}/edges/"))
            .header("Content-Type", "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if let Err(e) = assert_status_in(s, &[201, 200]) {
                    t.fail(e);
                } else {
                    edge_kappa = resp
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

    // L3-02 MUST "Edge κ in response"
    {
        let t = runner.must("L3-02", 3, "Edge κ-label in response matches expected");
        if edge_kappa.is_empty() {
            t.fail("no edge κ from L3-01".into());
        } else if let Err(e) = assert_kappa_valid(&edge_kappa) {
            t.fail(format!("invalid κ: {e}"));
        } else if edge_kappa != expected_edge_kappa.as_str() {
            t.fail(format!(
                "edge κ wrong: server={edge_kappa}, independently computed={}",
                expected_edge_kappa.as_str()
            ));
        } else {
            t.pass();
        }
    }

    // L3-03 MUST "Edge is blob"
    let mut edge_body: Option<Vec<u8>> = None;
    {
        let mut t = runner.must("L3-03", 3, "Edge retrievable as blob");
        if edge_kappa.is_empty() {
            t.fail("no edge κ".into());
        } else {
            let path = format!("/v2/{ns}/blobs/{edge_kappa}");
            let kappa_parsed = assert_kappa_valid(&edge_kappa);
            match client.send_traced(client.get(&path)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                        t.fail(e);
                    } else {
                        match resp.bytes() {
                            Ok(body) => match &kappa_parsed {
                                Ok(expected) => {
                                    if let Err(e) = assert_kappa_matches(&body, expected) {
                                        t.fail(e);
                                    } else {
                                        edge_body = Some(body.to_vec());
                                        t.pass();
                                    }
                                }
                                Err(e) => t.fail(format!("cannot verify: {e}")),
                            },
                            Err(e) => t.fail(format!("body read failed: {e}")),
                        }
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L3-04 MUST "Canonical form"
    {
        let t = runner.must("L3-04", 3, "Edge canonical form correct");
        match &edge_body {
            Some(body) => {
                // The edge was created with "metadata":{} in JSON. Per RFC 8949 §4.2
                // deterministic encoding, an empty JSON object maps to CBOR empty map 0xA0
                // (major type 5, additional info 0). This is unambiguous.
                if let Err(e) = assert_body_eq(body, &expected_edge_bytes) {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            None => t.fail("no edge body from L3-03".into()),
        }
    }

    // L3-05 MUST "Query outbound"
    {
        let mut t = runner.must("L3-05", 3, "Edge query outbound");
        let path = format!(
            "/v2/{ns}/edges/{}?direction=outbound&relation=derives-from",
            data.simple_manifest_sha256.as_str()
        );
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    match resp.text() {
                        Ok(body) => {
                            if body.contains(data.config.sha256.as_str()) {
                                t.pass();
                            } else {
                                t.fail(format!("target not in results: {body}"));
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-06 MUST "Query inbound"
    {
        let mut t = runner.must("L3-06", 3, "Edge query inbound");
        let path = format!(
            "/v2/{ns}/edges/{}?direction=inbound&relation=derives-from",
            data.config.sha256.as_str()
        );
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    match resp.text() {
                        Ok(body) => {
                            if body.contains(data.simple_manifest_sha256.as_str()) {
                                t.pass();
                            } else {
                                t.fail(format!("source not in results: {body}"));
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-07 MUST "Relation filter"
    {
        let mut t = runner.must("L3-07", 3, "Edge query relation filter");
        let path = format!(
            "/v2/{ns}/edges/{}?direction=outbound&relation=owns",
            data.simple_manifest_sha256.as_str()
        );
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    match resp.text() {
                        Ok(body) => {
                            let v: serde_json::Value =
                                serde_json::from_str(&body).unwrap_or_default();
                            let edges = v
                                .get("edges")
                                .and_then(|e| e.as_array())
                                .map(|a| a.len())
                                .unwrap_or(0);
                            if edges == 0 {
                                t.pass();
                            } else {
                                t.fail(format!("expected 0 'owns' edges, got {edges}"));
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-08 MUST "Direction both"
    {
        let mut t = runner.must("L3-08", 3, "Edge query both directions");
        let path = format!(
            "/v2/{ns}/edges/{}?direction=both",
            data.config.sha256.as_str()
        );
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    match resp.text() {
                        Ok(body) => {
                            if body.contains("derives-from") {
                                t.pass();
                            } else {
                                t.fail("derives-from edge not in both results".into());
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-09 MUST "Idempotent"
    {
        let mut t = runner.must("L3-09", 3, "Edge creation idempotent");
        let body = format!(
            r#"{{"source":"{}","relation":"derives-from","target":"{}","metadata":{{}}}}"#,
            data.simple_manifest_sha256.as_str(),
            data.config.sha256.as_str(),
        );
        let rb = client
            .put(&format!("/v2/{ns}/edges/"))
            .header("Content-Type", "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let h = hdrs(&resp);
                let ek = hdr_val(&h, "x-kappa-label").unwrap_or("");
                if ek == edge_kappa {
                    t.pass();
                } else {
                    t.fail(format!("idempotent edge κ mismatch: {ek} vs {edge_kappa}"));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-10 MUST "Absent source rejected"
    {
        let mut t = runner.must(
            "L3-10",
            3,
            "Edge absent source rejected with EDGE_SOURCE_ABSENT",
        );
        let absent = format!("sha256:{}", "a".repeat(64));
        let body = format!(
            r#"{{"source":"{absent}","relation":"owns","target":"{}","metadata":{{}}}}"#,
            data.hello.sha256.as_str(),
        );
        let rb = client
            .put(&format!("/v2/{ns}/edges/"))
            .header("Content-Type", "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s < 400 {
                    t.fail(format!("expected 4xx, got {s}"));
                } else {
                    match resp.bytes() {
                        Ok(body) => {
                            if let Err(e) = verify_error_response(&body, "EDGE_SOURCE_ABSENT") {
                                t.fail(format!("status {s} but error code wrong: {e}"));
                            } else {
                                t.pass();
                            }
                        }
                        Err(_) => {
                            // Status was 4xx but body unreadable - pass on status, warn on body.
                            t.pass();
                        }
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-11 MUST "Absent target accepted"
    {
        let mut t = runner.must("L3-11", 3, "Edge absent target accepted");
        let absent_target = format!("sha256:{}", "b".repeat(64));
        let body = format!(
            r#"{{"source":"{}","relation":"owns","target":"{absent_target}","metadata":{{}}}}"#,
            data.hello.sha256.as_str(),
        );
        let rb = client
            .put(&format!("/v2/{ns}/edges/"))
            .header("Content-Type", "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status_in(resp.status().as_u16(), &[201, 200]) {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-12 MUST "Axis matches source"
    {
        let t = runner.must("L3-12", 3, "Edge κ-label axis matches source");
        let expected_prefix = format!("{}:", data.simple_manifest_sha256.axis());
        if edge_kappa.starts_with(&expected_prefix) {
            t.pass();
        } else {
            t.fail(format!(
                "edge κ axis does not match source axis '{}': {edge_kappa}",
                data.simple_manifest_sha256.axis()
            ));
        }
    }

    // L3-13 MUST "Referrer edge"
    let mut referrer_edge_kappa = String::new();
    {
        let mut t = runner.must("L3-13", 3, "Referrer edge created");
        let body = format!(
            r#"{{"source":"{}","relation":"refers-to","target":"{}","metadata":{{}}}}"#,
            data.multi_manifest_sha256.as_str(),
            data.simple_manifest_sha256.as_str(),
        );
        let rb = client
            .put(&format!("/v2/{ns}/edges/"))
            .header("Content-Type", "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status_in(resp.status().as_u16(), &[201, 200]) {
                    t.fail(e);
                } else {
                    referrer_edge_kappa = resp
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

    // L3-14 MUST "Referrer queryable"
    {
        let mut t = runner.must("L3-14", 3, "Referrer edge queryable");
        if referrer_edge_kappa.is_empty() {
            t.fail("referrer edge κ-label not captured from L3-13".into());
        } else {
            let path = format!(
                "/v2/{ns}/edges/{}?direction=inbound&relation=refers-to",
                data.simple_manifest_sha256.as_str()
            );
            match client.send_traced(client.get(&path)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                        t.fail(e);
                    } else {
                        match resp.text() {
                            Ok(body) => {
                                let has_source = body.contains(data.multi_manifest_sha256.as_str());
                                let has_edge = body.contains(&referrer_edge_kappa);
                                if has_source && has_edge {
                                    t.pass();
                                } else {
                                    t.fail(format!(
                                        "referrer edge not fully in results (source={has_source} edge_kappa={has_edge})"
                                    ));
                                }
                            }
                            Err(e) => t.fail(format!("body read failed: {e}")),
                        }
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L3-15 MUST "Absent subject edge"
    {
        let mut t = runner.must("L3-15", 3, "Edge to absent subject accepted");
        let absent_subject = format!("sha256:{}", "c".repeat(64));
        let body = format!(
            r#"{{"source":"{}","relation":"refers-to","target":"{absent_subject}","metadata":{{}}}}"#,
            data.hello.sha256.as_str(),
        );
        let rb = client
            .put(&format!("/v2/{ns}/edges/"))
            .header("Content-Type", "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status_in(resp.status().as_u16(), &[201, 200]) {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-16 MUST "Query absent subject"
    {
        let mut t = runner.must("L3-16", 3, "Edge queryable for absent subject");
        let absent_subject = format!("sha256:{}", "c".repeat(64));
        let path = format!("/v2/{ns}/edges/{absent_subject}?direction=inbound");
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    match resp.text() {
                        Ok(body) => {
                            if body.contains(data.hello.sha256.as_str()) {
                                t.pass();
                            } else {
                                t.fail("edge to absent subject not queryable".into());
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-17 SHOULD "Edge pagination"
    {
        let mut t = runner.should("L3-17", 3, "Edge pagination");
        let path = format!(
            "/v2/{ns}/edges/{}?direction=outbound&n=1",
            data.simple_manifest_sha256.as_str()
        );
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if resp.status().as_u16() == 200 {
                    t.pass();
                } else {
                    t.warn(format!("edge pagination returned {}", resp.status()));
                }
            }
            Err(e) => t.warn(format!("request failed: {e}")),
        }
    }

    // L3-20 MUST "Delete edge"
    let mut sac_kappa = String::new();
    {
        let mut t = runner.must("L3-20", 3, "Edge deletion");
        let body = format!(
            r#"{{"source":"{}","relation":"sacrificial","target":"{}","metadata":{{}}}}"#,
            data.hello.sha256.as_str(),
            data.config.sha256.as_str(),
        );
        let rb = client
            .put(&format!("/v2/{ns}/edges/"))
            .header("Content-Type", "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok((create_resp, create_trace)) => {
                t.trace(create_trace);
                sac_kappa = create_resp
                    .headers()
                    .get("x-kappa-label")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                if sac_kappa.is_empty() {
                    t.fail("no edge κ from sacrificial edge create".into());
                } else {
                    match client.send_traced(client.delete(&format!("/v2/{ns}/edges/{sac_kappa}")))
                    {
                        Ok((del_resp, del_trace)) => {
                            t.trace(del_trace);
                            if let Err(e) = assert_status(del_resp.status().as_u16(), 202) {
                                t.fail(e);
                            } else {
                                t.pass();
                            }
                        }
                        Err(e) => t.fail(format!("delete failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("create failed: {e}")),
        }
    }

    // L3-21 MUST "Deleted absent from queries"
    {
        let mut t = runner.must("L3-21", 3, "Deleted edge absent from queries");
        // The sacrificial edge was hello -> config with relation "sacrificial".
        // Query hello's outbound edges for relation "sacrificial" - it should be gone.
        let path = format!(
            "/v2/{ns}/edges/{}?direction=outbound&relation=sacrificial",
            data.hello.sha256.as_str()
        );
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                match resp.text() {
                    Ok(body) => {
                        if !sac_kappa.is_empty() && body.contains(&sac_kappa) {
                            t.fail("deleted edge still in query results".into());
                        } else {
                            // Also verify the edge κ-label itself is not present.
                            let v: serde_json::Value =
                                serde_json::from_str(&body).unwrap_or_default();
                            let edges = v
                                .get("edges")
                                .and_then(|e| e.as_array())
                                .map(|a| a.len())
                                .unwrap_or(0);
                            if edges == 0 {
                                t.pass();
                            } else {
                                t.fail(format!(
                                    "expected 0 sacrificial edges after delete, got {edges}"
                                ));
                            }
                        }
                    }
                    Err(e) => t.fail(format!("body read failed: {e}")),
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-22 MUST "Blobs survive edge delete"
    {
        let mut t = runner.must(
            "L3-22",
            3,
            "Edge delete does not delete blobs (source + target)",
        );
        // The sacrificial edge was hello (source) -> config (target).
        let source_path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
        let target_path = format!("/v2/{ns}/blobs/{}", data.config.sha256.as_str());
        match (
            client.send_traced(client.head(&source_path)),
            client.send_traced(client.head(&target_path)),
        ) {
            (Ok((r1, t1)), Ok((r2, t2))) => {
                t.trace(t1);
                t.trace(t2);
                let s1 = r1.status().as_u16();
                let s2 = r2.status().as_u16();
                if s1 == 200 && s2 == 200 {
                    t.pass();
                } else {
                    t.fail(format!(
                        "blobs deleted after edge delete: source={s1}, target={s2}"
                    ));
                }
            }
            (Err(e), _) => t.fail(format!("source HEAD failed: {e}")),
            (_, Err(e)) => t.fail(format!("target HEAD failed: {e}")),
        }
    }
}
