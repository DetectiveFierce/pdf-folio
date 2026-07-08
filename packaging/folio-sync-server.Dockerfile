FROM rust:1-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release -p pdf-folio-cloud --bin pdf-folio-sync-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/pdf-folio-sync-server /usr/local/bin/pdf-folio-sync-server

EXPOSE 53148
ENTRYPOINT ["/usr/local/bin/pdf-folio-sync-server"]
