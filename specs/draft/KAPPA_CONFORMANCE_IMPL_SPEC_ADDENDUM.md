# kappa-conformance Implementation Specification — Addendum
## v1.0.0-addendum-1
Date: 2026-07-21
Applies to: KAPPA_CONFORMANCE_IMPL_SPEC.md (v1.0.0)
Status: Normative
---
This addendum is read alongside the base impl spec. It provides
the per-test expansion that makes each test independently
implementable. Every test in the base spec's §5-§9 is expanded
here with: exact HTTP method and path, exact request headers,
exact request body (by TestData field name), exact expected
response status, exact response header assertions, exact body
assertions, assertion function calls with arguments, preconditions,
and side effects.
Tests not expanded here are unchanged from the base spec.
Notation:
    {ns}        = config.namespace
    {κ}         = a KappaLabel value (specified per test)
    →           = request
    ←           = expected response
    assert:     = assertion function call
    capture:    = value stored in test-suite state for subsequent tests
    precondition: = state that must exist before this test runs
    side effect:  = state this test creates for subsequent tests
---
## E1. Level 1 Expansion — Tests Not Fully Specified in Base
### L1-15
    precondition: L1-02, L1-10, L1-12, L1-14 have run (≥5 blobs stored)
    → GET /v2/{ns}/blobs/?prefix=sha256:
    headers: (standard auth + user-agent)
    ← 200
    body: JSON (implementation-defined format)
    assert_status(200)
    If 200: parse body, verify ≥5 κ-labels present
    If non-200: this endpoint is not standardized across
    implementations. Emit SHOULD warning, do not fail.
    side effect: none
### L1-41
    precondition: L1-40 returned 201 (mount succeeded)
    If L1-40 was not 201: SKIP "mount not supported"
    → GET /v2/{ns}-mount/blobs/{hello.sha256}
    headers: (standard)
    ← 200
    assert_status(200)
    assert_kappa_matches(body, hello.sha256)
    side effect: none
### L1-43
    precondition: L1-42 returned 202 with Location header
    If L1-42 was not 202: SKIP
    assert: Location header from L1-42 is non-empty
    assert: Location from L1-42, when resolved via
            resolve_location(base_url, loc), produces a URL
            that starts with "http"
    No HTTP request — this validates the L1-42 response.
    side effect: none
### L1-51
    precondition: L1-50 returned 201 or 200 (multi-label supported)
    If L1-50 was not 201/200: SKIP
    → GET /v2/{ns}/blobs/{hello.sha256}
    ← 200
    assert_status(200)
    assert_kappa_matches(body, hello.sha256)
    → GET /v2/{ns}/blobs/{hello.blake3}
    ← 200
    assert_status(200)
    assert_kappa_matches(body_blake3, hello.blake3)
    Note: assert_kappa_matches for blake3 uses KappaLabel::blake3(body)
    side effect: none
### L1-52
    precondition: L1-50 returned non-201/200 (multi-label not supported)
    If L1-50 was 201/200: SKIP
    → GET /v2/{ns}/blobs/{hello.sha256}
    ← 200
    assert_status(200)
    Primary κ-label still works despite multi-label failure.
    side effect: none
### L1-60
    precondition: none (opportunistic — checks if any prior L1
    request received a 3xx redirect)
    If no redirects observed in Level 1: SKIP "no redirects observed"
    If redirect observed:
    assert: the final response after following redirects was 200
    assert: the redirect was followed (reqwest does this by default)
    side effect: none
    Note: this test validates the test harness configuration,
    not a specific registry endpoint. It confirms
    KAPPA_FOLLOW_REDIRECTS=true works.
### L1-61
    precondition: none
    This test passes by construction: the RegistryClient's redirect
    policy strips Authorization on cross-host redirects. The test
    records PASS and documents that the policy is configured.
    If KAPPA_FOLLOW_REDIRECTS=false: SKIP "redirects disabled"
    No HTTP request — this validates the client configuration.
    side effect: none
