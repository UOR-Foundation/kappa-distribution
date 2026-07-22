//! kappa-conformance - the sole authority for κ-Distribution protocol conformance.
//!
//! Exit codes:
//!   0 - all MUST tests passed (warnings are acceptable)
//!   1 - one or more MUST tests failed
//!   2 - configuration error
//!   3 - registry unreachable

mod assertions;
mod client;
mod config;
mod edge_canon;
mod kappa;
mod report;
mod runner;
mod testdata;
mod tests;

use std::fs;
use std::process;

fn main() {
    let config = config::Config::from_env();
    let client = client::RegistryClient::new(&config);
    let data = testdata::TestData::generate();
    let mut runner = runner::TestRunner::new();

    // Connectivity check.
    match client.get("/v2/").send() {
        Err(e) => {
            eprintln!("registry unreachable: {e}");
            process::exit(3);
        }
        Ok(r) if r.status().as_u16() == 404 => {
            eprintln!("endpoint does not implement κ-Distribution (404 on /v2/)");
            process::exit(3);
        }
        Ok(_) => {}
    }

    // Run requested levels.
    if config.levels.contains(&1) {
        tests::level1::run(&mut runner, &client, &config, &data);
    }
    if config.levels.contains(&2) {
        tests::level2::run(&mut runner, &client, &config, &data);
    }
    if config.levels.contains(&3) {
        tests::level3::run(&mut runner, &client, &config, &data);
    }
    if config.levels.contains(&4) {
        tests::level4::run(&mut runner, &client, &config, &data);
    }
    if config.levels.contains(&5) {
        tests::level5::run(&mut runner, &client, &config, &data);
    }

    // Reports.
    fs::create_dir_all(&config.report_dir).ok();
    report::write_junit(
        &runner.results,
        &config.report_dir.join("kappa-conformance.xml"),
    );
    report::write_html(
        &runner.results,
        &config.registry_url,
        &config.report_dir.join("kappa-conformance.html"),
    );

    let (p, f, w, s) = runner.summary();
    eprintln!("\n{p} pass, {f} fail, {w} warn, {s} skip");

    process::exit(if runner.has_failures() { 1 } else { 0 });
}
