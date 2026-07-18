FROM rust:1.96-bookworm

ARG CARGO_XWIN_VERSION=0.23.0
ARG TAURI_CLI_VERSION=2.11.4

RUN set -eux; \
    apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        clang \
        libasound2-dev \
        lld \
        llvm \
        nsis \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    for tool in llvm-rc lld-link; do \
         candidate="$(find /usr/bin -maxdepth 1 -name "$tool-[0-9]*" -print | sort -V | tail -n 1)"; \
         test -n "$candidate"; \
         ln -s "$candidate" "/usr/local/bin/$tool"; \
       done; \
    ln -s "$(command -v clang)" /usr/local/bin/clang-cl

RUN rustup target add x86_64-pc-windows-msvc \
    && cargo install --locked --version "$CARGO_XWIN_VERSION" cargo-xwin \
    && cargo install --locked --version "$TAURI_CLI_VERSION" tauri-cli

WORKDIR /workspace
