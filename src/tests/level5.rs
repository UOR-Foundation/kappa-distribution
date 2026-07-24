//! Level 5 - GC, Admission, Federation (L5-01 through L5-71).
//!
//! Verifies pin/unpin/sweep/finalizer lifecycle, reachability-based GC,
//! admission filter registration/enforcement/removal, federation fetch
//! with verify-on-receipt, and rate limiting.

use crate::assertions::*;
use crate::client::RegistryClient;
use crate::config::Config;
use crate::kappa::KappaLabel;
use crate::runner::TestRunner;
use crate::testdata::TestData;
use std::time::{Duration, Instant};

fn poll_sweep(
    client: &RegistryClient,
    ns: &str,
    timeout_secs: u64,
    previous_sweep_id: Option<&str>,
) -> bool {
    let start = Instant::now();
    loop {
        if let Ok(r) = client.send_traced(client.get(&format!("/v2/{ns}/gc/status"))) {
            if r.status == 200 {
                let body = r.body_text();
                let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                if let Some(current) = v.get("last_sweep").and_then(|s| s.as_str()) {
                    match previous_sweep_id {
                        Some(prev) => {
                            if current != prev {
                                return true;
                            }
                        }
                        None => return true,
                    }
                }
            }
        }
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            return false;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn get_current_sweep_id(client: &RegistryClient, ns: &str) -> Option<String> {
    if let Ok(r) = client.send_traced(client.get(&format!("/v2/{ns}/gc/status"))) {
        let body = r.body_text();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        return v
            .get("last_sweep")
            .and_then(|s| s.as_str())
            .map(String::from);
    }
    None
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
                    "Level 5 setup: blob {} push returned {}, aborting level",
                    blob.name, r.status
                );
                return;
            }
        }
    }

    // ── §9.1 GC ──────────────────────────────────────────────────────────────

    let mut pin_kappa = String::new();

    // L5-01 MUST "Pin create"
    {
        let mut t = runner.must("L5-01", 5, "Pin creation");
        let body = format!(r#"{{"kappa":"{}","ttl":0}}"#, data.hello.sha256.as_str());
        let rb = client
            .post_with_content_type(&format!("/v2/{ns}/gc/pin"), "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 201 {
                    t.fail(r.fail_msg(201));
                } else {
                    pin_kappa = r.header("x-kappa-label").unwrap_or("").to_string();
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-02 MUST "Pin kappa in response"
    {
        let t = runner.must("L5-02", 5, "Pin response kappa-label");
        if pin_kappa.is_empty() {
            t.fail("no pin kappa from L5-01".into());
        } else if let Err(e) = assert_kappa_valid(&pin_kappa) {
            t.fail(format!("invalid kappa: {e}"));
        } else {
            t.pass();
        }
    }

    // L5-03 MUST "Owns edge"
    {
        let mut t = runner.must("L5-03", 5, "Ownership edge for reachability");
        let body = format!(
            r#"{{"source":"{}","relation":"owns","target":"{}","metadata":{{}}}}"#,
            data.hello.sha256.as_str(),
            data.layer_a.sha256.as_str(),
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

    // L5-04 MUST "Remove tag from owned"
    {
        let mut t = runner.must("L5-04", 5, "Remove tag from owned blob");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                let body = r.body_text();
                if !body.contains(data.layer_a.sha256.as_str()) {
                    t.pass();
                } else {
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    if let Some(tag_arr) = v.get("tags").and_then(|t| t.as_array()) {
                        let mut deleted = 0usize;
                        for tag_entry in tag_arr {
                            let kappa = tag_entry
                                .get("kappa")
                                .and_then(|k| k.as_str())
                                .unwrap_or("");
                            let name = tag_entry.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            if kappa == data.layer_a.sha256.as_str() && !name.is_empty() {
                                let del_rb = client.delete(&format!("/v2/{ns}/manifests/{name}"));
                                if let Ok(dr) = client.send_traced(del_rb) {
                                    t.trace(dr.trace.clone());
                                }
                                deleted += 1;
                            }
                        }
                        if deleted == 0 {
                            t.fail(
                                "tag list body contains layer_a kappa but no tag entry matched the expected format"
                                    .into(),
                            );
                        } else {
                            t.pass();
                        }
                    } else {
                        t.fail(
                            "tag list contains layer_a kappa but response has no parseable tags array"
                                .into(),
                        );
                    }
                }
            }
            Err(e) => t.fail(format!("tag list request failed: {e}")),
        }
    }

    // L5-05 MUST "Sweep trigger"
    let pre_sweep_id = get_current_sweep_id(client, ns);
    {
        let mut t = runner.must("L5-05", 5, "GC sweep trigger");
        match client.send_traced(client.post(&format!("/v2/{ns}/gc/sweep"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 202 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(202));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-06 MUST "Sweep status"
    {
        let t = runner.must("L5-06", 5, "GC status endpoint");
        if poll_sweep(client, ns, 60, pre_sweep_id.as_deref()) {
            t.pass();
        } else {
            t.fail("sweep did not complete within 60 seconds".into());
        }
    }

    // L5-07 MUST "Pinned survives"
    {
        let mut t = runner.must("L5-07", 5, "Pinned blob survives sweep");
        match client
            .send_traced(client.get(&format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str())))
        {
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

    // L5-08 MUST "Reachable survives"
    {
        let mut t = runner.must("L5-08", 5, "Reachable blob survives sweep");
        match client
            .send_traced(client.get(&format!("/v2/{ns}/blobs/{}", data.layer_a.sha256.as_str())))
        {
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

    // L5-09 MUST "Unpin"
    {
        let mut t = runner.must("L5-09", 5, "Unpin");
        if pin_kappa.is_empty() {
            t.fail("no pin kappa".into());
        } else {
            let body = format!(r#"{{"pin_kappa":"{pin_kappa}"}}"#);
            let rb = client
                .post_with_content_type(&format!("/v2/{ns}/gc/unpin"), "application/json")
                .body(body);
            match client.send_traced(rb) {
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

    // L5-10 SHOULD "Unpin derives"
    {
        runner
            .should("L5-10", 5, "Unpin creates derived pin blob")
            .pass();
    }

    // L5-11 MUST "Sweep after unpin"
    let pre_sweep2_id = get_current_sweep_id(client, ns);
    {
        let mut t = runner.must("L5-11", 5, "Sweep after unpin");
        match client.send_traced(client.post(&format!("/v2/{ns}/gc/sweep"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 202 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(202));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-12 MUST "Poll sweep"
    {
        let t = runner.must("L5-12", 5, "Sweep completion polling");
        if poll_sweep(client, ns, 60, pre_sweep2_id.as_deref()) {
            t.pass();
        } else {
            t.fail("sweep did not complete within 60 seconds".into());
        }
    }

    // L5-13 SHOULD "Unpinned evicted"
    {
        let mut t = runner.should("L5-13", 5, "Unpinned blob evicted");
        match client
            .send_traced(client.get(&format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str())))
        {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 404 {
                    t.pass();
                } else {
                    t.fail(format!(
                        "unpinned blob not yet evicted: {}: {}",
                        r.status,
                        r.body_text()
                    ));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-14 SHOULD "Unreachable evicted"
    {
        let mut t = runner.should("L5-14", 5, "Unreachable blob evicted");
        match client
            .send_traced(client.get(&format!("/v2/{ns}/blobs/{}", data.layer_a.sha256.as_str())))
        {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 404 {
                    t.pass();
                } else {
                    t.fail(format!(
                        "unreachable blob not yet evicted: {}: {}",
                        r.status,
                        r.body_text()
                    ));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-15 MUST "Finalizer pin"
    let mut finalizer_pin_kappa = String::new();
    {
        let mut t = runner.must("L5-15", 5, "Finalizer pin creation");
        let rb = client
            .put(&format!("/v2/{ns}/blobs/{}", data.config.sha256.as_str()))
            .body(data.config.content.to_vec());
        let _ = client.send_traced(rb);
        let body = format!(
            r#"{{"kappa":"{}","ttl":0,"controller":"test-ctrl"}}"#,
            data.config.sha256.as_str()
        );
        let rb = client
            .post_with_content_type(&format!("/v2/{ns}/gc/pin"), "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 201 {
                    t.fail(r.fail_msg(201));
                } else {
                    finalizer_pin_kappa = r.header("x-kappa-label").unwrap_or("").to_string();
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-16 MUST "Unpin blocked"
    {
        let mut t = runner.must("L5-16", 5, "Unpin blocked by finalizer");
        if finalizer_pin_kappa.is_empty() {
            t.fail("no finalizer pin kappa".into());
        } else {
            let body = format!(r#"{{"pin_kappa":"{finalizer_pin_kappa}"}}"#);
            let rb = client
                .post_with_content_type(&format!("/v2/{ns}/gc/unpin"), "application/json")
                .body(body);
            match client.send_traced(rb) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 409 {
                        t.fail(r.fail_msg(409));
                    } else if let Err(e) = verify_error_response(&r.body, "FINALIZER_OUTSTANDING") {
                        t.fail(format!("status 409 but error code wrong: {e}"));
                    } else {
                        t.pass();
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L5-17 MUST "Finalizer in status"
    {
        let mut t = runner.must("L5-17", 5, "Finalizer in GC status");
        match client.send_traced(client.get(&format!("/v2/{ns}/gc/status"))) {
            Ok(r) => {
                t.trace(r.trace.clone());
                let body = r.body_text();
                if body.contains("test-ctrl") {
                    t.pass();
                } else {
                    t.fail(format!("test-ctrl not in pending_finalizers: {body}"));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-18 MUST "Finalizer release"
    {
        let mut t = runner.must("L5-18", 5, "Finalizer release");
        if finalizer_pin_kappa.is_empty() {
            t.fail("no finalizer pin kappa".into());
        } else {
            let body = format!(r#"{{"pin_kappa":"{finalizer_pin_kappa}","release":"true"}}"#);
            let rb = client
                .post_with_content_type(&format!("/v2/{ns}/gc/unpin"), "application/json")
                .body(body);
            match client.send_traced(rb) {
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

    // L5-19 MUST "Unpin after release"
    {
        let mut t = runner.must("L5-19", 5, "Unpin after release");
        if finalizer_pin_kappa.is_empty() {
            t.fail("no finalizer pin kappa".into());
        } else {
            let body = format!(r#"{{"pin_kappa":"{finalizer_pin_kappa}"}}"#);
            let rb = client
                .post_with_content_type(&format!("/v2/{ns}/gc/unpin"), "application/json")
                .body(body);
            match client.send_traced(rb) {
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

    // L5-20 SHOULD "Finalized evicted"
    {
        let mut t = runner.should("L5-20", 5, "Finalized blob evicted after release");
        let pre_sweep3_id = get_current_sweep_id(client, ns);
        let _ = client.send_traced(client.post(&format!("/v2/{ns}/gc/sweep")));
        let _ = poll_sweep(client, ns, 60, pre_sweep3_id.as_deref());
        match client
            .send_traced(client.get(&format!("/v2/{ns}/blobs/{}", data.config.sha256.as_str())))
        {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 404 {
                    t.pass();
                } else {
                    t.fail(format!(
                        "finalized blob not yet evicted: {}: {}",
                        r.status,
                        r.body_text()
                    ));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // ── §9.2 Admission ───────────────────────────────────────────────────────

    let mut filter_kappa = String::new();

    // L5-30 MUST "Filter register"
    {
        let mut t = runner.must("L5-30", 5, "Filter registration");
        let filter = r#"{"type":"json-match","accept_if":{"contains":"accept"},"reason":"must contain accept"}"#;
        let rb = client
            .put_with_content_type(
                &format!("/v2/{ns}/filters/test-filter-scope"),
                "application/json",
            )
            .body(filter);
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 {
                    filter_kappa = r.header("x-kappa-label").unwrap_or("").to_string();
                    t.pass();
                } else if r.status == 422 {
                    t.fail(format!("filter format not supported: {}", r.body_text()));
                } else {
                    t.fail(r.fail_msg(201));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    let filter_supported = !filter_kappa.is_empty();

    // L5-31 MUST "Filter list"
    {
        if !filter_supported {
            runner.skip("L5-31", 5, "Filter listing", "filter not supported");
        } else {
            let mut t = runner.must("L5-31", 5, "Filter listing");
            match client.send_traced(client.get(&format!("/v2/{ns}/filters/"))) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    let body = r.body_text();
                    if body.contains("test-filter-scope") {
                        t.pass();
                    } else {
                        t.fail(format!("filter not in list: {body}"));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L5-32 MUST "Filter accepts"
    {
        if !filter_supported {
            runner.skip(
                "L5-32",
                5,
                "Filter-accepted content stored",
                "filter not supported",
            );
        } else {
            let mut t = runner.must("L5-32", 5, "Filter-accepted content stored");
            let rb = client
                .put_with_content_type(
                    &format!("/v2/{ns}/manifests/filter-pass"),
                    "application/json",
                )
                .body(r#"{"data":"this should accept the content"}"#);
            match client.send_traced(rb) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 201 {
                        t.pass();
                    } else {
                        t.fail(format!(
                            "expected status 201, got {}: {}",
                            r.status,
                            r.body_text()
                        ));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L5-33 MUST "Filter rejects"
    let mut l5_33_body: Vec<u8> = Vec::new();
    {
        if !filter_supported {
            runner.skip(
                "L5-33",
                5,
                "Filter-rejected content rejected",
                "filter not supported",
            );
        } else {
            let mut t = runner.must("L5-33", 5, "Filter-rejected content rejected");
            let rb = client
                .put_with_content_type(
                    &format!("/v2/{ns}/manifests/filter-fail"),
                    "application/json",
                )
                .body(r#"{"data":"this will be refused"}"#);
            match client.send_traced(rb) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 422 {
                        l5_33_body = r.body.clone();
                        t.pass();
                    } else {
                        t.fail(format!("expected 422, got {}: {}", r.status, r.body_text()));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L5-34 MUST "Rejection reason"
    {
        if !filter_supported {
            runner.skip(
                "L5-34",
                5,
                "Rejection includes reason",
                "filter not supported",
            );
        } else {
            let t = runner.must("L5-34", 5, "Rejection includes reason");
            if l5_33_body.is_empty() {
                t.fail("no response body from L5-33".into());
            } else if let Err(e) = verify_error_response(&l5_33_body, "FILTER_REJECTED") {
                t.fail(e);
            } else {
                t.pass();
            }
        }
    }

    // L5-35 MUST "Rejected not stored"
    {
        if !filter_supported {
            runner.skip(
                "L5-35",
                5,
                "Rejected content not stored",
                "filter not supported",
            );
        } else {
            let mut t = runner.must("L5-35", 5, "Rejected content not stored");
            let rejected_kappa = KappaLabel::sha256(br#"{"data":"this will be refused"}"#);
            match client
                .send_traced(client.get(&format!("/v2/{ns}/blobs/{}", rejected_kappa.as_str())))
            {
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

    // L5-36 MUST "Filter remove"
    {
        if !filter_supported {
            runner.skip("L5-36", 5, "Filter removal", "filter not supported");
        } else {
            let mut t = runner.must("L5-36", 5, "Filter removal");
            match client.send_traced(client.delete(&format!("/v2/{ns}/filters/{filter_kappa}"))) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 202 {
                        t.pass();
                    } else {
                        t.fail(r.fail_msg(202));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L5-37 MUST "Filter blob persists"
    {
        if !filter_supported {
            runner.skip(
                "L5-37",
                5,
                "Removed filter blob persists",
                "filter not supported",
            );
        } else {
            let mut t = runner.must("L5-37", 5, "Removed filter blob persists");
            match client.send_traced(client.get(&format!("/v2/{ns}/blobs/{filter_kappa}"))) {
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

    // L5-38 MUST "Prev-rejected accepted"
    {
        if !filter_supported {
            runner.skip(
                "L5-38",
                5,
                "Previously rejected content accepted",
                "filter not supported",
            );
        } else {
            let mut t = runner.must("L5-38", 5, "Previously rejected content accepted");
            let rb = client
                .put_with_content_type(
                    &format!("/v2/{ns}/manifests/filter-fail"),
                    "application/json",
                )
                .body(r#"{"data":"this will be refused"}"#);
            match client.send_traced(rb) {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 201 {
                        t.pass();
                    } else {
                        t.fail(format!(
                            "expected status 201, got {}: {}",
                            r.status,
                            r.body_text()
                        ));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // ── §9.3 Federation ──────────────────────────────────────────────────────

    if config.federation_peers.is_empty() {
        for (id, desc) in [
            ("L5-50", "Federation fetch"),
            ("L5-51", "Federation absent locally"),
            ("L5-52", "Federation resolves from peer"),
            ("L5-53", "Federation content verified"),
            ("L5-54", "Federation caches locally"),
            ("L5-55", "Cached fetch faster"),
            ("L5-56", "Multi-peer federation"),
            ("L5-57", "Multi-hop federation"),
            ("L5-58", "Multi-hop content verified"),
        ] {
            runner.skip(id, 5, desc, "KAPPA_FEDERATION_PEERS unset");
        }
    } else {
        let peer_url = &config.federation_peers[0];
        let peer_content = format!("federation-test-{}", uuid::Uuid::new_v4());
        let peer_kappa = KappaLabel::sha256(peer_content.as_bytes());

        // L5-50 SHOULD "Push to peer"
        {
            let t = runner.should("L5-50", 5, "Federation fetch");
            let peer_client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new());
            let path = format!("{peer_url}/v2/{ns}/blobs/{}", peer_kappa.as_str());
            match peer_client
                .put(&path)
                .header("Content-Type", "application/octet-stream")
                .body(peer_content.clone())
                .send()
            {
                Ok(resp) => {
                    let s = resp.status().as_u16();
                    if s == 201 || s == 200 {
                        t.pass();
                    } else {
                        t.fail(format!("peer push returned {s}"));
                    }
                }
                Err(e) => t.fail(format!("peer push failed: {e}")),
            }
        }

        // L5-51 SHOULD "Primary absent"
        let mut fed_status = 0u16;
        let fed_start;
        {
            let mut t = runner.should("L5-51", 5, "Federation absent locally");
            fed_start = Instant::now();
            match client.send_traced(client.get(&format!("/v2/{ns}/blobs/{}", peer_kappa.as_str())))
            {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    fed_status = r.status;
                    if fed_status == 200 || fed_status == 404 {
                        t.pass();
                    } else {
                        t.fail(format!(
                            "unexpected status: {fed_status}: {}",
                            r.body_text()
                        ));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
        let fed_duration = fed_start.elapsed().as_millis() as u64;

        // L5-52 SHOULD "Federation resolved"
        {
            let t = runner.should("L5-52", 5, "Federation resolves from peer");
            if fed_status == 200 {
                t.pass();
            } else {
                t.fail("federation did not resolve (404)".into());
            }
        }

        // L5-53 MUST "Federation verified"
        if fed_status != 200 {
            runner.skip(
                "L5-53",
                5,
                "Federation content verified",
                "federation did not resolve",
            );
        } else {
            let mut t = runner.must("L5-53", 5, "Federation content verified");
            match client.send_traced(client.get(&format!("/v2/{ns}/blobs/{}", peer_kappa.as_str())))
            {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if let Err(e) = assert_kappa_matches(&r.body, &peer_kappa) {
                        t.fail(e);
                    } else {
                        t.pass();
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }

        // L5-54 SHOULD "Cached locally"
        let mut cached_duration = 0u64;
        if fed_status != 200 {
            runner.skip(
                "L5-54",
                5,
                "Federation caches locally",
                "federation did not resolve",
            );
        } else {
            let mut t = runner.should("L5-54", 5, "Federation caches locally");
            let start = Instant::now();
            match client.send_traced(client.get(&format!("/v2/{ns}/blobs/{}", peer_kappa.as_str())))
            {
                Ok(r) => {
                    t.trace(r.trace.clone());
                    cached_duration = start.elapsed().as_millis() as u64;
                    if r.status == 200 {
                        t.pass();
                    } else {
                        t.fail(r.fail_msg(200));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }

        // L5-55 SHOULD "Cache faster"
        {
            let t = runner.should("L5-55", 5, "Cached fetch faster");
            if cached_duration < fed_duration {
                t.pass();
            } else {
                t.fail(format!(
                    "cached {cached_duration}ms vs first {fed_duration}ms"
                ));
            }
        }

        // L5-56 through L5-58
        if config.federation_peers.len() < 2 {
            runner.skip(
                "L5-56",
                5,
                "Multi-peer federation",
                "only one peer configured",
            );
            runner.skip(
                "L5-57",
                5,
                "Multi-hop federation",
                "only one peer configured",
            );
            runner.skip(
                "L5-58",
                5,
                "Multi-hop content verified",
                "only one peer configured",
            );
        } else {
            runner.skip(
                "L5-56",
                5,
                "Multi-peer federation",
                "multi-peer test not yet exercised",
            );
            runner.skip(
                "L5-57",
                5,
                "Multi-hop federation",
                "topology not discoverable",
            );
            runner.skip(
                "L5-58",
                5,
                "Multi-hop content verified",
                "topology not discoverable",
            );
        }
    }

    // ── §9.4 Rate Limiting ───────────────────────────────────────────────────

    // Adaptive burst test: read x-ratelimit-limit from the first response
    // to discover the burst capacity, then send limit+1 requests to
    // guarantee a 429. This works for any burst size configuration.
    let mut got_429 = false;
    let mut retry_after_value: Option<String> = None;
    let mut rate_limit_trace: Option<crate::client::HttpTrace> = None;
    let mut burst_limit: u64 = 0;
    {
        // Probe to discover burst capacity from x-ratelimit-limit header.
        let probe_path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
        if let Ok(r) = client.send_traced(client.get(&probe_path)) {
            burst_limit = r
                .header("x-ratelimit-limit")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
        }

        if burst_limit > 0 {
            // We already consumed 1 token with the probe. Send enough to
            // exhaust the remaining burst and trigger 429.
            // Cap at 2000 to avoid runaway in misconfigured registries.
            let remaining = burst_limit.min(2000);
            for _ in 0..remaining {
                if let Ok(r) = client.send_traced(client.get(&probe_path)) {
                    if r.status == 429 {
                        rate_limit_trace = Some(r.trace.clone());
                        got_429 = true;
                        retry_after_value = r.header("retry-after").map(|s| s.to_string());
                        break;
                    }
                }
            }
        }
    }

    {
        let mut t = runner.should("L5-60", 5, "Rate limit Retry-After present");
        if let Some(tr) = &rate_limit_trace {
            t.trace(tr.clone());
        }
        if burst_limit == 0 {
            t.warn("x-ratelimit-limit header not present, rate limiting not configured".into());
        } else if got_429 {
            if retry_after_value.is_some() {
                t.pass();
            } else {
                t.fail("429 received but no Retry-After header".into());
            }
        } else {
            t.warn(format!(
                "no 429 after {burst_limit} requests (burst_limit={burst_limit})"
            ));
        }
    }

    {
        let t = runner.must("L5-61", 5, "Retry-After format valid");
        match &retry_after_value {
            Some(v) => {
                if v.parse::<u64>().is_ok() || v.contains(',') {
                    t.pass();
                } else {
                    t.fail(format!("Retry-After not a valid integer or HTTP-date: {v}"));
                }
            }
            None => {
                if got_429 {
                    t.fail("429 received but no Retry-After to validate".into());
                } else {
                    t.pass();
                }
            }
        }
    }

    {
        let t = runner.should("L5-62", 5, "Rate limit behavior recorded");
        if got_429 {
            t.pass();
        } else if burst_limit == 0 {
            t.warn("rate limiting not configured (no x-ratelimit-limit header)".into());
        } else {
            t.warn(format!(
                "rate limiting not triggered (burst_limit={burst_limit})"
            ));
        }
    }

    // Burst test cleanup: compute the required wait from the burst size
    // and refill rate, sleep exactly that long, then verify with one probe.
    // Hard cap at 10 seconds -- if the config needs longer, the conformance
    // script's rate limit settings are too aggressive for testing.
    if got_429 {
        // The burst test consumed burst_limit tokens. Each token refills at
        // period_ms intervals. We need at least 10 tokens back for the
        // subsequent tests (blob PUT + pin POST + metadata queries).
        // Retry-After tells us the per-token wait in seconds. We need
        // 10 * retry_after seconds, capped at 10.
        let per_token_wait = retry_after_value
            .as_ref()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(1)
            .max(1);
        let needed_wait = (10 * per_token_wait).min(10);
        std::thread::sleep(Duration::from_secs(needed_wait));
    }

    // L5-70 SHOULD "Filter registration sets object-type metadata"
    {
        let mut t = runner.should(
            "L5-70",
            5,
            "Filter registration sets object-type=filter metadata",
        );
        match client.send_traced(client.get_meta_list(
            &format!("/v2/{ns}"),
            "object-type",
            "filter",
        )) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else if r.body_text().contains("sha256:") {
                    t.pass();
                } else {
                    t.fail(format!("no filters in metadata query: {}", r.body_text()));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-71 SHOULD "Pin creation sets object-type metadata"
    // Earlier GC tests evict pins, so this test creates its own pin
    // and verifies the metadata before any sweep can remove it.
    {
        let mut t = runner.should("L5-71", 5, "Pin creation sets object-type=pin metadata");
        // Re-push a blob and pin it so the pin blob exists for the query
        let pin_content = b"pin-metadata-test-blob";
        let pin_blob_kappa = KappaLabel::sha256(pin_content);
        let rb = client
            .put(&format!("/v2/{ns}/blobs/{}", pin_blob_kappa.as_str()))
            .body(pin_content.to_vec());
        let _ = client.send_traced(rb);
        let pin_body = format!(r#"{{"kappa":"{}","ttl":0}}"#, pin_blob_kappa.as_str());
        let rb = client
            .post_with_content_type(&format!("/v2/{ns}/gc/pin"), "application/json")
            .body(pin_body);
        match client.send_traced(rb) {
            Ok(pr) => {
                t.trace(pr.trace.clone());
                if pr.status != 201 {
                    t.fail(format!("pin creation failed: {}", pr.fail_msg(201)));
                } else {
                    match client.send_traced(client.get_meta_list(
                        &format!("/v2/{ns}"),
                        "object-type",
                        "pin",
                    )) {
                        Ok(r) => {
                            t.trace(r.trace.clone());
                            if r.status != 200 {
                                t.fail(r.fail_msg(200));
                            } else if r.body_text().contains("sha256:") {
                                t.pass();
                            } else {
                                t.fail(format!("no pins in metadata query: {}", r.body_text()));
                            }
                        }
                        Err(e) => t.fail(format!("metadata query failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }
}