### L1-62
    precondition: a redirect was observed in Level 1
    If no redirects observed: SKIP
    assert: the final response body from the redirected request
            matches the expected κ-label (re-hash body, compare)
    No new HTTP request — this validates the body from the
    redirected request already captured in L1-60.
    side effect: none
### L1-91
    precondition: L1-90 has run
    If KAPPA_TEARDOWN_ENABLED=false: SKIP
    If KAPPA_TEARDOWN_ORDER=tags-first:
        (no tags at Level 1 — proceed to blobs)
    Delete each remaining test blob:
        for κ in [empty.sha256, config.sha256, layer_a.sha256,
                  layer_b.sha256, unknown_fields.sha256, large_sha256]:
            → DELETE /v2/{ns}/blobs/{κ}
            ← 202 or 405
            (do not fail on any status — teardown is best-effort)
    side effect: test namespace cleaned
---
## E2. Level 2 Expansion
### L2-02
    precondition: L2-01 has run and returned a response
    This test validates the response from L2-01, not a new request.
    assert_header_present(L2_01_response, "x-kappa-label")
    let label = L2_01_response.header("x-kappa-label")
    assert_kappa_valid(label)
    assert: label == data.simple_manifest_sha256.as_str()
    side effect: none
### L2-32
    precondition: CAS mechanism discoverable
    If no CAS mechanism: SKIP "CAS not discoverable"
    Attempt CAS on a tag name that does not exist, with a
    sentinel value indicating "expected absent":
    Implementation-specific: if the registry supports
    If-None-Match: * on PUT /manifests/{tag}:
        → PUT /v2/{ns}/manifests/cas-create
          header If-None-Match: *
          body: simple_manifest
        ← 201
    If not discoverable: SKIP
    side effect: tag cas-create bound if succeeded
### L2-33
    precondition: L2-30 succeeded (CAS success path ran)
    If L2-30 was SKIP: SKIP
    → GET /v2/{ns}/manifests/{tag_used_in_L2_30}
    ← 200
    assert: X-Kappa-Label matches the κ-label CAS'd in L2-30
    side effect: none
### L2-34
    precondition: L2-31 succeeded (CAS conflict path ran)
    If L2-31 was SKIP: SKIP
    → GET /v2/{ns}/manifests/{tag_used_in_L2_31}
    ← 200
    assert: X-Kappa-Label matches the value BEFORE the failed CAS
            (unchanged)
    side effect: none
### L2-40
    → PUT /v2/{ns}/manifests/mtag-a?tag=mtag-b
      header Content-Type: application/json
      body: data.simple_manifest
    ← 201 or 200 (supported) or 4xx (not supported)
    If 201/200: capture multi_tag_supported = true
    If 4xx: multi_tag_supported = false, WARN
    side effect: tags mtag-a and mtag-b bound if supported
### L2-41
    precondition: L2-40 returned 201/200
    If not: SKIP
    → GET /v2/{ns}/manifests/mtag-a
    ← 200
    let κ_a = response.header("x-kappa-label")
    → GET /v2/{ns}/manifests/mtag-b
    ← 200
    let κ_b = response.header("x-kappa-label")
    assert: κ_a == κ_b
    assert: κ_a == data.simple_manifest_sha256.as_str()
    side effect: none
### L2-42
    precondition: L2-40 returned 4xx (not supported)
    If L2-40 was 201/200: SKIP
    → GET /v2/{ns}/manifests/mtag-a
    ← 200 (the primary tag should still work even if multi-tag
           was rejected — the single-tag path succeeded)
    OR ← 404 (the entire PUT was rejected)
    If 200: PASS  If 404: WARN "multi-tag rejection also rejected primary"
    side effect: none
### L2-61
    precondition: L2-60 returned 400
    If L2-60 was not 400: SKIP "deletion was accepted or disabled"
    let body = L2_60_response.body
    verify_error_response(body, "UNSUPPORTED")
    side effect: none
