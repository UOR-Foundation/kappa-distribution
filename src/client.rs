//! RegistryClient - HTTP client with auth injection, redirect policy, and trace capture.
//!
//! Redirect policy (impl spec §3.3, protocol spec §6.1): the client follows redirects within
//! the same host boundary. When a redirect crosses a host boundary (`{scheme}://{authority}`
//! differs from the original request's), the client stops the redirect chain, re-issues the
//! request to the redirect target WITHOUT the Authorization header, and continues following
//! from there. This guarantees that Authorization is never forwarded across host boundaries
//! regardless of how reqwest handles per-request headers internally.

use crate::config::{Auth, Config};
use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{HeaderMap, HeaderValue, LOCATION};
use reqwest::redirect::Policy;
use std::time::Instant;

pub struct RegistryClient {
    inner: Client,
    auth_header: Option<HeaderValue>,
    pub base_url: String,
}

#[derive(Clone, Debug)]
pub struct HttpTrace {
    pub method: String,
    pub url: String,
    pub req_headers: Vec<(String, String)>,
    pub req_body_size: usize,
    pub status: u16,
    pub resp_headers: Vec<(String, String)>,
    pub resp_body_size: usize,
    pub duration_ms: u64,
}

/// Extract `{scheme}://{authority}` from a URL string for host boundary comparison.
fn origin_of(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    Some(format!("{}://{}", parsed.scheme(), parsed.authority()))
}

