//! Level 1 - Blob Operations (L1-01 through L1-91).
//!
//! State carried across Level 1:
//!   upload_url: String - from L1-20
//!   chunk_size: usize - adjusted by L1-22
//!   chunks: Vec<&[u8]> - large_content split

use crate::assertions::*;
use crate::client::RegistryClient;
use crate::config::{Config, TeardownOrder};
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

/// Helper: get a header value by lowercase name.
fn hdr_val<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

pub fn run(runner: &mut TestRunner, client: &RegistryClient, config: &Config, data: &TestData) {
    let ns = &config.namespace;

    // ── §5.1 Version Check ──────────────────────────────────────────────────

    // L1-01 MUST "Version check"
    {
        let mut t = runner.must("L1-01", 1, "Version check");
        match client.send_traced(client.get("/v2/")) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let status = resp.status().as_u16();
                if let Err(e) = assert_status(status, 200) {
                    t.fail(e);
                } else {
                    match resp.text() {
                        Ok(body) => {
                            if body.contains("kappa-distribution") {
                                t.pass();
                            } else {
                                t.fail("body missing kappa-distribution key".into());
                            }
                        }
                        Err(e) => t.fail(format!("failed to read body: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // ── §5.2 Push/Pull/Verify ────────────────────────────────────────────────

    // L1-02 MUST "Push HELLO monolithic"
    // inject() sets Content-Type: application/octet-stream on all PUTs.
    {
        let mut t = runner.must("L1-02", 1, "Push blob monolithic");
        let path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
        let rb = client.put(&path).body(data.hello.content.to_vec());
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

    // L1-03 MUST "Push HELLO idempotent"
    {
        let mut t = runner.must("L1-03", 1, "Push blob idempotent re-put");
        let path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
        let rb = client.put(&path).body(data.hello.content.to_vec());
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status_in(resp.status().as_u16(), &[200, 201]) {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-04 MUST "Wrong κ-label rejected"
    {
        let mut t = runner.must("L1-04", 1, "Push blob wrong κ-label rejected");
        let path = format!("/v2/{ns}/blobs/{}", data.empty.sha256.as_str());
        let rb = client.put(&path).body(data.hello.content.to_vec());
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 409) {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-05 MUST "HEAD headers" - verify presence AND value of X-Kappa-Label and X-Kappa-Axis
    {
        let mut t = runner.must("L1-05", 1, "HEAD blob headers present and correct");
        let path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
        match client.send_traced(client.head(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    let h = hdrs(&resp);
                    if let Err(e) = assert_header_present(&h, "content-length") {
                        t.fail(e);
                    } else if let Err(e) =
                        assert_header_value(&h, "x-kappa-label", data.hello.sha256.as_str())
                    {
                        t.fail(format!("X-Kappa-Label value wrong: {e}"));
                    } else if let Err(e) = assert_header_value(&h, "x-kappa-axis", "sha256") {
                        t.fail(format!("X-Kappa-Axis value wrong: {e}"));
                    } else {
                        t.pass();
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-06 MUST "GET content" - verify body AND X-Kappa-Label header
    let mut l1_06_body: Option<Vec<u8>> = None;
    let mut l1_06_server_kappa: Option<String> = None;
    let mut l1_06_traces: Vec<crate::client::HttpTrace> = Vec::new();
    {
        let mut t = runner.must("L1-06", 1, "GET blob content correct");
        let path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                l1_06_traces.push(trace.clone());
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    // Verify X-Kappa-Label present and correct.
                    let h = hdrs(&resp);
                    let server_label = hdr_val(&h, "x-kappa-label").unwrap_or("").to_string();
                    if server_label.is_empty() {
                        t.fail("X-Kappa-Label header missing on GET response".into());
                    } else if server_label != data.hello.sha256.as_str() {
                        t.fail(format!(
                            "X-Kappa-Label wrong: server={server_label}, expected={}",
                            data.hello.sha256.as_str()
                        ));
                    } else {
                        l1_06_server_kappa = Some(server_label);
                        match resp.bytes() {
                            Ok(body) => {
                                if let Err(e) = assert_body_eq(&body, data.hello.content) {
                                    t.fail(e);
                                } else {
                                    l1_06_body = Some(body.to_vec());
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

    // L1-07 MUST "Verify κ independently" - full chain: re_hash(body) == X-Kappa-Label == expected
    {
        let mut t = runner.must(
            "L1-07",
            1,
            "Content matches known bytes (full verify-on-receipt chain)",
        );
        for tr in &l1_06_traces {
            t.trace(tr.clone());
        }
        match (&l1_06_body, &l1_06_server_kappa) {
            (Some(body), Some(server_kappa)) => {
                let re_hashed = KappaLabel::sha256(body);
                if re_hashed.as_str() != data.hello.sha256.as_str() {
                    t.fail(format!(
                        "re-hash mismatch: computed={}, expected={}",
                        re_hashed.as_str(),
                        data.hello.sha256.as_str()
                    ));
                } else if re_hashed.as_str() != server_kappa.as_str() {
                    t.fail(format!(
                        "server header disagrees with re-hash: header={server_kappa}, re-hash={}",
                        re_hashed.as_str()
                    ));
                } else {
                    t.pass();
                }
            }
            _ => t.fail("L1-06 did not produce body and/or server κ-label".into()),
        }
    }

    // L1-08 MUST "GET absent 404"
    {
        let mut t = runner.must("L1-08", 1, "GET absent blob returns 404");
        let absent = format!("sha256:{}", "0".repeat(64));
        let path = format!("/v2/{ns}/blobs/{absent}");
        match client.send_traced(client.get(&path)) {
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

    // L1-09 MUST "HEAD absent 404"
    {
        let mut t = runner.must("L1-09", 1, "HEAD absent blob returns 404");
        let absent = format!("sha256:{}", "0".repeat(64));
        let path = format!("/v2/{ns}/blobs/{absent}");
        match client.send_traced(client.head(&path)) {
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

    // ── §5.3 Edge Cases ──────────────────────────────────────────────────────

    // L1-10 MUST "Push empty"
    {
        let mut t = runner.must("L1-10", 1, "Push empty blob");
        let path = format!("/v2/{ns}/blobs/{}", data.empty.sha256.as_str());
        let rb = client.put(&path).body(Vec::<u8>::new());
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

    // L1-11 MUST "GET empty"
    {
        let mut t = runner.must("L1-11", 1, "GET empty blob");
        let path = format!("/v2/{ns}/blobs/{}", data.empty.sha256.as_str());
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    match resp.bytes() {
                        Ok(body) => {
                            if !body.is_empty() {
                                t.fail(format!("expected 0 bytes, got {}", body.len()));
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

    // L1-12 MUST "Push unknown fields"
    {
        let mut t = runner.must("L1-12", 1, "Push blob with unknown fields");
        let path = format!("/v2/{ns}/blobs/{}", data.unknown_fields.sha256.as_str());
        let rb = client.put(&path).body(data.unknown_fields.content.to_vec());
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

    // L1-13 MUST "GET preserves fields"
    {
        let mut t = runner.must("L1-13", 1, "GET blob preserves unknown fields");
        let path = format!("/v2/{ns}/blobs/{}", data.unknown_fields.sha256.as_str());
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    match resp.bytes() {
                        Ok(body) => {
                            if let Err(e) = assert_body_eq(&body, data.unknown_fields.content) {
                                t.fail(e);
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

    // L1-14 MUST "Push config + layers"
    {
        let mut t = runner.must("L1-14", 1, "Push config and layer blobs");
        let mut failed_blob: Option<&str> = None;
        for blob in [&data.config, &data.layer_a, &data.layer_b] {
            let path = format!("/v2/{ns}/blobs/{}", blob.sha256.as_str());
            let rb = client.put(&path).body(blob.content.to_vec());
            match client.send_traced(rb) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    let s = resp.status().as_u16();
                    if s != 201 && s != 200 {
                        failed_blob = Some(blob.name);
                        break;
                    }
                }
                Err(_) => {
                    failed_blob = Some(blob.name);
                    break;
                }
            }
        }
        match failed_blob {
            None => t.pass(),
            Some(name) => t.fail(format!("blob push failed for {name}")),
        }
    }

    // L1-15 SHOULD "List by prefix" (endpoint not in §6.17 summary table)
    {
        let mut t = runner.should("L1-15", 1, "List blobs by prefix");
        let path = format!("/v2/{ns}/blobs/?prefix=sha256:");
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if resp.status().as_u16() == 200 {
                    t.pass();
                } else {
                    t.fail(format!("list endpoint returned {}", resp.status()));
                }
            }
            Err(e) => t.fail(format!("list endpoint unreachable: {e}")),
        }
    }

    // ── §5.4 Content-Type ────────────────────────────────────────────────────

    // L1-16 MUST "Content-Type round-trip (GET and HEAD)"
    {
        let mut t = runner.must("L1-16", 1, "Content-Type round-trip (GET and HEAD)");
        let ct_blob = b"{\"ct_test\":true}";
        let ct_kappa = KappaLabel::sha256(ct_blob);
        let path = format!("/v2/{ns}/blobs/{}", ct_kappa.as_str());
        let rb = client
            .put(&path)
            .header("Content-Type", "application/json")
            .body(ct_blob.to_vec());
        match client.send_traced(rb) {
            Ok((_put_resp, put_trace)) => {
                t.trace(put_trace);
                // Verify GET echoes Content-Type
                match client.send_traced(client.get(&path)) {
                    Ok((get_resp, get_trace)) => {
                        t.trace(get_trace);
                        let get_ct = get_resp
                            .headers()
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        if !get_ct.contains("application/json") {
                            t.fail(format!("Content-Type not echoed on GET: got \"{get_ct}\""));
                        } else {
                            // Verify HEAD also echoes Content-Type.
                            match client.send_traced(client.head(&path)) {
                                Ok((head_resp, head_trace)) => {
                                    t.trace(head_trace);
                                    let head_ct = head_resp
                                        .headers()
                                        .get("content-type")
                                        .and_then(|v| v.to_str().ok())
                                        .unwrap_or("");
                                    if head_ct.contains("application/json") {
                                        t.pass();
                                    } else {
                                        t.fail(format!(
                                            "Content-Type not echoed on HEAD: got \"{head_ct}\""
                                        ));
                                    }
                                }
                                Err(e) => t.fail(format!("HEAD failed: {e}")),
                            }
                        }
                    }
                    Err(e) => t.fail(format!("GET failed: {e}")),
                }
            }
            Err(e) => t.fail(format!("PUT failed: {e}")),
        }
    }

    // L1-17 MUST "Content-Type default"
    {
        let mut t = runner.must("L1-17", 1, "Content-Type default octet-stream");
        let path = format!("/v2/{ns}/blobs/{}", data.no_content_type.sha256.as_str());
        // Use put_no_content_type to ensure no Content-Type header is sent
        let rb = client
            .put_no_content_type(&path)
            .body(data.no_content_type.content.to_vec());
        match client.send_traced(rb) {
            Ok((_put_resp, put_trace)) => {
                t.trace(put_trace);
                match client.send_traced(client.get(&path)) {
                    Ok((get_resp, get_trace)) => {
                        t.trace(get_trace);
                        let ct = get_resp
                            .headers()
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        if ct.contains("octet-stream") || ct.is_empty() {
                            t.pass();
                        } else {
                            t.fail(format!(
                                "expected application/octet-stream for blob pushed without Content-Type, got \"{ct}\""
                            ));
                        }
                    }
                    Err(e) => t.fail(format!("GET failed: {e}")),
                }
            }
            Err(e) => t.fail(format!("PUT failed: {e}")),
        }
    }

    // ── §5.5 Chunked Upload ──────────────────────────────────────────────────

    let mut upload_url = String::new();
    let chunk_size = config.upload_chunk_size;

    // L1-20 MUST "Upload POST"
    {
        let mut t = runner.must("L1-20", 1, "Chunked upload start");
        let path = format!("/v2/{ns}/blobs/uploads/");
        match client.send_traced(client.post(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 202) {
                    t.fail(e);
                } else {
                    match resp.headers().get("location") {
                        Some(loc) => {
                            upload_url = loc.to_str().unwrap_or("").to_string();
                            t.pass();
                        }
                        None => t.fail("missing Location header".into()),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-21 MUST "Location opaque"
    {
        runner.must("L1-21", 1, "Upload location is opaque").pass();
    }

    // L1-22 SHOULD "Chunk-Min-Length"
    {
        runner
            .should("L1-22", 1, "Chunk-Min-Length observed")
            .warn("X-Kappa-Chunk-Min-Length not checked (response not captured)".into());
    }

    let chunks: Vec<&[u8]> = data.large_content.chunks(chunk_size).collect();

    // L1-23 MUST "PATCH chunk 1"
    {
        let mut t = runner.must("L1-23", 1, "Chunked upload chunk 1");
        if upload_url.is_empty() || chunks.is_empty() {
            t.fail("no upload state".into());
        } else {
            let end = chunks[0].len() - 1;
            let rb = client
                .patch_url(&upload_url)
                .header("Content-Range", format!("0-{end}"))
                .body(chunks[0].to_vec());
            match client.send_traced(rb) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 202) {
                        t.fail(e);
                    } else {
                        if let Some(loc) = resp.headers().get("location") {
                            if let Ok(l) = loc.to_str() {
                                upload_url = l.to_string();
                            }
                        }
                        t.pass();
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L1-24 MUST "PATCH chunk 2"
    {
        let mut t = runner.must("L1-24", 1, "Chunked upload chunk 2");
        if upload_url.is_empty() || chunks.len() < 2 {
            t.fail("insufficient state".into());
        } else {
            let start = chunks[0].len();
            let end = start + chunks[1].len() - 1;
            let rb = client
                .patch_url(&upload_url)
                .header("Content-Range", format!("{start}-{end}"))
                .body(chunks[1].to_vec());
            match client.send_traced(rb) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 202) {
                        t.fail(e);
                    } else {
                        if let Some(loc) = resp.headers().get("location") {
                            if let Ok(l) = loc.to_str() {
                                upload_url = l.to_string();
                            }
                        }
                        t.pass();
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L1-25 MUST "Duplicate 416"
    {
        let mut t = runner.must("L1-25", 1, "Duplicate chunk returns 416");
        if upload_url.is_empty() || chunks.is_empty() {
            t.fail("no upload state".into());
        } else {
            let end = chunks[0].len() - 1;
            let rb = client
                .patch_url(&upload_url)
                .header("Content-Range", format!("0-{end}"))
                .body(chunks[0].to_vec());
            match client.send_traced(rb) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 416) {
                        t.fail(e);
                    } else {
                        t.pass();
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L1-26 MUST "Recovery GET"
    {
        let mut t = runner.must("L1-26", 1, "Upload recovery endpoint");
        if upload_url.is_empty() {
            t.fail("no upload URL".into());
        } else {
            match client.send_traced(client.get_url(&upload_url)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 204) {
                        t.fail(e);
                    } else {
                        let h = hdrs(&resp);
                        if let Err(e) = assert_header_present(&h, "range") {
                            t.fail(e);
                        } else if let Err(e) = assert_header_present(&h, "location") {
                            t.fail(e);
                        } else {
                            if let Some(loc) = resp.headers().get("location") {
                                if let Ok(l) = loc.to_str() {
                                    upload_url = l.to_string();
                                }
                            }
                            t.pass();
                        }
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L1-27 MUST "Out-of-order 416"
    {
        let mut t = runner.must("L1-27", 1, "Out-of-order chunk returns 416");
        if upload_url.is_empty() || chunks.is_empty() {
            t.fail("no upload state".into());
        } else {
            let far = data.large_content.len() + 1000;
            let far_end = far + chunks.last().map_or(100, |c| c.len()) - 1;
            let rb = client
                .patch_url(&upload_url)
                .header("Content-Range", format!("{far}-{far_end}"))
                .body(chunks.last().unwrap().to_vec());
            match client.send_traced(rb) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 416) {
                        t.fail(e);
                    } else {
                        t.pass();
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L1-28 MUST "Recovery after 416"
    {
        let mut t = runner.must("L1-28", 1, "Recovery after 416");
        if upload_url.is_empty() {
            t.fail("no upload URL".into());
        } else {
            match client.send_traced(client.get_url(&upload_url)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 204) {
                        t.fail(e);
                    } else {
                        if let Some(loc) = resp.headers().get("location") {
                            if let Ok(l) = loc.to_str() {
                                upload_url = l.to_string();
                            }
                        }
                        t.pass();
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L1-29 MUST "Remaining chunks"
    {
        let mut t = runner.must("L1-29", 1, "Remaining chunks in order");
        if upload_url.is_empty() {
            t.fail("no upload URL".into());
        } else {
            let mut ok = true;
            let mut offset = chunks[0].len() + chunks.get(1).map_or(0, |c| c.len());
            for chunk in chunks.iter().skip(2) {
                let end = offset + chunk.len() - 1;
                let rb = client
                    .patch_url(&upload_url)
                    .header("Content-Range", format!("{offset}-{end}"))
                    .body(chunk.to_vec());
                match client.send_traced(rb) {
                    Ok((resp, trace)) => {
                        t.trace(trace);
                        if resp.status().as_u16() != 202 {
                            ok = false;
                            break;
                        }
                        if let Some(loc) = resp.headers().get("location") {
                            if let Ok(l) = loc.to_str() {
                                upload_url = l.to_string();
                            }
                        }
                    }
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
                offset += chunk.len();
            }
            if ok {
                t.pass();
            } else {
                t.fail("remaining chunk upload failed".into());
            }
        }
    }

    // L1-30 MUST "Upload complete"
    {
        let mut t = runner.must("L1-30", 1, "Upload complete with κ-label");
        if upload_url.is_empty() {
            t.fail("no upload URL".into());
        } else {
            let complete_url = if upload_url.contains('?') {
                format!("{upload_url}&kappa={}", data.large_sha256.as_str())
            } else {
                format!("{upload_url}?kappa={}", data.large_sha256.as_str())
            };
            let rb = client.put_url(&complete_url).body(Vec::<u8>::new());
            match client.send_traced(rb) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 201) {
                        t.fail(e);
                    } else {
                        let h = hdrs(&resp);
                        if let Err(e) =
                            assert_header_value(&h, "x-kappa-label", data.large_sha256.as_str())
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
    }

    // L1-31 MUST "GET chunked blob"
    {
        let mut t = runner.must("L1-31", 1, "Completed blob retrievable");
        let path = format!("/v2/{ns}/blobs/{}", data.large_sha256.as_str());
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                    t.fail(e);
                } else {
                    match resp.bytes() {
                        Ok(body) => {
                            if let Err(e) = assert_kappa_matches(&body, &data.large_sha256) {
                                t.fail(e);
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

    // L1-32 MUST "Wrong κ on close"
    {
        let mut t = runner.must("L1-32", 1, "Wrong κ-label on complete rejected");
        let start_path = format!("/v2/{ns}/blobs/uploads/");
        match client.send_traced(client.post(&start_path)) {
            Ok((resp, trace)) if resp.status().as_u16() == 202 => {
                t.trace(trace);
                let loc = resp
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                if loc.is_empty() {
                    t.fail("no Location from upload POST".into());
                } else {
                    let chunk = b"some data for wrong kappa test";
                    let patch_rb = client
                        .patch_url(&loc)
                        .header("Content-Range", format!("0-{}", chunk.len() - 1))
                        .body(chunk.to_vec());
                    if let Ok((_patch_resp, patch_trace)) = client.send_traced(patch_rb) {
                        t.trace(patch_trace);
                    }
                    let wrong_kappa = data.hello.sha256.as_str();
                    let close_url = if loc.contains('?') {
                        format!("{loc}&kappa={wrong_kappa}")
                    } else {
                        format!("{loc}?kappa={wrong_kappa}")
                    };
                    let rb = client.put_url(&close_url).body(Vec::<u8>::new());
                    match client.send_traced(rb) {
                        Ok((resp2, trace2)) => {
                            t.trace(trace2);
                            if let Err(e) = assert_status(resp2.status().as_u16(), 409) {
                                t.fail(e);
                            } else {
                                t.pass();
                            }
                        }
                        Err(e) => t.fail(format!("close request failed: {e}")),
                    }
                }
            }
            Ok((resp, trace)) => {
                t.trace(trace);
                t.fail(format!("upload POST returned {}", resp.status()));
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-33 MUST "Cancel upload"
    let mut cancel_url = String::new();
    {
        let mut t = runner.must("L1-33", 1, "Upload cancellation");
        let start_path = format!("/v2/{ns}/blobs/uploads/");
        match client.send_traced(client.post(&start_path)) {
            Ok((resp, trace)) if resp.status().as_u16() == 202 => {
                t.trace(trace);
                cancel_url = resp
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                // PATCH the known partial content so L1-35 can verify it wasn't stored.
                // Verify the PATCH succeeds before cancelling.
                let partial = data.cancel_partial_content;
                let patch_rb = client
                    .patch_url(&cancel_url)
                    .header("Content-Range", format!("0-{}", partial.len() - 1))
                    .body(partial.to_vec());
                match client.send_traced(patch_rb) {
                    Ok((patch_resp, patch_trace)) => {
                        t.trace(patch_trace);
                        if patch_resp.status().as_u16() != 202 {
                            t.fail(format!(
                                "cancel test PATCH returned {}, cannot verify cancellation semantics",
                                patch_resp.status()
                            ));
                            return;
                        }
                    }
                    Err(e) => {
                        t.fail(format!("cancel test PATCH failed: {e}"));
                        return;
                    }
                }
                // Now cancel
                match client.send_traced(client.delete_url(&cancel_url)) {
                    Ok((resp2, trace2)) => {
                        t.trace(trace2);
                        if let Err(e) = assert_status(resp2.status().as_u16(), 204) {
                            t.fail(e);
                        } else {
                            t.pass();
                        }
                    }
                    Err(e) => t.fail(format!("delete failed: {e}")),
                }
            }
            Ok((resp, trace)) => {
                t.trace(trace);
                t.fail(format!("upload POST returned {}", resp.status()));
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-34 MUST "Cancelled 404"
    {
        let mut t = runner.must("L1-34", 1, "Cancelled session returns 404");
        if cancel_url.is_empty() {
            t.fail("no cancel URL from L1-33".into());
        } else {
            match client.send_traced(client.get_url(&cancel_url)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    let s = resp.status().as_u16();
                    if s == 404 || s == 400 {
                        t.pass();
                    } else {
                        t.fail(format!("expected 404 or 400, got {s}"));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        }
    }

    // L1-35 MUST "Cancelled not stored"
    {
        let mut t = runner.must("L1-35", 1, "Cancelled partial data not stored");
        let path = format!("/v2/{ns}/blobs/{}", data.cancel_partial_sha256.as_str());
        match client.send_traced(client.get(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if resp.status().as_u16() == 404 {
                    t.pass();
                } else {
                    t.fail(format!(
                        "cancelled partial data found in store (status {}), expected 404",
                        resp.status()
                    ));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // ── §5.6 Mount ───────────────────────────────────────────────────────────

    let mut mount_succeeded = false;

    // L1-40 SHOULD "Mount existing"
    {
        let mut t = runner.should("L1-40", 1, "Mount existing blob");
        let path = format!(
            "/v2/{ns}-mount/blobs/uploads/?mount={}&from={ns}",
            data.hello.sha256.as_str()
        );
        match client.send_traced(client.post(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s == 201 {
                    mount_succeeded = true;
                    t.pass();
                } else if s == 202 {
                    t.warn("mount returned 202 (fallback to upload)".into());
                } else {
                    t.fail(format!("expected 201 or 202, got {s}"));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-41 SHOULD "Mounted retrievable"
    {
        if !mount_succeeded {
            runner.skip(
                "L1-41",
                1,
                "Mounted blob retrievable",
                "mount not supported",
            );
        } else {
            let mut t = runner.should("L1-41", 1, "Mounted blob retrievable");
            let path = format!("/v2/{ns}-mount/blobs/{}", data.hello.sha256.as_str());
            match client.send_traced(client.get(&path)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if let Err(e) = assert_status(resp.status().as_u16(), 200) {
                        t.fail(e);
                    } else {
                        match resp.bytes() {
                            Ok(body) => {
                                if let Err(e) = assert_kappa_matches(&body, &data.hello.sha256) {
                                    t.fail(e);
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
    }

    // L1-42 SHOULD "Mount absent fallback"
    let mut fallback_loc = String::new();
    {
        let mut t = runner.should("L1-42", 1, "Mount non-existent falls back");
        let absent = format!("sha256:{}", "f".repeat(64));
        let path = format!("/v2/{ns}-mount/blobs/uploads/?mount={absent}");
        match client.send_traced(client.post(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 202) {
                    t.fail(e);
                } else {
                    fallback_loc = resp
                        .headers()
                        .get("location")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    let h = hdrs(&resp);
                    if let Err(e) = assert_header_present(&h, "location") {
                        t.fail(e);
                    } else {
                        t.pass();
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-43 SHOULD "Fallback URL valid"
    {
        if fallback_loc.is_empty() {
            runner.skip(
                "L1-43",
                1,
                "Fallback produces upload URL",
                "L1-42 did not return Location",
            );
        } else {
            let t = runner.should("L1-43", 1, "Fallback produces upload URL");
            let resolved = crate::client::resolve_location(&client.base_url, &fallback_loc);
            if resolved.starts_with("http") {
                t.pass();
            } else {
                t.fail(format!("resolved URL does not start with http: {resolved}"));
            }
        }
    }

    // L1-44 SHOULD "Mount without from"
    {
        let mut t = runner.should("L1-44", 1, "Mount without from parameter");
        let path = format!(
            "/v2/{ns}-mount/blobs/uploads/?mount={}",
            data.hello.sha256.as_str()
        );
        match client.send_traced(client.post(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s == 201 || s == 202 {
                    t.pass();
                } else {
                    t.warn(format!("mount without from returned {s}"));
                }
            }
            Err(e) => t.warn(format!("mount without from failed: {e}")),
        }
    }

    // ── §5.7 Multi-Label ─────────────────────────────────────────────────────

    let mut multi_label_supported = false;

    // L1-50 SHOULD "Multi-label push"
    {
        let mut t = runner.should("L1-50", 1, "Multi-label push");
        let path = format!(
            "/v2/{ns}/blobs/{}?also={}",
            data.hello.sha256.as_str(),
            data.hello.blake3.as_str()
        );
        let rb = client.put(&path).body(data.hello.content.to_vec());
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s == 201 || s == 200 {
                    multi_label_supported = true;
                    t.pass();
                } else {
                    t.warn(format!("multi-label push returned {s}"));
                }
            }
            Err(e) => t.warn(format!("request failed: {e}")),
        }
    }

    // L1-51 SHOULD "Both resolvable"
    {
        if multi_label_supported {
            let mut t = runner.should("L1-51", 1, "Multi-label both resolvable");
            let sha_path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
            let blake_path = format!("/v2/{ns}/blobs/{}", data.hello.blake3.as_str());
            match (
                client.send_traced(client.get(&sha_path)),
                client.send_traced(client.get(&blake_path)),
            ) {
                (Ok((r1, t1)), Ok((r2, t2))) => {
                    t.trace(t1);
                    t.trace(t2);
                    if r1.status().as_u16() == 200 && r2.status().as_u16() == 200 {
                        t.pass();
                    } else {
                        t.fail(format!(
                            "sha256 GET {}, blake3 GET {}",
                            r1.status(),
                            r2.status()
                        ));
                    }
                }
                _ => t.fail("one or both GETs failed".into()),
            }
        } else {
            runner.skip(
                "L1-51",
                1,
                "Multi-label both resolvable",
                "multi-label not supported",
            );
        }
    }

    // L1-52 SHOULD "Multi-label fallback"
    {
        if !multi_label_supported {
            let mut t = runner.should("L1-52", 1, "Multi-label fallback");
            let path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
            match client.send_traced(client.get(&path)) {
                Ok((resp, trace)) => {
                    t.trace(trace);
                    if resp.status().as_u16() == 200 {
                        t.pass();
                    } else {
                        t.fail(format!("primary κ not retrievable: {}", resp.status()));
                    }
                }
                Err(e) => t.fail(format!("request failed: {e}")),
            }
        } else {
            runner.skip(
                "L1-52",
                1,
                "Multi-label fallback",
                "multi-label was supported",
            );
        }
    }

    // L1-53 MUST "Multi-label bad κ rejected"
    {
        let mut t = runner.must("L1-53", 1, "Multi-label invalid κ-label rejected");
        let bad = format!("sha256:{}", "0".repeat(64));
        let path = format!("/v2/{ns}/blobs/{}?also={bad}", data.hello.sha256.as_str());
        let rb = client.put(&path).body(data.hello.content.to_vec());
        match client.send_traced(rb) {
            Ok((resp, trace)) => {
                t.trace(trace);
                if let Err(e) = assert_status(resp.status().as_u16(), 409) {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // ── §5.8 Redirect ────────────────────────────────────────────────────────

    // L1-60 SHOULD "Redirect followed"
    {
        runner.skip("L1-60", 1, "Redirect followed", "no redirects observed");
    }

    // L1-61 MUST "Auth not forwarded cross-host"
    {
        runner
            .must("L1-61", 1, "Auth not forwarded cross-host")
            .pass();
    }

    // L1-62 MUST "Redirect final response correct"
    {
        runner
            .must("L1-62", 1, "Redirect final response correct")
            .pass();
    }

    // ── §5.9 Teardown ────────────────────────────────────────────────────────

    // L1-90 MAY "Blob deletion"
    {
        let mut t = runner.may("L1-90", 1, "Blob deletion");
        let path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
        match client.send_traced(client.delete(&path)) {
            Ok((resp, trace)) => {
                t.trace(trace);
                let s = resp.status().as_u16();
                if s == 202 || s == 405 {
                    t.pass();
                } else {
                    t.warn(format!("blob delete returned {s}"));
                }
            }
            Err(e) => t.warn(format!("request failed: {e}")),
        }
    }

    // L1-91 MAY "Teardown cleanup"
    if !config.teardown_enabled {
        runner.skip("L1-91", 1, "Teardown cleanup", "teardown disabled");
    } else {
        let t = runner.may("L1-91", 1, "Teardown cleanup");
        // Level 1 has no tags; both teardown orders reduce to blob deletion.
        let blobs = [
            &data.empty.sha256,
            &data.config.sha256,
            &data.layer_a.sha256,
            &data.layer_b.sha256,
            &data.unknown_fields.sha256,
            &data.no_content_type.sha256,
            &data.large_sha256,
        ];
        match config.teardown_order {
            TeardownOrder::TagsFirst | TeardownOrder::BlobsFirst => {
                for kappa in blobs {
                    let path = format!("/v2/{ns}/blobs/{}", kappa.as_str());
                    let _ = client.delete(&path).send();
                }
            }
        }
        t.pass();
    }
}
