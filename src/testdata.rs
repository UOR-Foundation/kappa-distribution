//! Test data generation (impl spec §3.10-§3.11).

use crate::kappa::KappaLabel;

pub struct TestBlob {
    pub name: &'static str,
    pub content: &'static [u8],
    pub sha1: KappaLabel,
    pub sha256: KappaLabel,
    pub blake3: KappaLabel,
}

pub struct TestData {
    pub empty: TestBlob,
    pub hello: TestBlob,
    pub config: TestBlob,
    pub layer_a: TestBlob,
    pub layer_b: TestBlob,
    pub unknown_fields: TestBlob,
    /// Dedicated blob for L1-17: pushed WITHOUT Content-Type to test default response.
    pub no_content_type: TestBlob,
    pub large_content: Vec<u8>,
    pub large_sha256: KappaLabel,
    pub simple_manifest: Vec<u8>,
    pub simple_manifest_sha256: KappaLabel,
    pub multi_manifest: Vec<u8>,
    pub multi_manifest_sha256: KappaLabel,
    /// Content sent as partial upload data in L1-33 before cancellation. L1-35 computes
    /// its kappa-label and verifies the partial data was NOT stored.
    pub cancel_partial_content: &'static [u8],
    pub cancel_partial_sha256: KappaLabel,
    /// Dedicated blob for L1-71 sha1 chunked upload complete test.
    pub sha1_upload: TestBlob,
    /// Dedicated blob for L1-72 sha1 multi-label test.
    pub sha1_multi: TestBlob,
}

impl TestData {
    pub fn generate() -> Self {
        let empty = make_blob("empty", b"");
        let hello = make_blob("hello", b"hello");

        let config_bytes: &'static [u8] =
            Box::leak(br#"{"type":"config"}"#.to_vec().into_boxed_slice());
        let config = make_blob("config", config_bytes);

        let layer_a_bytes: &'static [u8] = Box::leak(
            b"layer-a-seed-data-0123456789abcdef"
                .repeat(120)
                .into_boxed_slice(),
        );
        let layer_a = make_blob("layer_a", layer_a_bytes);

        let layer_b_bytes: &'static [u8] = Box::leak(
            b"layer-b-different-seed-9876543210!!"
                .repeat(120)
                .into_boxed_slice(),
        );
        let layer_b = make_blob("layer_b", layer_b_bytes);

        let unknown_fields_bytes: &'static [u8] =
            Box::leak(br#"{"known":"v","_xyzzy":"preserve"}"#.to_vec().into_boxed_slice());
        let unknown_fields = make_blob("unknown_fields", unknown_fields_bytes);

        let no_ct_bytes: &'static [u8] = Box::leak(
            b"{\"no_content_type_test\":true}"
                .to_vec()
                .into_boxed_slice(),
        );
        let no_content_type = make_blob("no_content_type", no_ct_bytes);

        let cancel_partial_content: &'static [u8] = Box::leak(
            b"partial-upload-data-for-cancel-test"
                .to_vec()
                .into_boxed_slice(),
        );
        let cancel_partial_sha256 = KappaLabel::sha256(cancel_partial_content);

        let sha1_upload_bytes: &'static [u8] = Box::leak(
            b"sha1-upload-complete-test-content"
                .to_vec()
                .into_boxed_slice(),
        );
        let sha1_upload = make_blob("sha1_upload", sha1_upload_bytes);

        let sha1_multi_bytes: &'static [u8] =
            Box::leak(b"sha1-multi-label-test-content".to_vec().into_boxed_slice());
        let sha1_multi = make_blob("sha1_multi", sha1_multi_bytes);

        let large_content = b"large-blob-chunk-fill-pattern-1234!".repeat(120_000); // ~4 MiB
        let large_sha256 = KappaLabel::sha256(&large_content);

        let simple_manifest = format!(
            r#"{{"config":"{}","layers":["{}"]}}"#,
            config.sha256.as_str(),
            layer_a.sha256.as_str(),
        )
        .into_bytes();
        let simple_manifest_sha256 = KappaLabel::sha256(&simple_manifest);

        let multi_manifest = format!(
            r#"{{"config":"{}","layers":["{}","{}"]}}"#,
            config.sha256.as_str(),
            layer_a.sha256.as_str(),
            layer_b.sha256.as_str(),
        )
        .into_bytes();
        let multi_manifest_sha256 = KappaLabel::sha256(&multi_manifest);

        TestData {
            empty,
            hello,
            config,
            layer_a,
            layer_b,
            unknown_fields,
            no_content_type,
            large_content,
            large_sha256,
            simple_manifest,
            simple_manifest_sha256,
            multi_manifest,
            multi_manifest_sha256,
            cancel_partial_content,
            cancel_partial_sha256,
            sha1_upload,
            sha1_multi,
        }
    }
}

fn make_blob(name: &'static str, content: &'static [u8]) -> TestBlob {
    TestBlob {
        name,
        content,
        sha1: KappaLabel::sha1(content).expect("sha1 collision in test data"),
        sha256: KappaLabel::sha256(content),
        blake3: KappaLabel::blake3(content),
    }
}
