# κ-Distribution conformance test binary.
#
# Build (single arch):
#   cargo build --release --target x86_64-unknown-linux-musl
#   docker build -t kappa-conformance .
#
# Build (multi-arch):
#   cargo build --release --target x86_64-unknown-linux-musl
#   cargo build --release --target aarch64-unknown-linux-musl
#   docker buildx build --platform linux/amd64,linux/arm64 -t kappa-conformance .
#
# Run:
#   docker run --rm \
#     -e KAPPA_REGISTRY_URL=https://registry.example.com \
#     -e KAPPA_AUTH_TOKEN=eyJ... \
#     -v ./reports:/kappa-conformance-report \
#     kappa-conformance

ARG RUST_TARGET=x86_64-unknown-linux-musl
FROM scratch
COPY target/${RUST_TARGET}/release/kappa-conformance /kappa-conformance
ENTRYPOINT ["/kappa-conformance"]
