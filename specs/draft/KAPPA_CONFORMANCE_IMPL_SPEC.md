# kappa-conformance Implementation Specification
## Omnibus — All Context, All Decisions, All Depth
Version: 1.0.0
Date: 2026-07-20
---
## §0. Acceptance Outcome
When this binary executes against a κ-Distribution registry and
exits 0, the following API surface is proven conformant:
    Endpoint                                       Level  Tests
    ──────────────────────────────────────────────  ─────  ─────
    GET  /v2/                                      1      L1-01
    PUT  /v2/{p}/blobs/{κ}                         1      L1-02,03,04,10,12,16
    HEAD /v2/{p}/blobs/{κ}                         1      L1-05,09
    GET  /v2/{p}/blobs/{κ}                         1      L1-06,07,08,11,13,17
    POST /v2/{p}/blobs/uploads/                    1      L1-20,33
    PATCH <upload-url>                             1      L1-23,24,25,27,29
    GET  <upload-url>                              1      L1-26,28
    PUT  <upload-url>?kappa={κ}                    1      L1-30,32
    DELETE <upload-url>                            1      L1-33,34,35
    DELETE /v2/{p}/blobs/{κ}                       1      L1-90
    POST /v2/{p}/blobs/uploads/?mount=...          1      L1-40,41,42,43,44
    PUT  /v2/{p}/blobs/{κ}?also={κ2}              1      L1-50,51,52,53
    (redirect behavior)                            1      L1-60,61,62
    PUT  /v2/{p}/manifests/{tag}                   2      L2-01,02,04,06,09,20,21
    GET  /v2/{p}/manifests/{tag-or-κ}              2      L2-03,05,07,08,10,16
    PUT  /v2/{p}/manifests/{t}?tag={t2}            2      L2-40,41,42
    GET  /v2/{p}/tags/list[?params]                2      L2-11,12,13,14,15,22,23,24
    DELETE /v2/{p}/manifests/{tag}                 2      L2-60,61,62,63,64
    PUT  /v2/{p}/edges/                            3      L3-01,02,09,10,11,12,13,15
    GET  /v2/{p}/edges/{κ}                         3      L3-05,06,07,08,14,16,17
    GET  /v2/{p}/blobs/{edge-κ}                    3      L3-03,04
    DELETE /v2/{p}/edges/{edge-κ}                  3      L3-20,21,22
    POST /v2/{p}/compose/{op}                      4      L4-01,02,03,10,11,20,21,22,23,24,25,26,27,28
    GET  /v2/{p}/witnesses/{κ}                     4      L4-07,08
    GET  /v2/{p}/edges/{κ} (composed-of, witness)  4      L4-05,06,09
    PUT  /v2/{p}/schemas/{scope}                   4      L4-40,47
    GET  /v2/{p}/schemas/{scope}                   4      L4-41
    GET  /v2/{p}/schemas/                          4      L4-42
    PUT  /v2/{p}/manifests/{tag} (schema-scoped)   4      L4-43,44,45,46
    POST /v2/{p}/gc/pin                            5      L5-01,02,15
    POST /v2/{p}/gc/unpin                          5      L5-09,16,19
    POST /v2/{p}/gc/sweep                          5      L5-05,11,20
    GET  /v2/{p}/gc/status                         5      L5-06,12,17
    PUT  /v2/{p}/filters/{scope}                   5      L5-30
    GET  /v2/{p}/filters/                          5      L5-31
    DELETE /v2/{p}/filters/{κ}                     5      L5-36
    PUT/GET /v2/{p}/manifests (filter-scoped)      5      L5-32,33,34,35,37,38
    GET  /v2/{p}/blobs/{κ} (federation)            5      L5-50,51,52,53,54,55,56,57,58
Total: 35 endpoints, 149 tests, 107 MUST / 36 SHOULD / 6 MAY.
Exit 0 = all MUST pass. Exit 1 = MUST failure. Exit 2 = config
error. Exit 3 = registry unreachable.
---
## §1. Crate Identity
    [package]
    name = "kappa-conformance"
    version = "2.0.0"
    edition = "2021"
    rust-version = "1.75"
    publish = false
    [[bin]]
    name = "kappa-conformance"
    path = "src/main.rs"
Dependencies with version, features, reason:
    sha2 = "0.10"              independent SHA-256 (verify-on-receipt for the harness itself)
    blake3 = "1"               independent BLAKE3 for multi-axis tests
    reqwest = { version = "0.12", features = ["blocking"] }   synchronous HTTP client
    serde = { version = "1", features = ["derive"] }          response deserialization
    serde_json = "1"           JSON parsing and test data generation
    base64 = "0.22"            HTTP Basic auth encoding
    chrono = "0.4"             report timestamps
    uuid = { version = "1", features = ["v4"] }               federation test blob uniqueness
Forbidden dependencies:
    uor-addr        circular validation
    Any registry     code sharing breaks test independence
    tokio            binary is synchronous
    Any OCI crate    not an OCI tool
    Any database     not a store
Profile:
    [profile.release]
    lto = true
    strip = true
---
## §2. File Tree
    src/
    ├── main.rs           entrypoint: config → client → data → levels → report → exit
    ├── kappa.rs           KappaLabel + LabelError + sha256() + blake3() + complement()
    ├── config.rs          env var parsing → Config struct
    ├── client.rs          RegistryClient + HttpTrace + redaction + resolve_location
    ├── runner.rs          TestRunner + TestContext + TestResult + Requirement + TestVerdict
    ├── assertions.rs      assert_status, assert_header_*, assert_body_*, verify_error_response
    ├── testdata.rs        TestBlob + TestData with all blobs, manifests, expected κ-labels
    ├── report.rs          write_junit + write_html
    ├── edge_canon.rs      edge canonical form encoder for L3-04 verification
    └── tests/
        ├── mod.rs         pub mod level1..level5
        ├── level1.rs      L1-01 through L1-91
        ├── level2.rs      L2-01 through L2-64
        ├── level3.rs      L3-01 through L3-22
        ├── level4.rs      L4-01 through L4-48
        └── level5.rs      L5-01 through L5-62
---
## §3. Type Specifications
### §3.1 KappaLabel (kappa.rs)
Design decision: oversized fixed array. Rationale: heterogeneous
collections require one type, not KappaLabel<71> vs KappaLabel<135>.
Follows Rust stdlib SourceFileHash pattern.
    pub struct KappaLabel {
        buf: [u8; 135],
        len: u8,
    }
Traits: Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash,
Display, Debug (κ(...)), Deref<Target=str>, AsRef<str>, AsRef<[u8]>.
Methods:
    pub fn parse(s: &str) -> Result<Self, LabelError>
        Validate: len 71..=135, single colon, axis token, hex count, lowercase.
        Axis table: sha256→64hex→71, blake3→64hex→71, sha3-256→64hex→73,
        keccak256→64hex→74, sha512→128hex→135.
        #[cold] #[inline(never)] on error path. #[inline] on happy path.
    pub fn sha256(content: &[u8]) -> Self
        const HEX: &[u8;16] = b"0123456789abcdef";
        buf[7+2*i] = HEX[(byte>>4) as usize]; buf[7+2*i+1] = HEX[(byte&0x0f) as usize];
        Prefix b"sha256:" into buf[..7]. len=71.
    pub fn blake3(content: &[u8]) -> Self
        Same pattern, prefix b"blake3:".
    pub fn as_str(&self) -> &str
    pub fn axis(&self) -> &str
    pub fn complement(&self) -> Self
        Decode hex to bytes, bitwise NOT each, re-encode, same axis prefix.
        Used by L4-21 (F4 mirror test).