### L2-62
    precondition: L2-60 returned 202 (deletion accepted)
    If not: SKIP
    → GET /v2/{ns}/manifests/v1.0
    ← 404
    assert_status(404)
    side effect: tag v1.0 is gone
### L2-63
    precondition: L2-60 returned 202 (deletion accepted)
    If not: SKIP
    → GET /v2/{ns}/blobs/{data.simple_manifest_sha256}
    ← 200
    assert_status(200)
    Content survives tag deletion — the blob is not deleted
    when its tag is removed.
    side effect: none
### L2-64
    precondition: KAPPA_TEARDOWN_ENABLED=true
    If false: SKIP
    Delete remaining tags per KAPPA_TEARDOWN_ORDER:
        for tag in [latest, mtag-a, mtag-b, 2026-07-20T00:00:00,
                    2026-07-20T12:00:00, cas-create, ...]:
            → DELETE /v2/{ns}/manifests/{tag}
            ← 202 or 404 or 405 (best-effort, do not fail)
    side effect: test tags cleaned
---
## E3. Level 3 Expansion
### L3-02
    precondition: L3-01 has run and returned a response
    This validates the L3-01 response, not a new request.
    assert_header_present(L3_01_response, "x-kappa-label")
    let label_str = L3_01_response.header("x-kappa-label")
    assert_kappa_valid(label_str)
    The label is already captured as edge_kappa in L3-01.
    side effect: none
### L3-04
    precondition: L3-03 has run and returned the edge blob body
    let body = L3_03_response.body
    let expected = edge_canon::encode_edge(
        &data.simple_manifest_sha256,
        "derives-from",
        &data.config.sha256,
        &[0xA0],  // CBOR empty map
    )
    assert_body_eq(body, expected)
    This test verifies the registry stores the edge in the
    canonical form defined in Appendix A of the protocol spec.
    If the registry uses a different internal format, this test
    fails — edge blobs MUST be stored in canonical form because
    their κ-label is computed from the canonical bytes.
    side effect: none
### L3-13
    precondition: L1-14 and L2-01 have run (multi_manifest and
    simple_manifest blobs exist)
    → PUT /v2/{ns}/edges/
      header Content-Type: application/json
      body:
      {
          "source": "{data.multi_manifest_sha256}",
          "relation": "refers-to",
          "target": "{data.simple_manifest_sha256}",
          "metadata": {}
      }
    ← 201 or 200
    assert_status_in(status, [201, 200])
    capture: referrer_edge_kappa from X-Kappa-Label header
    side effect: referrer edge exists from multi→simple
### L3-14
    precondition: L3-13 has run
    → GET /v2/{ns}/edges/{data.simple_manifest_sha256}?direction=inbound&relation=refers-to
    ← 200
    assert_status(200)
    let edges = parse body.edges array
    assert: edges contains entry where source == data.multi_manifest_sha256
    side effect: none
### L3-15
    precondition: hello blob exists (L1-02 or Level 3 setup)
    let absent_subject = format!("sha256:{}", "c".repeat(64))
    → PUT /v2/{ns}/edges/
      body:
      {
          "source": "{data.hello.sha256}",
          "relation": "refers-to",
          "target": "{absent_subject}",
          "metadata": {}
      }
    ← 201 or 200
    assert_status_in(status, [201, 200])
    Target does not exist — edge MUST still be accepted (lazy
    replication tolerance, §5.9 invariant 7).
    side effect: edge to absent subject exists
### L3-16
    precondition: L3-15 has run
    let absent_subject = format!("sha256:{}", "c".repeat(64))
    → GET /v2/{ns}/edges/{absent_subject}?direction=inbound
    ← 200
    assert_status(200)
    let edges = parse body.edges array
    assert: edges contains entry where source == data.hello.sha256
            and relation == "refers-to"
    The subject blob does not exist. The edge is still queryable.
    This tests that edge indexes work for absent targets.
    side effect: none
