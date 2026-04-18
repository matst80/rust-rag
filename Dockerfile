FROM rust:1.94-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets

RUN cargo build --release --bin rust-rag

FROM debian:bookworm-slim

RUN apt-get update \
	&& apt-get install -y --no-install-recommends ca-certificates libgomp1 \
	&& rm -rf /var/lib/apt/lists/* \
	&& groupadd --system rustrag \
	&& useradd --system --gid rustrag --create-home --home-dir /app rustrag

WORKDIR /app

COPY --from=builder /app/target/release/rust-rag /usr/local/bin/rust-rag
COPY --from=builder /app/assets /app/assets

RUN mkdir -p /app/data \
	&& chown -R rustrag:rustrag /app

ENV RAG_HOST=0.0.0.0 \
	RAG_PORT=4001 \
	RAG_DB_PATH=/app/data/rag.db \
	RAG_MODEL_PATH=/app/assets/bge-small-en-v1.5/model.onnx \
	RAG_TOKENIZER_PATH=/app/assets/bge-small-en-v1.5/tokenizer.json \
	RAG_EMBEDDING_DIMENSION=384 \
	RAG_INTRA_THREADS=2 \
	RAG_GRAPH_ENABLED=false \
	RAG_GRAPH_BUILD_ON_STARTUP=false \
	RAG_GRAPH_K=5 \
	RAG_GRAPH_MAX_DISTANCE=0.75 \
	RAG_GRAPH_CROSS_SOURCE=false

VOLUME ["/app/data"]
EXPOSE 4001

USER rustrag

CMD ["rust-rag"]