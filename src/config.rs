//! Environment-variable configuration (spec §3).

use std::path::PathBuf;

pub struct Config {
    pub registry_url: String,
    pub namespace: String,
    pub levels: Vec<u8>,
    pub auth: Auth,
    pub timeout_secs: u64,
    pub upload_chunk_size: usize,
    pub teardown_order: TeardownOrder,
    pub teardown_enabled: bool,
    pub follow_redirects: bool,
    pub federation_peers: Vec<String>,
    pub report_dir: PathBuf,
}

pub enum Auth {
    None,
    Bearer(String),
    Basic { username: String, password: String },
}

pub enum TeardownOrder {
    TagsFirst,
    BlobsFirst,
}

impl Config {
    pub fn from_env() -> Self {
        let registry_url = required_env("KAPPA_REGISTRY_URL")
            .trim_end_matches('/')
            .to_string();

        let namespace = env_or("KAPPA_NAMESPACE", "_conformance/test/v1:conformance");

        let levels: Vec<u8> = env_or("KAPPA_TEST_LEVELS", "1,2,3,4,5")
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .filter(|&n| (1..=5).contains(&n))
            .collect();

        let auth = if let Ok(token) = std::env::var("KAPPA_AUTH_TOKEN") {
            Auth::Bearer(token)
        } else if let (Ok(u), Ok(p)) = (
            std::env::var("KAPPA_AUTH_USERNAME"),
            std::env::var("KAPPA_AUTH_PASSWORD"),
        ) {
            Auth::Basic {
                username: u,
                password: p,
            }
        } else {
            Auth::None
        };

        let timeout_secs: u64 = env_or("KAPPA_TIMEOUT", "30").parse().unwrap_or(30);
        let upload_chunk_size: usize = env_or("KAPPA_UPLOAD_CHUNK_SIZE", "1048576")
            .parse()
            .unwrap_or(1_048_576);

        let teardown_order = match env_or("KAPPA_TEARDOWN_ORDER", "tags-first").as_str() {
            "blobs-first" => TeardownOrder::BlobsFirst,
            _ => TeardownOrder::TagsFirst,
        };

        let teardown_enabled = env_or("KAPPA_TEARDOWN_ENABLED", "true") == "true";
        let follow_redirects = env_or("KAPPA_FOLLOW_REDIRECTS", "true") == "true";

        let federation_peers: Vec<String> = std::env::var("KAPPA_FEDERATION_PEERS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let report_dir = PathBuf::from(env_or("KAPPA_REPORT_DIR", "./kappa-conformance-report/"));

        Config {
            registry_url,
            namespace,
            levels,
            auth,
            timeout_secs,
            upload_chunk_size,
            teardown_order,
            teardown_enabled,
            follow_redirects,
            federation_peers,
            report_dir,
        }
    }
}

fn required_env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| config_exit(&format!("{key} is required")))
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[cold]
#[inline(never)]
fn config_exit(msg: &str) -> ! {
    eprintln!("configuration error: {msg}");
    std::process::exit(2);
}
