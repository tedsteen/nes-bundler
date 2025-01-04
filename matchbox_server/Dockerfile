FROM alpine/git AS cloner
WORKDIR /
ARG CACHEBUST=$(git rev-parse https://github.com/johanhelsing/matchbox.git)
RUN echo ${CACHEBUST}
RUN git clone https://github.com/johanhelsing/matchbox.git

FROM rust:1.83-slim-bullseye AS builder
WORKDIR /usr/src/matchbox_server
COPY --from=cloner matchbox/matchbox_server/Cargo.toml /usr/src/matchbox_server/Cargo.toml
COPY --from=cloner matchbox/matchbox_protocol /usr/src/matchbox_protocol
COPY --from=cloner matchbox/matchbox_server /usr/src/matchbox_server
COPY --from=cloner matchbox/matchbox_signaling /usr/src/matchbox_signaling
COPY --from=cloner matchbox/README.md /usr/src/

RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y libssl1.1 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/matchbox_server/target/release/matchbox_server /usr/local/bin/matchbox_server
CMD ["matchbox_server"]