### L3-17
    precondition: ≥2 edges exist from prior L3 tests
    → GET /v2/{ns}/edges/{data.simple_manifest_sha256}?direction=outbound&n=1
    ← 200
    let edges = parse body.edges
    assert: edges.len() <= 1
    If edges.len() == 1 and more edges exist:
        assert: response has a pagination mechanism (Link header
                or "last" field in body)
        PASS
    If edges.len() == 0 or only 1 edge exists total:
        WARN "insufficient edges to test pagination"
    side effect: none
### L3-21
    precondition: L3-20 has run (sacrificial edge deleted)
    → GET /v2/{ns}/edges/{data.simple_manifest_sha256}?direction=outbound
    ← 200
    let edges = parse body.edges
    assert: edges does NOT contain the deleted edge's κ-label
    side effect: none
---
## E4. Level 4 Expansion
### L4-02
    precondition: L4-01 has run
    This validates the L4-01 response body, not a new request.
    let body: serde_json::Value = L4_01_response.body
    assert_json_field(body, "composed", composed_kappa.as_str())
    assert_json_field(body, "witness", witness_kappa.as_str())
    assert_json_field(body, "operation", "g2")
    let operands = body["operands"].as_array()
    assert: operands is Some
    assert: operands contains data.hello.sha256.as_str()
    assert: operands contains data.config.sha256.as_str()
    side effect: none
### L4-11
    precondition: L4-10 has run and returned 422
    let body = L4_10_response.body
    verify_error_response(body, "AXIS_MISMATCH")
    side effect: none
### L4-23
    precondition: L4-22 has run, e6_kappa captured
    Decode the first hex byte of data.hello.sha256's digest:
        let hex_str = data.hello.sha256.as_str()  // "sha256:2cf2..."
        let first_hex = &hex_str[7..9]            // "2c"
        let first_byte = u8::from_str_radix(first_hex, 16) // 0x2c = 44
        let class = first_byte % 9                // 44 % 9 = 8
    If class ∈ 0..=7: expected tag byte = 0x05
    If class == 8:    expected tag byte = 0x06
    → GET /v2/{ns}/blobs/{e6_kappa}
    ← 200
    let composed_bytes = response.body
    The composed content's canonical form starts with the tag byte.
    Retrieve the composed blob and verify the tag byte is present
    in the content at the expected position.
    Note: the exact position of the tag byte depends on the
    canonical form (Appendix B.3 of protocol spec):
    [tag] || operand_label_bytes. The tag is byte 0 of the content.
    assert: composed_bytes[0] == expected_tag_byte
    side effect: none
### L4-27
    precondition: L4-20, L4-25 have run (f4_kappa, e8_kappa captured)
    If f4_kappa.as_str()[7..] == e8_kappa.as_str()[7..]:
        (digests are identical — coincidence observed)
        Log in report: "F4/E8 digest coincidence observed for
        operand {hello.sha256}: f4={f4_kappa} e8={e8_kappa}"
        PASS (informational)
    Else:
        Log in report: "No F4/E8 digest coincidence for this operand"
        PASS (informational)
    Similarly check e7 result if captured.
    side effect: none (logging only)
### L4-28
    precondition: L4-27 observed a coincidence
    If no coincidence observed: SKIP "no coincidence to disambiguate"
    For each pair of composed κ-labels with identical digests:
    → GET /v2/{ns}/edges/{coincident_kappa}?direction=outbound&relation=composed-of
    ← 200
    let edges = parse body.edges
    For each edge:
        assert: edge.metadata contains an "operation" key
        assert: the operation values differ between the
                coincident compositions (e.g., "f4" vs "e8")
    The edge metadata is the disambiguation mechanism. Without it,
    two different operations producing the same digest would be
    indistinguishable.
    side effect: none
