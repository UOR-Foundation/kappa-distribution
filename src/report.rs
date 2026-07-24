//! Report generation - JUnit XML + HTML (impl spec §4).

use crate::runner::{TestResult, TestVerdict};
use std::fs;
use std::path::Path;

/// Write JUnit XML report.
pub fn write_junit(results: &[TestResult], path: &Path) {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites>\n");
    for level in 1..=5u8 {
        let level_results: Vec<_> = results.iter().filter(|r| r.level == level).collect();
        if level_results.is_empty() {
            continue;
        }
        let failures = level_results
            .iter()
            .filter(|r| r.verdict == TestVerdict::Fail)
            .count();
        xml.push_str(&format!(
            "  <testsuite name=\"Level {level}\" tests=\"{}\" failures=\"{failures}\">\n",
            level_results.len()
        ));
        for r in &level_results {
            let escaped_desc = xml_escape(r.description);
            match r.verdict {
                TestVerdict::Pass => {
                    xml.push_str(&format!(
                        "    <testcase name=\"{} {}\" time=\"{:.3}\"/>\n",
                        r.id,
                        escaped_desc,
                        r.duration_ms as f64 / 1000.0
                    ));
                }
                TestVerdict::Fail => {
                    let msg = r.message.as_deref().unwrap_or("");
                    xml.push_str(&format!(
                        "    <testcase name=\"{} {}\" time=\"{:.3}\">\n      <failure message=\"{}\">{}</failure>\n    </testcase>\n",
                        r.id,
                        escaped_desc,
                        r.duration_ms as f64 / 1000.0,
                        xml_escape(msg),
                        xml_escape(msg),
                    ));
                }
                TestVerdict::Warn => {
                    let msg = r.message.as_deref().unwrap_or("");
                    xml.push_str(&format!(
                        "    <testcase name=\"{} {}\" time=\"{:.3}\">\n      <system-out>WARNING: {}</system-out>\n    </testcase>\n",
                        r.id,
                        escaped_desc,
                        r.duration_ms as f64 / 1000.0,
                        xml_escape(msg),
                    ));
                }
                TestVerdict::Skip => {
                    let msg = r.message.as_deref().unwrap_or("");
                    xml.push_str(&format!(
                        "    <testcase name=\"{} {}\">\n      <skipped message=\"{}\"/>\n    </testcase>\n",
                        r.id,
                        escaped_desc,
                        xml_escape(msg),
                    ));
                }
            }
        }
        xml.push_str("  </testsuite>\n");
    }
    xml.push_str("</testsuites>\n");
    if let Err(e) = fs::write(path, &xml) {
        eprintln!("warning: failed to write JUnit report: {e}");
    }
}

/// Write HTML report.
pub fn write_html(results: &[TestResult], registry_url: &str, path: &Path) {
    let (p, f, w, s) = results
        .iter()
        .fold((0, 0, 0, 0), |(p, f, w, s), r| match r.verdict {
            TestVerdict::Pass => (p + 1, f, w, s),
            TestVerdict::Fail => (p, f + 1, w, s),
            TestVerdict::Warn => (p, f, w + 1, s),
            TestVerdict::Skip => (p, f, w, s + 1),
        });
    let mut html = format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>κ-Distribution Conformance</title>
<style>
body {{ font-family: sans-serif; margin: 2em; }}
table {{ border-collapse: collapse; width: 100%; margin-bottom: 2em; }}
th, td {{ border: 1px solid #ccc; padding: 6px 10px; text-align: left; }}
.pass {{ border-left: 4px solid #2a2; }}
.fail {{ border-left: 4px solid #c22; }}
.warn {{ border-left: 4px solid #ca0; }}
.skip {{ border-left: 4px solid #999; }}
.summary {{ display: flex; gap: 1em; margin: 1em 0; }}
.box {{ padding: 8px 16px; border-radius: 4px; color: white; font-weight: bold; }}
.box-pass {{ background: #2a2; }}
.box-fail {{ background: #c22; }}
.box-warn {{ background: #ca0; color: #333; }}
.box-skip {{ background: #999; }}
</style></head><body>
<h1>κ-Distribution Conformance Report</h1>
<p>Registry: <code>{registry_url}</code></p>
<p>Version: kappa-conformance {}</p>
<p>Timestamp: {}</p>
<div class="summary">
  <span class="box box-pass">{p} pass</span>
  <span class="box box-fail">{f} fail</span>
  <span class="box box-warn">{w} warn</span>
  <span class="box box-skip">{s} skip</span>
</div>
"#,
        env!("CARGO_PKG_VERSION"),
        chrono::Utc::now().to_rfc3339(),
    );

    for level in 1..=5u8 {
        let level_results: Vec<_> = results.iter().filter(|r| r.level == level).collect();
        if level_results.is_empty() {
            continue;
        }
        html.push_str(&format!("<h2>Level {level}</h2>\n<table>\n<tr><th>ID</th><th>Req</th><th>Result</th><th>Description</th><th>ms</th><th>Detail</th></tr>\n"));
        for r in &level_results {
            let class = match r.verdict {
                TestVerdict::Pass => "pass",
                TestVerdict::Fail => "fail",
                TestVerdict::Warn => "warn",
                TestVerdict::Skip => "skip",
            };
            let req = match r.requirement {
                crate::runner::Requirement::Must => "MUST",
                crate::runner::Requirement::Should => "SHOULD",
                crate::runner::Requirement::May => "MAY",
            };
            let verdict = format!("{:?}", r.verdict).to_uppercase();
            let detail = r.message.as_deref().unwrap_or("");
            let mut trace_info = String::new();
            for trace in &r.traces {
                trace_info.push_str(&format!(
                    "[{} {} -> {} ({}ms, req {}B, resp {}B) req_hdrs: {:?} resp_hdrs: {:?}] ",
                    trace.method,
                    trace.url,
                    trace.status,
                    trace.duration_ms,
                    trace.req_body_size,
                    trace.resp_body_size,
                    trace.req_headers,
                    trace.resp_headers,
                ));
            }
            let full_detail = if trace_info.is_empty() {
                html_escape(detail)
            } else {
                format!("{} | {}", html_escape(detail), html_escape(&trace_info))
            };
            html.push_str(&format!(
                "<tr class=\"{class}\"><td>{}</td><td>{req}</td><td>{verdict}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                r.id,
                html_escape(r.description),
                r.duration_ms,
                full_detail,
            ));
        }
        html.push_str("</table>\n");
    }
    html.push_str("</body></html>\n");
    if let Err(e) = fs::write(path, &html) {
        eprintln!("warning: failed to write HTML report: {e}");
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn html_escape(s: &str) -> String {
    xml_escape(s)
}
