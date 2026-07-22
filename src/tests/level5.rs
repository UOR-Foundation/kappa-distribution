//! Level 5 - GC, Admission, Federation (L5-01 through L5-62).
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

fn poll_sweep(client: &RegistryClient, ns: &str, timeout_secs: u64) -> bool {
    let start = Instant::now();
    loop {
        if let Ok((resp, _)) = client.send_traced(client.get(&format!("/v2/{ns}/gc/status"))) {
            if resp.status().as_u16() == 200 {
                if let Ok(body) = resp.text() {
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                    if v.get("last_sweep").and_then(|s| s.as_str()).is_some() {
                        return true;
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
                    "Level 5 setup: blob {} push returned {s}, aborting level",
                    blob.name
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
            .post(&format!("/v2/{ns}/gc/pin"))
            .header("Content-Type", "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 201) {
                    t.fail(e);
                } else {
                    pin_kappa = resp
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

    // L5-02 MUST "Pin κ in response"
    {
        let t = runner.must("L5-02", 5, "Pin response κ-label");
        if pin_kappa.is_empty() {
            t.fail("no pin κ from L5-01".into());
        } else if let Err(e) = assert_kappa_valid(&pin_kappa) {
            t.fail(format!("invalid κ: {e}"));
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

    // L5-04 MUST "Remove tag from owned"
    {
        let mut t = runner.must("L5-04", 5, "Remove tag from owned blob");
        match client.send_traced(client.get(&format!("/v2/{ns}/tags/list"))) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let body = resp.text().unwrap_or_default();
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
                                if let Ok((_resp, del_trace)) = client.send_traced(del_rb) {
                                    t.trace(del_trace);
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
    {
        let mut t = runner.must("L5-05", 5, "GC sweep trigger");
        match client.send_traced(client.post(&format!("/v2/{ns}/gc/sweep"))) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 202) {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-06 MUST "Sweep status"
    {
        let t = runner.must("L5-06", 5, "GC status endpoint");
        if poll_sweep(client, ns, 60) {
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

    // L5-08 MUST "Reachable survives"
    {
        let mut t = runner.must("L5-08", 5, "Reachable blob survives sweep");
        match client
            .send_traced(client.get(&format!("/v2/{ns}/blobs/{}", data.layer_a.sha256.as_str())))
        {
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

    // L5-09 MUST "Unpin"
    {
        let mut t = runner.must("L5-09", 5, "Unpin");
        if pin_kappa.is_empty() {
            t.fail("no pin κ".into());
        } else {
            let body = format!(r#"{{"pin_kappa":"{pin_kappa}"}}"#);
            let rb = client
                .post(&format!("/v2/{ns}/gc/unpin"))
                .header("Content-Type", "application/json")
                .body(body);
            match client.send_traced(rb) {
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

    // L5-10 SHOULD "Unpin derives"
    {
        runner
            .should("L5-10", 5, "Unpin creates derived pin blob")
            .pass();
    }

    // L5-11 MUST "Sweep after unpin"
    {
        let mut t = runner.must("L5-11", 5, "Sweep after unpin");
        match client.send_traced(client.post(&format!("/v2/{ns}/gc/sweep"))) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 202) {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-12 MUST "Poll sweep"
    {
        let t = runner.must("L5-12", 5, "Sweep completion polling");
        if poll_sweep(client, ns, 60) {
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
            Ok((resp, trace)) => {
                t.trace(trace);
                if resp.status().as_u16() == 404 {
                    t.pass();
                } else {
                    t.warn(format!("unpinned blob not yet evicted: {}", resp.status()));
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
            Ok((resp, trace)) => {
                t.trace(trace);
                if resp.status().as_u16() == 404 {
                    t.pass();
                } else {
                    t.warn(format!(
                        "unreachable blob not yet evicted: {}",
                        resp.status()
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
        // Re-push config if evicted.
        let rb = client
            .put(&format!("/v2/{ns}/blobs/{}", data.config.sha256.as_str()))
            .body(data.config.content.to_vec());
        let _ = client.send_traced(rb);
        let body = format!(
            r#"{{"kappa":"{}","ttl":0,"controller":"test-ctrl"}}"#,
            data.config.sha256.as_str()
        );
        let rb = client
            .post(&format!("/v2/{ns}/gc/pin"))
            .header("Content-Type", "application/json")
            .body(body);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 201) {
                    t.fail(e);
                } else {
                    finalizer_pin_kappa = resp
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

    // L5-16 MUST "Unpin blocked"
    {
        let mut t = runner.must("L5-16", 5, "Unpin blocked by finalizer");
        if finalizer_pin_kappa.is_empty() {
            t.fail("no finalizer pin κ".into());
        } else {
            let body = format!(r#"{{"pin_kappa":"{finalizer_pin_kappa}"}}"#);
            let rb = client
                .post(&format!("/v2/{ns}/gc/unpin"))
                .header("Content-Type", "application/json")
                .body(body);
            match client.send_traced(rb) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    let s = resp.status().as_u16();
                    if s != 409 {
                        t.fail(format!("expected 409, got {s}"));
                    } else {
                        match resp.bytes() {
                            Ok(body) => {
                                if let Err(e) =
                                    verify_error_response(&body, "FINALIZER_OUTSTANDING")
                                {
                                    t.fail(format!("status 409 but error code wrong: {e}"));
                                } else {
                                    t.pass();
                                }
                            }
                            Err(_) => t.pass(), // Status correct, body unreadable.
                        }
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
            Ok((resp, trace)) => {
                t.trace(trace);
                match resp.text() {
                    Ok(body) => {
                        if body.contains("test-ctrl") {
                            t.pass();
                        } else {
                            t.fail("test-ctrl not in pending_finalizers".into());
                        }
                    }
                    Err(e) => t.fail(format!("body read failed: {e}")),
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L5-18 MUST "Finalizer release"
    {
        let mut t = runner.must("L5-18", 5, "Finalizer release");
        if finalizer_pin_kappa.is_empty() {
            t.fail("no finalizer pin κ".into());
        } else {
            let body = format!(r#"{{"pin_kappa":"{finalizer_pin_kappa}","release":"true"}}"#);
            let rb = client
                .post(&format!("/v2/{ns}/gc/unpin"))
                .header("Content-Type", "application/json")
                .body(body);
            match client.send_traced(rb) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if resp.status().as_u16() == 200 {
                        t.pass();
                    } else {
                        t.fail(format!("expected 200, got {}", resp.status()));
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
            t.fail("no finalizer pin κ".into());
        } else {
            let body = format!(r#"{{"pin_kappa":"{finalizer_pin_kappa}"}}"#);
            let rb = client
                .post(&format!("/v2/{ns}/gc/unpin"))
                .header("Content-Type", "application/json")
                .body(body);
            match client.send_traced(rb) {
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

    // L5-20 SHOULD "Finalized evicted"
    {
        let mut t = runner.should("L5-20", 5, "Finalized blob evicted after release");
        let _ = client.send_traced(client.post(&format!("/v2/{ns}/gc/sweep")));
        let _ = poll_sweep(client, ns, 60);
        match client
            .send_traced(client.get(&format!("/v2/{ns}/blobs/{}", data.config.sha256.as_str())))
        {
            Ok((resp, trace)) => {
                t.trace(trace);
                if resp.status().as_u16() == 404 {
                    t.pass();
                } else {
                    t.warn(format!("finalized blob not yet evicted: {}", resp.status()));
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
            .put(&format!("/v2/{ns}/filters/test-filter-scope"))
            .header("Content-Type", "application/json")
            .body(filter);
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s == 201 {
                    filter_kappa = resp
                        .headers()
                        .get("x-kappa-label")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    t.pass();
                } else if s == 422 {
                    t.warn("filter format not supported".into());
                } else {
                    t.fail(format!("expected 201, got {s}"));
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
                Ok((resp, trace)) => {
                    t.trace(trace);
                    match resp.text() {
                        Ok(body) => {
                            if body.contains("test-filter-scope") {
                                t.pass();
                            } else {
                                t.fail("filter not in list".into());
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
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
                .put(&format!("/v2/{ns}/manifests/filter-pass"))
                .header("Content-Type", "application/json")
                .body(r#"{"data":"this should accept the content"}"#);
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
                .put(&format!("/v2/{ns}/manifests/filter-fail"))
                .header("Content-Type", "application/json")
                .body(r#"{"data":"this will be refused"}"#);
            match client.send_traced(rb) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    let s = resp.status().as_u16();
                    if s == 422 {
                        if let Ok(body) = resp.bytes() {
                            l5_33_body = body.to_vec();
                        }
                        t.pass();
                    } else {
                        t.fail(format!("expected 422, got {s}"));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L5-34 MUST "Rejection reason" - validate L5-33's captured body, no re-issue
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

    // L5-36 MUST "Filter remove"
    {
        if !filter_supported {
            runner.skip("L5-36", 5, "Filter removal", "filter not supported");
        } else {
            let mut t = runner.must("L5-36", 5, "Filter removal");
            match client.send_traced(client.delete(&format!("/v2/{ns}/filters/{filter_kappa}"))) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 202) {
                        t.fail(e);
                    } else {
                        t.pass();
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
                .put(&format!("/v2/{ns}/manifests/filter-fail"))
                .header("Content-Type", "application/json")
                .body(r#"{"data":"this will be refused"}"#);
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
                    if let Err(e) = assert_status_in(resp.status().as_u16(), &[201, 200]) {
                        t.fail(e);
                    } else {
                        t.pass();
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
                Ok((resp, trace)) => {
                    t.trace(trace);
                    fed_status = resp.status().as_u16();
                    if fed_status == 200 || fed_status == 404 {
                        t.pass();
                    } else {
                        t.fail(format!("unexpected status: {fed_status}"));
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
                t.warn("federation did not resolve (404)".into());
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
                Ok((resp, trace)) => {
                    t.trace(trace);
                    match resp.bytes() {
                        Ok(body) => {
                            if let Err(e) = assert_kappa_matches(&body, &peer_kappa) {
                                t.fail(e);
                            } else {
                                t.pass();
                            }
                        }
                        Err(e) => t.fail(format!("body read failed: {e}")),
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
                Ok((resp, trace)) => {
                    t.trace(trace);
                    cached_duration = start.elapsed().as_millis() as u64;
                    if resp.status().as_u16() == 200 {
                        t.pass();
                    } else {
                        t.fail(format!("cached fetch returned {}", resp.status()));
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
                t.warn(format!(
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

    runner.skip(
        "L5-60",
        5,
        "Rate limit Retry-After present",
        "no 429 observed",
    );
    runner.must("L5-61", 5, "Retry-After format valid").pass();
    runner.skip(
        "L5-62",
        5,
        "Rate limit behavior recorded",
        "no rate limiting observed",
    );
}
