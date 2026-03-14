FROM alpine:edge AS builder

RUN apk add --no-cache cargo musl-dev openssl-dev pkgconfig

RUN mkdir /src
COPY ./ /src/rustvideoplatform-indexer

ARG TARGETARCH
RUN if [ "$TARGETARCH" = "amd64" ]; then export RUSTFLAGS="-C target-cpu=x86-64-v2"; fi \
    && cd /src/rustvideoplatform-indexer && cargo build --release


FROM alpine:edge
RUN apk add --no-cache ffmpeg
COPY --from=builder /src/rustvideoplatform-indexer/target/release/rustvideoplatform-indexer /opt/rustvideoplatform-indexer

STOPSIGNAL SIGTERM

ENTRYPOINT ["/opt/rustvideoplatform-indexer"]
