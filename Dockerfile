FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev pkgconfig

RUN mkdir /src
COPY ./ /src/rustvideoplatform-indexer

ENV RUSTFLAGS="-C target-cpu=x86-64-v2"
RUN cd /src/rustvideoplatform-indexer && cargo build --release


FROM alpine:latest
COPY --from=builder /src/rustvideoplatform-indexer/target/release/rustvideoplatform-indexer /opt/rustvideoplatform-indexer

STOPSIGNAL SIGTERM

ENTRYPOINT ["/opt/rustvideoplatform-indexer"]
