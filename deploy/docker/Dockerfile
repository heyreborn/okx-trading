FROM rust:1.98.1-bookworm AS builder
WORKDIR /src
COPY . .
ARG BIN_NAME
RUN test -n "$BIN_NAME" && cargo build --locked --release --bin "$BIN_NAME"

FROM debian:bookworm-slim
ARG BIN_NAME
COPY --from=builder /src/target/release/${BIN_NAME} /usr/local/bin/app
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/app"]
