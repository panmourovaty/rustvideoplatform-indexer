FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev pkgconfig

RUN mkdir /src
COPY ./ /src/rustvideoplatform-indexer

RUN cd /src/rustvideoplatform-indexer && cargo build --release


FROM alpine:latest
RUN apk add --no-cache ffmpeg
COPY --from=builder /src/rustvideoplatform-indexer/target/release/rustvideoplatform-indexer /opt/rustvideoplatform-indexer

STOPSIGNAL SIGTERM

ENTRYPOINT ["/opt/rustvideoplatform-indexer"]
