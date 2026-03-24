# syntax=docker/dockerfile:1
FROM alpine:edge AS builder

RUN apk add --no-cache cargo musl-dev openssl-dev pkgconfig

ARG TARGETARCH

# Pre-build dependencies (cached layer - only invalidated when Cargo.toml changes)
COPY Cargo.toml /src/rustvideoplatform-indexer/
RUN mkdir -p /src/rustvideoplatform-indexer/src && echo 'fn main() {}' > /src/rustvideoplatform-indexer/src/main.rs
RUN --mount=type=cache,id=cargo-registry-${TARGETARCH},target=/root/.cargo/registry \
    --mount=type=cache,id=rustvideoplatform-indexer-target-${TARGETARCH},target=/src/rustvideoplatform-indexer/target \
    case "$TARGETARCH" in \
        amd64)   export RUSTFLAGS="-C target-cpu=x86-64-v2" ;; \
        ppc64le) export RUSTFLAGS="-C target-cpu=pwr8" ;; \
    esac && \
    cd /src/rustvideoplatform-indexer && cargo build --release 2>/dev/null ; true

# Build actual project
COPY ./ /src/rustvideoplatform-indexer
# Touch source files to ensure cargo detects changes over the dummy pre-build
RUN --mount=type=cache,id=cargo-registry-${TARGETARCH},target=/root/.cargo/registry \
    --mount=type=cache,id=rustvideoplatform-indexer-target-${TARGETARCH},target=/src/rustvideoplatform-indexer/target \
    find /src/rustvideoplatform-indexer/src -name '*.rs' -exec touch {} + && \
    case "$TARGETARCH" in \
        amd64)   export RUSTFLAGS="-C target-cpu=x86-64-v2" ;; \
        ppc64le) export RUSTFLAGS="-C target-cpu=pwr8" ;; \
    esac && \
    cd /src/rustvideoplatform-indexer && cargo build --release && \
    cp target/release/rustvideoplatform-indexer /rustvideoplatform-indexer


FROM alpine:edge
RUN apk add --no-cache ffmpeg
COPY --from=builder /rustvideoplatform-indexer /opt/rustvideoplatform-indexer

STOPSIGNAL SIGTERM

ENTRYPOINT ["/opt/rustvideoplatform-indexer"]