### L4-47
    precondition: L4-40 has run, schema_kappa captured
    Construct an updated schema that adds an optional field:
    let updated_schema = {
        "scope": "test-scope",
        "format": "json-schema",
        "validation": {
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {"type": "string"},
                "description": {"type": "string"}
            }
        }
    }
    → PUT /v2/{ns}/schemas/test-scope
      header Content-Type: application/json
      body: updated_schema
    ← 201
    capture: new_schema_kappa from X-Kappa-Label
    assert: new_schema_kappa != schema_kappa
    side effect: schema updated, new κ captured
### L4-48
    precondition: L4-47 has run, new_schema_kappa captured
    → GET /v2/{ns}/edges/{new_schema_kappa}?direction=outbound&relation=derives-from
    ← 200
    let edges = parse body.edges
    assert: edges contains entry where target == schema_kappa
    The new schema blob is linked to the old schema blob by a
    derives-from edge, forming the schema evolution chain.
    side effect: none
---
## E5. Level 5 Expansion
### L5-02
    precondition: L5-01 has run and returned a response
    This validates L5-01's response.
    assert_header_present(L5_01_response, "x-kappa-label")
    let pin_label = L5_01_response.header("x-kappa-label")
    assert_kappa_valid(pin_label)
    capture: pin_kappa = parse(pin_label)
    side effect: pin_kappa available for L5-09
### L5-04
    precondition: L5-03 has run (owns edge from hello → layer_a)
    Check if layer_a is tagged:
    → GET /v2/{ns}/tags/list
    ← 200
    If tag list contains a tag pointing to layer_a.sha256:
        → DELETE /v2/{ns}/manifests/{that_tag}
        ← 202 or 405
    If no tag points to layer_a: PASS (already untagged)
    side effect: layer_a has no tag root (only reachable via
    owns edge from pinned hello)
### L5-10
    precondition: L5-09 has run and returned 200
    If L5-09 response has X-Kappa-Label header:
        let new_pin_kappa = response.header("x-kappa-label")
        assert: new_pin_kappa != pin_kappa from L5-02
        PASS
    If no X-Kappa-Label: WARN "unpin did not return new pin blob κ"
    side effect: none
### L5-12
    precondition: L5-11 has triggered a sweep
    let start = Instant::now()
    loop {
        → GET /v2/{ns}/gc/status
        ← 200
        let body: Value = response.json()
        if body["last_sweep"] is a timestamp within 120s of now:
            break (sweep complete)
        if start.elapsed() > 60s:
            FAIL "sweep did not complete within 60 seconds"
            return
        sleep(1s)
    }
    PASS
    side effect: sweep complete, GC has run
### L5-14
    precondition: L5-12 complete (sweep ran after hello unpinned)
    → GET /v2/{ns}/blobs/{data.layer_a.sha256}
    ← 404 (expected) or 200 (GC timing)
    If 404: PASS — layer_a evicted (no longer reachable: hello
            was its only owner, hello was unpinned)
    If 200: WARN "unreachable blob not yet evicted (timing)"
    side effect: none
### L5-18
    precondition: L5-17 confirmed finalizer in status
    The finalizer release mechanism is implementation-specific.
    Attempt: POST a release pin blob or use an implementation-
    specific release endpoint. Options the test tries in order:
    1. POST /v2/{ns}/gc/pin
       body: {"kappa":"{config.sha256}","ttl":0,
              "controller":"test-ctrl","released":true}
       ← 201 or 200
    2. If (1) fails: POST /v2/{ns}/gc/release
       body: {"controller":"test-ctrl","kappa":"{config.sha256}"}
       ← 200
    3. If (2) fails: SKIP "finalizer release mechanism not
       discoverable"
    side effect: finalizer released (if succeeded)
### L5-19
    precondition: L5-18 succeeded
    If L5-18 was SKIP: SKIP
    → POST /v2/{ns}/gc/unpin
      body: {"pin_kappa":"{finalizer_pin_kappa}"}
    ← 200
    assert_status(200)
    side effect: config.sha256 unpinned
