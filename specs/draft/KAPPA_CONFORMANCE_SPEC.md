# κ-Distribution Conformance Test Binary
## Implementation Specification
Status: Draft
Version: 1.0.0
Date: 2026-07-20
---
## 1. Purpose
This document specifies `kappa-conformance`, a compiled Rust binary
that tests any κ-Distribution registry implementation for protocol
conformance. The binary exercises every MUST, SHOULD, and MAY
behavior defined in the κ-Distribution Protocol Specification
(version 2.0.0) and produces machine-parseable and human-readable
reports.
The binary is the sole authority for conformance. A registry that
passes all MUST tests at a given conformance level is conformant
at that level. A registry that fails any MUST test is
non-conformant regardless of any other passing tests.
---
## 2. Design Principles
### 2.1 Single Binary, Zero Configuration Files
The binary is a single statically-linked executable. All
configuration is via environment variables. No configuration files,
no TOML, no YAML. The binary runs in CI/CD without any filesystem
setup beyond placing the binary on PATH.
### 2.2 Hermetic Test Data
The binary generates all test data internally. It does not depend
on external fixture files, pre-populated registries, or network
resources beyond the registry under test. Each test run starts
from an empty namespace, populates it, exercises operations, and
tears down.
### 2.3 Deterministic Ordering
Tests within each level execute in a fixed order. The order is
defined by this specification and does not change between runs.
Non-deterministic test ordering masks timing-dependent failures.
### 2.4 MUST Fails, SHOULD Warns
Operations marked MUST in the protocol spec produce test failures
on non-compliance. Operations marked SHOULD produce warnings.
Warnings appear in the report but do not affect the pass/fail
verdict. This distinction is the difference between a conformant
registry (no MUST failures) and an ideal registry (no MUST
failures and no SHOULD warnings).
### 2.5 No Mocking
The binary tests a live registry over HTTP. It does not mock HTTP
responses, simulate network conditions, or stub internal registry
behavior. The registry under test must be running and reachable.
---
## 3. Environment Variables
### 3.1 Required
    KAPPA_REGISTRY_URL
        Base URL of the registry under test.
        Example: https://registry.example.com
        The binary appends /v2/ paths to this URL.
        MUST include the scheme (http:// or https://).
        MUST NOT include a trailing slash.
### 3.2 Optional — Scope
    KAPPA_NAMESPACE
        The org-hierarchy:resource-path prefix for test data.
        Default: _conformance/test/v1:conformance
        The binary creates all test blobs, tags, edges, and
        schemas under this namespace. The namespace SHOULD be
        empty before the test run. The binary does NOT verify
        emptiness — pre-existing data may cause false positives
        on list operations.
    KAPPA_TEST_LEVELS
        Comma-separated list of conformance levels to execute.
        Values: 1, 2, 3, 4, 5
        Default: 1,2,3,4,5
        Example: KAPPA_TEST_LEVELS=1,2 runs only Level 1 and 2.
        Levels are independent: specifying 3 without 1 or 2 is
        valid. The binary runs setup for the requested levels
        only.
### 3.3 Optional — Authentication
    KAPPA_AUTH_TOKEN
        Bearer token for Authorization header.
        Format: the raw token value (the binary prepends "Bearer ").
        If unset, requests are sent without Authorization headers.
    KAPPA_AUTH_USERNAME
        Username for HTTP Basic authentication.
        Used only if KAPPA_AUTH_TOKEN is unset.
    KAPPA_AUTH_PASSWORD
        Password for HTTP Basic authentication.
        Used only if KAPPA_AUTH_TOKEN is unset.
### 3.4 Optional — Behavior
    KAPPA_TIMEOUT
        Per-operation HTTP timeout in seconds.
        Default: 30
    KAPPA_UPLOAD_CHUNK_SIZE
        Chunk size in bytes for chunked upload tests.
        Default: 1048576 (1 MiB)
        If the registry returns X-Kappa-Chunk-Min-Length on the
        upload POST response, the binary adjusts to the larger
        of this value and the registry's minimum.
    KAPPA_TEARDOWN_ORDER
        Order of teardown operations.
        Values: tags-first, blobs-first
        Default: tags-first
        Some registries require manifests/tags deleted before
        blobs. Others require the reverse. This controls the
        teardown sequence.
    KAPPA_TEARDOWN_ENABLED
        Whether to run teardown at all.
        Values: true, false
        Default: true
        Set to false to leave test data in the registry for
        manual inspection after a failed run.
    KAPPA_FOLLOW_REDIRECTS
        Whether the HTTP client follows redirects.
        Values: true, false
        Default: true
        Set to false to test that the registry returns the
        correct redirect status codes without following them.
    KAPPA_FEDERATION_PEERS
        Comma-separated list of peer registry URLs for Level 5
        federation tests.
        Default: (empty — federation tests are skipped if unset)
        Example: https://peer1.example.com,https://peer2.example.com
    KAPPA_REPORT_DIR
        Directory for report output files.
        Default: ./kappa-conformance-report/
---
## 4. Test Data Generation
### 4.1 Canonical Test Blobs
The binary generates test blobs internally with deterministic
content. All blobs are constructed from the test data module and
have known κ-labels.
    Blob ID             Content                         Size    Purpose
    -------             -------                         ----    -------
    EMPTY_BLOB          (zero bytes)                    0 B     edge case
    HELLO_BLOB          UTF-8 "hello"                   5 B     basic round-trip
    CONFIG_BLOB         {"type": "config"}              17 B    manifest config
    LAYER_A_BLOB        random 4 KiB                    4 KiB   manifest layer
    LAYER_B_BLOB        random 4 KiB (different seed)   4 KiB   second layer
    LARGE_BLOB          random 4 MiB                    4 MiB   chunked upload
    SCHEMA_BLOB         OpenAPI v3 schema JSON           ~500 B  schema validation
    FILTER_BLOB         WASM filter bytecode (noop)      ~100 B  admission filter
    UNKNOWN_FIELD_BLOB  {"known":"v","_xyzzy":"test"}   ~40 B   field preservation
    EDGE_META_BLOB      CBOR metadata for edge tests    ~50 B   edge metadata
All blob κ-labels are computed at binary compile time using the
sha256 and blake3 axes. The expected κ-labels are hardcoded
constants. If the registry returns a different κ-label for the
same content, the test fails — this detects normalization
violations (§3.1 exact-bytes invariant).
### 4.2 Canonical Test Manifests
    Manifest ID            Content                           Purpose
    -----------            -------                           -------
    SIMPLE_MANIFEST        JSON referencing CONFIG + LAYER_A  basic manifest
    MULTI_LAYER_MANIFEST   JSON referencing CONFIG + A + B    multi-layer
    REFERRER_MANIFEST      JSON with subject = SIMPLE κ       referrer edge
    ABSENT_SUBJECT_MANIFEST JSON with subject = absent κ      lazy subject
Each manifest is a blob whose content is JSON containing κ-label
references to other blobs. The manifest format is intentionally
simple — it is not an OCI manifest. It is a minimal JSON object
with a `layers` array of κ-label strings and a `config` κ-label
string.
### 4.3 Test Namespace Layout
After setup, the test namespace contains:
    {namespace}/blobs/sha256:{EMPTY_κ}
    {namespace}/blobs/sha256:{HELLO_κ}
    {namespace}/blobs/sha256:{CONFIG_κ}
    {namespace}/blobs/sha256:{LAYER_A_κ}
    {namespace}/blobs/sha256:{LAYER_B_κ}
    {namespace}/blobs/sha256:{LARGE_κ}           (Level 1 only if chunked supported)
    {namespace}/manifests/simple@v1.0
    {namespace}/manifests/simple@latest
    {namespace}/manifests/multi@v1.0              (Level 2)
    {namespace}/edges/{referrer_edge_κ}           (Level 3)
    {namespace}/schemas/test-schema               (Level 4)
    {namespace}/filters/test-filter               (Level 5)
---
## 5. Test Execution Order
### 5.1 Level 1 — Blob Operations
Tests execute in this order:
    L1-01  Version check
    L1-02  Push HELLO_BLOB (monolithic PUT)
    L1-03  Push HELLO_BLOB again (idempotent re-put)
    L1-04  Push with wrong κ-label (expect 409)
    L1-05  HEAD HELLO_BLOB (existence + Content-Length + X-Kappa-Label)
    L1-06  GET HELLO_BLOB (content + headers)
    L1-07  Verify retrieved content matches known bytes
    L1-08  GET absent κ-label (expect 404)
    L1-09  HEAD absent κ-label (expect 404)
    L1-10  Push EMPTY_BLOB (zero-length edge case)
    L1-11  GET EMPTY_BLOB (zero-length retrieval)
    L1-12  Push UNKNOWN_FIELD_BLOB
    L1-13  GET UNKNOWN_FIELD_BLOB — byte-compare against original
    L1-14  Push CONFIG_BLOB, LAYER_A_BLOB, LAYER_B_BLOB
    L1-15  List blobs by prefix "sha256:" — verify all pushed blobs present
    L1-16  Content-Type round-trip: push with Content-Type, verify echoed on GET
    L1-17  Push without Content-Type, verify GET returns application/octet-stream
**Chunked upload sub-suite (L1-20 through L1-35):**
    L1-20  POST upload start — verify 202, Location header
    L1-21  Verify Location is treated as opaque (do not parse UUID)
    L1-22  If X-Kappa-Chunk-Min-Length returned, record and adjust chunk size
    L1-23  PATCH chunk 1 — verify 202, Range header
    L1-24  PATCH chunk 2 — verify 202, Range header
    L1-25  PATCH chunk 1 again (duplicate) — expect 416
    L1-26  GET upload URL — verify 204, Range, Location (recovery endpoint)
    L1-27  PATCH out-of-order chunk — expect 416
    L1-28  GET upload URL after 416 — verify correct Range for recovery
    L1-29  PATCH remaining chunks in order
    L1-30  PUT with κ-label — verify 201, Location, X-Kappa-Label
    L1-31  GET the completed blob — verify content matches
    L1-32  PUT with wrong κ-label on close — expect 409
    L1-33  POST new upload, DELETE upload URL — verify 204 (cancel)
    L1-34  GET cancelled upload URL — expect 404
    L1-35  Verify cancelled upload's partial data is not retrievable
**Mount sub-suite (L1-40 through L1-44):**
    L1-40  POST mount existing blob from test namespace — expect 201
    L1-41  Verify mounted blob is retrievable under target path
    L1-42  POST mount non-existent blob — expect 202 (fallback)
    L1-43  Verify fallback produces a valid upload session URL
    L1-44  POST mount without from parameter (SHOULD test — warn if 4xx)
**Multi-label push sub-suite (L1-50 through L1-53):**
    L1-50  Push blob with ?also= for second κ-label (blake3) — SHOULD test
    L1-51  If supported (201): verify both κ-labels resolve to same content
    L1-52  If not supported (non-201): warn, verify primary κ-label works
    L1-53  Push with invalid also κ-label — expect 409
**Redirect sub-suite (L1-60 through L1-62):**
    L1-60  If registry redirects blob GET: verify redirect followed
    L1-61  Verify Authorization header NOT forwarded across host boundary
    L1-62  Verify final response has correct content and X-Kappa-Label
**Cleanup:**
    L1-90  DELETE HELLO_BLOB (unpin) — expect 202 or 405
    L1-91  DELETE remaining test blobs per KAPPA_TEARDOWN_ORDER
### 5.2 Level 2 — Tag Operations
Prerequisite: Level 1 blobs are pushed (setup re-pushes if Level 1
was not run).
    L2-01  PUT manifest SIMPLE_MANIFEST with tag "v1.0" — expect 201
    L2-02  Verify X-Kappa-Label in response matches expected
    L2-03  GET manifest by tag "v1.0" — verify content and X-Kappa-Label
    L2-04  PUT same content with tag "latest" — expect 201
    L2-05  GET manifest by tag "latest" — verify same κ-label as "v1.0"
    L2-06  PUT MULTI_LAYER_MANIFEST with tag "latest" — expect 201 (update)
    L2-07  GET "latest" — verify κ-label changed to MULTI_LAYER
    L2-08  GET "v1.0" — verify κ-label still points to SIMPLE
    L2-09  PUT tag for absent κ-label — expect rejection (content-before-tag)
    L2-10  GET absent tag — expect 404
    L2-11  List tags — verify "v1.0" and "latest" present, ASCIIbetical order
    L2-12  List tags with n=1 — verify one result and Link header
    L2-13  List tags with n=0 — verify empty result, no Link header
    L2-14  List tags with last=<tag> — verify pagination cursor works
    L2-15  Verify last is a tag name, not a numerical index
    L2-16  GET manifest by κ-label (direct, no tag) — verify content
**Timestamp tag sub-suite (L2-20 through L2-24):**
    L2-20  PUT with timestamp tag "2026-07-20T00:00:00" — expect 201
    L2-21  PUT with timestamp tag "2026-07-20T12:00:00" — expect 201
    L2-22  List tags with after/before range — verify only matching tags
    L2-23  List tags order=desc — verify descending order
    L2-24  List tags order=asc — verify ascending order
**Compare-and-swap sub-suite (L2-30 through L2-34):**
    L2-30  tag_set_if with correct expected — expect success
    L2-31  tag_set_if with wrong expected — expect conflict
    L2-32  tag_set_if on non-existent tag with absence sentinel — SHOULD test
    L2-33  Verify tag value after successful CAS
    L2-34  Verify tag value unchanged after failed CAS
**Multi-tag bind sub-suite (L2-40 through L2-42):**
    L2-40  PUT manifest with ?tag=a&tag=b — SHOULD test
    L2-41  If supported: verify both tags resolve to same κ-label
    L2-42  If not supported: warn, verify single tag works
**Warning header sub-suite (L2-50 through L2-52):**
    L2-50  If registry includes Warning headers: verify format 299
    L2-51  Verify Warning data does not exceed 4096 bytes
    L2-52  Record warnings in report (do not fail)
**Tag deletion sub-suite (L2-60 through L2-64):**
    L2-60  DELETE tag "v1.0" — expect 202, 400, or 405
    L2-61  If 400: verify error code is UNSUPPORTED
    L2-62  If 202: GET deleted tag — expect 404
    L2-63  If 202: GET content by κ-label — still 200 (content not deleted)
    L2-64  DELETE remaining tags per teardown order
### 5.3 Level 3 — Edge Operations
Prerequisite: Level 1 blobs and Level 2 tags are populated.
    L3-01  PUT edge (SIMPLE_κ derives-from CONFIG_κ) — expect 201
    L3-02  Verify response includes edge-κ-label in X-Kappa-Label
    L3-03  GET edge blob by its own κ-label — verify retrievable as blob
    L3-04  Verify edge blob content matches canonical form (Appendix A of spec)
    L3-05  Query edges outbound from SIMPLE_κ — verify derives-from edge present
    L3-06  Query edges inbound to CONFIG_κ — verify derives-from edge present
    L3-07  Query edges with relation filter — verify only matching edges
    L3-08  Query edges with direction=both — verify both directions
    L3-09  PUT same edge again — verify idempotent (200 or 201, same κ-label)
    L3-10  PUT edge with absent source — expect rejection
    L3-11  PUT edge with absent target — expect success (lazy replication)
    L3-12  Verify edge κ-label uses source's σ-axis
    L3-13  PUT REFERRER_MANIFEST with subject edge (refers-to) — expect 201
    L3-14  Query edges on SIMPLE_κ with relation=refers-to, inbound — verify
    L3-15  PUT edge to non-existent subject (ABSENT_SUBJECT) — expect 201
    L3-16  Query edges on absent subject — verify edge present even though
           subject blob is absent
    L3-17  Verify edge pagination (if enough edges: n, last parameters)
**Edge deletion sub-suite (L3-20 through L3-22):**
    L3-20  DELETE edge by edge-κ-label — expect 202
    L3-21  Query edges — verify deleted edge absent
    L3-22  Verify source and target blobs still exist (edge delete != blob delete)
### 5.4 Level 4 — Composition, Witnesses, Schemas
Prerequisite: Level 1-3 populated.
    L4-01  POST compose g2 with (HELLO_κ, CONFIG_κ) — expect 200
    L4-02  Verify response includes composed κ-label, witness κ-label, operation
    L4-03  POST compose g2 with (CONFIG_κ, HELLO_κ) — verify same composed κ-label
           (commutativity)
    L4-04  GET composed blob by its κ-label — verify retrievable
    L4-05  Query edges from composed_κ with relation=composed-of — verify both
           operands present
    L4-06  Verify composed-of edge metadata includes operation type "g2"
    L4-07  GET witness by witnessed blob κ-label — verify 200
    L4-08  Verify witness blob starts with self-describing header (u16 LE widths)
    L4-09  Query edge from witness_κ to composed_κ with relation=witness-of
    L4-10  POST compose with cross-axis operands (sha256 + blake3) — expect 422
    L4-11  Verify error code is AXIS_MISMATCH
**F4/E6/E7/E8 composition sub-suite (L4-20 through L4-30):**
    L4-20  POST compose f4 — verify result
    L4-21  POST compose f4 with bitwise mirror operand — verify same result
    L4-22  POST compose e6 — verify result
    L4-23  Verify e6 tag byte matches first_digest_byte % 9 classification
    L4-24  POST compose e7 — verify result
    L4-25  POST compose e8 — verify result
    L4-26  Verify e8 composed κ-label differs from operand κ-label
    L4-27  Document F4/E7/E8 digest coincidence if observed (informational)
    L4-28  Verify composed-of edge metadata distinguishes operation type in
           all coincidence cases
**Schema sub-suite (L4-40 through L4-48):**
    L4-40  PUT schema registration for test scope — expect 201
    L4-41  GET schema — verify content matches
    L4-42  List schemas — verify test schema present
    L4-43  PUT content matching schema to scoped path — expect 201
    L4-44  PUT content violating schema to scoped path — expect 422
    L4-45  Verify rejected content is NOT stored (GET by would-be κ-label → 404)
    L4-46  Verify schema validation does not alter κ-label (compare with
           unscoped push of same valid content)
    L4-47  PUT updated schema (derives-from edge to old schema) — expect 201
    L4-48  Verify derives-from edge exists between old and new schema blobs
### 5.5 Level 5 — GC, Admission, Federation
Prerequisite: Level 1-4 populated.
**GC sub-suite (L5-01 through L5-20):**
    L5-01  POST pin on HELLO_κ with TTL=0 (permanent) — expect 201
    L5-02  Verify pin response includes pin-blob κ-label
    L5-03  PUT edge (HELLO_κ owns LAYER_A_κ) — establish reachability
    L5-04  DELETE tag pointing to LAYER_A_κ (if tagged) — make it tag-unreachable
    L5-05  POST gc/sweep — expect 202
    L5-06  GET gc/status — verify sweep ran
    L5-07  GET HELLO_κ — verify survives (pinned)
    L5-08  GET LAYER_A_κ — verify survives (reachable via owns from pinned)
    L5-09  POST unpin on HELLO_κ — expect 200
    L5-10  Verify unpin creates a new pin blob (derives-from original)
    L5-11  POST gc/sweep — expect 202
    L5-12  Wait for sweep completion (poll gc/status)
    L5-13  GET HELLO_κ — SHOULD be absent (unpinned, no tags)
    L5-14  GET LAYER_A_κ — SHOULD be absent (unreachable after owner unpinned)
    L5-15  POST pin with controller="test-ctrl" (finalizer) — expect 201
    L5-16  POST unpin — expect 409 (outstanding finalizer)
    L5-17  GET gc/status — verify pending_finalizers includes test-ctrl
    L5-18  POST finalizer release (new pin blob with released state)
    L5-19  POST unpin — expect 200
    L5-20  POST gc/sweep — verify finalized blob evicted
**Admission filter sub-suite (L5-30 through L5-38):**
    L5-30  PUT filter registration for test scope — expect 201
    L5-31  GET filters — verify test filter present
    L5-32  PUT content to scoped path that filter accepts — expect 201
    L5-33  PUT content to scoped path that filter rejects — expect 422
    L5-34  Verify error code is FILTER_REJECTED and reason is present
    L5-35  Verify rejected content not stored
    L5-36  DELETE filter — expect 202
    L5-37  Verify filter blob still exists (unlinked, not deleted)
    L5-38  PUT content that was previously rejected — expect 201 (filter removed)
**Federation sub-suite (L5-50 through L5-58):**
Requires KAPPA_FEDERATION_PEERS to be set. Skipped if unset.
    L5-50  Push unique blob to peer registry (not to primary)
    L5-51  GET blob from primary — primary does not have it locally
    L5-52  If primary supports federation: expect 200 (fetched from peer)
    L5-53  Verify content matches by re-hash
    L5-54  GET same blob from primary again — expect 200 (cached locally)
    L5-55  Verify second GET is faster than first (federation not re-triggered)
    L5-56  Push unique blob to second peer (if available)
    L5-57  GET from primary where primary can reach peer1 but not peer2
    L5-58  Verify multi-hop resolution if topology supports it
**Rate limiting sub-suite (L5-60 through L5-62):**
    L5-60  If registry returns 429 on any operation: verify Retry-After present
    L5-61  Verify Retry-After value is a positive integer or HTTP-date
    L5-62  Record rate limit behavior in report (informational, do not fail)
---
## 6. Report Generation
### 6.1 JUnit XML
The binary produces a JUnit XML report conforming to the JUnit 4
schema. One `<testsuite>` per conformance level. One `<testcase>`
per test. Failed tests include `<failure>` with the HTTP request,
response status, response headers, and response body (redacted).
Warned tests include a `<system-out>` annotation.
Output path: `{KAPPA_REPORT_DIR}/kappa-conformance.xml`
### 6.2 HTML Report
The binary produces an HTML report for human review. The report
includes:
    Header:     registry URL, timestamp, binary version, levels tested
    Summary:    pass / fail / warn / skip counts per level
    Progress:   visual meter per level
    Details:    expandable per-test sections with:
                - test ID and description
                - MUST / SHOULD / MAY classification
                - pass / fail / warn / skip verdict
                - HTTP request (method, URL, headers, body summary)
                - HTTP response (status, headers, body summary)
                - Assertion details on failure
                - Timing (milliseconds)
    Footer:     total duration, binary version, Rust toolchain version
Output path: `{KAPPA_REPORT_DIR}/kappa-conformance.html`
### 6.3 Terminal Output
During execution, the binary prints one line per test to stderr:
    [PASS]  L1-01  Version check (12ms)
    [PASS]  L1-02  Push HELLO_BLOB monolithic (45ms)
    [FAIL]  L1-05  HEAD HELLO_BLOB missing X-Kappa-Label header
    [WARN]  L1-44  Mount without from parameter returned 400
    [SKIP]  L5-50  Federation test skipped (KAPPA_FEDERATION_PEERS unset)
Color output is enabled when stderr is a TTY. Disabled otherwise.
### 6.4 Exit Codes
    0   all executed MUST tests passed (warnings are acceptable)
    1   one or more MUST tests failed
    2   configuration error (missing KAPPA_REGISTRY_URL, etc.)
    3   network error (registry unreachable)
---
## 7. Credential Redaction
All report output (JUnit XML, HTML, terminal) MUST redact
sensitive values from HTTP traces:
    Authorization: Bearer eyJ... → Authorization: [REDACTED]
    Authorization: Basic dXNl... → Authorization: [REDACTED]
    Set-Cookie: session=abc...   → Set-Cookie: [REDACTED]
Redaction applies to request headers, response headers, and any
header value in URL query parameters that matches known sensitive
patterns (token, password, secret, key, credential).
The redaction MUST be applied before writing to any output stream
— not after. Sensitive values MUST NOT appear in memory-mapped
report files, temporary files, or buffered output.
---
## 8. HTTP Client Implementation
### 8.1 Client Library
The binary uses `reqwest` (blocking) for HTTP operations. The
client is configured with:
- Timeout: KAPPA_TIMEOUT seconds per operation
- Redirect policy: configurable via KAPPA_FOLLOW_REDIRECTS
- TLS: system certificate store, no certificate pinning
- Connection pooling: one pool per registry URL
- User-Agent: `kappa-conformance/{version}`
### 8.2 Request Construction
Every request includes:
    User-Agent: kappa-conformance/1.0.0
    Accept: application/octet-stream, application/json
If KAPPA_AUTH_TOKEN is set:
    Authorization: Bearer {token}
If KAPPA_AUTH_USERNAME and KAPPA_AUTH_PASSWORD are set:
    Authorization: Basic {base64(username:password)}
### 8.3 Redirect Handling
When KAPPA_FOLLOW_REDIRECTS is true (default), the client follows
redirects per RFC 9110 §15.4. The client MUST strip the
Authorization header when a redirect crosses a host boundary.
"Crosses a host boundary" means the redirect target's
`{scheme}://{authority}` differs from the original request's.
When KAPPA_FOLLOW_REDIRECTS is false, the client records the
redirect status and target URL in the report but does not follow.
This mode tests that the registry produces correct redirect
responses.
### 8.4 Request/Response Logging
Every HTTP request and response is captured in a structured log
entry:
    RequestLog {
        test_id:     String,          // "L1-02"
        method:      String,          // "PUT"
        url:         String,          // full URL
        headers:     Vec<(String, String)>,  // redacted
        body_size:   usize,
        body_sha256: String,          // hash of body for comparison
    }
    ResponseLog {
        status:      u16,
        headers:     Vec<(String, String)>,  // redacted
        body_size:   usize,
        body_sha256: String,
        duration_ms: u64,
    }
Body content is not stored in the log — only the size and hash.
This keeps the log bounded regardless of blob sizes.
---
## 9. Test Assertion Framework
### 9.1 Assertion Types
    assert_status(response, expected)
        Verify HTTP status code. MUST-level failure if wrong.
    assert_header_present(response, header_name)
        Verify header exists. MUST-level failure if absent.
    assert_header_value(response, header_name, expected_value)
        Verify header has expected value. MUST-level failure.
    assert_body_matches_kappa(response, expected_kappa)
        Re-hash the response body and compare against expected
        κ-label. MUST-level failure if mismatch.
    assert_body_bytes_equal(response, expected_bytes)
        Byte-for-byte comparison. MUST-level failure.
        Used for exact-bytes invariant and unknown field
        preservation tests.
    assert_json_field(response, field_path, expected_value)
        Parse response body as JSON and verify a field value.
        MUST-level failure.
    assert_kappa_label_valid(label_string)
        Verify the string is a well-formed κ-label: correct
        length, ASCII, single colon, recognized axis token,
        lowercase hex. MUST-level failure.
    warn_status(response, expected)
        Same as assert_status but emits a warning instead of
        a failure. Used for SHOULD-level tests.
    warn_header_present(response, header_name)
        Same as assert_header_present but warns. SHOULD-level.
### 9.2 κ-Label Computation
The binary computes κ-labels for all test blobs using an embedded
SHA-256 and BLAKE3 implementation. The binary does NOT use the
registry's κ-label computation — it computes independently and
compares. This is the verify-on-receipt invariant applied to the
test harness itself.
The hash computation uses a 16 KiB streaming buffer. The hash
result is formatted as `sha256:<lowercase-hex>` using a static
lookup table (`b"0123456789abcdef"`). No allocation for the
format step — the 71-byte result is written to a stack buffer.
---
## 10. Timing and Timeout Handling
### 10.1 Per-Operation Timeout
Every HTTP request has a timeout of KAPPA_TIMEOUT seconds. If the
timeout expires, the test fails with a timeout error. The timeout
is not retried — a timeout is a deterministic failure.
### 10.2 GC Sweep Polling
GC sweep tests (L5-05 through L5-20) require waiting for an
asynchronous sweep to complete. The binary polls GET gc/status
every 1 second, up to 60 seconds. If the sweep does not complete
within 60 seconds, the test fails with a timeout error.
### 10.3 Timing Recording
Every test records its wall-clock duration in milliseconds. The
duration includes all HTTP round-trips for that test (setup
requests are attributed to the setup phase, not to individual
tests). Timing is reported in all three output formats (terminal,
JUnit, HTML).
---
## 11. Build and Distribution
### 11.1 Compilation
    cargo build --release -p kappa-conformance
The binary is statically linked (musl target for Linux, native
for macOS/Windows). It has no runtime dependencies beyond libc.
### 11.2 Supported Targets
    Target                  Tier
    ------                  ----
    x86_64-unknown-linux-musl    1 (CI-tested, statically linked)
    aarch64-unknown-linux-musl   1
    x86_64-apple-darwin          2 (built, not CI-tested)
    aarch64-apple-darwin         2
    x86_64-pc-windows-msvc       2
### 11.3 Container Image
A container image is provided for CI/CD environments:
    FROM scratch
    COPY kappa-conformance /kappa-conformance
    ENTRYPOINT ["/kappa-conformance"]
Usage:
    docker run --rm \
      -e KAPPA_REGISTRY_URL=https://registry.example.com \
      -e KAPPA_AUTH_TOKEN=eyJ... \
      -v ./reports:/kappa-conformance-report \
      ghcr.io/uor-foundation/kappa-conformance:latest
### 11.4 Versioning
The binary version matches the κ-Distribution Protocol
Specification version. Binary 2.0.0 tests against spec 2.0.0.
Minor version bumps (2.1.0) add tests for new SHOULD/MAY
features without removing existing MUST tests. Major version
bumps (3.0.0) may change MUST requirements.
---
## 12. Dependencies
### 12.1 Direct Dependencies
    reqwest         HTTP client (blocking feature)
    sha2            SHA-256 computation
    blake3          BLAKE3 computation
    serde           JSON serialization (derive feature)
    serde_json      JSON parsing for response bodies
    quick-xml       JUnit XML generation
    clap            CLI argument parsing (env feature)
    chrono          Timestamp generation for reports
### 12.2 Forbidden Dependencies
The binary MUST NOT depend on:
- Any κ-Distribution registry implementation crate
- Any uor-addr crate (the binary computes hashes independently
  to avoid circular validation)
- Any OCI-specific crate
- Any database or storage crate
- Any async runtime (the binary is synchronous/blocking)
The binary tests a registry over HTTP. It does not share code
with the registry. This ensures the conformance test is
independent of the implementation under test.
---
## 13. Example Usage
### 13.1 Basic Run
    export KAPPA_REGISTRY_URL=https://registry.example.com
    export KAPPA_AUTH_TOKEN=eyJhbGciOiJSUzI1NiIs...
    ./kappa-conformance
### 13.2 Level 1 Only
    KAPPA_REGISTRY_URL=http://localhost:5000 \
    KAPPA_TEST_LEVELS=1 \
    ./kappa-conformance
### 13.3 CI/CD Pipeline
    - name: Run κ-Distribution conformance
      run: |
        docker run --rm \
          -e KAPPA_REGISTRY_URL=${{ secrets.REGISTRY_URL }} \
          -e KAPPA_AUTH_TOKEN=${{ secrets.REGISTRY_TOKEN }} \
          -e KAPPA_TEST_LEVELS=1,2,3 \
          -v ${{ github.workspace }}/reports:/kappa-conformance-report \
          ghcr.io/uor-foundation/kappa-conformance:latest
    - name: Upload conformance report
      uses: actions/upload-artifact@v4
      with:
        name: conformance-report
        path: reports/
### 13.4 Debug a Failure
    KAPPA_REGISTRY_URL=http://localhost:5000 \
    KAPPA_TEST_LEVELS=1 \
    KAPPA_TEARDOWN_ENABLED=false \
    RUST_LOG=debug \
    ./kappa-conformance
    # Inspect the registry state after failure
    curl -s http://localhost:5000/v2/_conformance/test/v1:conformance/tags/list
### 13.5 Federation Test
    KAPPA_REGISTRY_URL=https://primary.example.com \
    KAPPA_FEDERATION_PEERS=https://peer1.example.com,https://peer2.example.com \
    KAPPA_TEST_LEVELS=5 \
    ./kappa-conformance
---
## 14. Test ID Registry
Every test has a stable identifier that does not change between
binary versions. Tests may be added but never renumbered. Retired
tests are marked `(retired @ v{version})` in this registry and
produce SKIP results.
    ID      Level  Req.   Description
    ------  -----  ----   -----------
    L1-01   1      MUST   Version check
    L1-02   1      MUST   Push blob monolithic
    L1-03   1      MUST   Push blob idempotent re-put
    L1-04   1      MUST   Push blob wrong κ-label rejected
    L1-05   1      MUST   HEAD blob headers present
    L1-06   1      MUST   GET blob content correct
    L1-07   1      MUST   Content matches known bytes
    L1-08   1      MUST   GET absent blob returns 404
    L1-09   1      MUST   HEAD absent blob returns 404
    L1-10   1      MUST   Push empty blob
    L1-11   1      MUST   GET empty blob
    L1-12   1      MUST   Push blob with unknown fields
    L1-13   1      MUST   GET blob preserves unknown fields
    L1-14   1      MUST   Push config and layer blobs
    L1-15   1      MUST   List blobs by prefix
    L1-16   1      MUST   Content-Type round-trip
    L1-17   1      MUST   Content-Type default octet-stream
    L1-20   1      MUST   Chunked upload start
    L1-21   1      MUST   Upload location is opaque
    L1-22   1      SHOULD Chunk-Min-Length observed
    L1-23   1      MUST   Chunked upload chunk 1
    L1-24   1      MUST   Chunked upload chunk 2
    L1-25   1      MUST   Duplicate chunk returns 416
    L1-26   1      MUST   Upload recovery endpoint
    L1-27   1      MUST   Out-of-order chunk returns 416
    L1-28   1      MUST   Recovery after 416
    L1-29   1      MUST   Remaining chunks in order
    L1-30   1      MUST   Upload complete with κ-label
    L1-31   1      MUST   Completed blob retrievable
    L1-32   1      MUST   Wrong κ-label on complete rejected
    L1-33   1      MUST   Upload cancellation
    L1-34   1      MUST   Cancelled session returns 404
    L1-35   1      MUST   Cancelled data not retrievable
    L1-40   1      SHOULD Mount existing blob
    L1-41   1      SHOULD Mounted blob retrievable
    L1-42   1      SHOULD Mount non-existent falls back
    L1-43   1      SHOULD Fallback produces upload URL
    L1-44   1      SHOULD Mount without from parameter
    L1-50   1      SHOULD Multi-label push
    L1-51   1      SHOULD Multi-label both resolvable
    L1-52   1      SHOULD Multi-label fallback
    L1-53   1      MUST   Multi-label invalid κ-label rejected
    L1-60   1      SHOULD Redirect followed
    L1-61   1      MUST   Auth not forwarded cross-host
    L1-62   1      MUST   Redirect final response correct
    L1-90   1      MAY    Blob deletion
    L1-91   1      MAY    Teardown cleanup
    L2-01   2      MUST   Bind tag
    L2-02   2      MUST   Tag bind returns X-Kappa-Label
    L2-03   2      MUST   Resolve tag
    L2-04   2      MUST   Bind second tag to same content
    L2-05   2      MUST   Both tags resolve same κ-label
    L2-06   2      MUST   Update tag to different content
    L2-07   2      MUST   Updated tag resolves new κ-label
    L2-08   2      MUST   Old tag retains original κ-label
    L2-09   2      MUST   Tag to absent κ-label rejected
    L2-10   2      MUST   Absent tag returns 404
    L2-11   2      MUST   Tag list present and ordered
    L2-12   2      MUST   Tag list pagination n=1
    L2-13   2      MUST   Tag list n=0 empty no Link
    L2-14   2      MUST   Tag list pagination cursor
    L2-15   2      MUST   Cursor is tag name not index
    L2-16   2      MUST   Resolve by κ-label directly
    L2-20   2      SHOULD Timestamp tag bind
    L2-21   2      SHOULD Timestamp tag second
    L2-22   2      SHOULD Timestamp range filter
    L2-23   2      SHOULD Tag list descending
    L2-24   2      SHOULD Tag list ascending
    L2-30   2      SHOULD Compare-and-swap success
    L2-31   2      SHOULD Compare-and-swap conflict
    L2-32   2      SHOULD CAS on non-existent tag
    L2-33   2      SHOULD CAS value after success
    L2-34   2      SHOULD CAS value after failure
    L2-40   2      SHOULD Multi-tag bind
    L2-41   2      SHOULD Multi-tag both resolve
    L2-42   2      SHOULD Multi-tag fallback
    L2-50   2      SHOULD Warning header format
    L2-51   2      MUST   Warning data ≤ 4096 bytes
    L2-52   2      SHOULD Warning reported
    L2-60   2      MAY    Tag deletion
    L2-61   2      MUST   Deletion rejection uses UNSUPPORTED
    L2-62   2      MAY    Deleted tag returns 404
    L2-63   2      MUST   Content survives tag deletion
    L2-64   2      MAY    Teardown tag cleanup
    L3-01   3      MUST   Create edge
    L3-02   3      MUST   Edge κ-label in response
    L3-03   3      MUST   Edge retrievable as blob
    L3-04   3      MUST   Edge canonical form correct
    L3-05   3      MUST   Edge query outbound
    L3-06   3      MUST   Edge query inbound
    L3-07   3      MUST   Edge query relation filter
    L3-08   3      MUST   Edge query both directions
    L3-09   3      MUST   Edge creation idempotent
    L3-10   3      MUST   Edge absent source rejected
    L3-11   3      MUST   Edge absent target accepted
    L3-12   3      MUST   Edge κ-label axis matches source
    L3-13   3      MUST   Referrer edge created
    L3-14   3      MUST   Referrer edge queryable
    L3-15   3      MUST   Edge to absent subject accepted
    L3-16   3      MUST   Edge queryable for absent subject
    L3-17   3      SHOULD Edge pagination
    L3-20   3      MUST   Edge deletion
    L3-21   3      MUST   Deleted edge absent from queries
    L3-22   3      MUST   Edge delete does not delete blobs
    L4-01   4      MUST   G2 composition
    L4-02   4      MUST   Composition response fields
    L4-03   4      MUST   G2 commutativity
    L4-04   4      MUST   Composed blob retrievable
    L4-05   4      MUST   Composed-of edges created
    L4-06   4      MUST   Edge metadata includes operation
    L4-07   4      MUST   Witness retrievable
    L4-08   4      MUST   Witness self-describing header
    L4-09   4      MUST   Witness-of edge exists
    L4-10   4      MUST   Cross-axis composition rejected
    L4-11   4      MUST   Axis mismatch error code
    L4-20   4      MUST   F4 composition
    L4-21   4      MUST   F4 mirror collapse
    L4-22   4      MUST   E6 composition
    L4-23   4      MUST   E6 tag byte correct
    L4-24   4      MUST   E7 composition
    L4-25   4      MUST   E8 composition
    L4-26   4      MUST   E8 differs from operand
    L4-27   4      MAY    F4/E7/E8 coincidence documented
    L4-28   4      MUST   Coincidence edge metadata disambiguates
    L4-40   4      MUST   Schema registration
    L4-41   4      MUST   Schema retrieval
    L4-42   4      MUST   Schema listing
    L4-43   4      MUST   Valid content accepted under schema
    L4-44   4      MUST   Invalid content rejected under schema
    L4-45   4      MUST   Rejected content not stored
    L4-46   4      MUST   Schema does not alter κ-label
    L4-47   4      SHOULD Schema evolution with edge
    L4-48   4      SHOULD Schema evolution edge verified
    L5-01   5      MUST   Pin creation
    L5-02   5      MUST   Pin response κ-label
    L5-03   5      MUST   Ownership edge for reachability
    L5-04   5      MUST   Remove tag from owned blob
    L5-05   5      MUST   GC sweep trigger
    L5-06   5      MUST   GC status endpoint
    L5-07   5      MUST   Pinned blob survives sweep
    L5-08   5      MUST   Reachable blob survives sweep
    L5-09   5      MUST   Unpin
    L5-10   5      SHOULD Unpin creates derived pin blob
    L5-11   5      MUST   Sweep after unpin
    L5-12   5      MUST   Sweep completion polling
    L5-13   5      SHOULD Unpinned blob evicted
    L5-14   5      SHOULD Unreachable blob evicted
    L5-15   5      MUST   Finalizer pin creation
    L5-16   5      MUST   Unpin blocked by finalizer
    L5-17   5      MUST   Finalizer in GC status
    L5-18   5      MUST   Finalizer release
    L5-19   5      MUST   Unpin after release
    L5-20   5      SHOULD Finalized blob evicted after release
    L5-30   5      MUST   Filter registration
    L5-31   5      MUST   Filter listing
    L5-32   5      MUST   Filter-accepted content stored
    L5-33   5      MUST   Filter-rejected content rejected
    L5-34   5      MUST   Rejection includes reason
    L5-35   5      MUST   Rejected content not stored
    L5-36   5      MUST   Filter removal
    L5-37   5      MUST   Removed filter blob persists
    L5-38   5      MUST   Previously rejected content accepted
    L5-50   5      SHOULD Federation fetch
    L5-51   5      SHOULD Federation absent locally
    L5-52   5      SHOULD Federation resolves from peer
    L5-53   5      MUST   Federation content verified
    L5-54   5      SHOULD Federation caches locally
    L5-55   5      SHOULD Cached fetch faster
    L5-56   5      SHOULD Multi-peer federation
    L5-57   5      SHOULD Multi-hop federation
    L5-58   5      SHOULD Multi-hop content verified
    L5-60   5      SHOULD Rate limit Retry-After present
    L5-61   5      MUST   Retry-After format valid
    L5-62   5      MAY    Rate limit behavior recorded
    Total: 149 tests
    MUST:    107
    SHOULD:   36
    MAY:       6
