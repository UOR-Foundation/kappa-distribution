# kappa-distribution

κ-Distribution protocol standard + conformance binary.

## Repository structure

- `crates/kappa-distribution/` — the standard library (no_std, zero deps). Byte contracts
  that any conforming registry implements: edge canonical form, composition canonical forms,
  witness header, error taxonomy, conformance levels.
- `src/` — the `kappa-conformance` binary. 149 tests across 5 conformance levels. The sole
  authority for registry conformance. Computes hashes independently (sha2, blake3) — no
  dependency on uor-addr or any registry implementation.
- `specs/draft/` — protocol and conformance specifications. Draft until validated across
  file, HTTP, SQL, and Ceph implementations.

## Conventions

- Conventional commits. Objective, diff-derived, verbose technical bodies. No AI attribution.
  No PII. No `Co-Authored-By`.
- Read files in full before modification.
- Do not commit secrets or generated credentials.
- Every HTTP call in the conformance binary uses `send_traced` — no `.send()` directly.
- Every assertion verifies values, not just presence.
- No "pass by construction" — if an invariant can be tested, test it.
- No orphaned `TestContext` — check conditions before creating the context, not after.
- Test vector constants are verified against system tools (`sha256sum`, `b3sum`).

## Toolchain

Hermetic via Nix. `flake.nix` provides Rust stable + musl targets + b3sum + taplo +
shellcheck + markdownlint-cli2 via the Konductor flake. Do not install via pip/npm/cargo/brew.

## Quality gates

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

All three must pass with zero warnings before any commit.

## Static binary

```
cargo build --release --target x86_64-unknown-linux-musl
```

Container: `docker build -t kappa-conformance .`