### L5-20
    precondition: L5-19 succeeded
    If L5-19 was SKIP: SKIP
    → POST /v2/{ns}/gc/sweep
    ← 202
    Poll gc/status (same as L5-12).
    → GET /v2/{ns}/blobs/{data.config.sha256}
    ← 404 or 200
    If 404: PASS
    If 200: WARN "finalized blob not yet evicted"
    side effect: none
### L5-32
    precondition: L5-30 succeeded (filter registered)
    If L5-30 was SKIP: SKIP
    → PUT /v2/{ns}/manifests/filter-pass
      header Content-Type: application/json
      body: {"data":"this should accept the content"}
    ← 201
    assert_status(201)
    The content contains "accept" which the JSON-match filter
    accepts.
    side effect: filter-pass tag bound
### L5-34
    precondition: L5-33 has run and returned 422
    If L5-33 was SKIP: SKIP
    let body = L5_33_response.body
    verify_error_response(body, "FILTER_REJECTED")
    Parse errors[0]:
    assert: errors[0].message or errors[0].detail is non-empty
    The filter's rejection reason ("must contain accept") should
    appear in the error response.
    side effect: none
### L5-55
    precondition: L5-54 has run, timing captured for L5-51 and L5-54
    let first_fetch_ms = L5_51_duration_ms
    let cached_fetch_ms = L5_54_duration_ms
    If cached_fetch_ms < first_fetch_ms:
        PASS — cached response was faster
    Else:
        WARN "cached federation fetch not faster than first
        ({cached_fetch_ms}ms vs {first_fetch_ms}ms)"
    side effect: none (informational)
### L5-56
    precondition: KAPPA_FEDERATION_PEERS has ≥2 entries
    If <2 peers: SKIP "only one peer configured"
    let peer2_url = config.federation_peers[1]
    let peer2_content = format!("federation-peer2-{}", uuid::Uuid::new_v4())
    let peer2_kappa = KappaLabel::sha256(peer2_content.as_bytes())
    → PUT {peer2_url}/v2/{ns}/blobs/{peer2_kappa}
      body: peer2_content
    ← 201 or 200
    → GET /v2/{ns}/blobs/{peer2_kappa}  (from primary)
    ← 200 (federation from peer2) or 404
    If 200: assert_kappa_matches(body, peer2_kappa). PASS.
    If 404: WARN "multi-peer federation not supported"
    side effect: peer2_kappa may be cached on primary
### L5-57
    precondition: L5-56 succeeded AND topology supports multi-hop
    Multi-hop requires: primary can reach peer1, peer1 can reach
    peer2, primary cannot directly reach peer2. This topology is
    not discoverable from environment variables.
    If L5-56 succeeded (primary fetched from peer2 directly):
        SKIP "primary reached peer2 directly, multi-hop not tested"
    If L5-56 returned 404 and peer1 might relay:
        → GET {peer1_url}/v2/{ns}/blobs/{peer2_kappa}
        If peer1 has it (relayed from peer2):
            → GET /v2/{ns}/blobs/{peer2_kappa} (primary, via peer1)
            If 200: PASS (multi-hop: primary→peer1→peer2)
            If 404: WARN
        Else: SKIP "topology does not support multi-hop test"
    side effect: none
### L5-58
    precondition: L5-57 returned 200 (multi-hop succeeded)
    If L5-57 was not 200: SKIP
    assert_kappa_matches(body_from_L5_57, peer2_kappa)
    Verify-on-receipt: content traversed multiple hops and
    re-hash confirms integrity end-to-end.
    side effect: none
### L5-61
    precondition: L5-60 observed a 429 response
    If L5-60 was SKIP: SKIP
    let retry_after = L5_60_response.header("retry-after")
    assert: retry_after is present
    assert: retry_after parses as:
        a positive integer (seconds), OR
        an HTTP-date per RFC 9110 §10.2.3
    side effect: none
### L5-62
    precondition: any 429 observed
    If no 429 observed: SKIP "no rate limiting observed"
    Log in report:
        "Rate limiting observed: {count} requests received 429.
        Retry-After values: {list}. Average backoff: {avg}s."
    PASS (informational recording)
    side effect: none
