FROM node:22-bookworm
RUN apt-get update \
 && apt-get install -y --no-install-recommends python3 git ripgrep jq build-essential pkg-config libssl-dev curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal -c clippy -c rustfmt -c llvm-tools-preview
RUN cargo install --locked cargo-deny cargo-nextest cargo-llvm-cov cargo-machete
RUN npm install -g opencode-ai pnpm @playwright/mcp playwright \
 && playwright install --with-deps chromium
WORKDIR /work
