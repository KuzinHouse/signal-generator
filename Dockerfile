FROM rust:1-slim-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true
RUN rm -rf src

COPY src/ src/
# touch обязателен: файлы из COPY получают старый mtime, и cargo считает
# кэш-артефакты (от фиктивного main.rs) свежими -> собирается пустой бинарь.
RUN find src -type f -exec touch {} + && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/signal-generator .
COPY static/ static/

EXPOSE 8080
ENV RUST_LOG=info
CMD ["./signal-generator"]