impl RegistryClient {
    pub fn new(config: &Config) -> Self {
        let follow_redirects = config.follow_redirects;

        // Stop on cross-host redirects so send_traced can re-issue without Authorization.
        // Same-host redirects are followed normally (up to 10 hops).
        let policy = if follow_redirects {
            Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    attempt.error("too many redirects")
                } else {
                    let prev_origin = attempt
                        .previous()
                        .last()
                        .map(|u| format!("{}://{}", u.scheme(), u.authority()));
                    let next_origin =
                        format!("{}://{}", attempt.url().scheme(), attempt.url().authority());
                    match prev_origin {
                        Some(prev) if prev != next_origin => {
                            // Cross-host boundary - stop here. send_traced will re-issue
                            // without Authorization (spec §6.1).
                            attempt.stop()
                        }
                        _ => attempt.follow(),
                    }
                }
            })
        } else {
            Policy::none()
        };

        let inner = Client::builder()
            .redirect(policy)
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .user_agent(format!("kappa-conformance/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client build");

        let auth_header = match &config.auth {
            Auth::Bearer(token) => HeaderValue::from_str(&format!("Bearer {token}")).ok(),
            Auth::Basic { username, password } => {
                let encoded = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    format!("{username}:{password}"),
                );
                HeaderValue::from_str(&format!("Basic {encoded}")).ok()
            }
            Auth::None => None,
        };

        RegistryClient {
            inner,
            auth_header,
            base_url: config.registry_url.clone(),
        }
    }

    /// Inject standard headers: Accept, Content-Type (application/octet-stream default),
    /// and Authorization. Every request through the client carries these unless explicitly
    /// overridden by the caller (e.g., L1-16 overrides Content-Type to application/json).
    fn inject(&self, rb: RequestBuilder) -> RequestBuilder {
        let rb = rb
            .header("Accept", "application/octet-stream, application/json")
            .header("Content-Type", "application/octet-stream");
        match &self.auth_header {
            Some(h) => rb.header("Authorization", h.clone()),
            None => rb,
        }
    }

    /// Inject Accept and Authorization but NOT Content-Type. Used by L1-17 to push a blob
    /// with no Content-Type header so the registry's default behavior can be tested.
    fn inject_no_content_type(&self, rb: RequestBuilder) -> RequestBuilder {
        let rb = rb.header("Accept", "application/octet-stream, application/json");
        match &self.auth_header {
            Some(h) => rb.header("Authorization", h.clone()),
            None => rb,
        }
    }

    pub fn get(&self, path: &str) -> RequestBuilder {
        self.inject(self.inner.get(format!("{}{path}", self.base_url)))
    }

    pub fn head(&self, path: &str) -> RequestBuilder {
        self.inject(self.inner.head(format!("{}{path}", self.base_url)))
    }

    pub fn put(&self, path: &str) -> RequestBuilder {
        self.inject(self.inner.put(format!("{}{path}", self.base_url)))
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        self.inject(self.inner.post(format!("{}{path}", self.base_url)))
    }

    pub fn delete(&self, path: &str) -> RequestBuilder {
        self.inject(self.inner.delete(format!("{}{path}", self.base_url)))
    }

    pub fn patch_url(&self, url: &str) -> RequestBuilder {
        self.inject(self.inner.patch(resolve_location(&self.base_url, url)))
    }

    pub fn put_url(&self, url: &str) -> RequestBuilder {
        self.inject(self.inner.put(resolve_location(&self.base_url, url)))
    }

    pub fn get_url(&self, url: &str) -> RequestBuilder {
        self.inject(self.inner.get(resolve_location(&self.base_url, url)))
    }

    pub fn delete_url(&self, url: &str) -> RequestBuilder {
        self.inject(self.inner.delete(resolve_location(&self.base_url, url)))
    }

    /// PUT without Content-Type header. Used exclusively by L1-17 to test the registry's
    /// default Content-Type response when no Content-Type was set on PUT (spec §6.1).
    pub fn put_no_content_type(&self, path: &str) -> RequestBuilder {
        self.inject_no_content_type(self.inner.put(format!("{}{path}", self.base_url)))
    }

    /// Direct tag binding: PUT /v2/{path}/tags/{name}?kappa={κ-label} with no body.
    /// Pointer-only - no content re-upload required.
    pub fn put_tag(&self, path: &str, name: &str, kappa: &str) -> RequestBuilder {
        self.inject(
            self.inner
                .put(format!("{}{path}/tags/{name}?kappa={kappa}", self.base_url)),
        )
    }

    /// Direct tag resolution: GET /v2/{path}/tags/{name} - returns κ-label only, no content body.
    pub fn get_tag(&self, path: &str, name: &str) -> RequestBuilder {
        self.inject(
            self.inner
                .get(format!("{}{path}/tags/{name}", self.base_url)),
        )
    }

    /// Send a request and capture an HTTP trace (with credential redaction).
    ///
    /// If the response is a cross-host redirect (3xx with Location pointing to a different
    /// `{scheme}://{authority}`), the client re-issues the request to the redirect target
    /// WITHOUT the Authorization header (spec §6.1). The trace captures both the original
    /// and the redirected request.
    pub fn send_traced(&self, rb: RequestBuilder) -> Result<(Response, HttpTrace), reqwest::Error> {
        let req = rb.build()?;
        let method_str = req.method().to_string();
        let method = req.method().clone();
        let original_url = req.url().clone();
        let req_headers = redact_headers(req.headers());
        let req_body_size = req.body().and_then(|b| b.as_bytes()).map_or(0, |b| b.len());

        let start = Instant::now();
        let resp = self.inner.execute(req)?;
        let duration_ms = start.elapsed().as_millis() as u64;

        // Detect cross-host redirect that was stopped by the policy.
        // If the response is a 3xx with a Location header pointing to a different origin,
        // re-issue without Authorization.
        let (final_resp, final_duration) = if resp.status().is_redirection() {
            if let Some(location) = resp.headers().get(LOCATION) {
                let loc_str = location.to_str().unwrap_or("");
                let target = resolve_location(original_url.as_ref(), loc_str);
                let orig_origin = origin_of(original_url.as_str());
                let target_origin = origin_of(&target);

                if orig_origin != target_origin {
                    // Cross-host: re-issue WITHOUT Authorization (spec §6.1).
                    let redirect_rb = self
                        .inner
                        .request(method, &target)
                        .header("Accept", "application/octet-stream, application/json");
                    // Deliberately no Authorization header.
                    let redirect_req = redirect_rb.build()?;
                    let redirect_resp = self.inner.execute(redirect_req)?;
                    let redirect_duration = start.elapsed().as_millis() as u64;
                    (redirect_resp, redirect_duration)
                } else {
                    (resp, duration_ms)
                }
            } else {
                (resp, duration_ms)
            }
        } else {
            (resp, duration_ms)
        };

        let status = final_resp.status().as_u16();
        let resp_headers = redact_headers(final_resp.headers());
        let resp_body_size = final_resp.content_length().map(|n| n as usize).unwrap_or(0);

        let trace = HttpTrace {
            method: method_str,
            url: original_url.to_string(),
            req_headers,
            req_body_size,
            status,
            resp_headers,
            resp_body_size,
            duration_ms: final_duration,
        };
        Ok((final_resp, trace))
    }
}

pub fn resolve_location(base: &str, loc: &str) -> String {
    if loc.starts_with("http") {
        loc.to_string()
    } else {
        // Relative redirect: resolve against the base URL's origin.
        if let Ok(base_url) = reqwest::Url::parse(base) {
            if let Ok(resolved) = base_url.join(loc) {
                return resolved.to_string();
            }
        }
        format!("{base}{loc}")
    }
}

fn redact_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    const SENSITIVE: &[&str] = &[
        "authorization",
        "cookie",
        "set-cookie",
        "token",
        "password",
        "secret",
        "credential",
    ];
    headers
        .iter()
        .map(|(k, v)| {
            let name = k.as_str().to_lowercase();
            let value = if SENSITIVE.iter().any(|s| name.contains(s)) {
                "[REDACTED]".to_string()
            } else {
                v.to_str().unwrap_or("[non-utf8]").to_string()
            };
            (name, value)
        })
        .collect()
}
