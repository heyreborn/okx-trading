FROM rust:1.98.1-bookworm@sha256:93ce27a88655056a51dbdd8f5f2d7ddc071c7b0070fb288a37b5a285fc83971e AS builder
WORKDIR /src
COPY . .
ARG BIN_NAME
RUN test -n "$BIN_NAME" && cargo build --locked --release --bin "$BIN_NAME"

FROM debian:12.13-slim@sha256:67b30a61dc87758f0caf819646104f29ecbda97d920aaf5edc834128ac8493d3
ARG BIN_NAME
COPY --from=builder /src/target/release/${BIN_NAME} /usr/local/bin/app
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/app"]