LabelError:
    #[non_exhaustive] pub enum LabelError {
        BadLength { got: usize },
        NoColon,
        UnknownAxis,
        BadHex { position: usize, byte: u8 },
        WrongDigitCount { expected: usize, got: usize },
    }
    Display: lowercase, no punctuation. impl std::error::Error.
Unit tests (7): sha256_empty, sha256_hello, blake3_empty, blake3_hello,
parse_roundtrip, reject_unknown_axis, reject_uppercase.
Constants:
    EMPTY_SHA256 = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    HELLO_SHA256 = "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    EMPTY_BLAKE3 = "blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    HELLO_BLAKE3 = "blake3:ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f"
### §3.2 Config (config.rs)
    pub struct Config {
        pub registry_url: String,         // KAPPA_REGISTRY_URL, required, strip trailing /
        pub namespace: String,            // KAPPA_NAMESPACE, default "_conformance/test/v1:conformance"
        pub levels: Vec<u8>,              // KAPPA_TEST_LEVELS, default "1,2,3,4,5", validated 1..=5
        pub auth: Auth,                   // KAPPA_AUTH_TOKEN or USERNAME+PASSWORD
        pub timeout_secs: u64,            // KAPPA_TIMEOUT, default 30
        pub upload_chunk_size: usize,     // KAPPA_UPLOAD_CHUNK_SIZE, default 1048576
        pub teardown_order: TeardownOrder,// KAPPA_TEARDOWN_ORDER, default "tags-first"
        pub teardown_enabled: bool,       // KAPPA_TEARDOWN_ENABLED, default true
        pub follow_redirects: bool,       // KAPPA_FOLLOW_REDIRECTS, default true
        pub federation_peers: Vec<String>,// KAPPA_FEDERATION_PEERS, default empty
        pub report_dir: PathBuf,          // KAPPA_REPORT_DIR, default "./kappa-conformance-report/"
    }
    pub enum Auth { None, Bearer(String), Basic { username: String, password: String } }
    pub enum TeardownOrder { TagsFirst, BlobsFirst }
    impl Config { pub fn from_env() -> Self }
    fn required_env(key: &str) -> String
    fn env_or(key: &str, default: &str) -> String
    #[cold] #[inline(never)] fn config_exit(msg: &str) -> !
### §3.3 RegistryClient (client.rs)
    pub struct RegistryClient {
        inner: reqwest::blocking::Client,
        auth_header: Option<HeaderValue>,
        pub base_url: String,
    }
    pub fn new(config: &Config) -> Self
    Redirect: Policy::custom stripping Authorization on cross-host.
    Methods (each injects auth + User-Agent):
        pub fn get(&self, path: &str) -> RequestBuilder
        pub fn head(&self, path: &str) -> RequestBuilder
        pub fn put(&self, path: &str) -> RequestBuilder
        pub fn post(&self, path: &str) -> RequestBuilder
        pub fn delete(&self, path: &str) -> RequestBuilder
        pub fn patch_url(&self, url: &str) -> RequestBuilder
        pub fn put_url(&self, url: &str) -> RequestBuilder
        pub fn get_url(&self, url: &str) -> RequestBuilder
        pub fn delete_url(&self, url: &str) -> RequestBuilder
    pub fn resolve_location(base: &str, loc: &str) -> String
        if loc.starts_with("http") { loc.to_string() } else { format!("{base}{loc}") }
    fn redact_headers(headers: &HeaderMap) -> Vec<(String, String)>
        Sensitive substrings: authorization, cookie, set-cookie, token,
        password, secret, credential. Replace value with "[REDACTED]"
        BEFORE storing into any String/Vec.
### §3.4 HttpTrace (client.rs)
    #[derive(Clone)]
    pub struct HttpTrace {
        pub method: String, pub url: String,
        pub req_headers: Vec<(String, String)>,  // redacted
        pub req_body_size: usize, pub status: u16,
        pub resp_headers: Vec<(String, String)>,  // redacted
        pub resp_body_size: usize, pub duration_ms: u64,
    }
    pub fn send_traced(&self, rb: RequestBuilder) -> Result<(Response, HttpTrace), Error>
### §3.5 Assertion Helpers (assertions.rs)
    fn assert_status(actual: u16, expected: u16) -> Result<(), String>
    fn assert_status_in(actual: u16, expected: &[u16]) -> Result<(), String>
    fn assert_header_present(resp: &Response, name: &str) -> Result<(), String>
    fn assert_header_value(resp: &Response, name: &str, expected: &str) -> Result<(), String>
    fn assert_header_starts_with(resp: &Response, name: &str, prefix: &str) -> Result<(), String>
    fn assert_body_eq(body: &[u8], expected: &[u8]) -> Result<(), String>
    fn assert_kappa_matches(body: &[u8], expected: &KappaLabel) -> Result<(), String>
    fn assert_kappa_valid(s: &str) -> Result<KappaLabel, String>
    fn assert_json_field(body: &[u8], path: &str, expected: &str) -> Result<(), String>
    fn verify_error_response(body: &[u8], expected_code: &str) -> Result<(), String>
        Parse JSON → check errors[0].code == expected_code, code matches [A-Z_]+.
    Usage pattern:
        let mut t = runner.must("L1-04", 1, "Wrong κ rejected");
        match client.put(&path).body(wrong).send() {
            Ok(resp) => {
                if let Err(e) = assert_status(resp.status().as_u16(), 409) { t.fail(e); return; }
                t.pass();
            }
            Err(e) => t.fail(format!("request failed: {e}")),
        }
### §3.6 TestRunner (runner.rs)
    pub struct TestRunner { pub results: Vec<TestResult> }
    impl TestRunner {
        pub fn new() -> Self
        pub fn must(...) -> TestContext<'_>
        pub fn should(...) -> TestContext<'_>
        pub fn may(...) -> TestContext<'_>
        pub fn skip(&mut self, id, level, desc, reason)
        pub fn summary(&self) -> (usize, usize, usize, usize)
        pub fn has_failures(&self) -> bool
    }
