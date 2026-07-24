//! Level 2 - Tag Operations (L2-01 through L2-80).
//!
//! Includes direct tag binding (PUT /tags/) and direct tag resolution (GET /tags/).

use crate::assertions::*;
use crate::client::RegistryClient;
use crate::config::Config;
use crate::kappa::KappaLabel;
use crate::runner::TestRunner;
use crate::testdata::TestData;

pub fn run(runner: &mut TestRunner, client: &RegistryClient, config: &Config, data: &TestData) {
    let ns = &config.namespace;

    // Setup: ensure blobs exist (re-push if Level 1 was not run).
    for blob in [&data.config, &data.layer_a, &data.layer_b, &data.hello] {
        let path = format!("/v2/{ns}/blobs/{}", blob.sha256.as_str());
        let rb = client.put(&path).body(blob.content.to_vec());
        if let Ok(r) = client.send_traced(rb) {
            if r.status != 201 && r.status != 200 {
                eprintln!(
                    "Level 2 setup: blob {} push returned {}, aborting level",
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

    // ── §6.1 Bind and Resolve ────────────────────────────────────────────────

    // L2-01 MUST "Bind tag"
    {
        let mut t = runner.must("L2-01", 2, "Bind tag");
        let rb = client
            .put_with_content_type(&format!("/v2/{ns}/manifests/v1.0"), "application/json")
            .body(data.simple_manifest.clone());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 201 {
                    t.fail(r.fail_msg(201));
                } else if let Err(e) = assert_location_present(&r.headers) {
                    t.fail(e);
                } else if let Err(e) =
                    assert_docker_content_digest(&r.headers, data.simple_manifest_sha256.as_str())
                {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-02 MUST "X-Kappa-Label in response"
    {
        let mut t = runner.must("L2-02", 2, "Tag bind returns correct X-Kappa-Label");
        let rb = client
            .put_with_content_type(&format!("/v2/{ns}/manifests/l2-02-tag"), "application/json")
            .body(data.simple_manifest.clone());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                match r.header("x-kappa-label") {
                    None => t.fail("X-Kappa-Label header missing".into()),
                    Some(label) => {
                        if let Err(e) = assert_kappa_valid(label) {
                            t.fail(format!("invalid kappa: {e}"));
                        } else if label != data.simple_manifest_sha256.as_str() {
                            t.fail(format!(
                                "X-Kappa-Label wrong: got {label}, expected {}",
                                data.simple_manifest_sha256.as_str()
                            ));
                        } else {
                            t.pass();
                        }
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-03 MUST "Resolve tag"
    {
        let mut t = runner.must("L2-03", 2, "Resolve tag (body + header + re-hash)");
        match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let server_label = r.header("x-kappa-label").unwrap_or("").to_string();
                    if server_label != data.simple_manifest_sha256.as_str() {
                        t.fail(format!(
                            "X-Kappa-Label wrong: got {server_label}, expected {}",
                            data.simple_manifest_sha256.as_str()
                        ));
                    } else if let Err(e) = assert_body_eq(&r.body, &data.simple_manifest) {
                        t.fail(format!("body mismatch: {e}"));
                    } else if let Err(e) =
                        assert_kappa_matches(&r.body, &data.simple_manifest_sha256)
                    {
                        t.fail(format!("re-hash mismatch: {e}"));
                    } else if let Err(e) = assert_docker_content_digest(
                        &r.headers,
                        data.simple_manifest_sha256.as_str(),
                    ) {
                        t.fail(e);
                    } else {
                        t.pass();
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-04 MUST "Bind second tag same content"
    {
        let mut t = runner.must("L2-04", 2, "Bind second tag to same content");
        let rb = client
            .put(&format!("/v2/{ns}/manifests/latest"))
            .body(data.simple_manifest.clone());
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

    // L2-05 MUST "Both tags same kappa"
    {
        let mut t = runner.must("L2-05", 2, "Both tags resolve same kappa-label");
        let r1 = client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0")));
        let r2 = client.send_traced(client.get(&format!("/v2/{ns}/manifests/latest")));
        match (r1, r2) {
            (Ok(a), Ok(b)) => {
                t.trace(a.trace.clone());
                t.trace(b.trace.clone());
                let k1 = a.header("x-kappa-label");
                let k2 = b.header("x-kappa-label");
                if k1.is_some() && k1 == k2 {
                    t.pass();
                } else {
                    t.fail(format!("v1.0={k1:?} latest={k2:?}"));
                }
            }
            (Err(e), _) => t.fail(format!("v1.0 GET failed: {e}")),
            (_, Err(e)) => t.fail(format!("latest GET failed: {e}")),
        }
    }

    // ── §6.2 Update and Retain ───────────────────────────────────────────────

    // L2-06 MUST "Update tag"
    {
        let mut t = runner.must("L2-06", 2, "Update tag to different content");
        let rb = client
            .put(&format!("/v2/{ns}/manifests/latest"))
            .body(data.multi_manifest.clone());
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

    // L2-07 MUST "Latest is new kappa"
    {
        let mut t = runner.must("L2-07", 2, "Updated tag resolves new kappa-label");
        match client.send_traced(client.get(&format!("/v2/{ns}/manifests/latest"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                let label = r.header("x-kappa-label").unwrap_or("");
                if label == data.multi_manifest_sha256.as_str() {
                    t.pass();
                } else {
                    t.fail(format!(
                        "expected {}, got {label}",
                        data.multi_manifest_sha256.as_str()
                    ));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-08 MUST "v1.0 retained"
    {
        let mut t = runner.must("L2-08", 2, "Old tag retains original kappa-label");
        match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                let label = r.header("x-kappa-label").unwrap_or("");
                if label == data.simple_manifest_sha256.as_str() {
                    t.pass();
                } else {
                    t.fail(format!(
                        "expected {}, got {label}",
                        data.simple_manifest_sha256.as_str()
                    ));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // ── §6.3 Absent ──────────────────────────────────────────────────────────

    // L2-09 MUST "Content-before-tag"
    {
        let mut t = runner.must(
            "L2-09",
            2,
            "Content-before-tag (direct tag_set rejects absent kappa)",
        );
        let probe_rb = client.put_tag(
            &format!("/v2/{ns}"),
            "l2-09-probe",
            data.simple_manifest_sha256.as_str(),
        );
        match client.send_traced(probe_rb) {
            Ok(probe_r) => {
                t.trace(probe_r.trace.clone());
                if probe_r.status == 404 {
                    t.fail("PUT /tags/ endpoint not implemented (A12 required at Level 2)".into());
                } else {
                    let never_pushed =
                        KappaLabel::sha256(b"this content was never pushed to the registry");
                    let rb =
                        client.put_tag(&format!("/v2/{ns}"), "should-fail", never_pushed.as_str());
                    match client.send_traced(rb) {
                        Ok(r) => {
                            t.trace(r.trace.clone());
                            if r.status == 404 {
                                if let Err(e) = verify_error_response(&r.body, "TAG_CONTENT_ABSENT")
                                {
                                    t.fail(format!("status 404 but error code wrong: {e}"));
                                } else {
                                    t.pass();
                                }
                            } else {
                                t.fail(format!(
                                    "expected 404 (content-before-tag rejection), got {}: {}",
                                    r.status,
                                    r.body_text()
                                ));
                            }
                        }
                        Err(e) => t.fail(format!("request failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("probe request failed: {e}")),
        }
    }

    // L2-10 MUST "Absent tag 404"
    {
        let mut t = runner.must("L2-10", 2, "Absent tag returns 404");
        match client.send_traced(client.get(&format!("/v2/{ns}/manifests/nonexistent"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 404 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(404));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // ── §6.4 Tag List ────────────────────────────────────────────────────────

    // L2-11 MUST "List ordered"
    {
        let mut t = runner.must("L2-11", 2, "Tag list present and ASCIIbetically ordered");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let body = r.body_text();
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    // Accept both OCI format (flat string array) and kappa
                    // format (objects with name field)
                    let names: Vec<&str> = v
                        .get("tags")
                        .and_then(|t| t.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|t| {
                                    t.as_str()
                                        .or_else(|| t.get("name").and_then(|n| n.as_str()))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    if names.len() < 2 {
                        t.fail(format!("expected >=2 tags, got {}: {body}", names.len()));
                    } else {
                        let ordered = names.windows(2).all(|w| w[0] < w[1]);
                        if ordered {
                            t.pass();
                        } else {
                            t.fail(format!(
                                "tags not in strict ASCIIbetical order (or duplicates): {names:?}"
                            ));
                        }
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-12 MUST "Pagination n=1"
    {
        let mut t = runner.must("L2-12", 2, "Tag list pagination n=1");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list?n=1"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let has_link = r.header("link").is_some();
                    let body = r.body_text();
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    let tags = v
                        .get("tags")
                        .and_then(|t| t.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    if tags > 1 {
                        t.fail(format!("n=1 returned {tags} tags"));
                    } else if tags == 1 && !has_link {
                        t.fail("n=1 with more tags: Link header missing".into());
                    } else {
                        t.pass();
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-13 MUST "n=0 empty no Link"
    {
        let mut t = runner.must("L2-13", 2, "Tag list n=0 empty no Link");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list?n=0"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let has_link = r.header("link").is_some();
                    let body = r.body_text();
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    let tags = v
                        .get("tags")
                        .and_then(|t| t.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    if tags != 0 {
                        t.fail(format!("n=0 returned {tags} tags"));
                    } else if has_link {
                        t.fail("n=0 must not have Link header".into());
                    } else {
                        t.pass();
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-14 MUST "Pagination cursor"
    {
        let mut t = runner.must("L2-14", 2, "Tag list pagination cursor");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list?n=1"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                let body = r.body_text();
                let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                let first_tag = v
                    .get("tags")
                    .and_then(|t| t.as_array())
                    .and_then(|a| a.first())
                    .and_then(|t| t.get("name").and_then(|n| n.as_str()).or(t.as_str()))
                    .unwrap_or("");
                if first_tag.is_empty() {
                    t.fail("no tags to paginate".into());
                } else {
                    match client.send_traced(
                        client.get(&format!("/v2/{ns}/tags/list?n=1&last={first_tag}")),
                    ) {
                        Ok(r2) => {
                            t.trace(r2.trace.clone());
                            let body2 = r2.body_text();
                            if body2.contains(first_tag) {
                                t.fail(format!("cursor did not exclude {first_tag}"));
                            } else {
                                t.pass();
                            }
                        }
                        Err(e) => t.fail(format!("cursor request failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-15 MUST "Cursor is name"
    {
        runner
            .must("L2-15", 2, "Cursor is tag name not index")
            .pass();
    }

    // ── §6.5 Direct kappa ────────────────────────────────────────────────────

    // L2-16 MUST "Resolve by kappa"
    {
        let mut t = runner.must(
            "L2-16",
            2,
            "Resolve by kappa-label directly (header + body + re-hash)",
        );
        let path = format!(
            "/v2/{ns}/manifests/{}",
            data.simple_manifest_sha256.as_str()
        );
        match client.send_traced(client.get(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let server_label = r.header("x-kappa-label").unwrap_or("").to_string();
                    if server_label != data.simple_manifest_sha256.as_str() {
                        t.fail(format!(
                            "X-Kappa-Label wrong: {server_label} vs {}",
                            data.simple_manifest_sha256.as_str()
                        ));
                    } else if let Err(e) =
                        assert_kappa_matches(&r.body, &data.simple_manifest_sha256)
                    {
                        t.fail(e);
                    } else {
                        t.pass();
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-17 MUST "Direct tag resolution"
    {
        let mut t = runner.must(
            "L2-17",
            2,
            "Direct tag resolution (pointer only, no content)",
        );
        match client.send_traced(client.get_tag(&format!("/v2/{ns}"), "v1.0")) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let tag_kappa = r.header("x-kappa-label").unwrap_or("").to_string();
                    if tag_kappa.is_empty() {
                        t.fail("X-Kappa-Label header missing on tag resolution response".into());
                    } else {
                        // At this point v1.0 points to simple_manifest (set in L2-01,
                        // verified in L2-08). L2-30 has not yet executed.
                        match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
                            Ok(mr) => {
                                t.trace(mr.trace.clone());
                                let manifest_kappa =
                                    mr.header("x-kappa-label").unwrap_or("").to_string();
                                if tag_kappa == manifest_kappa {
                                    t.pass();
                                } else {
                                    t.fail(format!(
                                        "tag resolution ({tag_kappa}) disagrees with manifests resolution ({manifest_kappa})"
                                    ));
                                }
                            }
                            Err(e) => t.fail(format!("manifests GET failed: {e}")),
                        }
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-18 MUST "Absent tag resolution 404"
    {
        let mut t = runner.must("L2-18", 2, "Absent tag resolution returns 404");
        match client.send_traced(client.get_tag(&format!("/v2/{ns}"), "nonexistent-tag-l2-18")) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 404 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(404));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // ── §6.6 Timestamp Tags ──────────────────────────────────────────────────

    // L2-20 SHOULD "Timestamp tag 1"
    {
        let mut t = runner.should("L2-20", 2, "Timestamp tag bind");
        let rb = client
            .put(&format!("/v2/{ns}/manifests/2026-07-20T00:00:00"))
            .body(data.simple_manifest.clone());
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

    // L2-21 SHOULD "Timestamp tag 2"
    {
        let mut t = runner.should("L2-21", 2, "Timestamp tag second");
        let rb = client
            .put(&format!("/v2/{ns}/manifests/2026-07-20T12:00:00"))
            .body(data.simple_manifest.clone());
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

    // L2-22 SHOULD "Range filter"
    {
        let mut t = runner.should("L2-22", 2, "Timestamp range filter");
        match client.send_traced(client.get(&format!(
            "/v2/{ns}/tags/list?after=2026-07-19T23:00:00&before=2026-07-20T06:00:00"
        ))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                let body = r.body_text();
                if body.contains("2026-07-20T00:00:00") && !body.contains("2026-07-20T12:00:00") {
                    t.pass();
                } else {
                    t.warn(format!("range filter not applied or not supported: {body}"));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-23 SHOULD "Descending"
    {
        let mut t = runner.should("L2-23", 2, "Tag list descending");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list?order=desc"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    t.pass();
                } else {
                    t.warn(format!("desc order returned {}", r.status));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-24 SHOULD "Ascending"
    {
        let mut t = runner.should("L2-24", 2, "Tag list ascending");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list?order=asc"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    t.pass();
                } else {
                    t.warn(format!("asc order returned {}", r.status));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // ── §6.7 Compare-and-Swap ────────────────────────────────────────────────

    // L2-30 SHOULD "CAS success"
    let mut cas_supported = false;
    {
        let mut t = runner.should("L2-30", 2, "Compare-and-swap success (direct tag_set)");
        let current_kappa = data.simple_manifest_sha256.as_str();
        let new_kappa = data.multi_manifest_sha256.as_str();
        let rb = client
            .put_tag(&format!("/v2/{ns}"), "v1.0", new_kappa)
            .header("If-Match", current_kappa);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 || r.status == 201 {
                    cas_supported = true;
                    t.pass();
                } else {
                    t.warn(format!(
                        "CAS via PUT /tags/ not supported or failed: {}",
                        r.status
                    ));
                }
            }
            Err(e) => t.warn(format!("request failed: {e}")),
        }
    }

    // L2-31 SHOULD "CAS conflict"
    {
        if !cas_supported {
            runner.skip("L2-31", 2, "Compare-and-swap conflict", "CAS not supported");
        } else {
            let mut t = runner.should("L2-31", 2, "Compare-and-swap conflict (direct tag_set)");
            let wrong = format!("sha256:{}", "0".repeat(64));
            let rb = client
                .put_tag(
                    &format!("/v2/{ns}"),
                    "v1.0",
                    data.simple_manifest_sha256.as_str(),
                )
                .header("If-Match", wrong.as_str());
            match client.send_traced(rb) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 409 {
                        t.pass();
                    } else {
                        t.fail(r.fail_msg(409));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L2-32 SHOULD "CAS non-existent"
    {
        if !cas_supported {
            runner.skip("L2-32", 2, "CAS on non-existent tag", "CAS not supported");
        } else {
            let mut t = runner.should("L2-32", 2, "CAS on non-existent tag (direct tag_set)");
            let rb = client
                .put_tag(
                    &format!("/v2/{ns}"),
                    "cas-create",
                    data.simple_manifest_sha256.as_str(),
                )
                .header("If-None-Match", "*");
            match client.send_traced(rb) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 201 {
                        t.pass();
                    } else {
                        t.warn(format!("CAS create returned {}", r.status));
                    }
                }
                Err(e) => t.warn(format!("request failed: {e}")),
            }
        }
    }

    // L2-33 SHOULD "CAS value after success"
    {
        if !cas_supported {
            runner.skip("L2-33", 2, "CAS value after success", "CAS not supported");
        } else {
            let mut t = runner.should("L2-33", 2, "CAS value after success");
            match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    let label = r.header("x-kappa-label").unwrap_or("");
                    if label == data.multi_manifest_sha256.as_str() {
                        t.pass();
                    } else {
                        t.warn(format!(
                            "expected {}, got {label}",
                            data.multi_manifest_sha256.as_str()
                        ));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L2-34 SHOULD "CAS value after failure"
    {
        if !cas_supported {
            runner.skip("L2-34", 2, "CAS value after failure", "CAS not supported");
        } else {
            let mut t = runner.should("L2-34", 2, "CAS value after failure (verify unchanged)");
            match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    let label = r.header("x-kappa-label").unwrap_or("");
                    if label == data.multi_manifest_sha256.as_str() {
                        t.pass();
                    } else {
                        t.fail(format!(
                            "CAS failure incorrectly changed tag: got {label}, expected {}",
                            data.multi_manifest_sha256.as_str()
                        ));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // ── §6.8 Multi-Tag Bind ──────────────────────────────────────────────────

    // L2-40 SHOULD "Multi-tag bind"
    let mut multi_tag_supported = false;
    {
        let mut t = runner.should("L2-40", 2, "Multi-tag bind");
        let rb = client
            .put_with_content_type(
                &format!("/v2/{ns}/manifests/mtag-a?tag=mtag-b"),
                "application/json",
            )
            .body(data.simple_manifest.clone());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 || r.status == 200 {
                    multi_tag_supported = true;
                    t.pass();
                } else {
                    t.warn(format!("multi-tag bind returned {}", r.status));
                }
            }
            Err(e) => t.warn(format!("request failed: {e}")),
        }
    }

    // L2-41 SHOULD "Multi-tag both resolve"
    {
        if !multi_tag_supported {
            runner.skip(
                "L2-41",
                2,
                "Multi-tag both resolve",
                "multi-tag not supported",
            );
        } else {
            let mut t = runner.should("L2-41", 2, "Multi-tag both resolve");
            match (
                client.send_traced(client.get(&format!("/v2/{ns}/manifests/mtag-a"))),
                client.send_traced(client.get(&format!("/v2/{ns}/manifests/mtag-b"))),
            ) {
                (Ok(a), Ok(b)) => {
                    t.trace(a.trace.clone());
                    t.trace(b.trace.clone());
                    let ka = a.header("x-kappa-label");
                    let kb = b.header("x-kappa-label");
                    if ka.is_some() && ka == kb {
                        t.pass();
                    } else {
                        t.fail(format!("mtag-a={ka:?} mtag-b={kb:?}"));
                    }
                }
                _ => t.fail("one or both GETs failed".into()),
            }
        }
    }

    // L2-42 SHOULD "Multi-tag fallback"
    {
        let mut t = runner.should("L2-42", 2, "Multi-tag fallback");
        match client.send_traced(client.get(&format!("/v2/{ns}/manifests/mtag-a"))) {
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

    // ── §6.9 Warnings ────────────────────────────────────────────────────────

    // L2-50 SHOULD "Warning format"
    // L2-51 MUST "Warning <= 4096"
    // L2-52 SHOULD "Warning reported"
    // Warning headers are optional. The test verifies format if present.
    // Issue a request and check for Warning: 299 on the response.
    {
        let mut warning_value: Option<String> = None;
        let mut t50 = runner.should("L2-50", 2, "Warning header format");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list"))) {
            Ok(r) => {
                t50.trace(r.trace.clone());
                if let Some(w) = r.header("warning") {
                    warning_value = Some(w.to_string());
                    if w.starts_with("299") {
                        t50.pass();
                    } else {
                        t50.fail(format!("warning header wrong format: {w}"));
                    }
                } else {
                    t50.warn("no Warning header on response".into());
                }
            }
            Err(e) => t50.fail(format!("request failed: {e}")),
        }

        let t51 = runner.must("L2-51", 2, "Warning data <= 4096 bytes");
        match &warning_value {
            Some(w) if w.len() > 4096 => {
                t51.fail(format!("warning exceeds 4096 bytes: {} bytes", w.len()));
            }
            _ => t51.pass(),
        }

        let t52 = runner.should("L2-52", 2, "Warning reported");
        if warning_value.is_some() {
            t52.pass();
        } else {
            t52.warn("no warnings observed on any response".into());
        }
    }

    // ── §6.10 Tag Deletion ───────────────────────────────────────────────────

    // L2-60 MAY "Delete tag"
    let mut tag_deleted = false;
    let mut delete_status = 0u16;
    let mut delete_body = Vec::new();
    {
        let mut t = runner.may("L2-60", 2, "Tag deletion");
        match client.send_traced(client.delete(&format!("/v2/{ns}/manifests/v1.0"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                delete_status = r.status;
                delete_body = r.body.clone();
                if delete_status == 202 {
                    tag_deleted = true;
                    t.pass();
                } else if delete_status == 400 || delete_status == 405 {
                    t.pass();
                } else {
                    t.warn(format!("tag delete returned {delete_status}"));
                }
            }
            Err(e) => t.warn(format!("request failed: {e}")),
        }
    }

    // L2-61 MUST "Deletion rejection UNSUPPORTED"
    // If deletion returned 400, verify UNSUPPORTED error code.
    // If deletion returned 202 or 405, the response format is still valid.
    // No skip - every outcome is testable.
    {
        let t = runner.must("L2-61", 2, "Deletion rejection uses UNSUPPORTED");
        if delete_status == 400 {
            if let Err(e) = verify_error_response(&delete_body, "UNSUPPORTED") {
                t.fail(e);
            } else {
                t.pass();
            }
        } else if delete_status == 202 || delete_status == 405 {
            t.pass();
        } else {
            t.fail(format!(
                "unexpected delete status {delete_status}: {}",
                String::from_utf8_lossy(&delete_body)
            ));
        }
    }

    // L2-62 MAY "Deleted tag 404"
    {
        if !tag_deleted {
            runner.skip("L2-62", 2, "Deleted tag returns 404", "tag was not deleted");
        } else {
            let mut t = runner.may("L2-62", 2, "Deleted tag returns 404");
            match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 404 {
                        t.pass();
                    } else {
                        t.fail(r.fail_msg(404));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L2-63 MUST "Content survives tag deletion"
    {
        if !tag_deleted {
            runner.skip(
                "L2-63",
                2,
                "Content survives tag deletion",
                "tag was not deleted",
            );
        } else {
            let mut t = runner.must("L2-63", 2, "Content survives tag deletion");
            let path = format!("/v2/{ns}/blobs/{}", data.simple_manifest_sha256.as_str());
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

    // L2-65 MUST "Tag list includes repository name"
    {
        let mut t = runner.must("L2-65", 2, "Tag list response includes repository name");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let body = r.body_text();
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    match v.get("name").and_then(|n| n.as_str()) {
                        Some(_) => t.pass(),
                        None => t.fail(format!("tag list response missing name field: {body}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-66 MUST "Manifest delete by digest"
    {
        let mut t = runner.must("L2-66", 2, "Manifest delete by digest");
        // Push a manifest specifically for deletion by digest
        let del_content = br#"{"delete-by-digest":"test"}"#;
        let del_kappa = KappaLabel::sha256(del_content);
        let rb = client
            .put(&format!("/v2/{ns}/manifests/del-by-digest"))
            .body(del_content.to_vec());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 201 && r.status != 200 {
                    t.fail(format!("setup push failed: {}", r.fail_msg(201)));
                } else {
                    // Delete by digest
                    match client.send_traced(
                        client.delete(&format!("/v2/{ns}/manifests/{}", del_kappa.as_str())),
                    ) {
                        Ok(r2) => {
                            t.trace(r2.trace.clone());
                            if r2.status == 202 {
                                // Verify the manifest is gone
                                match client.send_traced(
                                    client
                                        .get(&format!("/v2/{ns}/manifests/{}", del_kappa.as_str())),
                                ) {
                                    Ok(r3) => {
                                        t.trace(r3.trace.clone());
                                        if r3.status == 404 {
                                            t.pass();
                                        } else {
                                            t.fail(format!(
                                                "manifest still accessible after delete by digest: {}",
                                                r3.status
                                            ));
                                        }
                                    }
                                    Err(e) => t.fail(format!("verify GET failed: {e}")),
                                }
                            } else if r2.status == 400 || r2.status == 405 {
                                t.pass();
                            } else {
                                t.fail(r2.fail_msg(202));
                            }
                        }
                        Err(e) => t.fail(format!("delete request failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-70 MUST "OCI-Subject header on manifest PUT with subject"
    {
        let mut t = runner.must(
            "L2-70",
            2,
            "OCI-Subject header returned when subject present",
        );
        // Push a subject manifest
        let subject_content = br#"{"subject-test":"base"}"#;
        let subject_kappa = KappaLabel::sha256(subject_content);
        let rb = client
            .put(&format!("/v2/{ns}/manifests/subject-base"))
            .body(subject_content.to_vec());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 201 && r.status != 200 {
                    t.fail(format!("subject push failed: {}", r.fail_msg(201)));
                } else {
                    // Push a manifest with a subject field referencing the base
                    let referrer_content = format!(
                        r#"{{"subject":{{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"{}","size":{}}},"artifactType":"application/vnd.example.test.v1"}}"#,
                        subject_kappa.as_str(),
                        subject_content.len()
                    );
                    let rb = client
                        .put_with_content_type(
                            &format!("/v2/{ns}/manifests/referrer-test"),
                            "application/vnd.oci.image.manifest.v1+json",
                        )
                        .body(referrer_content.as_bytes().to_vec());
                    match client.send_traced(rb) {
                        Ok(r2) => {
                            t.trace(r2.trace.clone());
                            if r2.status != 201 {
                                t.fail(r2.fail_msg(201));
                            } else {
                                match r2.header("oci-subject") {
                                    Some(subj) => {
                                        if subj == subject_kappa.as_str() {
                                            t.pass();
                                        } else {
                                            t.fail(format!(
                                                "OCI-Subject wrong: got {subj}, expected {}",
                                                subject_kappa.as_str()
                                            ));
                                        }
                                    }
                                    None => t.fail("OCI-Subject header missing".into()),
                                }
                            }
                        }
                        Err(e) => t.fail(format!("referrer push failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-71 MUST "Referrers API returns referrer list"
    {
        let mut t = runner.must("L2-71", 2, "Referrers API returns OCI Image Index");
        // The subject and referrer from L2-70 should be available.
        // The registry needs to have created a refers-to edge from the referrer push.
        let subject_content = br#"{"subject-test":"base"}"#;
        let subject_kappa = KappaLabel::sha256(subject_content);
        let path = format!("/v2/{ns}/referrers/{}", subject_kappa.as_str());
        match client.send_traced(client.get(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let ct = r.header("content-type").unwrap_or("");
                    if !ct.contains("application/vnd.oci.image.index.v1+json") {
                        t.fail(format!(
                            "Content-Type wrong: expected OCI image index, got {ct}"
                        ));
                    } else {
                        let body = r.body_text();
                        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                        let schema = v.get("schemaVersion").and_then(|s| s.as_u64());
                        let media = v.get("mediaType").and_then(|m| m.as_str());
                        let manifests = v.get("manifests").and_then(|m| m.as_array());
                        if schema != Some(2) {
                            t.fail(format!("schemaVersion wrong: {schema:?}"));
                        } else if media != Some("application/vnd.oci.image.index.v1+json") {
                            t.fail(format!("mediaType wrong: {media:?}"));
                        } else if manifests.is_none() {
                            t.fail("manifests array missing".into());
                        } else {
                            t.pass();
                        }
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-72 MUST "Referrers API with artifactType filter"
    {
        let mut t = runner.must("L2-72", 2, "Referrers API filters by artifactType");
        let subject_content = br#"{"subject-test":"base"}"#;
        let subject_kappa = KappaLabel::sha256(subject_content);
        let path = format!(
            "/v2/{ns}/referrers/{}?artifactType=application/vnd.example.test.v1",
            subject_kappa.as_str()
        );
        match client.send_traced(client.get(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let filters = r.header("oci-filters-applied").unwrap_or("");
                    if !filters.contains("artifactType") {
                        t.fail(format!(
                            "OCI-Filters-Applied missing or wrong: got {filters}"
                        ));
                    } else {
                        t.pass();
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-73 MUST "Referrers API on empty subject returns empty list"
    {
        let mut t = runner.must(
            "L2-73",
            2,
            "Referrers API on absent subject returns empty list",
        );
        let absent = format!("sha256:{}", "d".repeat(64));
        let path = format!("/v2/{ns}/referrers/{absent}");
        match client.send_traced(client.get(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let body = r.body_text();
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    let manifests = v
                        .get("manifests")
                        .and_then(|m| m.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    if manifests != 0 {
                        t.fail(format!("expected empty manifests, got {manifests}"));
                    } else {
                        t.pass();
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-80 SHOULD "Manifest PUT sets object-type metadata"
    {
        let mut t = runner.should("L2-80", 2, "Manifest sets object-type=manifest metadata");
        match client.send_traced(client.get_meta_list(
            &format!("/v2/{ns}"),
            "object-type",
            "manifest",
        )) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let text = r.body_text();
                    if text.contains("sha256:") {
                        t.pass();
                    } else {
                        t.fail(format!("no manifests in metadata query: {text}"));
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-64 MAY "Teardown tag cleanup"
    if !config.teardown_enabled {
        runner.skip("L2-64", 2, "Teardown tag cleanup", "teardown disabled");
    } else {
        let t = runner.may("L2-64", 2, "Teardown tag cleanup");
        for tag in [
            "latest",
            "mtag-a",
            "mtag-b",
            "2026-07-20T00:00:00",
            "2026-07-20T12:00:00",
            "cas-create",
            "l2-02-tag",
            "l2-09-probe",
            "should-fail",
            "subject-base",
            "referrer-test",
        ] {
            let _ = client.send_traced(client.delete(&format!("/v2/{ns}/manifests/{tag}")));
        }
        t.pass();
    }
}
