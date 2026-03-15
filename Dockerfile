# syntax=docker/dockerfile:1
FROM alpine:edge AS builder

RUN apk add --no-cache cargo musl-dev openssl-dev pkgconfig

ARG TARGETARCH

# Pre-build dependencies (cached layer - only invalidated when Cargo.toml changes)
COPY Cargo.toml /src/rustvideoplatform-indexer/
RUN mkdir -p /src/rustvideoplatform-indexer/src && echo 'fn main() {}' > /src/rustvideoplatform-indexer/src/main.rs
RUN --mount=type=cache,target=/root/.cargo/registry,id=cargo-reg-${TARGETARCH},sharing=locked \
    --mount=type=cache,target=/root/.cargo/git,id=cargo-git-${TARGETARCH},sharing=locked \
    if [ "$TARGETARCH" = "amd64" ]; then export RUSTFLAGS="-C target-cpu=x86-64-v2"; fi \
    && cd /src/rustvideoplatform-indexer && cargo build --release 2>/dev/null ; true

# Build actual project
COPY ./ /src/rustvideoplatform-indexer
RUN --mount=type=cache,target=/root/.cargo/registry,id=cargo-reg-${TARGETARCH},sharing=locked \
    --mount=type=cache,target=/root/.cargo/git,id=cargo-git-${TARGETARCH},sharing=locked \
    if [ "$TARGETARCH" = "amd64" ]; then export RUSTFLAGS="-C target-cpu=x86-64-v2"; fi \
    && cd /src/rustvideoplatform-indexer && cargo build --release


FROM alpine:edge
RUN apk add --no-cache ffmpeg
COPY --from=builder /src/rustvideoplatform-indexer/target/release/rustvideoplatform-indexer /opt/rustvideoplatform-indexer

STOPSIGNAL SIGTERM

ENTRYPOINT ["/opt/rustvideoplatform-indexer"]
