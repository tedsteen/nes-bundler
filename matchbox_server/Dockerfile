FROM alpine/git as cloner
WORKDIR /
RUN git clone https://github.com/johanhelsing/matchbox.git

FROM rust:1.66 as builder
WORKDIR /usr/matchbox_server
RUN ls -hal
COPY --from=cloner matchbox/matchbox_server/Cargo.toml .
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && cargo build --release
COPY --from=cloner /matchbox/matchbox_server/ .
RUN touch src/main.rs
RUN cargo build --release

FROM debian:buster-slim
RUN apt-get update && apt-get install -y libssl1.1 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/matchbox_server/target/release/matchbox_server /usr/local/bin/matchbox_server
ENTRYPOINT ["matchbox_server"]