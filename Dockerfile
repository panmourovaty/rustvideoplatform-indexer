FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev pkgconfig

RUN mkdir /src
COPY ./ /src/rustvideoplatform-indexer

ARG TARGETARCH
RUN case "$TARGETARCH" in \
        amd64)   export RUSTFLAGS="-C target-cpu=x86-64-v3" ;; \
        ppc64le) export RUSTFLAGS="-C target-cpu=pwr8" ;; \
    esac && \
    cd /src/rustvideoplatform-indexer && cargo build --release


FROM alpine:latest
RUN apk add --no-cache ffmpeg
COPY --from=builder /src/rustvideoplatform-indexer/target/release/rustvideoplatform-indexer /opt/rustvideoplatform-indexer

STOPSIGNAL SIGTERM

ENTRYPOINT ["/opt/rustvideoplatform-indexer"]
