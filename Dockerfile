# syntax=docker/dockerfile:1
FROM alpine:edge AS builder

RUN apk add --no-cache cargo musl-dev openssl-dev pkgconfig

ARG TARGETARCH

# Pre-build dependencies (cached layer - only invalidated when Cargo.toml changes)
COPY Cargo.toml /src/rustvideoplatform-indexer/
RUN mkdir -p /src/rustvideoplatform-indexer/src && echo 'fn main() {}' > /src/rustvideoplatform-indexer/src/main.rs
RUN case "$TARGETARCH" in \
        amd64)   export RUSTFLAGS="-C target-cpu=x86-64-v2" ;; \
        ppc64le) export RUSTFLAGS="-C target-cpu=pwr8" ;; \
    esac && \
    cd /src/rustvideoplatform-indexer && cargo build --release 2>/dev/null ; true

# Build actual project
COPY ./ /src/rustvideoplatform-indexer
RUN case "$TARGETARCH" in \
        amd64)   export RUSTFLAGS="-C target-cpu=x86-64-v2" ;; \
        ppc64le) export RUSTFLAGS="-C target-cpu=pwr8" ;; \
    esac && \
    cd /src/rustvideoplatform-indexer && cargo build --release


FROM alpine:edge
RUN apk add --no-cache ffmpeg
COPY --from=builder /src/rustvideoplatform-indexer/target/release/rustvideoplatform-indexer /opt/rustvideoplatform-indexer

STOPSIGNAL SIGTERM

ENTRYPOINT ["/opt/rustvideoplatform-indexer"]
