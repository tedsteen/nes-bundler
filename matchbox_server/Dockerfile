FROM alpine/git as cloner
WORKDIR /
ARG CACHEBUST=$(git rev-parse https://github.com/johanhelsing/matchbox.git)
RUN echo ${CACHEBUST}
RUN git clone https://github.com/johanhelsing/matchbox.git

FROM rust:1.66 as builder
WORKDIR /usr/src/matchbox_server
RUN ls -hal
COPY --from=cloner matchbox/matchbox_server/Cargo.toml /usr/src/matchbox_server/Cargo.toml
COPY --from=cloner matchbox/matchbox_protocol /usr/src/matchbox_protocol
COPY --from=cloner matchbox/matchbox_server /usr/src/matchbox_server

RUN cargo build --release

FROM debian:buster-slim
RUN apt-get update && apt-get install -y libssl1.1
RUN rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/matchbox_server/target/release/matchbox_server /usr/local/bin/matchbox_server
CMD ["matchbox_server"]