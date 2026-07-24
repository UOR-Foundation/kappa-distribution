//! RegistryClient - HTTP client with auth injection, redirect policy, and trace capture.
//!
//! Redirect policy (impl spec §3.3, protocol spec §6.1): the client follows redirects within
//! the same host boundary. When a redirect crosses a host boundary (`{scheme}://{authority}`
//! differs from the original request's), the client stops the redirect chain, re-issues the
//! request to the redirect target WITHOUT the Authorization header, and continues following
//! from there. This guarantees that Authorization is never forwarded across host boundaries
//! regardless of how reqwest handles per-request headers internally.

use crate::config::{Auth, Config};
use reqwest::blocking::{Client, RequestBuilder};
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

/// A fully consumed HTTP response with trace metadata. The body is read eagerly
/// so status, headers, and body are all available without consuming the response
/// object. Every FAIL verdict includes the body for diagnostics (B35).
#[derive(Clone, Debug)]
pub struct TracedResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub trace: HttpTrace,
}

impl TracedResponse {
    /// The body as a UTF-8 string, lossy.
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// Get a header value by case-insensitive name.
    pub fn header(&self, name: &str) -> Option<&str> {
        let lower = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == lower)
            .map(|(_, v)| v.as_str())
    }

    /// Format a failure message with status and body context.
    pub fn fail_msg(&self, expected_status: u16) -> String {
        let body_preview = self.body_text();
        let preview = if body_preview.len() > 4096 {
            &body_preview[..4096]
        } else {
            &body_preview
        };
        format!(
            "expected status {expected_status}, got {}: {preview}",
            self.status
        )
    }
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

    /// Build a complete HeaderMap for requests without a body (GET, HEAD, DELETE).
    fn headers_no_body(&self) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(
            "accept",
            "application/octet-stream, application/json"
                .parse()
                .unwrap(),
        );
        if let Some(auth) = &self.auth_header {
            h.insert("authorization", auth.clone());
        }
        h
    }

    /// Build a complete HeaderMap for requests with a body (PUT, POST, PATCH).
    /// Content-Type defaults to application/octet-stream. Callers that need a
    /// different Content-Type pass it as a parameter.
    fn headers_with_body(&self, content_type: &str) -> reqwest::header::HeaderMap {
        let mut h = self.headers_no_body();
        h.insert("content-type", content_type.parse().unwrap());
        h
    }

    pub fn get(&self, path: &str) -> RequestBuilder {
        self.inner
            .get(format!("{}{path}", self.base_url))
            .headers(self.headers_no_body())
    }

    pub fn head(&self, path: &str) -> RequestBuilder {
        self.inner
            .head(format!("{}{path}", self.base_url))
            .headers(self.headers_no_body())
    }

    pub fn put(&self, path: &str) -> RequestBuilder {
        self.inner
            .put(format!("{}{path}", self.base_url))
            .headers(self.headers_with_body("application/octet-stream"))
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        self.inner
            .post(format!("{}{path}", self.base_url))
            .headers(self.headers_with_body("application/octet-stream"))
    }

    pub fn delete(&self, path: &str) -> RequestBuilder {
        self.inner
            .delete(format!("{}{path}", self.base_url))
            .headers(self.headers_no_body())
    }

    pub fn patch_url(&self, url: &str) -> RequestBuilder {
        self.inner
            .patch(resolve_location(&self.base_url, url))
            .headers(self.headers_with_body("application/octet-stream"))
    }

    pub fn put_url(&self, url: &str) -> RequestBuilder {
        self.inner
            .put(resolve_location(&self.base_url, url))
            .headers(self.headers_with_body("application/octet-stream"))
    }

    pub fn get_url(&self, url: &str) -> RequestBuilder {
        self.inner
            .get(resolve_location(&self.base_url, url))
            .headers(self.headers_no_body())
    }

    pub fn delete_url(&self, url: &str) -> RequestBuilder {
        self.inner
            .delete(resolve_location(&self.base_url, url))
            .headers(self.headers_no_body())
    }

    /// PUT with a specific Content-Type. Used when the test needs a non-default
    /// Content-Type (e.g., L1-16 uses application/json). The HeaderMap is built
    /// with the specified Content-Type, so callers do not append a second value.
    pub fn put_with_content_type(&self, path: &str, content_type: &str) -> RequestBuilder {
        self.inner
            .put(format!("{}{path}", self.base_url))
            .headers(self.headers_with_body(content_type))
    }

    /// POST with a specific Content-Type. Used when the request sends JSON
    /// (e.g., gc/pin, gc/unpin, gc/sweep, filter registration).
    pub fn post_with_content_type(&self, path: &str, content_type: &str) -> RequestBuilder {
        self.inner
            .post(format!("{}{path}", self.base_url))
            .headers(self.headers_with_body(content_type))
    }

    /// PUT without Content-Type header. Used by L1-17 to test the registry's
    /// default Content-Type response when no Content-Type was set on PUT.
    pub fn put_no_content_type(&self, path: &str) -> RequestBuilder {
        self.inner
            .put(format!("{}{path}", self.base_url))
            .headers(self.headers_no_body())
    }

    /// Direct tag binding: PUT /v2/{path}/tags/{name}?kappa={κ-label} with no body.
    /// Pointer-only - no content re-upload required.
    pub fn put_tag(&self, path: &str, name: &str, kappa: &str) -> RequestBuilder {
        self.inner
            .put(format!("{}{path}/tags/{name}?kappa={kappa}", self.base_url))
            .headers(self.headers_no_body())
    }

    /// Direct tag resolution: GET /v2/{path}/tags/{name} - returns κ-label only, no content body.
    pub fn get_tag(&self, path: &str, name: &str) -> RequestBuilder {
        self.inner
            .get(format!("{}{path}/tags/{name}", self.base_url))
            .headers(self.headers_no_body())
    }

    /// Query blobs by metadata: GET /v2/{path}/blobs/_meta?key={key}&value={value}
    pub fn get_meta_list(&self, path: &str, key: &str, value: &str) -> RequestBuilder {
        self.inner
            .get(format!(
                "{}{path}/blobs/_meta?key={key}&value={value}",
                self.base_url
            ))
            .headers(self.headers_no_body())
    }

    /// Send a request, fully consume the response, and return a TracedResponse
    /// with status, headers, body, and trace metadata all available as fields.
    ///
    /// If the response is a cross-host redirect (3xx with Location pointing to a different
    /// `{scheme}://{authority}`), the client re-issues the request to the redirect target
    /// WITHOUT the Authorization header (spec §6.1).
    pub fn send_traced(&self, rb: RequestBuilder) -> Result<TracedResponse, reqwest::Error> {
        let req = rb.build()?;
        let method_str = req.method().to_string();
        let method = req.method().clone();
        let original_url = req.url().clone();
        let req_headers = redact_headers(req.headers());
        let req_body_size = req.body().and_then(|b| b.as_bytes()).map_or(0, |b| b.len());

        let start = Instant::now();
        let resp = self.inner.execute(req)?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let (final_resp, final_duration) = if resp.status().is_redirection() {
            if let Some(location) = resp.headers().get(LOCATION) {
                let loc_str = location.to_str().unwrap_or("");
                let target = resolve_location(original_url.as_ref(), loc_str);
                let orig_origin = origin_of(original_url.as_str());
                let target_origin = origin_of(&target);

                if orig_origin != target_origin {
                    let redirect_rb = self
                        .inner
                        .request(method, &target)
                        .header("Accept", "application/octet-stream, application/json");
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
        let body = final_resp.bytes().unwrap_or_default().to_vec();

        let trace = HttpTrace {
            method: method_str,
            url: original_url.to_string(),
            req_headers,
            req_body_size,
            status,
            resp_headers: resp_headers.clone(),
            resp_body_size: body.len(),
            duration_ms: final_duration,
        };

        Ok(TracedResponse {
            status,
            headers: resp_headers,
            body,
            trace,
        })
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