### §3.7 TestContext (runner.rs)
    pub struct TestContext<'a> {
        id: &'static str, level: u8, requirement: Requirement,
        description: &'static str, traces: Vec<HttpTrace>,
        runner: &'a mut TestRunner, start: Instant,
    }
    Methods: trace, pass, fail, warn.
    Downgrade: Should+Fail→Warn, May+Fail→Warn.
    Terminal: [PASS]\x1b[32m  [FAIL]\x1b[31m  [WARN]\x1b[33m  [SKIP]\x1b[90m
    No color when TERM unset.
### §3.8 TestResult (runner.rs)
    pub struct TestResult {
        pub id: &'static str, pub level: u8, pub requirement: Requirement,
        pub description: &'static str, pub verdict: TestVerdict,
        pub message: Option<String>, pub traces: Vec<HttpTrace>,
        pub duration_ms: u64,
    }
### §3.9 Enums (runner.rs)
    pub enum Requirement { Must, Should, May }
    pub enum TestVerdict { Pass, Fail, Warn, Skip }
### §3.10 TestBlob (testdata.rs)
    pub struct TestBlob {
        pub name: &'static str,
        pub content: &'static [u8],
        pub sha256: KappaLabel,
        pub blake3: KappaLabel,
    }
### §3.11 TestData (testdata.rs)
    pub struct TestData {
        pub empty: TestBlob,           // b""
        pub hello: TestBlob,           // b"hello"
        pub config: TestBlob,          // b"{\"type\":\"config\"}"
        pub layer_a: TestBlob,         // 4 KiB: b"layer-a-seed-data-0123456789abcdef" repeated
        pub layer_b: TestBlob,         // 4 KiB: b"layer-b-different-seed-9876543210!!" repeated
        pub unknown_fields: TestBlob,  // b"{\"known\":\"v\",\"_xyzzy\":\"preserve\"}"
        pub large_content: Vec<u8>,    // 4 MiB: b"large-blob-chunk-fill-pattern-1234!" repeated
        pub large_sha256: KappaLabel,
        pub simple_manifest: Vec<u8>,
        pub simple_manifest_sha256: KappaLabel,
        pub multi_manifest: Vec<u8>,
        pub multi_manifest_sha256: KappaLabel,
    }
    impl TestData { pub fn generate() -> Self }
    Manifests built from blob κ-labels:
        simple: {"config":"{config.sha256}","layers":["{layer_a.sha256}"]}
        multi:  {"config":"{config.sha256}","layers":["{layer_a.sha256}","{layer_b.sha256}"]}
    layer_a/layer_b content leaked to 'static via Box::leak.
### §3.12 Edge Canonical Form (edge_canon.rs)
    pub fn encode_edge(source: &KappaLabel, relation: &str, target: &KappaLabel, metadata: &[u8]) -> Vec<u8>
        source.as_bytes() || 0x00 || relation.as_bytes() || 0x00 ||
        target.as_bytes() || 0x00 || (metadata.len() as u32).to_be_bytes() || metadata
    For tests, metadata is always CBOR empty map: &[0xA0] (1 byte).
    No CBOR library needed.
---
## §4. Report Specifications
### §4.1 JUnit XML
    fn write_junit(results: &[TestResult], path: &Path)
    Pass → <testcase .../> self-closing
    Fail → <testcase><failure message="...">...</failure></testcase>
    Warn → <testcase><system-out>WARNING: ...</system-out></testcase>
    Skip → <testcase><skipped message="..."/></testcase>
    Grouped: <testsuite name="Level N"> per level.
    XML-escape: & < > "
### §4.2 HTML Report
    fn write_html(results: &[TestResult], registry_url: &str, path: &Path)
    Self-contained. No external CSS/JS.
    Header: registry URL, binary version, timestamp.
    Summary: pass/fail/warn/skip colored boxes.
    Per-level tables: ID | Req | Result | Description | ms | Detail.
    Row border-left color per verdict.
### §4.3 Terminal
    One line per test to stderr:
    [PASS]  L1-01  Version check (12ms)
    [FAIL]  L1-05  HEAD missing X-Kappa-Label — expected header present
---
## §5. Level 1 — Blob Operations (L1-01 through L1-91)
Each test: ID, requirement, HTTP request, expected response, assertions.
State carried across Level 1:
    upload_url: String — from L1-20
    chunk_size: usize — adjusted by L1-22
    chunks: Vec<&[u8]> — large_content split
### §5.1 Version Check
    L1-01 MUST "Version check"
    → GET /v2/  ← 200
    assert: body JSON has "kappa-distribution" key
### §5.2 Push/Pull/Verify
    L1-02 MUST "Push HELLO monolithic"
    → PUT /v2/{ns}/blobs/{hello.sha256} body: hello.content  ← 201|200
    L1-03 MUST "Push HELLO idempotent"
    → PUT /v2/{ns}/blobs/{hello.sha256} body: hello.content  ← 200|201
    L1-04 MUST "Wrong κ-label rejected"
    → PUT /v2/{ns}/blobs/{empty.sha256} body: hello.content  ← 409
    L1-05 MUST "HEAD headers"
    → HEAD /v2/{ns}/blobs/{hello.sha256}  ← 200
    assert: X-Kappa-Label present, Content-Length present
    L1-06 MUST "GET content"
    → GET /v2/{ns}/blobs/{hello.sha256}  ← 200
    assert: body == hello.content byte-for-byte
    L1-07 MUST "Verify κ independently"
    assert: KappaLabel::sha256(body_from_L1_06) == hello.sha256
    L1-08 MUST "GET absent 404"
    → GET /v2/{ns}/blobs/sha256:{"0"×64}  ← 404
    L1-09 MUST "HEAD absent 404"
    → HEAD /v2/{ns}/blobs/sha256:{"0"×64}  ← 404
### §5.3 Edge Cases
    L1-10 MUST "Push empty"
    → PUT /v2/{ns}/blobs/{empty.sha256} body: (0 bytes)  ← 201|200
    L1-11 MUST "GET empty"
    → GET /v2/{ns}/blobs/{empty.sha256}  ← 200  assert: body.len() == 0
    L1-12 MUST "Push unknown fields"
    → PUT /v2/{ns}/blobs/{unknown_fields.sha256} body: unknown_fields.content  ← 201|200
    L1-13 MUST "GET preserves fields"
    → GET /v2/{ns}/blobs/{unknown_fields.sha256}  ← 200
    assert: body == unknown_fields.content byte-for-byte
    L1-14 MUST "Push config + layers"
    Push config, layer_a, layer_b each via PUT. All ← 201|200.
    L1-15 MUST "List by prefix" (SHOULD if endpoint not implemented)
    → GET /v2/{ns}/blobs/?prefix=sha256:  ← 200
### §5.4 Content-Type
    L1-16 MUST "Content-Type round-trip"
    → PUT with Content-Type: application/json → GET same blob
    assert: response Content-Type contains "application/json"
    L1-17 MUST "Content-Type default"
    → GET /v2/{ns}/blobs/{hello.sha256}
    assert: Content-Type is "application/octet-stream" or absent
### §5.5 Chunked Upload
    L1-20 MUST "Upload POST"
    → POST /v2/{ns}/blobs/uploads/  ← 202
    assert: Location header present. Capture upload_url via resolve_location.
    L1-21 MUST "Location opaque"
    Pass by construction — code uses resolve_location, never parses UUID.
    L1-22 SHOULD "Chunk-Min-Length"
    If X-Kappa-Chunk-Min-Length in L1-20 response: chunk_size = max(config, header).
    Else: WARN. Split large_content into chunks.
    L1-23 MUST "PATCH chunk 1"
    → PATCH <upload_url> Content-Range: 0-{end} body: chunks[0]  ← 202
    assert: Range header present. Capture updated Location if changed.
    L1-24 MUST "PATCH chunk 2"
    → PATCH <upload_url> Content-Range: {next_start}-{next_end} body: chunks[1]  ← 202
    Capture updated Location if changed.
    L1-25 MUST "Duplicate 416"
    → PATCH <upload_url> Content-Range: 0-{chunks[0].len()-1} body: chunks[0]  ← 416
    L1-26 MUST "Recovery GET"
    → GET <upload_url>  ← 204
    assert: Range header present. assert: Location header present.
    L1-27 MUST "Out-of-order 416"
    → PATCH <upload_url> Content-Range: {far_offset}-{far_end} body: chunks[last]  ← 416
    L1-28 MUST "Recovery after 416"
    → GET <upload_url>  ← 204
    Parse Range to determine resume offset.
    L1-29 MUST "Remaining chunks"
    PATCH all remaining from resume offset. All ← 202.
    Capture updated Location from each response.
    L1-30 MUST "Upload complete"
    → PUT <upload_url>?kappa={large_sha256} body: (empty)  ← 201
    assert: X-Kappa-Label == large_sha256
    L1-31 MUST "GET chunked blob"
    → GET /v2/{ns}/blobs/{large_sha256}  ← 200
    assert: KappaLabel::sha256(body) == large_sha256
    L1-32 MUST "Wrong κ on close"
    New session: POST → PATCH one chunk → PUT ?kappa={hello.sha256}  ← 409
    L1-33 MUST "Cancel upload"
    → POST /v2/{ns}/blobs/uploads/  ← 202 capture cancel_url
    → DELETE <cancel_url>  ← 204
    L1-34 MUST "Cancelled 404"
    → GET <cancel_url>  ← 404|400
    L1-35 MUST "Cancelled not stored"
    Verify no new blob appeared from cancelled partial data.
### §5.6 Mount
    L1-40 SHOULD "Mount existing"
    → POST /v2/{ns}-mount/blobs/uploads/?mount={hello.sha256}&from={ns}
    ← 201 (success) or 202 (fallback). If 201: PASS. If 202: WARN.
    L1-41 SHOULD "Mounted retrievable"
    If L1-40 was 201: → GET /v2/{ns}-mount/blobs/{hello.sha256}  ← 200
    L1-42 SHOULD "Mount absent fallback"
    → POST mount with sha256:{"f"×64}  ← 202
    assert: Location header present (upload session for fallback).
    L1-43 SHOULD "Fallback URL valid"
    assert: Location from L1-42 is a usable upload URL.
    L1-44 SHOULD "Mount without from"
    → POST mount without from= parameter. ← 201|202. If 4xx: WARN.
### §5.7 Multi-Label
    L1-50 SHOULD "Multi-label push"
    → PUT /v2/{ns}/blobs/{hello.sha256}?also={hello.blake3} body: hello.content
    ← 201|200 (supported) or other (unsupported → WARN).
    L1-51 SHOULD "Both resolvable"
    If L1-50 succeeded: GET by sha256 → 200, GET by blake3 → 200.
    L1-52 SHOULD "Multi-label fallback"
    If L1-50 unsupported: verify primary κ-label still works.
    L1-53 MUST "Multi-label bad κ rejected"
    → PUT /v2/{ns}/blobs/{hello.sha256}?also=sha256:{"0"×64} body: hello.content  ← 409
### §5.8 Redirect
    L1-60 SHOULD "Redirect followed"
    If any blob GET in Level 1 received a 3xx: verify final response is 200.
    If no redirects observed: SKIP.
    L1-61 MUST "Auth not forwarded cross-host"
    Validated by reqwest redirect policy configuration. Pass by construction.
    L1-62 MUST "Redirect final response correct"
    If redirect observed: verify final body matches expected κ-label.
### §5.9 Teardown
    L1-90 MAY "Blob deletion"
    → DELETE /v2/{ns}/blobs/{hello.sha256}  ← 202|405. If 405: WARN.
    L1-91 MAY "Teardown cleanup"
    Delete remaining test blobs per KAPPA_TEARDOWN_ORDER.
---
## §6. Level 2 — Tag Operations (L2-01 through L2-64)
Precondition: blobs from Level 1 exist. Setup re-pushes if Level 1 was not run.
### §6.1 Bind and Resolve
    L2-01 MUST "Bind tag v1.0"
    → PUT /v2/{ns}/manifests/v1.0 Content-Type: application/json body: simple_manifest  ← 201
    L2-02 MUST "X-Kappa-Label in response"
    assert: X-Kappa-Label header == simple_manifest_sha256
    L2-03 MUST "Resolve tag v1.0"
    → GET /v2/{ns}/manifests/v1.0  ← 200
    assert: body == simple_manifest, X-Kappa-Label == simple_manifest_sha256
    L2-04 MUST "Bind latest same content"
    → PUT /v2/{ns}/manifests/latest body: simple_manifest  ← 201|200
    L2-05 MUST "Both tags same κ"
    → GET /v2/{ns}/manifests/latest  assert: X-Kappa-Label == simple_manifest_sha256
### §6.2 Update and Retain
    L2-06 MUST "Update latest"
    → PUT /v2/{ns}/manifests/latest body: multi_manifest  ← 201|200
    L2-07 MUST "Latest is new κ"
    → GET /v2/{ns}/manifests/latest  assert: X-Kappa-Label == multi_manifest_sha256
    L2-08 MUST "v1.0 retained"
    → GET /v2/{ns}/manifests/v1.0  assert: X-Kappa-Label == simple_manifest_sha256
### §6.3 Absent
    L2-09 MUST "Content-before-tag"
    PUT /manifests/{tag} stores+tags atomically. Verify X-Kappa-Label matches
    independently computed hash. PASS by construction.
    L2-10 MUST "Absent tag 404"
    → GET /v2/{ns}/manifests/nonexistent  ← 404
### §6.4 Tag List
    L2-11 MUST "List ordered"
    → GET /v2/{ns}/tags/list  ← 200  assert: tags in ASCIIbetical order, includes v1.0 + latest
    L2-12 MUST "Pagination n=1"
    → GET /v2/{ns}/tags/list?n=1  ← 200  assert: ≤1 result, Link header present
    L2-13 MUST "n=0 empty no Link"
    → GET /v2/{ns}/tags/list?n=0  ← 200  assert: empty, no Link header
    L2-14 MUST "Pagination cursor"
    → GET /v2/{ns}/tags/list?n=1&last={first_tag}  ← 200
    assert: result excludes first_tag (cursor is exclusive)
    L2-15 MUST "Cursor is name"
    Pass by construction: L2-14 passes a tag name string, not a number.
### §6.5 Direct κ
    L2-16 MUST "Resolve by κ"
    → GET /v2/{ns}/manifests/{simple_manifest_sha256}  ← 200
    assert: body == simple_manifest, KappaLabel::sha256(body) matches
### §6.6 Timestamp Tags
    L2-20 SHOULD "Timestamp tag 1"
    → PUT /v2/{ns}/manifests/2026-07-20T00:00:00 body: simple_manifest  ← 201|200
    L2-21 SHOULD "Timestamp tag 2"
    → PUT /v2/{ns}/manifests/2026-07-20T12:00:00 body: simple_manifest  ← 201|200
    L2-22 SHOULD "Range filter"
    → GET /v2/{ns}/tags/list?after=2026-07-19T23:00:00&before=2026-07-20T06:00:00
    assert: contains T00:00:00, not T12:00:00. If registry ignores range: WARN.
    L2-23 SHOULD "Descending"
    → GET /v2/{ns}/tags/list?order=desc  assert: reverse ASCIIbetical
    L2-24 SHOULD "Ascending"
    → GET /v2/{ns}/tags/list?order=asc  assert: ASCIIbetical
### §6.7 Compare-and-Swap
    L2-30 SHOULD "CAS success"
    If registry supports CAS (implementation-specific mechanism): attempt with correct expected → success.
    If no CAS mechanism: SKIP.
    L2-31 SHOULD "CAS conflict"
    Attempt with wrong expected → conflict.
    L2-32 SHOULD "CAS non-existent"
    CAS on absent tag with absence sentinel → success if supported.
    L2-33 SHOULD "CAS value after success"
    → GET tag → verify updated value.
    L2-34 SHOULD "CAS value after failure"
    → GET tag → verify unchanged.
### §6.8 Multi-Tag Bind
    L2-40 SHOULD "Multi-tag bind"
    → PUT /v2/{ns}/manifests/mtag-a?tag=mtag-b body: simple_manifest
    ← 201|200 (supported) or other (WARN).
    L2-41 SHOULD "Both resolve"
    If L2-40 supported: GET mtag-a → 200, GET mtag-b → 200, same X-Kappa-Label.
    L2-42 SHOULD "Multi-tag fallback"
    If unsupported: verify single tag works.
### §6.9 Warnings
    L2-50 SHOULD "Warning format"
    If any Warning header observed: assert format "299 - \"...\"".
    L2-51 MUST "Warning ≤ 4096"
    assert: total Warning bytes ≤ 4096.
    L2-52 SHOULD "Warning reported"
    Record all warnings in report.
### §6.10 Tag Deletion
    L2-60 MAY "Delete tag"
    → DELETE /v2/{ns}/manifests/v1.0  ← 202|400|405
    L2-61 MUST "Rejection uses UNSUPPORTED"
    If L2-60 returned 400: verify_error_response(body, "UNSUPPORTED").
    L2-62 MAY "Deleted 404"
    If L2-60 was 202: → GET /v2/{ns}/manifests/v1.0  ← 404
    L2-63 MUST "Content survives"
    If L2-60 was 202: → GET /v2/{ns}/blobs/{simple_manifest_sha256}  ← 200
    L2-64 MAY "Teardown tags"
    Delete remaining tags per teardown order.
---
## §7. Level 3 — Edge Operations (L3-01 through L3-22)
Precondition: Level 1 blobs + Level 2 tags populated.
State: edge_kappa (from L3-01), referrer_edge_kappa (from L3-13).
    L3-01 MUST "Create edge"
    → PUT /v2/{ns}/edges/ Content-Type: application/json
      body: {"source":"{simple_manifest_sha256}","relation":"derives-from",
             "target":"{config.sha256}","metadata":{}}
    ← 201|200  capture: edge_kappa from X-Kappa-Label
    L3-02 MUST "Edge κ in response"
    assert: X-Kappa-Label present in L3-01 response, parses as valid κ-label.
    L3-03 MUST "Edge is blob"
    → GET /v2/{ns}/blobs/{edge_kappa}  ← 200
    assert: KappaLabel::sha256(body) == edge_kappa
    L3-04 MUST "Canonical form"
    Compute expected = edge_canon::encode_edge(simple_manifest_sha256, "derives-from", config.sha256, &[0xA0])
    assert: body from L3-03 == expected byte-for-byte
    L3-05 MUST "Query outbound"
    → GET /v2/{ns}/edges/{simple_manifest_sha256}?direction=outbound&relation=derives-from  ← 200
    assert: edges array has entry with target == config.sha256
    L3-06 MUST "Query inbound"
    → GET /v2/{ns}/edges/{config.sha256}?direction=inbound&relation=derives-from  ← 200
    assert: edges has source == simple_manifest_sha256
    L3-07 MUST "Relation filter"
    → GET /v2/{ns}/edges/{simple_manifest_sha256}?direction=outbound&relation=owns  ← 200
    assert: edges empty (no "owns" edges)
    L3-08 MUST "Direction both"
    → GET /v2/{ns}/edges/{config.sha256}?direction=both  ← 200
    assert: includes the derives-from edge
    L3-09 MUST "Idempotent"
    Repeat L3-01 body. assert: X-Kappa-Label == edge_kappa from first time.
    L3-10 MUST "Absent source rejected"
    → PUT edge with source sha256:{"a"×64}  ← 4xx
    L3-11 MUST "Absent target accepted"
    → PUT edge with source hello.sha256, target sha256:{"b"×64}  ← 201|200
    L3-12 MUST "Axis matches source"
    assert: edge_kappa from L3-01 starts with "sha256:" (source was sha256)
    L3-13 MUST "Referrer edge"
    → PUT edge: source=multi_manifest_sha256, relation="refers-to", target=simple_manifest_sha256  ← 201|200
    capture: referrer_edge_kappa
    L3-14 MUST "Referrer queryable"
    → GET /v2/{ns}/edges/{simple_manifest_sha256}?direction=inbound&relation=refers-to  ← 200
    assert: edges includes source == multi_manifest_sha256
    L3-15 MUST "Absent subject edge"
    → PUT edge: source=hello.sha256, relation="refers-to", target=sha256:{"c"×64}  ← 201|200
    L3-16 MUST "Query absent subject"
    → GET /v2/{ns}/edges/sha256:{"c"×64}?direction=inbound  ← 200
    assert: edges includes the refers-to edge from L3-15
    L3-17 SHOULD "Edge pagination"
    → GET /v2/{ns}/edges/{simple_manifest_sha256}?n=1  ← 200
    If multiple edges exist: assert result ≤1, pagination available.
    L3-20 MUST "Delete edge"
    Create sacrificial edge, capture κ. → DELETE /v2/{ns}/edges/{sac_kappa}  ← 202
    L3-21 MUST "Deleted absent"
    → GET /v2/{ns}/edges/{simple_manifest_sha256}?direction=outbound
    assert: deleted edge not in results
    L3-22 MUST "Blobs survive"
    → HEAD /v2/{ns}/blobs/{hello.sha256}  ← 200
---
## §8. Level 4 — Composition, Witnesses, Schemas (L4-01 through L4-48)
Precondition: Levels 1-3. State: composed_kappa, witness_kappa, f4_kappa,
e8_kappa, schema_kappa, valid_kappa.
### §8.1 G2
    L4-01 MUST "G2 compose"
    → POST /v2/{ns}/compose/g2 body: {"operands":["{hello.sha256}","{config.sha256}"]}  ← 200
    capture: composed_kappa, witness_kappa from body.composed, body.witness
    L4-02 MUST "Response fields"
    assert: body has composed, witness, operands (array with both), operation=="g2"
    L4-03 MUST "Commutative"
    → POST /v2/{ns}/compose/g2 body: {"operands":["{config.sha256}","{hello.sha256}"]}  ← 200
    assert: body.composed == composed_kappa
    L4-04 MUST "Composed stored"
    → GET /v2/{ns}/blobs/{composed_kappa}  ← 200
    L4-05 MUST "Composed-of edges"
    → GET /v2/{ns}/edges/{composed_kappa}?direction=outbound&relation=composed-of  ← 200
    assert: edges contain targets hello.sha256 and config.sha256
    L4-06 MUST "Edge metadata operation"
    assert: ≥1 composed-of edge metadata contains "g2"
    L4-07 MUST "Witness retrievable"
    → GET /v2/{ns}/witnesses/{composed_kappa}  ← 200
    L4-08 MUST "Witness header"
    assert: body[0..2] u16 LE == 71, body[2..4] u16 LE == 32
    L4-09 MUST "Witness-of edge"
    → GET /v2/{ns}/edges/{composed_kappa}?direction=inbound&relation=witness-of  ← 200
    assert: edges contain source == witness_kappa
    L4-10 MUST "Cross-axis rejected"
    → POST /v2/{ns}/compose/g2 body: {"operands":["{hello.sha256}","{hello.blake3}"]}  ← 422
    L4-11 MUST "AXIS_MISMATCH code"
    verify_error_response(body, "AXIS_MISMATCH")
### §8.2 F4/E6/E7/E8
    L4-20 MUST "F4 compose"
    → POST /v2/{ns}/compose/f4 body: {"operands":["{hello.sha256}"]}  ← 200
    capture: f4_kappa
    L4-21 MUST "F4 mirror"
    complement = hello.sha256.complement()
    → POST /v2/{ns}/compose/f4 body: {"operands":["{complement}"]}  ← 200
    assert: body.composed == f4_kappa
    L4-22 MUST "E6 compose"
    → POST /v2/{ns}/compose/e6 body: {"operands":["{hello.sha256}"]}  ← 200
    L4-23 MUST "E6 tag byte"
    Decode first byte of hello.sha256 digest. Compute byte % 9.
    assert: if %9 ∈ [0,7] tag should be 0x05; if %9 == 8 tag should be 0x06.
    Retrieve composed blob, verify first byte after axis prefix matches.
    L4-24 MUST "E7 compose"
    → POST /v2/{ns}/compose/e7 body: {"operands":["{hello.sha256}"]}  ← 200
    L4-25 MUST "E8 compose"
    → POST /v2/{ns}/compose/e8 body: {"operands":["{hello.sha256}"]}  ← 200
    capture: e8_kappa
    L4-26 MUST "E8 ≠ operand"
    assert: e8_kappa != hello.sha256
    L4-27 MAY "Coincidence documented"
    If f4_kappa or e7 result has same digest as e8_kappa: log in report.
    L4-28 MUST "Coincidence disambiguated"
    For any coincidence: verify composed-of edge metadata distinguishes operation type.
### §8.3 Schema
    L4-40 MUST "Schema register"
    → PUT /v2/{ns}/schemas/test-scope body: {"scope":"test-scope","format":"json-schema",
      "validation":{"type":"object","required":["name"],"properties":{"name":{"type":"string"}}}}
    ← 201  capture: schema_kappa
    L4-41 MUST "Schema retrieve"
    → GET /v2/{ns}/schemas/test-scope  ← 200  assert: X-Kappa-Label == schema_kappa
    L4-42 MUST "Schema list"
    → GET /v2/{ns}/schemas/  ← 200  assert: includes test-scope
    L4-43 MUST "Valid accepted"
    → PUT /v2/{ns}/manifests/schema-pass Content-Type: application/json body: {"name":"valid"}
    ← 201  capture: valid_kappa from X-Kappa-Label
    L4-44 MUST "Invalid rejected"
    → PUT /v2/{ns}/manifests/schema-fail body: {"not_name":"invalid"}  ← 422
    verify_error_response(body, "SCHEMA_VIOLATION")
    L4-45 MUST "Rejected not stored"
    rejected_kappa = KappaLabel::sha256(b'{"not_name":"invalid"}')
    → GET /v2/{ns}/blobs/{rejected_kappa}  ← 404
    L4-46 MUST "Schema ≠ κ-modifier"
    expected = KappaLabel::sha256(b'{"name":"valid"}')
    assert: expected == valid_kappa from L4-43
    Push same to unscoped path: → PUT /v2/{ns}-noscope/blobs/{expected}  ← 201|200
    L4-47 SHOULD "Schema evolution"
    → PUT /v2/{ns}/schemas/test-scope body: (updated schema with added optional field)  ← 201
    capture: new_schema_kappa
    L4-48 SHOULD "Evolution edge"
    → GET /v2/{ns}/edges/{new_schema_kappa}?direction=outbound&relation=derives-from
    assert: edges contain target == schema_kappa
---
## §9. Level 5 — GC, Admission, Federation (L5-01 through L5-62)
Precondition: Levels 1-3. Setup re-pushes if needed.
State: pin_kappa, finalizer_pin_kappa, filter_kappa, peer_only_kappa.
### §9.1 GC
    L5-01 MUST "Pin create"
    → POST /v2/{ns}/gc/pin body: {"kappa":"{hello.sha256}","ttl":0}  ← 201
    L5-02 MUST "Pin κ in response"
    assert: X-Kappa-Label present. capture: pin_kappa.
    L5-03 MUST "Owns edge"
    → PUT edge: hello.sha256 owns layer_a.sha256  ← 201|200
    L5-04 MUST "Remove tag from owned"
    If layer_a is tagged: → DELETE tag. Otherwise PASS (not tagged).
    L5-05 MUST "Sweep trigger"
    → POST /v2/{ns}/gc/sweep  ← 202
    L5-06 MUST "Sweep status"
    Poll GET /v2/{ns}/gc/status 1s interval, 60s max. assert: last_sweep recent.
    L5-07 MUST "Pinned survives"
    → GET /v2/{ns}/blobs/{hello.sha256}  ← 200
    L5-08 MUST "Reachable survives"
    → GET /v2/{ns}/blobs/{layer_a.sha256}  ← 200
    L5-09 MUST "Unpin"
    → POST /v2/{ns}/gc/unpin body: {"pin_kappa":"{pin_kappa}"}  ← 200
    L5-10 SHOULD "Unpin derives"
    If response includes X-Kappa-Label for new pin blob: verify it differs from pin_kappa.
    L5-11 MUST "Sweep after unpin"
    → POST /v2/{ns}/gc/sweep  ← 202
    L5-12 MUST "Poll sweep"
    Poll gc/status until complete. Same as L5-06.
    L5-13 SHOULD "Unpinned evicted"
    → GET /v2/{ns}/blobs/{hello.sha256}  ← 404 (expected) or 200 (WARN timing)
    L5-14 SHOULD "Unreachable evicted"
    → GET /v2/{ns}/blobs/{layer_a.sha256}  ← 404 (expected) or 200 (WARN)
    L5-15 MUST "Finalizer pin"
    Re-push config if evicted. → POST gc/pin body: {"kappa":"{config.sha256}","ttl":0,"controller":"test-ctrl"}  ← 201
    capture: finalizer_pin_kappa
    L5-16 MUST "Unpin blocked"
    → POST gc/unpin body: {"pin_kappa":"{finalizer_pin_kappa}"}  ← 409
    verify_error_response(body, "FINALIZER_OUTSTANDING")
    L5-17 MUST "Finalizer in status"
    → GET gc/status  assert: pending_finalizers includes "test-ctrl"
    L5-18 MUST "Finalizer release"
    Implementation-specific: POST a release pin blob or endpoint.
    If not discoverable: SKIP with note.
    L5-19 MUST "Unpin after release"
    → POST gc/unpin body: {"pin_kappa":"{finalizer_pin_kappa}"}  ← 200
    L5-20 SHOULD "Finalized evicted"
    Trigger sweep, poll, → GET /v2/{ns}/blobs/{config.sha256}  ← 404|200
### §9.2 Admission
    L5-30 MUST "Filter register"
    → PUT /v2/{ns}/filters/test-filter-scope body: {"type":"json-match","accept_if":{"contains":"accept"},
      "reason":"must contain accept"}  ← 201  capture: filter_kappa
    If 422 (format unsupported): SKIP all filter tests.
    L5-31 MUST "Filter list"
    → GET /v2/{ns}/filters/  assert: includes test-filter-scope
    L5-32 MUST "Filter accepts"
    → PUT /v2/{ns}/manifests/filter-pass body: {"data":"this should accept"}  ← 201
    L5-33 MUST "Filter rejects"
    → PUT /v2/{ns}/manifests/filter-fail body: {"data":"this will be refused"}  ← 422
    L5-34 MUST "Rejection reason"
    verify_error_response(body, "FILTER_REJECTED")
    assert: error message or detail contains reason string
    L5-35 MUST "Rejected not stored"
    rejected_kappa = KappaLabel::sha256(rejected_body)
    → GET /v2/{ns}/blobs/{rejected_kappa}  ← 404
    L5-36 MUST "Filter remove"
    → DELETE /v2/{ns}/filters/{filter_kappa}  ← 202
    L5-37 MUST "Filter blob persists"
    → GET /v2/{ns}/blobs/{filter_kappa}  ← 200
    L5-38 MUST "Prev-rejected accepted"
    → PUT /v2/{ns}/manifests/filter-fail body: (same as L5-33)  ← 201
### §9.3 Federation
    Precondition: KAPPA_FEDERATION_PEERS set. All SKIP if unset.
    L5-50 SHOULD "Push to peer"
    peer_content = format!("federation-test-{}", uuid::Uuid::new_v4())
    peer_only_kappa = KappaLabel::sha256(peer_content.as_bytes())
    → PUT {peer_url}/v2/{ns}/blobs/{peer_only_kappa} body: peer_content  ← 201|200
    L5-51 SHOULD "Primary absent"
    → GET /v2/{ns}/blobs/{peer_only_kappa}  ← 200 (federation) or 404 (no federation)
    L5-52 SHOULD "Federation resolved"
    If 200: PASS. If 404: WARN.
    L5-53 MUST "Federation verified"
    If 200: assert KappaLabel::sha256(body) == peer_only_kappa
    L5-54 SHOULD "Cached locally"
    → GET same blob again. Measure time. Should be faster than L5-51.
    L5-55 SHOULD "Cache faster"
    If L5-54 time < L5-51 time: PASS. Else: WARN.
    L5-56 SHOULD "Multi-peer"
    If ≥2 peers: push unique blob to peer2, GET from primary.
    L5-57 SHOULD "Multi-hop"
    If topology supports A→B→C: push to C, GET from A.
    L5-58 SHOULD "Multi-hop verified"
    If multi-hop succeeded: assert KappaLabel::sha256(body) matches.
### §9.4 Rate Limiting
    L5-60 SHOULD "429 Retry-After"
    If any 429 observed: assert Retry-After header present.
    L5-61 MUST "Retry-After format"
    assert: value is positive integer or HTTP-date.
    L5-62 MAY "Rate limit recorded"
    Log rate limit behavior in report.
---
## §10. Error Response Verification
    fn verify_error_response(body: &[u8], expected_code: &str) -> Result<(), String>
    Parse JSON. Check errors array. Check errors[0].code. Check [A-Z_]+. Check == expected_code.
    Error code mapping:
        409 wrong κ-label        → DIGEST_INVALID       (L1-04, L1-32, L1-53)
        422 schema violation     → SCHEMA_VIOLATION      (L4-44)
        422 filter rejection     → FILTER_REJECTED       (L5-33, L5-34)
        422 axis mismatch        → AXIS_MISMATCH         (L4-10, L4-11)
        416 out-of-order chunk   → (status only, no JSON) (L1-25, L1-27)
        404 absent               → BLOB_UNKNOWN/TAG_UNKNOWN/EDGE_UNKNOWN
        400 unsupported delete   → UNSUPPORTED            (L2-61)
        409 finalizer block      → FINALIZER_OUTSTANDING  (L5-16)
        429 rate limit           → TOOMANYREQUESTS        (L5-60)
---
## §11. main.rs Control Flow
    fn main() {
        let config = Config::from_env();
        let client = RegistryClient::new(&config);
        let data = TestData::generate();
        let mut runner = TestRunner::new();
        // connectivity check
        match client.get("/v2/").send() {
            Err(e) => { eprintln!("unreachable: {e}"); exit(3); }
            Ok(r) if r.status() == 404 => { eprintln!("not kappa"); exit(3); }
            Ok(_) => {}
        }
        if config.levels.contains(&1) { tests::level1::run(&mut runner, &client, &config, &data); }
        if config.levels.contains(&2) { tests::level2::run(&mut runner, &client, &config, &data); }
        if config.levels.contains(&3) { tests::level3::run(&mut runner, &client, &config, &data); }
        if config.levels.contains(&4) { tests::level4::run(&mut runner, &client, &config, &data); }
        if config.levels.contains(&5) { tests::level5::run(&mut runner, &client, &config, &data); }
        fs::create_dir_all(&config.report_dir).ok();
        report::write_junit(&runner.results, &config.report_dir.join("kappa-conformance.xml"));
        report::write_html(&runner.results, &config.registry_url, &config.report_dir.join("kappa-conformance.html"));
        let (p,f,w,s) = runner.summary();
        eprintln!("{p} pass, {f} fail, {w} warn, {s} skip");
        exit(if runner.has_failures() { 1 } else { 0 });
    }
    Each level: pub fn run(runner: &mut TestRunner, client: &RegistryClient, config: &Config, data: &TestData)
---
## §12. Build, Distribution, CI
    cargo build --release --target x86_64-unknown-linux-musl
    cargo build --release --target aarch64-unknown-linux-musl
    Dockerfile: FROM scratch / COPY kappa-conformance / ENTRYPOINT
    CI: docker run -e KAPPA_REGISTRY_URL=... -v ./reports:/kappa-conformance-report
    Preconditions: each level setup ensures blobs exist (HEAD, re-push if 404).
    Teardown: configurable order, failures logged not fatal.
    HTTP coverage matrix:
        verify κ on PUT           → L1-04, L1-32, L1-53
        exact-bytes storage       → L1-13
        X-Kappa-Label required    → L1-05, L2-02
        Content-Length on HEAD    → L1-05
        404 for absent            → L1-08, L1-09, L2-10, L4-45, L5-35
        416 on bad range          → L1-25, L1-27
        204 on GET session        → L1-26, L1-28
        204 on DELETE session     → L1-33
        idempotent re-put         → L1-03
        ASCIIbetical tags         → L2-11
        n=0 empty no Link         → L2-13
        cursor is name            → L2-14, L2-15
        edge source exists        → L3-10
        edge target may absent    → L3-11
        edge axis = source        → L3-12
        cross-axis rejected       → L4-10
        G2 commutative            → L4-03
        schema ≠ κ-modifier       → L4-46
        rejected not stored       → L4-45, L5-35
        finalizer blocks unpin    → L5-16
        filter fail-closed        → L5-33
        federation verified       → L5-53
        warning ≤ 4096            → L2-51
        auth not forwarded        → L1-61
---
## §13. Stable Test ID Registry
    ID      Lvl  Req    Description
    ──────  ───  ─────  ──────────────────────────────────────────
    L1-01   1    MUST   Version check
    L1-02   1    MUST   Push blob monolithic
    L1-03   1    MUST   Push blob idempotent
    L1-04   1    MUST   Wrong κ-label rejected
    L1-05   1    MUST   HEAD headers present
    L1-06   1    MUST   GET content correct
    L1-07   1    MUST   Verify κ independently
    L1-08   1    MUST   GET absent 404
    L1-09   1    MUST   HEAD absent 404
    L1-10   1    MUST   Push empty blob
    L1-11   1    MUST   GET empty blob
    L1-12   1    MUST   Push unknown fields
    L1-13   1    MUST   GET preserves fields
    L1-14   1    MUST   Push config + layers
    L1-15   1    MUST   List blobs by prefix
    L1-16   1    MUST   Content-Type round-trip
    L1-17   1    MUST   Content-Type default
    L1-20   1    MUST   Upload POST start
    L1-21   1    MUST   Location opaque
    L1-22   1    SHLD   Chunk-Min-Length
    L1-23   1    MUST   PATCH chunk 1
    L1-24   1    MUST   PATCH chunk 2
    L1-25   1    MUST   Duplicate 416
    L1-26   1    MUST   Recovery GET
    L1-27   1    MUST   Out-of-order 416
    L1-28   1    MUST   Recovery after 416
    L1-29   1    MUST   Remaining chunks
    L1-30   1    MUST   Upload complete
    L1-31   1    MUST   GET chunked blob
    L1-32   1    MUST   Wrong κ on close
    L1-33   1    MUST   Cancel upload
    L1-34   1    MUST   Cancelled 404
    L1-35   1    MUST   Cancelled not stored
    L1-40   1    SHLD   Mount existing
    L1-41   1    SHLD   Mounted retrievable
    L1-42   1    SHLD   Mount absent fallback
    L1-43   1    SHLD   Fallback URL valid
    L1-44   1    SHLD   Mount without from
    L1-50   1    SHLD   Multi-label push
    L1-51   1    SHLD   Multi-label both resolve
    L1-52   1    SHLD   Multi-label fallback
    L1-53   1    MUST   Multi-label bad κ
    L1-60   1    SHLD   Redirect followed
    L1-61   1    MUST   Auth not forwarded cross-host
    L1-62   1    MUST   Redirect final response correct
    L1-90   1    MAY    Blob deletion
    L1-91   1    MAY    Teardown cleanup
    L2-01   2    MUST   Bind tag
    L2-02   2    MUST   X-Kappa-Label in response
    L2-03   2    MUST   Resolve tag
    L2-04   2    MUST   Bind second tag same content
    L2-05   2    MUST   Both tags same κ
    L2-06   2    MUST   Update tag
    L2-07   2    MUST   Updated tag new κ
    L2-08   2    MUST   Old tag retained
    L2-09   2    MUST   Content-before-tag
    L2-10   2    MUST   Absent tag 404
    L2-11   2    MUST   List ordered
    L2-12   2    MUST   Pagination n=1
    L2-13   2    MUST   n=0 empty no Link
    L2-14   2    MUST   Pagination cursor
    L2-15   2    MUST   Cursor is name
    L2-16   2    MUST   Direct κ resolve
    L2-20   2    SHLD   Timestamp tag 1
    L2-21   2    SHLD   Timestamp tag 2
    L2-22   2    SHLD   Range filter
    L2-23   2    SHLD   Descending order
    L2-24   2    SHLD   Ascending order
    L2-30   2    SHLD   CAS success
    L2-31   2    SHLD   CAS conflict
    L2-32   2    SHLD   CAS non-existent
    L2-33   2    SHLD   CAS value after success
    L2-34   2    SHLD   CAS value after failure
    L2-40   2    SHLD   Multi-tag bind
    L2-41   2    SHLD   Multi-tag both resolve
    L2-42   2    SHLD   Multi-tag fallback
    L2-50   2    SHLD   Warning format
    L2-51   2    MUST   Warning ≤ 4096
    L2-52   2    SHLD   Warning reported
    L2-60   2    MAY    Tag deletion
    L2-61   2    MUST   Deletion rejection UNSUPPORTED
    L2-62   2    MAY    Deleted tag 404
    L2-63   2    MUST   Content survives tag deletion
    L2-64   2    MAY    Teardown tag cleanup
    L3-01   3    MUST   Create edge
    L3-02   3    MUST   Edge κ in response
    L3-03   3    MUST   Edge is blob
    L3-04   3    MUST   Canonical form correct
    L3-05   3    MUST   Query outbound
    L3-06   3    MUST   Query inbound
    L3-07   3    MUST   Relation filter
    L3-08   3    MUST   Direction both
    L3-09   3    MUST   Idempotent
    L3-10   3    MUST   Absent source rejected
    L3-11   3    MUST   Absent target accepted
    L3-12   3    MUST   Axis matches source
    L3-13   3    MUST   Referrer edge
    L3-14   3    MUST   Referrer queryable
    L3-15   3    MUST   Absent subject edge
    L3-16   3    MUST   Query absent subject
    L3-17   3    SHLD   Edge pagination
    L3-20   3    MUST   Delete edge
    L3-21   3    MUST   Deleted absent from queries
    L3-22   3    MUST   Blobs survive edge delete
    L4-01   4    MUST   G2 compose
    L4-02   4    MUST   Response fields
    L4-03   4    MUST   G2 commutative
    L4-04   4    MUST   Composed stored
    L4-05   4    MUST   Composed-of edges
    L4-06   4    MUST   Edge metadata operation
    L4-07   4    MUST   Witness retrievable
    L4-08   4    MUST   Witness header
    L4-09   4    MUST   Witness-of edge
    L4-10   4    MUST   Cross-axis rejected
    L4-11   4    MUST   AXIS_MISMATCH code
    L4-20   4    MUST   F4 compose
    L4-21   4    MUST   F4 mirror
    L4-22   4    MUST   E6 compose
    L4-23   4    MUST   E6 tag byte
    L4-24   4    MUST   E7 compose
    L4-25   4    MUST   E8 compose
    L4-26   4    MUST   E8 ≠ operand
    L4-27   4    MAY    Coincidence documented
    L4-28   4    MUST   Coincidence disambiguated
    L4-40   4    MUST   Schema register
    L4-41   4    MUST   Schema retrieve
    L4-42   4    MUST   Schema list
    L4-43   4    MUST   Valid accepted
    L4-44   4    MUST   Invalid rejected
    L4-45   4    MUST   Rejected not stored
    L4-46   4    MUST   Schema ≠ κ-modifier
    L4-47   4    SHLD   Schema evolution
    L4-48   4    SHLD   Evolution edge
    L5-01   5    MUST   Pin create
    L5-02   5    MUST   Pin κ in response
    L5-03   5    MUST   Owns edge
    L5-04   5    MUST   Remove tag from owned
    L5-05   5    MUST   Sweep trigger
    L5-06   5    MUST   Sweep status
    L5-07   5    MUST   Pinned survives
    L5-08   5    MUST   Reachable survives
    L5-09   5    MUST   Unpin
    L5-10   5    SHLD   Unpin derives
    L5-11   5    MUST   Sweep after unpin
    L5-12   5    MUST   Poll sweep
    L5-13   5    SHLD   Unpinned evicted
    L5-14   5    SHLD   Unreachable evicted
    L5-15   5    MUST   Finalizer pin
    L5-16   5    MUST   Unpin blocked
    L5-17   5    MUST   Finalizer in status
    L5-18   5    MUST   Finalizer release
    L5-19   5    MUST   Unpin after release
    L5-20   5    SHLD   Finalized evicted
    L5-30   5    MUST   Filter register
    L5-31   5    MUST   Filter list
    L5-32   5    MUST   Filter accepts
    L5-33   5    MUST   Filter rejects
    L5-34   5    MUST   Rejection reason
    L5-35   5    MUST   Rejected not stored
    L5-36   5    MUST   Filter remove
    L5-37   5    MUST   Filter blob persists
    L5-38   5    MUST   Prev-rejected accepted
    L5-50   5    SHLD   Push to peer
    L5-51   5    SHLD   Primary absent
    L5-52   5    SHLD   Federation resolved
    L5-53   5    MUST   Federation verified
    L5-54   5    SHLD   Cached locally
    L5-55   5    SHLD   Cache faster
    L5-56   5    SHLD   Multi-peer
    L5-57   5    SHLD   Multi-hop
    L5-58   5    SHLD   Multi-hop verified
    L5-60   5    SHLD   429 Retry-After
    L5-61   5    MUST   Retry-After format
    L5-62   5    MAY    Rate limit recorded
    ─────────────────────────────────
    Total: 149 tests
    MUST:  107    SHOULD: 36    MAY: 6
---
## §14. Expected κ-Label Reference
    Content                                          SHA-256                                                                     BLAKE3
    ───────────────────────────────────────────────  ──────────────────────────────────────────────────────────────────────────  ──────────────────────────────────────────────────────────────────────────
    (empty)                                          sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  blake3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
    "hello"                                          sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  blake3:ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f
    {"type":"config"}                                (computed at build)                                                         (computed at build)
    {"known":"v","_xyzzy":"preserve"}                (computed at build)                                                         (computed at build)
    layer_a (4096B seed)                             (computed at build)                                                         (computed at build)
    layer_b (4096B seed)                             (computed at build)                                                         (computed at build)
    large (4MiB seed)                                (computed at build)                                                         (computed at build)
    simple_manifest                                  (computed from generated JSON)                                              —
    multi_manifest                                   (computed from generated JSON)                                              —
---
## §15. Implementation Constraints
    15.1  No shared code with registry implementations
    15.2  No async — blocking reqwest, synchronous execution
    15.3  No panics in test logic — match/if-let, never unwrap/expect
    15.4  Deterministic data — no RNG except uuid for L5-50
    15.5  No network beyond registry + peers
    15.6  Stable test IDs — never renumbered, retired = SKIP
    15.7  Edge canonical form — encode_edge() in edge_canon.rs (~20 lines, no CBOR lib, metadata = 0xA0)
    15.8  F4 complement — KappaLabel::complement() (~15 lines, decode hex, NOT, re-encode)
    15.9  Sweep polling — 1s interval, 60s timeout, not configurable
    15.10 Upload Location tracking — capture updated Location from every PATCH response
