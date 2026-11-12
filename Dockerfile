# syntax=docker/dockerfile:1
#
# Two stages: the Rust toolchain builds, a slim image runs. The web export is a build-time
# dependency only; nothing JavaScript runs in production (ADR-013).

FROM rust:1.99.1-bookworm AS builder
WORKDIR /src

# Manifests first so a source-only change reuses the dependency compilation.
COPY rust-toolchain.toml rustfmt.toml clippy.toml Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations

# `--locked` makes a missing or stale Cargo.lock a build failure instead of a silent resolve.
RUN cargo build --release --locked --bin boma

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install --yes --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home --shell /usr/sbin/nologin boma

COPY --from=builder /src/target/release/boma /usr/local/bin/boma

USER 10001:10001
EXPOSE 3000

HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=3 \
  CMD ["/usr/local/bin/boma", "probe", "--addr", "127.0.0.1:3000", "--path", "/ready"]

ENTRYPOINT ["/usr/local/bin/boma"]
CMD ["serve"]