---
## E6. Implementation Constraints Expansion
### E6.1 Upload Location Tracking
The base spec §15.10 states: "capture updated Location from
every PATCH response." The expansion:
Every PATCH in L1-23, L1-24, L1-29 MUST:
    let new_loc = response.header("location");
    if let Some(loc) = new_loc {
        upload_url = resolve_location(&client.base_url, loc);
    }
    // else: upload_url unchanged from previous value
The upload_url used by the NEXT PATCH or PUT is always the most
recently received Location. This follows OCI's pattern where the
Location MAY change between chunks (the server may redirect to a
different storage node mid-upload).
### E6.2 Edge Canonical Form Encoder
The edge_canon.rs module implements:
    pub fn encode_edge(
        source: &KappaLabel,
        relation: &str,
        target: &KappaLabel,
        metadata: &[u8],
    ) -> Vec<u8> {
        let src = source.as_bytes();
        let rel = relation.as_bytes();
        let tgt = target.as_bytes();
        let meta_len = (metadata.len() as u32).to_be_bytes();
        let mut out = Vec::with_capacity(
            src.len() + 1 + rel.len() + 1 + tgt.len() + 1 + 4 + metadata.len()
        );
        out.extend_from_slice(src);
        out.push(0x00);
        out.extend_from_slice(rel);
        out.push(0x00);
        out.extend_from_slice(tgt);
        out.push(0x00);
        out.extend_from_slice(&meta_len);
        out.extend_from_slice(metadata);
        out
    }
For all Level 3 tests, metadata is &[0xA0] (CBOR empty map, 1 byte).
The encode produces the canonical form whose SHA-256 hash equals
the edge κ-label returned by the registry.
### E6.3 F4 Complement Computation
The KappaLabel::complement() method:
    pub fn complement(&self) -> Self {
        let s = self.as_str();
        let colon = s.find(':').unwrap();
        let axis = &s[..colon];
        let hex = &s[colon+1..];
        let mut buf = [0u8; 135];
        let prefix = format!("{}:", axis);
        let p = prefix.len();
        buf[..p].copy_from_slice(prefix.as_bytes());
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let byte = u8::from_str_radix(
                std::str::from_utf8(chunk).unwrap(), 16
            ).unwrap();
            let comp = !byte;
            buf[p + 2*i]     = HEX_TABLE[(comp >> 4) as usize];
            buf[p + 2*i + 1] = HEX_TABLE[(comp & 0x0f) as usize];
        }
        KappaLabel { buf, len: self.len }
    }
Used by L4-21: compute complement of hello.sha256, compose F4 on
the complement, verify the result equals f4_kappa from L4-20.
### E6.4 Sweep Polling Helper
    fn poll_sweep_completion(
        client: &RegistryClient,
        ns: &str,
        timeout_secs: u64,
    ) -> Result<(), String> {
        let start = Instant::now();
        let path = format!("/v2/{ns}/gc/status");
        loop {
            match client.get(&path).send() {
                Ok(resp) if resp.status() == 200 => {
                    let body: Value = resp.json().unwrap_or_default();
                    if let Some(ts) = body["last_sweep"].as_str() {
                        if is_recent_timestamp(ts, 120) {
                            return Ok(());
                        }
                    }
                }
                _ => {}
            }
            if start.elapsed() > Duration::from_secs(timeout_secs) {
                return Err(format!(
                    "sweep did not complete within {timeout_secs}s"
                ));
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    fn is_recent_timestamp(ts: &str, max_age_secs: i64) -> bool {
        chrono::DateTime::parse_from_rfc3339(ts)
            .map(|dt| {
                let age = chrono::Utc::now().signed_duration_since(dt);
                age.num_seconds() < max_age_secs
            })
            .unwrap_or(false)
    }
Used by L5-06, L5-12, L5-20. The 60-second timeout and 1-second
interval are not configurable. They are implementation constants.
