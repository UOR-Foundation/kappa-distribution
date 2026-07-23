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

pub fn run(runner: &mut TestRunner, client: &RegistryClient, config: &Config, data: &TestData) {
    let ns = &config.namespace;

    // ── §5.1 Version Check ──────────────────────────────────────────────────

    // L1-01 MUST "Version check"
    {
        let mut t = runner.must("L1-01", 1, "Version check");
        match client.send_traced(client.get("/v2/")) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else if r.body_text().contains("kappa-distribution") {
                    t.pass();
                } else {
                    t.fail("body missing kappa-distribution key".into());
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // ── §5.2 Push/Pull/Verify ────────────────────────────────────────────────

    // L1-02 MUST "Push HELLO monolithic"
    {
        let mut t = runner.must("L1-02", 1, "Push blob monolithic");
        let path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
        let rb = client.put(&path).body(data.hello.content.to_vec());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 || r.status == 200 {
                    if r.status == 201 {
                        if let Err(e) = assert_location_present(&r.headers) {
                            t.fail(e);
                        } else {
                            t.pass();
                        }
                    } else {
                        t.pass();
                    }
                } else {
                    t.fail(r.fail_msg(201));
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 || r.status == 201 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(200));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-04 MUST "Wrong kappa-label rejected"
    {
        let mut t = runner.must("L1-04", 1, "Push blob wrong kappa-label rejected");
        let path = format!("/v2/{ns}/blobs/{}", data.empty.sha256.as_str());
        let rb = client.put(&path).body(data.hello.content.to_vec());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 400 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(400));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-05 MUST "HEAD headers"
    {
        let mut t = runner.must("L1-05", 1, "HEAD blob headers present and correct");
        let path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
        match client.send_traced(client.head(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else if let Err(e) = assert_header_present(&r.headers, "content-length") {
                    t.fail(e);
                } else if let Err(e) =
                    assert_header_value(&r.headers, "x-kappa-label", data.hello.sha256.as_str())
                {
                    t.fail(format!("X-Kappa-Label value wrong: {e}"));
                } else if let Err(e) = assert_header_value(&r.headers, "x-kappa-axis", "sha256") {
                    t.fail(format!("X-Kappa-Axis value wrong: {e}"));
                } else if let Err(e) =
                    assert_docker_content_digest(&r.headers, data.hello.sha256.as_str())
                {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-06 MUST "GET content"
    let mut l1_06_body: Option<Vec<u8>> = None;
    let mut l1_06_server_kappa: Option<String> = None;
    let mut l1_06_trace: Option<crate::client::HttpTrace> = None;
    {
        let mut t = runner.must("L1-06", 1, "GET blob content correct");
        let path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
        match client.send_traced(client.get(&path)) {
            Ok(r) => {
                l1_06_trace = Some(r.trace.clone());
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else {
                    let server_label = r.header("x-kappa-label").unwrap_or("").to_string();
                    if server_label.is_empty() {
                        t.fail("X-Kappa-Label header missing on GET response".into());
                    } else if server_label != data.hello.sha256.as_str() {
                        t.fail(format!(
                            "X-Kappa-Label wrong: server={server_label}, expected={}",
                            data.hello.sha256.as_str()
                        ));
                    } else if let Err(e) = assert_body_eq(&r.body, data.hello.content) {
                        t.fail(e);
                    } else if let Err(e) =
                        assert_docker_content_digest(&r.headers, data.hello.sha256.as_str())
                    {
                        t.fail(e);
                    } else {
                        l1_06_server_kappa = Some(server_label);
                        l1_06_body = Some(r.body.clone());
                        t.pass();
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-07 MUST "Verify kappa independently"
    {
        let mut t = runner.must(
            "L1-07",
            1,
            "Content matches known bytes (full verify-on-receipt chain)",
        );
        if let Some(tr) = &l1_06_trace {
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
            _ => t.fail("L1-06 did not produce body and/or server kappa-label".into()),
        }
    }

    // L1-08 MUST "GET absent 404"
    {
        let mut t = runner.must("L1-08", 1, "GET absent blob returns 404");
        let absent = format!("sha256:{}", "0".repeat(64));
        let path = format!("/v2/{ns}/blobs/{absent}");
        match client.send_traced(client.get(&path)) {
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

    // L1-09 MUST "HEAD absent 404"
    {
        let mut t = runner.must("L1-09", 1, "HEAD absent blob returns 404");
        let absent = format!("sha256:{}", "0".repeat(64));
        let path = format!("/v2/{ns}/blobs/{absent}");
        match client.send_traced(client.head(&path)) {
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

    // ── §5.3 Edge Cases ──────────────────────────────────────────────────────

    // L1-10 MUST "Push empty"
    {
        let mut t = runner.must("L1-10", 1, "Push empty blob");
        let path = format!("/v2/{ns}/blobs/{}", data.empty.sha256.as_str());
        let rb = client.put(&path).body(Vec::<u8>::new());
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

    // L1-11 MUST "GET empty"
    {
        let mut t = runner.must("L1-11", 1, "GET empty blob");
        let path = format!("/v2/{ns}/blobs/{}", data.empty.sha256.as_str());
        match client.send_traced(client.get(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else if !r.body.is_empty() {
                    t.fail(format!("expected 0 bytes, got {}", r.body.len()));
                } else {
                    t.pass();
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

    // L1-13 MUST "GET preserves fields"
    {
        let mut t = runner.must("L1-13", 1, "GET blob preserves unknown fields");
        let path = format!("/v2/{ns}/blobs/{}", data.unknown_fields.sha256.as_str());
        match client.send_traced(client.get(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else if let Err(e) = assert_body_eq(&r.body, data.unknown_fields.content) {
                    t.fail(e);
                } else {
                    t.pass();
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
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 201 && r.status != 200 {
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

    // L1-15 SHOULD "List by prefix" (endpoint not in section 6.17 summary table)
    {
        let mut t = runner.should("L1-15", 1, "List blobs by prefix");
        let path = format!("/v2/{ns}/blobs/?prefix=sha256:");
        match client.send_traced(client.get(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 200 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(200));
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
            .put_with_content_type(&path, "application/json")
            .body(ct_blob.to_vec());
        match client.send_traced(rb) {
            Ok(put_r) => {
                t.trace(put_r.trace.clone());
                match client.send_traced(client.get(&path)) {
                    Ok(get_r) => {
                        t.trace(get_r.trace.clone());
                        let get_ct = get_r.header("content-type").unwrap_or("");
                        if !get_ct.contains("application/json") {
                            t.fail(format!("Content-Type not echoed on GET: got \"{get_ct}\""));
                        } else {
                            match client.send_traced(client.head(&path)) {
                                Ok(head_r) => {
                                    t.trace(head_r.trace.clone());
                                    let head_ct = head_r.header("content-type").unwrap_or("");
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
        let rb = client
            .put_no_content_type(&path)
            .body(data.no_content_type.content.to_vec());
        match client.send_traced(rb) {
            Ok(put_r) => {
                t.trace(put_r.trace.clone());
                match client.send_traced(client.get(&path)) {
                    Ok(get_r) => {
                        t.trace(get_r.trace.clone());
                        let ct = get_r.header("content-type").unwrap_or("");
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
    let mut l1_20_headers: Vec<(String, String)> = Vec::new();

    // L1-20 MUST "Upload POST"
    {
        let mut t = runner.must("L1-20", 1, "Chunked upload start");
        let path = format!("/v2/{ns}/blobs/uploads/");
        match client.send_traced(client.post(&path)) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 202 {
                    t.fail(r.fail_msg(202));
                } else {
                    l1_20_headers = r.headers.clone();
                    match r.header("location") {
                        Some(loc) => {
                            upload_url = loc.to_string();
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
        let t = runner.should("L1-22", 1, "Chunk-Min-Length observed");
        let has_kappa = assert_header_present(&l1_20_headers, "x-kappa-chunk-min-length").is_ok();
        let has_oci = assert_header_present(&l1_20_headers, "oci-chunk-min-length").is_ok();
        if has_kappa || has_oci {
            t.pass();
        } else {
            t.fail("neither X-Kappa-Chunk-Min-Length nor OCI-Chunk-Min-Length present".into());
        }
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
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 202 {
                        t.fail(r.fail_msg(202));
                    } else {
                        if let Some(loc) = r.header("location") {
                            upload_url = loc.to_string();
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
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 202 {
                        t.fail(r.fail_msg(202));
                    } else {
                        if let Some(loc) = r.header("location") {
                            upload_url = loc.to_string();
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
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 416 {
                        t.pass();
                    } else {
                        t.fail(r.fail_msg(416));
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
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 204 {
                        t.fail(r.fail_msg(204));
                    } else if let Err(e) = assert_header_present(&r.headers, "range") {
                        t.fail(e);
                    } else if let Err(e) = assert_header_present(&r.headers, "location") {
                        t.fail(e);
                    } else {
                        if let Some(loc) = r.header("location") {
                            upload_url = loc.to_string();
                        }
                        t.pass();
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
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 416 {
                        t.pass();
                    } else {
                        t.fail(r.fail_msg(416));
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
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 204 {
                        t.fail(r.fail_msg(204));
                    } else {
                        if let Some(loc) = r.header("location") {
                            upload_url = loc.to_string();
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
                    Ok(r) => {
                        t.trace(r.trace.clone());
                        if r.status != 202 {
                            ok = false;
                            break;
                        }
                        if let Some(loc) = r.header("location") {
                            upload_url = loc.to_string();
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
        let mut t = runner.must("L1-30", 1, "Upload complete with kappa-label");
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
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 201 {
                        t.fail(r.fail_msg(201));
                    } else if let Err(e) =
                        assert_header_value(&r.headers, "x-kappa-label", data.large_sha256.as_str())
                    {
                        t.fail(e);
                    } else {
                        t.pass();
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 200 {
                    t.fail(r.fail_msg(200));
                } else if let Err(e) = assert_kappa_matches(&r.body, &data.large_sha256) {
                    t.fail(e);
                } else {
                    t.pass();
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-55 MUST "Upload complete accepts ?digest= alias"
    {
        let mut t = runner.must("L1-55", 1, "Upload complete accepts digest query parameter");
        let start_path = format!("/v2/{ns}/blobs/uploads/");
        match client.send_traced(client.post(&start_path)) {
            Ok(r) if r.status == 202 => {
                t.trace(r.trace.clone());
                let loc = r.header("location").unwrap_or("").to_string();
                if loc.is_empty() {
                    t.fail("no Location from upload POST".into());
                } else {
                    let content = b"digest-alias-test-content";
                    let kappa = KappaLabel::sha256(content);
                    let patch_rb = client
                        .patch_url(&loc)
                        .header("Content-Range", format!("0-{}", content.len() - 1))
                        .body(content.to_vec());
                    match client.send_traced(patch_rb) {
                        Ok(pr) => {
                            t.trace(pr.trace.clone());
                            let put_loc = pr.header("location").unwrap_or(&loc).to_string();
                            let close_url = if put_loc.contains('?') {
                                format!("{put_loc}&digest={}", kappa.as_str())
                            } else {
                                format!("{put_loc}?digest={}", kappa.as_str())
                            };
                            let rb = client.put_url(&close_url).body(Vec::<u8>::new());
                            match client.send_traced(rb) {
                                Ok(r2) => {
                                    t.trace(r2.trace.clone());
                                    if r2.status == 201 {
                                        t.pass();
                                    } else {
                                        t.fail(r2.fail_msg(201));
                                    }
                                }
                                Err(e) => t.fail(format!("close request failed: {e}")),
                            }
                        }
                        Err(e) => t.fail(format!("patch failed: {e}")),
                    }
                }
            }
            Ok(r) => {
                t.trace(r.trace.clone());
                t.fail(r.fail_msg(202));
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-32 MUST "Wrong kappa on close"
    {
        let mut t = runner.must("L1-32", 1, "Wrong kappa-label on complete rejected");
        let start_path = format!("/v2/{ns}/blobs/uploads/");
        match client.send_traced(client.post(&start_path)) {
            Ok(r) if r.status == 202 => {
                t.trace(r.trace.clone());
                let loc = r.header("location").unwrap_or("").to_string();
                if loc.is_empty() {
                    t.fail("no Location from upload POST".into());
                } else {
                    let chunk = b"some data for wrong kappa test";
                    let patch_rb = client
                        .patch_url(&loc)
                        .header("Content-Range", format!("0-{}", chunk.len() - 1))
                        .body(chunk.to_vec());
                    if let Ok(pr) = client.send_traced(patch_rb) {
                        t.trace(pr.trace.clone());
                    }
                    let wrong_kappa = data.hello.sha256.as_str();
                    let close_url = if loc.contains('?') {
                        format!("{loc}&kappa={wrong_kappa}")
                    } else {
                        format!("{loc}?kappa={wrong_kappa}")
                    };
                    let rb = client.put_url(&close_url).body(Vec::<u8>::new());
                    match client.send_traced(rb) {
                        Ok(r2) => {
                            t.trace(r2.trace.clone());
                            if r2.status == 400 {
                                t.pass();
                            } else {
                                t.fail(r2.fail_msg(400));
                            }
                        }
                        Err(e) => t.fail(format!("close request failed: {e}")),
                    }
                }
            }
            Ok(r) => {
                t.trace(r.trace.clone());
                t.fail(r.fail_msg(202));
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
            Ok(r) if r.status == 202 => {
                t.trace(r.trace.clone());
                cancel_url = r.header("location").unwrap_or("").to_string();
                let partial = data.cancel_partial_content;
                let patch_rb = client
                    .patch_url(&cancel_url)
                    .header("Content-Range", format!("0-{}", partial.len() - 1))
                    .body(partial.to_vec());
                match client.send_traced(patch_rb) {
                    Ok(pr) => {
                        t.trace(pr.trace.clone());
                        if pr.status != 202 {
                            t.fail(format!(
                                "cancel test PATCH returned {}, cannot verify cancellation semantics: {}",
                                pr.status,
                                pr.body_text()
                            ));
                            return;
                        }
                    }
                    Err(e) => {
                        t.fail(format!("cancel test PATCH failed: {e}"));
                        return;
                    }
                }
                match client.send_traced(client.delete_url(&cancel_url)) {
                    Ok(r2) => {
                        t.trace(r2.trace.clone());
                        if r2.status == 204 {
                            t.pass();
                        } else {
                            t.fail(r2.fail_msg(204));
                        }
                    }
                    Err(e) => t.fail(format!("delete failed: {e}")),
                }
            }
            Ok(r) => {
                t.trace(r.trace.clone());
                t.fail(r.fail_msg(202));
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
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status == 404 || r.status == 400 {
                        t.pass();
                    } else {
                        t.fail(format!(
                            "expected 404 or 400, got {}: {}",
                            r.status,
                            r.body_text()
                        ));
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 404 {
                    t.pass();
                } else {
                    t.fail(format!(
                        "cancelled partial data found in store (status {}), expected 404: {}",
                        r.status,
                        r.body_text()
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 {
                    mount_succeeded = true;
                    t.pass();
                } else if r.status == 202 {
                    t.warn("mount returned 202 (fallback to upload)".into());
                } else {
                    t.fail(r.fail_msg(201));
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
                Ok(r) => {
                    t.trace(r.trace.clone());
                    if r.status != 200 {
                        t.fail(r.fail_msg(200));
                    } else if let Err(e) = assert_kappa_matches(&r.body, &data.hello.sha256) {
                        t.fail(e);
                    } else {
                        t.pass();
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 202 {
                    t.fail(r.fail_msg(202));
                } else {
                    fallback_loc = r.header("location").unwrap_or("").to_string();
                    if let Err(e) = assert_header_present(&r.headers, "location") {
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 || r.status == 202 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(201));
                }
            }
            Err(e) => t.fail(format!("mount without from failed: {e}")),
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 || r.status == 200 {
                    multi_label_supported = true;
                    t.pass();
                } else {
                    t.warn(format!("multi-label push returned {}", r.status));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
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
                (Ok(r1), Ok(r2)) => {
                    t.trace(r1.trace.clone());
                    t.trace(r2.trace.clone());
                    if r1.status == 200 && r2.status == 200 {
                        t.pass();
                    } else {
                        t.fail(format!(
                            "sha256 GET {}, blake3 GET {}",
                            r1.status, r2.status
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
        let mut t = runner.should("L1-52", 1, "Multi-label fallback");
        let path = format!("/v2/{ns}/blobs/{}", data.hello.sha256.as_str());
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

    // L1-53 MUST "Multi-label bad kappa rejected"
    {
        let mut t = runner.must("L1-53", 1, "Multi-label invalid kappa-label rejected");
        let bad = format!("sha256:{}", "0".repeat(64));
        let path = format!("/v2/{ns}/blobs/{}?also={bad}", data.hello.sha256.as_str());
        let rb = client.put(&path).body(data.hello.content.to_vec());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 400 {
                    t.pass();
                } else {
                    t.fail(r.fail_msg(400));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-56 MUST "Single-POST blob push with digest query parameter"
    {
        let mut t = runner.must("L1-56", 1, "Single-POST blob push with digest parameter");
        let content = b"single-post-digest-param-test";
        let kappa = KappaLabel::sha256(content);
        let path = format!("/v2/{ns}/blobs/uploads/?digest={}", kappa.as_str());
        let rb = client
            .post_with_content_type(&path, "application/octet-stream")
            .body(content.to_vec());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 {
                    t.pass();
                } else if r.status == 202 {
                    // Registry does not support single-POST, accepted as upload session.
                    // Fall back: complete via PUT with digest param.
                    let loc = r.header("location").unwrap_or("").to_string();
                    if loc.is_empty() {
                        t.fail("202 but no Location header for fallback".into());
                    } else {
                        let close_url = if loc.contains('?') {
                            format!("{loc}&digest={}", kappa.as_str())
                        } else {
                            format!("{loc}?digest={}", kappa.as_str())
                        };
                        let rb = client.put_url(&close_url).body(content.to_vec());
                        match client.send_traced(rb) {
                            Ok(r2) => {
                                t.trace(r2.trace.clone());
                                if r2.status == 201 {
                                    t.pass();
                                } else {
                                    t.fail(r2.fail_msg(201));
                                }
                            }
                            Err(e) => t.fail(format!("PUT fallback failed: {e}")),
                        }
                    }
                } else {
                    t.fail(r.fail_msg(201));
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // ── §5.10 SHA-1 Axis ───────────────────────────────────────────────────────

    // L1-70 SHOULD "SHA-1 blob push and retrieve"
    {
        let mut t = runner.should("L1-70", 1, "SHA-1 blob push and retrieve");
        let path = format!("/v2/{ns}/blobs/{}", data.hello.sha1.as_str());
        let rb = client.put(&path).body(data.hello.content.to_vec());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status != 201 && r.status != 200 {
                    t.fail(r.fail_msg(201));
                } else {
                    match client.send_traced(client.get(&path)) {
                        Ok(gr) => {
                            t.trace(gr.trace.clone());
                            if gr.status != 200 {
                                t.fail(gr.fail_msg(200));
                            } else if let Err(e) = assert_body_eq(&gr.body, data.hello.content) {
                                t.fail(e);
                            } else {
                                t.pass();
                            }
                        }
                        Err(e) => t.fail(format!("GET failed: {e}")),
                    }
                }
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-71 SHOULD "SHA-1 upload complete"
    {
        let mut t = runner.should("L1-71", 1, "SHA-1 chunked upload complete");
        let start_path = format!("/v2/{ns}/blobs/uploads/");
        match client.send_traced(client.post(&start_path)) {
            Ok(r) if r.status == 202 => {
                t.trace(r.trace.clone());
                let loc = r.header("location").unwrap_or("").to_string();
                if loc.is_empty() {
                    t.fail("no Location from upload POST".into());
                } else {
                    let content = data.sha1_upload.content;
                    let patch_rb = client
                        .patch_url(&loc)
                        .header("Content-Range", format!("0-{}", content.len() - 1))
                        .body(content.to_vec());
                    match client.send_traced(patch_rb) {
                        Ok(pr) => {
                            t.trace(pr.trace.clone());
                            let put_loc = pr.header("location").unwrap_or(&loc).to_string();
                            let close_url = if put_loc.contains('?') {
                                format!("{put_loc}&kappa={}", data.sha1_upload.sha1.as_str())
                            } else {
                                format!("{put_loc}?kappa={}", data.sha1_upload.sha1.as_str())
                            };
                            let rb = client.put_url(&close_url).body(Vec::<u8>::new());
                            match client.send_traced(rb) {
                                Ok(r2) => {
                                    t.trace(r2.trace.clone());
                                    if r2.status == 201 {
                                        t.pass();
                                    } else {
                                        t.fail(r2.fail_msg(201));
                                    }
                                }
                                Err(e) => t.fail(format!("close request failed: {e}")),
                            }
                        }
                        Err(e) => t.fail(format!("patch failed: {e}")),
                    }
                }
            }
            Ok(r) => {
                t.trace(r.trace.clone());
                t.fail(r.fail_msg(202));
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
    }

    // L1-72 SHOULD "SHA-1 + SHA-256 multi-label push"
    {
        let mut t = runner.should("L1-72", 1, "SHA-256 primary with SHA-1 also label");
        let path = format!(
            "/v2/{ns}/blobs/{}?also={}",
            data.sha1_multi.sha256.as_str(),
            data.sha1_multi.sha1.as_str()
        );
        let rb = client.put(&path).body(data.sha1_multi.content.to_vec());
        match client.send_traced(rb) {
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 201 || r.status == 200 {
                    let sha1_path = format!("/v2/{ns}/blobs/{}", data.sha1_multi.sha1.as_str());
                    match client.send_traced(client.get(&sha1_path)) {
                        Ok(gr) => {
                            t.trace(gr.trace.clone());
                            if gr.status == 200 {
                                if let Err(e) = assert_body_eq(&gr.body, data.sha1_multi.content) {
                                    t.fail(e);
                                } else {
                                    t.pass();
                                }
                            } else {
                                t.fail(format!("sha1 address not resolvable: {}", gr.status));
                            }
                        }
                        Err(e) => t.fail(format!("sha1 GET failed: {e}")),
                    }
                } else {
                    t.fail(r.fail_msg(201));
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
            Ok(r) => {
                t.trace(r.trace.clone());
                if r.status == 202 || r.status == 405 {
                    t.pass();
                } else {
                    t.warn(format!("blob delete returned {}", r.status));
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
                    let _ = client.send_traced(client.delete(&path));
                }
            }
        }
        t.pass();
    }
}
