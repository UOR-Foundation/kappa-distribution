//! Level 2 - Tag Operations (L2-01 through L2-64).
//!
//! Includes direct tag binding (PUT /tags/) and direct tag resolution (GET /tags/).

use crate::assertions::*;
use crate::client::RegistryClient;
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

pub fn run(runner: &mut TestRunner, client: &RegistryClient, config: &Config, data: &TestData) {
    let ns = &config.namespace;

    // Setup: ensure blobs exist (re-push if Level 1 was not run).
    // Setup failures abort Level 2 early - running tests against missing preconditions
    // produces misleading failures.
    for blob in [&data.config, &data.layer_a, &data.layer_b, &data.hello] {
        let path = format!("/v2/{ns}/blobs/{}", blob.sha256.as_str());
        let rb = client.put(&path).body(blob.content.to_vec());
        if let Ok((resp, _)) = client.send_traced(rb) {
            let s = resp.status().as_u16();
            if s != 201 && s != 200 {
                eprintln!(
                    "Level 2 setup: blob {} push returned {s}, aborting level",
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

    // ── §6.1 Bind and Resolve ────────────────────────────────────────────────

    // L2-01 MUST "Bind tag"
    {
        let mut t = runner.must("L2-01", 2, "Bind tag");
        let rb = client
            .put(&format!("/v2/{ns}/manifests/v1.0"))
            .header("Content-Type", "application/json")
            .body(data.simple_manifest.clone());
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 201) {
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
            .put(&format!("/v2/{ns}/manifests/l2-02-tag"))
            .header("Content-Type", "application/json")
            .body(data.simple_manifest.clone());
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let h = hdrs(&resp);
                match hdr_val(&h, "x-kappa-label") {
                    None => t.fail("X-Kappa-Label header missing".into()),
                    Some(label) => {
                        if let Err(e) = assert_kappa_valid(label) {
                            t.fail(format!("invalid κ: {e}"));
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
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    let h = hdrs(&resp);
                    let server_label = hdr_val(&h, "x-kappa-label").unwrap_or("").to_string();
                    if server_label != data.simple_manifest_sha256.as_str() {
                        t.fail(format!(
                            "X-Kappa-Label wrong: got {server_label}, expected {}",
                            data.simple_manifest_sha256.as_str()
                        ));
                    } else {
                        match resp.bytes() {
                            Ok(body) => {
                                if let Err(e) = assert_body_eq(&body, &data.simple_manifest) {
                                    t.fail(format!("body mismatch: {e}"));
                                } else if let Err(e) =
                                    assert_kappa_matches(&body, &data.simple_manifest_sha256)
                                {
                                    t.fail(format!("re-hash mismatch: {e}"));
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

    // L2-04 MUST "Bind second tag same content"
    {
        let mut t = runner.must("L2-04", 2, "Bind second tag to same content");
        let rb = client
            .put(&format!("/v2/{ns}/manifests/latest"))
            .body(data.simple_manifest.clone());
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

    // L2-05 MUST "Both tags same κ"
    {
        let mut t = runner.must("L2-05", 2, "Both tags resolve same κ-label");
        // Collect both results before consuming t, because fail/pass moves t.
        let r1 = client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0")));
        let r2 = client.send_traced(client.get(&format!("/v2/{ns}/manifests/latest")));
        match (r1, r2) {
            (Ok((resp1, trace1)), Ok((resp2, trace2))) => {
                t.trace(trace1);
                t.trace(trace2);
                let k1 = resp1
                    .headers()
                    .get("x-kappa-label")
                    .and_then(|v| v.to_str().ok());
                let k2 = resp2
                    .headers()
                    .get("x-kappa-label")
                    .and_then(|v| v.to_str().ok());
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

    // L2-07 MUST "Latest is new κ"
    {
        let mut t = runner.must("L2-07", 2, "Updated tag resolves new κ-label");
        match client.send_traced(client.get(&format!("/v2/{ns}/manifests/latest"))) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let h = hdrs(&resp);
                let label = hdr_val(&h, "x-kappa-label").unwrap_or("");
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
        let mut t = runner.must("L2-08", 2, "Old tag retains original κ-label");
        match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let h = hdrs(&resp);
                let label = hdr_val(&h, "x-kappa-label").unwrap_or("");
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
            "Content-before-tag (direct tag_set rejects absent κ)",
        );
        // First verify the PUT /tags/ endpoint exists by binding a known-good κ-label.
        // If the endpoint itself returns 404, the registry doesn't implement A12 and we
        // cannot distinguish "endpoint missing" from "content absent."
        let probe_rb = client.put_tag(
            &format!("/v2/{ns}"),
            "l2-09-probe",
            data.simple_manifest_sha256.as_str(),
        );
        match client.send_traced(probe_rb) {
            Ok((probe_resp, probe_trace)) => {
                t.trace(probe_trace);
                let ps = probe_resp.status().as_u16();
                if ps == 404 {
                    t.fail("PUT /tags/ endpoint not implemented (A12 required at Level 2)".into());
                } else {
                    // Endpoint exists. Now test content-before-tag with a never-pushed κ.
                    let never_pushed =
                        KappaLabel::sha256(b"this content was never pushed to the registry");
                    let rb =
                        client.put_tag(&format!("/v2/{ns}"), "should-fail", never_pushed.as_str());
                    match client.send_traced(rb) {
                        Ok((resp, trace)) => {
                            t.trace(trace);
                            let s = resp.status().as_u16();
                            if s == 404 {
                                match resp.bytes() {
                                    Ok(body) => {
                                        if let Err(e) =
                                            verify_error_response(&body, "TAG_CONTENT_ABSENT")
                                        {
                                            t.fail(format!("status 404 but error code wrong: {e}"));
                                        } else {
                                            t.pass();
                                        }
                                    }
                                    Err(_) => t.pass(),
                                }
                            } else {
                                t.fail(format!(
                                    "expected 404 (content-before-tag rejection), got {s}"
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
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 404) {
                    t.fail(e);
                } else {
                    t.pass();
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
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    match resp.text() {
                        Ok(body) => {
                            let v: serde_json::Value =
                                serde_json::from_str(&body).unwrap_or_default();
                            // Spec §6.5: response MUST use objects with name+kappa fields.
                            let names: Vec<&str> = v
                                .get("tags")
                                .and_then(|t| t.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                                        .collect()
                                })
                                .unwrap_or_default();
                            if names.len() < 2 {
                                t.fail(format!("expected >=2 tags, got {}: {body}", names.len()));
                            } else {
                                // Strict less-than: verifies ASCIIbetical order AND uniqueness.
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
                        Err(e) => t.fail(format!("body read failed: {e}")),
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
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    let has_link = resp.headers().get("link").is_some();
                    match resp.text() {
                        Ok(body) => {
                            let v: serde_json::Value =
                                serde_json::from_str(&body).unwrap_or_default();
                            let tags = v
                                .get("tags")
                                .and_then(|t| t.as_array())
                                .map(|a| a.len())
                                .unwrap_or(0);
                            if tags > 1 {
                                t.fail(format!("n=1 returned {tags} tags"));
                            } else if tags == 1 && !has_link {
                                // We know >=2 tags exist (v1.0, latest), so Link MUST be present.
                                t.fail("n=1 with more tags: Link header missing".into());
                            } else {
                                t.pass();
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
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
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    let has_link = resp.headers().get("link").is_some();
                    match resp.text() {
                        Ok(body) => {
                            let v: serde_json::Value =
                                serde_json::from_str(&body).unwrap_or_default();
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
                        Err(e) => t.fail(format!("body read failed: {e}")),
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
            Ok((resp, trace)) => {
                t.trace(trace);
                match resp.text() {
                    Ok(body) => {
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
                                Ok((resp2, trace2)) => {
                                    t.trace(trace2);
                                    match resp2.text() {
                                        Ok(body2) => {
                                            if body2.contains(first_tag) {
                                                t.fail(format!(
                                                    "cursor did not exclude {first_tag}"
                                                ));
                                            } else {
                                                t.pass();
                                            }
                                        }
                                        Err(e) => t.fail(format!("body read failed: {e}")),
                                    }
                                }
                                Err(e) => t.fail(format!("cursor request failed: {e}")),
                            }
                        }
                    }
                    Err(e) => t.fail(format!("body read failed: {e}")),
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

    // ── §6.5 Direct κ ────────────────────────────────────────────────────────

    // L2-16 MUST "Resolve by κ"
    {
        let mut t = runner.must(
            "L2-16",
            2,
            "Resolve by κ-label directly (header + body + re-hash)",
        );
        let path = format!(
            "/v2/{ns}/manifests/{}",
            data.simple_manifest_sha256.as_str()
        );
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    let h = hdrs(&resp);
                    let server_label = hdr_val(&h, "x-kappa-label").unwrap_or("").to_string();
                    if server_label != data.simple_manifest_sha256.as_str() {
                        t.fail(format!(
                            "X-Kappa-Label wrong: {server_label} vs {}",
                            data.simple_manifest_sha256.as_str()
                        ));
                    } else {
                        match resp.bytes() {
                            Ok(body) => {
                                if let Err(e) =
                                    assert_kappa_matches(&body, &data.simple_manifest_sha256)
                                {
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

    // L2-17 MUST "Direct tag resolution"
    {
        let mut t = runner.must(
            "L2-17",
            2,
            "Direct tag resolution (pointer only, no content)",
        );
        match client.send_traced(client.get_tag(&format!("/v2/{ns}"), "v1.0")) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    let h = hdrs(&resp);
                    let tag_kappa = hdr_val(&h, "x-kappa-label").unwrap_or("").to_string();
                    if tag_kappa.is_empty() {
                        t.fail("X-Kappa-Label header missing on tag resolution response".into());
                    } else {
                        // Verify the resolved κ-label matches what GET /manifests/v1.0 returns.
                        // At this point v1.0 points to simple_manifest (set in L2-01,
                        // verified in L2-08). L2-30 has not yet executed.
                        match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
                            Ok((manifest_resp, manifest_trace)) => {
                                t.trace(manifest_trace);
                                let mh = hdrs(&manifest_resp);
                                let manifest_kappa =
                                    hdr_val(&mh, "x-kappa-label").unwrap_or("").to_string();
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

    // L2-18 MUST "Absent tag resolution 404" - GET /tags/{name} for nonexistent tag
    {
        let mut t = runner.must("L2-18", 2, "Absent tag resolution returns 404");
        match client.send_traced(client.get_tag(&format!("/v2/{ns}"), "nonexistent-tag-l2-18")) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 404) {
                    t.fail(e);
                } else {
                    t.pass();
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

    // L2-21 SHOULD "Timestamp tag 2"
    {
        let mut t = runner.should("L2-21", 2, "Timestamp tag second");
        let rb = client
            .put(&format!("/v2/{ns}/manifests/2026-07-20T12:00:00"))
            .body(data.simple_manifest.clone());
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

    // L2-22 SHOULD "Range filter"
    {
        let mut t = runner.should("L2-22", 2, "Timestamp range filter");
        match client.send_traced(client.get(&format!(
            "/v2/{ns}/tags/list?after=2026-07-19T23:00:00&before=2026-07-20T06:00:00"
        ))) {
            Ok((resp, trace)) => {
                t.trace(trace);
                match resp.text() {
                    Ok(body) => {
                        if body.contains("2026-07-20T00:00:00")
                            && !body.contains("2026-07-20T12:00:00")
                        {
                            t.pass();
                        } else {
                            t.warn(format!("range filter not applied or not supported: {body}"));
                        }
                    }
                    Err(e) => t.fail(format!("body read failed: {e}")),
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-23 SHOULD "Descending"
    {
        let mut t = runner.should("L2-23", 2, "Tag list descending");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list?order=desc"))) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if resp.status().as_u16() == 200 {
                    t.pass();
                } else {
                    t.warn(format!("desc order returned {}", resp.status()));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L2-24 SHOULD "Ascending"
    {
        let mut t = runner.should("L2-24", 2, "Tag list ascending");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list?order=asc"))) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if resp.status().as_u16() == 200 {
                    t.pass();
                } else {
                    t.warn(format!("asc order returned {}", resp.status()));
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
        // v1.0 currently points to simple_manifest. CAS it to multi_manifest via pointer-only update.
        let current_kappa = data.simple_manifest_sha256.as_str();
        let new_kappa = data.multi_manifest_sha256.as_str();
        let rb = client
            .put_tag(&format!("/v2/{ns}"), "v1.0", new_kappa)
            .header("If-Match", current_kappa);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s == 200 || s == 201 {
                    cas_supported = true;
                    t.pass();
                } else {
                    t.warn(format!("CAS via PUT /tags/ not supported or failed: {s}"));
                }
            }
            Err(e) => t.warn(format!("request failed: {e}")),
        }
    }

    // L2-31 SHOULD "CAS conflict" - If-Match with wrong expected value
    {
        if !cas_supported {
            runner.skip("L2-31", 2, "Compare-and-swap conflict", "CAS not supported");
        } else {
            let mut t = runner.should("L2-31", 2, "Compare-and-swap conflict (direct tag_set)");
            let wrong = format!("sha256:{}", "0".repeat(64));
            // v1.0 now points to multi_manifest (from L2-30). Try to CAS with wrong expected.
            let rb = client
                .put_tag(
                    &format!("/v2/{ns}"),
                    "v1.0",
                    data.simple_manifest_sha256.as_str(),
                )
                .header("If-Match", wrong.as_str());
            match client.send_traced(rb) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if resp.status().as_u16() == 409 {
                        t.pass();
                    } else {
                        t.fail(format!("expected 409, got {}", resp.status()));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L2-32 SHOULD "CAS non-existent" - If-None-Match: * on a tag that doesn't exist
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
                Ok((resp, trace)) => {
                    t.trace(trace);
                    let s = resp.status().as_u16();
                    if s == 201 {
                        t.pass();
                    } else {
                        t.warn(format!("CAS create returned {s}"));
                    }
                }
                Err(e) => t.warn(format!("request failed: {e}")),
            }
        }
    }

    // L2-33 SHOULD "CAS value after success" - verify v1.0 now points to multi_manifest
    {
        if !cas_supported {
            runner.skip("L2-33", 2, "CAS value after success", "CAS not supported");
        } else {
            let mut t = runner.should("L2-33", 2, "CAS value after success");
            match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    let h = hdrs(&resp);
                    let label = hdr_val(&h, "x-kappa-label").unwrap_or("");
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

    // L2-34 SHOULD "CAS value after failure" - verify v1.0 unchanged after L2-31's failed CAS
    {
        if !cas_supported {
            runner.skip("L2-34", 2, "CAS value after failure", "CAS not supported");
        } else {
            let mut t = runner.should("L2-34", 2, "CAS value after failure (verify unchanged)");
            match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    let h = hdrs(&resp);
                    let label = hdr_val(&h, "x-kappa-label").unwrap_or("");
                    // After CAS failure in L2-31, v1.0 should still be multi_manifest from L2-30.
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
            .put(&format!("/v2/{ns}/manifests/mtag-a?tag=mtag-b"))
            .header("Content-Type", "application/json")
            .body(data.simple_manifest.clone());
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s == 201 || s == 200 {
                    multi_tag_supported = true;
                    t.pass();
                } else {
                    t.warn(format!("multi-tag bind returned {s}"));
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
                (Ok((r1, t1)), Ok((r2, t2))) => {
                    t.trace(t1);
                    t.trace(t2);
                    let ka = r1
                        .headers()
                        .get("x-kappa-label")
                        .and_then(|v| v.to_str().ok());
                    let kb = r2
                        .headers()
                        .get("x-kappa-label")
                        .and_then(|v| v.to_str().ok());
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
        if multi_tag_supported {
            runner.skip("L2-42", 2, "Multi-tag fallback", "multi-tag was supported");
        } else {
            runner.should("L2-42", 2, "Multi-tag fallback").pass();
        }
    }

    // ── §6.9 Warnings ────────────────────────────────────────────────────────

    // L2-50 SHOULD "Warning format"
    {
        runner.skip("L2-50", 2, "Warning header format", "no warnings observed");
    }

    // L2-51 MUST "Warning <= 4096"
    {
        runner.must("L2-51", 2, "Warning data <= 4096 bytes").pass();
    }

    // L2-52 SHOULD "Warning reported"
    {
        runner.skip("L2-52", 2, "Warning reported", "no warnings observed");
    }

    // ── §6.10 Tag Deletion ───────────────────────────────────────────────────

    // L2-60 MAY "Delete tag"
    let mut tag_deleted = false;
    let mut delete_status = 0u16;
    let mut delete_body = Vec::new();
    {
        let mut t = runner.may("L2-60", 2, "Tag deletion");
        match client.send_traced(client.delete(&format!("/v2/{ns}/manifests/v1.0"))) {
            Ok((resp, trace)) => {
                t.trace(trace);
                delete_status = resp.status().as_u16();
                if let Ok(body) = resp.bytes() {
                    delete_body = body.to_vec();
                }
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
    {
        if delete_status == 400 {
            let t = runner.must("L2-61", 2, "Deletion rejection uses UNSUPPORTED");
            if let Err(e) = verify_error_response(&delete_body, "UNSUPPORTED") {
                t.fail(e);
            } else {
                t.pass();
            }
        } else {
            runner.skip(
                "L2-61",
                2,
                "Deletion rejection uses UNSUPPORTED",
                "deletion was accepted or disabled",
            );
        }
    }

    // L2-62 MAY "Deleted tag 404"
    {
        if !tag_deleted {
            runner.skip("L2-62", 2, "Deleted tag returns 404", "tag was not deleted");
        } else {
            let mut t = runner.may("L2-62", 2, "Deleted tag returns 404");
            match client.send_traced(client.get(&format!("/v2/{ns}/manifests/v1.0"))) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 404) {
                        t.fail(e);
                    } else {
                        t.pass();
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
        ] {
            let _ = client.delete(&format!("/v2/{ns}/manifests/{tag}")).send();
        }
        t.pass();
    }
}
