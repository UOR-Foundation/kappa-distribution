//! Level 3 - Edge Operations (L3-01 through L3-30).
//!
//! Verifies edge canonical form, axis inheritance, direction queries, and deletion semantics.

use crate::assertions::*;
use crate::client::RegistryClient;
use crate::config::Config;
use crate::edge_canon;
use crate::kappa::KappaLabel;
use crate::runner::TestRunner;
use crate::testdata::TestData;

pub fn run(runner: &mut TestRunner, client: &RegistryClient, config: &Config, data: &TestData) {
    let ns = &config.namespace;

    // Setup: ensure blobs exist. Abort level on failure.
    for blob in [&data.hello, &data.config, &data.layer_a, &data.layer_b] {
        let path = format!("/v2/{ns}/blobs/{}", blob.sha256.as_str());
        let rb = client.put(&path).body(blob.content.to_vec());
        if let Ok(r) = client.send_traced(rb) {
            if r.status != 201 && r.status != 200 {
                eprintln!(
                    "Level 3 setup: blob {} push returned {}, aborting level",
                    blob.name, r.status
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
            .put_with_content_type(&format!("/v2/{ns}/edges/"), "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 201 && r.status != 200 {
                    t.fail(r.fail_msg(201));
                } else {
                    edge_kappa = r.header("x-kappa-label").unwrap_or("").to_string();
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-02 MUST "Edge kappa in response"
    {
        let t = runner.must("L3-02", 3, "Edge kappa-label in response matches expected");
        if edge_kappa.is_empty() {
            t.fail("no edge kappa from L3-01".into());
        } else if let Err(e) = assert_kappa_valid(&edge_kappa) {
            t.fail(format!("invalid kappa: {e}"));
        } else if edge_kappa != expected_edge_kappa.as_str() {
            t.fail(format!(
                "edge kappa wrong: server={edge_kappa}, independently computed={}",
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
            t.fail("no edge kappa".into());
        } else {
            let path = format!("/v2/{ns}/blobs/{edge_kappa}");
            let kappa_parsed = assert_kappa_valid(&edge_kappa);
            match client.send_traced(client.get(&path)) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 200 {
                        t.fail(r.fail_msg(200));
                    } else {
                        match &kappa_parsed {
                            Ok(expected) => {
                                if let Err(e) = assert_kappa_matches(&r.body, expected) {
                                    t.fail(e);
                                } else {
                                    edge_body = Some(r.body.clone());
                                    t.pass();
                                }
                            }
                            Err(e) => t.fail(format!("cannot verify: {e}")),
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let body = r.body_text();
                    if body.contains(data.config.sha256.as_str()) {
                        t.pass();
                    } else {
                        t.fail(format!("target not in results: {body}"));
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let body = r.body_text();
                    if body.contains(data.simple_manifest_sha256.as_str()) {
                        t.pass();
                    } else {
                        t.fail(format!("source not in results: {body}"));
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let body = r.body_text();
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let body = r.body_text();
                    if body.contains("derives-from") {
                        t.pass();
                    } else {
                        t.fail("derives-from edge not in both results".into());
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
            .put_with_content_type(&format!("/v2/{ns}/edges/"), "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                let ek = r.header("x-kappa-label").unwrap_or("");
                if ek == edge_kappa {
                    t.pass();
                } else {
                    t.fail(format!(
                        "idempotent edge kappa mismatch: {ek} vs {edge_kappa}"
                    ));
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
            .put_with_content_type(&format!("/v2/{ns}/edges/"), "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status < 400 {
                    t.fail(format!("expected 4xx, got {}: {}", r.status, r.body_text()));
                } else if let Err(e) = verify_error_response(&r.body, "EDGE_SOURCE_ABSENT") {
                    t.fail(format!("status {} but error code wrong: {e}", r.status));
                } else {
                    t.pass();
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
            .put_with_content_type(&format!("/v2/{ns}/edges/"), "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 || r.status == 200 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(201));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L3-12 MUST "Axis matches source"
    {
        let t = runner.must("L3-12", 3, "Edge kappa-label axis matches source");
        let expected_prefix = format!("{}:", data.simple_manifest_sha256.axis());
        if edge_kappa.starts_with(&expected_prefix) {
            t.pass();
        } else {
            t.fail(format!(
                "edge kappa axis does not match source axis '{}': {edge_kappa}",
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
            .put_with_content_type(&format!("/v2/{ns}/edges/"), "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 201 && r.status != 200 {
                    t.fail(r.fail_msg(201));
                } else {
                    referrer_edge_kappa = r.header("x-kappa-label").unwrap_or("").to_string();
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
            t.fail("referrer edge kappa-label not captured from L3-13".into());
        } else {
            let path = format!(
                "/v2/{ns}/edges/{}?direction=inbound&relation=refers-to",
                data.simple_manifest_sha256.as_str()
            );
            match client.send_traced(client.get(&path)) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 200 {
                        t.fail(r.fail_msg(200));
                    } else {
                        let body = r.body_text();
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
            .put_with_content_type(&format!("/v2/{ns}/edges/"), "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 || r.status == 200 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(201));
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let body = r.body_text();
                    if body.contains(data.hello.sha256.as_str()) {
                        t.pass();
                    } else {
                        t.fail("edge to absent subject not queryable".into());
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    t.pass();
                } else {
                    t.warn(format!("edge pagination returned {}", r.status));
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
            .put_with_content_type(&format!("/v2/{ns}/edges/"), "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok(cr) => {
                t.trace(cr.trace.clone());
                sac_kappa = cr.header("x-kappa-label").unwrap_or("").to_string();
                if sac_kappa.is_empty() {
                    t.fail("no edge kappa from sacrificial edge create".into());
                } else {
                    match client.send_traced(client.delete(&format!("/v2/{ns}/edges/{sac_kappa}")))
                    {
                        Ok(dr) => {
                            t.trace(dr.trace.clone());
                            if dr.status == 202 {
                                t.pass();
                            } else {
                                t.fail(dr.fail_msg(202));
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
        let path = format!(
            "/v2/{ns}/edges/{}?direction=outbound&relation=sacrificial",
            data.hello.sha256.as_str()
        );
        match client.send_traced(client.get(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                let body = r.body_text();
                if !sac_kappa.is_empty() && body.contains(&sac_kappa) {
                    t.fail("deleted edge still in query results".into());
                } else {
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
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
            (Ok(r1), Ok(r2)) => {
                t.trace(r1.trace.clone());
                t.trace(r2.trace.clone());
                if r1.status == 200 && r2.status == 200 {
                    t.pass();
                } else {
                    t.fail(format!(
                        "blobs deleted after edge delete: source={}, target={}",
                        r1.status, r2.status
                    ));
                }
            }
            (Err(e), _) => t.fail(format!("source HEAD failed: {e}")),
            (_, Err(e)) => t.fail(format!("target HEAD failed: {e}")),
        }
    }

    // L3-30 SHOULD "Edge PUT sets object-type metadata"
    {
        let mut t = runner.should("L3-30", 3, "Edge sets object-type=edge metadata");
        match client.send_traced(client.get_meta_list(&format!("/v2/{ns}"), "object-type", "edge"))
        {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let text = r.body_text();
                    if text.contains("sha256:") {
                        t.pass();
                    } else {
                        t.fail(format!("no edges in metadata query: {text}"));
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }
}
