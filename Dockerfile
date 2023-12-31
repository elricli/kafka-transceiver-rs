FROM rust:slim as builder

WORKDIR /usr/src

RUN apt-get update -y && \
  apt-get install -y libc6-dev make cmake g++ libsasl2-dev pkg-config libssl-dev

RUN USER=root cargo new kafka-transceiver-rs

RUN echo -e "[source.rsproxy]\nregistry = 'https://rsproxy.cn/crates.io-index'\n[source.rsproxy-sparse]\nregistry = 'sparse+https://rsproxy.cn/index/'\n[registries.rsproxy]\nindex = 'https://rsproxy.cn/crates.io-index'\n[net]\ngit-fetch-with-cli = true" ~/.cargo/config

COPY Cargo.toml Cargo.lock /usr/src/kafka-transceiver-rs/

WORKDIR /usr/src/kafka-transceiver-rs

# Cache dependencies
RUN cargo build --release

COPY src /usr/src/kafka-transceiver-rs/src/

RUN rm -rf /usr/src/kafka-transceiver-rs/target/release/deps/kafka_transceiver_rs*
RUN RUST_BACKTRACE=1 cargo build --release

FROM debian:bookworm-slim

RUN sed -i 's/deb.debian.org/mirrors.ustc.edu.cn/g' /etc/apt/sources.list.d/debian.sources && \
    apt-get update && \
    apt-get install --no-install-recommends -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/kafka-transceiver-rs/target/release/kafka-transceiver-rs /usr/local/bin/kafka-transceiver-rs
