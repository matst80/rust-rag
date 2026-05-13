SHELL := /bin/sh
MODEL_DIR := $(CURDIR)/assets/bge-small-en-v1.5
MODEL_URL := https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/onnx/model.onnx
TOKENIZER_URL := https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/tokenizer.json

# bge-m3 (1024-d, CLS-pooled). Exported locally via scripts/export_bge_m3.sh
# into $(M3_MODEL_DIR). External-data ONNX layout — keep the whole directory
# together when moving.
M3_MODEL_DIR := $(CURDIR)/assets/bge-m3
M3_MODEL_PATH := $(M3_MODEL_DIR)/model.onnx
M3_TOKENIZER_PATH := $(M3_MODEL_DIR)/tokenizer.json

# Local-dev Postgres — points at the shared 10.10.10.207 instance with the
# `rust_rag_dev` database. Override RAG_DATABASE_URL=... to use a different
# host / DB. Leave unset to fall back to pure-SQLite mode.
RAG_DATABASE_URL ?= postgres://mats:jagharpostgres@10.10.10.207/rust_rag_dev

# Where the prod SQLite snapshot is fetched to / read from for migrations.
PROD_SNAPSHOT_DIR ?= /tmp/rust-rag-prod-snapshot
PROD_SNAPSHOT_DB ?= $(PROD_SNAPSHOT_DIR)/rag.db
PROD_POD_SELECTOR ?= app.kubernetes.io/name=rust-rag,app.kubernetes.io/variant=cuda
PROD_POD_NAMESPACE ?= home

RAG_MODEL_PATH ?= $(MODEL_DIR)/model.onnx
RAG_TOKENIZER_PATH ?= $(MODEL_DIR)/tokenizer.json
RAG_DB_PATH ?= $(CURDIR)/data/rag.db
RAG_PORT ?= 4001
RAG_GRAPH_ENABLED ?= true
RAG_GRAPH_BUILD_ON_STARTUP ?= true
RAG_GRAPH_K ?= 5
RAG_ANALYSIS_ENABLED ?= true
RAG_GRAPH_MAX_DISTANCE ?= 0.75
RAG_GRAPH_CROSS_SOURCE ?= false
RAG_AUTH_ENABLED ?= false
RAG_FRONTEND_API_KEY ?= replace-with-shared-frontend-backend-key
RAG_MCP_AUTH_BEARER ?=
RAG_MCP_ALLOWED_HOSTS ?= localhost,127.0.0.1,::1,rag.k6n.net
AUTH_SESSION_SECRET ?= replace-with-a-long-random-secret
RAG_OPENAI_API_BASE_URL ?= http://10.10.11.135:8082/v1
RAG_OPENAI_API_KEY ?=
RAG_OPENAI_MODEL ?= unsloth/Qwen3.5-4B-GGUF
RAG_OPENAI_TIMEOUT_SECS ?= 60
RAG_MULTIMODAL_BASE_URL ?= http://10.10.11.135:8082/v1
RAG_MULTIMODAL_API_KEY ?=
RAG_MULTIMODAL_MODEL ?= unsloth/Qwen3.5-4B-GGUF
RAG_MULTIMODAL_TIMEOUT_SECS ?= 120
RAG_UPLOAD_PATH ?= $(CURDIR)/data/uploads

# Ontology worker — tuned for a local LLM with a 65535-token context window.
# Token budget per call: ~700 (system prompt) + target_preview/4 + neighbors*(candidate_preview/4)
# With these defaults: ~700 + 500 + 10*375 = ~4950 input tokens, well within 65535.
# Increase NEIGHBOR_COUNT or preview sizes further if the model handles more context well.
RAG_ONTOLOGY_ENABLED ?= true
RAG_ONTOLOGY_CONFIDENCE_THRESHOLD ?= 0.6
RAG_ONTOLOGY_BATCH_SIZE ?= 5
RAG_ONTOLOGY_INTERVAL_SECS ?= 30
RAG_ONTOLOGY_NEIGHBOR_COUNT ?= 10
RAG_ONTOLOGY_TARGET_PREVIEW_CHARS ?= 2000
RAG_ONTOLOGY_CANDIDATE_PREVIEW_CHARS ?= 1500

# Manager worker — autonomous orchestrator. Uses xAI Grok by default.
RAG_MANAGER_ENABLED ?= true
RAG_MANAGER_CHANNEL ?= manager
RAG_MANAGER_MENTION ?= @manager
RAG_MANAGER_INTERVAL_SECS ?= 300
RAG_MANAGER_API_BASE_URL ?= https://api.x.ai/v1
RAG_MANAGER_API_KEY ?=
RAG_MANAGER_MODEL ?= grok-4-1-fast-reasoning
RAG_MANAGER_TIMEOUT_SECS ?= 120
RAG_MANAGER_MAX_ITERATIONS ?= 8
RAG_MANAGER_MEMORY_SOURCE_ID ?= manager_memory
RAG_MANAGER_SYSTEM_PROMPT_FILE ?= $(CURDIR)/manager-system-prompt.txt

# ACP WebSocket (telegram-acp). Manager uses these for acp_* tools.
RAG_ACP_WS_URL ?= ws://10.10.11.37:9001/
RAG_ACP_WS_TOKEN ?= test-token

# Chunking — tune to your embedding model's context window.
# RAG_LARGE_ITEM_THRESHOLD defaults to RAG_CHUNK_MAX_CHARS when unset.
RAG_CHUNK_MAX_CHARS ?= 1536
RAG_CHUNK_OVERLAP_CHARS ?= 200
RAG_LARGE_ITEM_THRESHOLD ?= $(RAG_CHUNK_MAX_CHARS)

# Logging — set to rust_rag::ontology=debug to see per-item LLM calls and raw responses
RUST_LOG ?= rust_rag=info
LOG_DIR ?= $(CURDIR)/data/logs
LOG_FILE ?= $(LOG_DIR)/rust-rag.log
RAG_CUDA_MEM_LIMIT_MB ?= 4096
RAG_CUDA_DEVICE_ID ?= 0
API_URL ?= https://127.0.0.1:$(RAG_PORT)
RAG_CDP_URL ?= ws://10.10.3.27:9222
ZITADEL_ISSUER ?= https://auth.k6n.net
ZITADEL_CLIENT_ID ?= 369530153681881434@rag
ZITADEL_CLIENT_SECRET ?= U8opCVRn3hrFNcyXDJpb7DLQAa5aHEikjuQn2Rr5KwG7RiofvzKifxdTB3yEO0ID
ZITADEL_REDIRECT_URI ?= https://rag.k6n.net/auth/callback
ZITADEL_SCOPES ?= openid profile email
CUDA_IMAGE_NAME ?= matst80/rust-rag:cuda
FRONTEND_IMAGE_NAME ?= matst80/rust-rag-frontend:latest
FRONTEND_DIR ?= $(CURDIR)/frontend
APP_BASE_URL ?= http://localhost:3000
K8S_FRONTEND_MANIFEST ?= deploy/kubernetes/rust-rag-frontend.yaml
K8S_INGRESS_MANIFEST ?= deploy/kubernetes/rust-rag-ingress.yaml
K8S_MCP_INGRESS_MANIFEST ?= deploy/kubernetes/rust-rag-mcp-ingress.yaml
K8S_CUDA_MANIFEST ?= deploy/kubernetes/rust-rag-cuda.yaml
K8S_RUNTIMECLASS_MANIFEST ?= deploy/kubernetes/rust-rag-runtimeclass.yaml
K8S_NVIDIA_PLUGIN_MANIFEST ?= deploy/kubernetes/nvidia-device-plugin.yaml
FRONTEND_DEV_PORT ?= 3000
FRONTEND_DEV_HOST ?= 0.0.0.0
K8S_NAMESPACE ?= home
KUBECTL_NS := $(if $(strip $(K8S_NAMESPACE)),-n $(K8S_NAMESPACE))
K8S_CUDA_DEPLOYMENT ?= rust-rag-cuda
K8S_FRONTEND_DEPLOYMENT ?= rust-rag-frontend


.PHONY: help fetch-assets export-bge-m3 export-bge-m3-sparse export-bge-reranker fetch-prod-snapshot migrate-prod cleanup-legacy-chunks backfill-section-paths e2e-local print-env fmt test verify check-env build build-cuda run run-pg run-baseline run-cuda eval tail-logs ontology-status ontology-edges docker-build-cuda docker-push-cuda docker-run-cuda frontend-docker-build frontend-docker-push frontend-docker-run frontend-install frontend-dev frontend-prod docker-push-all k8s-namespace k8s-apply-cuda k8s-delete-cuda k8s-apply-frontend k8s-delete-frontend k8s-apply-ingress k8s-delete-ingress k8s-apply-runtimeclass k8s-delete-runtimeclass k8s-apply-nvidia-plugin k8s-delete-nvidia-plugin k8s-apply-all k8s-delete-all rollout rollout-cuda rollout-frontend rollout-status push-and-rollout store-knowledge store-memory search-knowledge search-memory admin-categories admin-items graph-status graph-rebuild graph-neighborhood smoke http-files mcp-inspector-local mcp-inspector-hosted

help:
	@printf '%s\n' \
		'Targets:' \
		'  make fetch-assets     Download the default ONNX model and tokenizer' \
		'  make print-env        Print exact export commands for local runs' \
		'  make fmt              Format the Rust codebase' \
		'  make test             Run the automated test suite' \
		'  make tail-logs        Follow the live log file (run in a second terminal while make run is active)' \
		'  make ontology-status  Show ontology processing status counts from the DB' \
		'  make ontology-edges   Show committed ontology edges grouped by predicate' \
		'  make verify           Run formatting check and tests' \
		'  make check-env        Verify required runtime env vars are set' \
		'  make build            Build the rust-rag binary in release mode' \
		'  make build-cuda       Build the rust-rag binary with the cuda feature enabled' \

		'  make run              Start the service locally (SQLite, bge-small)' \
		'  make run-pg           Start the service locally against Postgres + bge-m3 (CLS-pooled, 1024-d)' \
		'  make run-cuda         Start the service locally with the cuda feature enabled' \
		'  make export-bge-m3    Export BAAI/bge-m3 to ONNX into $(M3_MODEL_DIR)' \
		'  make fetch-prod-snapshot  kubectl-cp the prod SQLite DB into $(PROD_SNAPSHOT_DIR)' \
		'  make migrate-prod     Re-embed $(PROD_SNAPSHOT_DB) with bge-m3 and write to Postgres' \
		'  make e2e-local        Print the two-command recipe for backend + frontend e2e' \

		'  make docker-build-cuda    Build the CUDA server container image (amd64)' \
		'  make docker-push-cuda     Build + push the CUDA server container image' \
		'  make docker-run-cuda      Run the CUDA server container with the local data directory mounted' \
		'  make frontend-install      Install frontend npm dependencies' \
		'  make frontend-dev          Run Next.js dev on $(FRONTEND_DEV_HOST):$(FRONTEND_DEV_PORT)' \
		'  make frontend-prod         Build and start Next.js on $(FRONTEND_DEV_HOST):$(FRONTEND_DEV_PORT)' \
		'  make frontend-docker-build Build the Next.js frontend container image' \
		'  make frontend-docker-push  Push the Next.js frontend container image' \
		'  make frontend-docker-run   Run the frontend container (override RAG_API_URL as needed)' \
		'  make docker-push-all       Push CUDA backend + frontend images' \
		'  make k8s-apply-cuda            Apply the CUDA backend Deployment' \
		'  make k8s-delete-cuda           Delete the CUDA backend Deployment' \
		'  make k8s-apply-frontend        Apply the in-cluster frontend Deployment' \
		'  make k8s-delete-frontend       Delete the in-cluster frontend Deployment' \
		'  make k8s-apply-ingress         Apply Ingress (frontend + MCP)' \
		'  make k8s-delete-ingress        Delete Ingress (frontend + MCP)' \
		'  make k8s-apply-runtimeclass    Apply the nvidia RuntimeClass (cluster-scoped)' \
		'  make k8s-apply-nvidia-plugin   Apply the NVIDIA device plugin DaemonSet' \
		'  make k8s-apply-all             Apply CUDA backend + frontend manifests' \
		'  make k8s-delete-all            Delete CUDA backend + frontend manifests' \
		'  make k8s-namespace             Create K8S_NAMESPACE (currently: $(K8S_NAMESPACE)) if it does not exist' \
		'  (override target namespace with K8S_NAMESPACE=<name>)' \
		'  make rollout              Restart CUDA backend + frontend Deployments + wait for ready' \
		'  make rollout-cuda         Restart only the CUDA backend Deployment' \
		'  make rollout-frontend     Restart only the frontend Deployment' \
		'  make rollout-status       Show rollout status for both Deployments' \
		'  make push-and-rollout     docker-push-all → rollout' \

		'  make store-knowledge  POST a sample knowledge document' \
		'  make store-memory     POST a sample memory document' \
		'  make search-knowledge Search with source_id=knowledge' \
		'  make search-memory    Search with source_id=memory' \
		'  make admin-categories GET category summary' \
		'  make admin-items      GET all items or set SOURCE_ID=memory' \
		'  make graph-status     GET graph runtime status' \
		'  make graph-rebuild    Rebuild similarity edges (graph must be enabled)' \
		'  make graph-neighborhood GET one item neighborhood (set ITEM_ID=doc-memory-1)' \
		'  make smoke            Run sample store + search requests with curl' \
		'  make http-files       List the .http request files' \
		'  make mcp-inspector-local  Test local MCP server using npx @modelcontextprotocol/inspector' \
		'  make mcp-inspector-hosted Test hosted MCP server using npx @modelcontextprotocol/inspector'

fetch-assets:
	mkdir -p "$(MODEL_DIR)" "$(CURDIR)/data" "$(RAG_UPLOAD_PATH)"
	curl -L --fail --silent --show-error "$(MODEL_URL)" -o "$(RAG_MODEL_PATH)"
	curl -L --fail --silent --show-error "$(TOKENIZER_URL)" -o "$(RAG_TOKENIZER_PATH)"
	@printf '%s\n' "Fetched model to $(RAG_MODEL_PATH)"
	@printf '%s\n' "Fetched tokenizer to $(RAG_TOKENIZER_PATH)"

print-env:
	@printf '%s\n' \
		"export RAG_MODEL_PATH=$(RAG_MODEL_PATH)" \
		"export RAG_TOKENIZER_PATH=$(RAG_TOKENIZER_PATH)" \
		"export RAG_DB_PATH=$(RAG_DB_PATH)" \
		"export RAG_PORT=$(RAG_PORT)" \
		"export RAG_GRAPH_ENABLED=$(RAG_GRAPH_ENABLED)" \
		"export RAG_GRAPH_BUILD_ON_STARTUP=$(RAG_GRAPH_BUILD_ON_STARTUP)" \
		"export RAG_GRAPH_K=$(RAG_GRAPH_K)" \
		"export RAG_GRAPH_MAX_DISTANCE=$(RAG_GRAPH_MAX_DISTANCE)" \
		"export RAG_GRAPH_CROSS_SOURCE=$(RAG_GRAPH_CROSS_SOURCE)" \
		"export RAG_AUTH_ENABLED=$(RAG_AUTH_ENABLED)" \
		"export RAG_FRONTEND_API_KEY=$(RAG_FRONTEND_API_KEY)" \
		"export AUTH_SESSION_SECRET=$(AUTH_SESSION_SECRET)" \
		"export RAG_OPENAI_API_BASE_URL=$(RAG_OPENAI_API_BASE_URL)" \
		"export RAG_OPENAI_API_KEY=$(RAG_OPENAI_API_KEY)" \
		"export RAG_OPENAI_MODEL=$(RAG_OPENAI_MODEL)" \
		"export RAG_OPENAI_TIMEOUT_SECS=$(RAG_OPENAI_TIMEOUT_SECS)" \
		"export RUST_LOG=$(RUST_LOG)" \
		"export RAG_ONTOLOGY_ENABLED=$(RAG_ONTOLOGY_ENABLED)" \
		"export RAG_ONTOLOGY_CONFIDENCE_THRESHOLD=$(RAG_ONTOLOGY_CONFIDENCE_THRESHOLD)" \
		"export RAG_ONTOLOGY_BATCH_SIZE=$(RAG_ONTOLOGY_BATCH_SIZE)" \
		"export RAG_ONTOLOGY_INTERVAL_SECS=$(RAG_ONTOLOGY_INTERVAL_SECS)" \
		"export RAG_ONTOLOGY_NEIGHBOR_COUNT=$(RAG_ONTOLOGY_NEIGHBOR_COUNT)" \
		"export RAG_ONTOLOGY_TARGET_PREVIEW_CHARS=$(RAG_ONTOLOGY_TARGET_PREVIEW_CHARS)" \
		"export RAG_ONTOLOGY_CANDIDATE_PREVIEW_CHARS=$(RAG_ONTOLOGY_CANDIDATE_PREVIEW_CHARS)" \
		"export RAG_CHUNK_MAX_CHARS=$(RAG_CHUNK_MAX_CHARS)" \
		"export RAG_CHUNK_OVERLAP_CHARS=$(RAG_CHUNK_OVERLAP_CHARS)" \
		"export RAG_LARGE_ITEM_THRESHOLD=$(RAG_LARGE_ITEM_THRESHOLD)" \
		"export RAG_MULTIMODAL_BASE_URL=$(RAG_MULTIMODAL_BASE_URL)" \
		"export RAG_MULTIMODAL_API_KEY=$(RAG_MULTIMODAL_API_KEY)" \
		"export RAG_MULTIMODAL_MODEL=$(RAG_MULTIMODAL_MODEL)" \
		"export RAG_MULTIMODAL_TIMEOUT_SECS=$(RAG_MULTIMODAL_TIMEOUT_SECS)" \
		"export RAG_UPLOAD_PATH=$(RAG_UPLOAD_PATH)"

fmt:
	cargo fmt

test:
	cargo test

verify:
	cargo fmt --check
	cargo test

check-env: fetch-assets
	@test -f "$(RAG_MODEL_PATH)" || { echo "missing model at $(RAG_MODEL_PATH)"; exit 1; }
	@test -f "$(RAG_TOKENIZER_PATH)" || { echo "missing tokenizer at $(RAG_TOKENIZER_PATH)"; exit 1; }

build:
	cargo build --release --bin rust-rag

build-cuda:
	cargo build --release --features cuda --bin rust-rag



run:
	@mkdir -p "$(LOG_DIR)" "$(RAG_UPLOAD_PATH)"
	@printf 'Logging to %s — run "make tail-logs" in another terminal to follow\n' "$(LOG_FILE)"
	RUST_LOG="$(RUST_LOG)" \
	RAG_MODEL_PATH="$(RAG_MODEL_PATH)" \
	RAG_TOKENIZER_PATH="$(RAG_TOKENIZER_PATH)" \
	RAG_DB_PATH="$(RAG_DB_PATH)" \
	RAG_PORT="$(RAG_PORT)" \
	RAG_GRAPH_ENABLED="$(RAG_GRAPH_ENABLED)" \
	RAG_GRAPH_BUILD_ON_STARTUP="$(RAG_GRAPH_BUILD_ON_STARTUP)" \
	RAG_GRAPH_K="$(RAG_GRAPH_K)" \
	RAG_GRAPH_MAX_DISTANCE="$(RAG_GRAPH_MAX_DISTANCE)" \
	RAG_GRAPH_CROSS_SOURCE="$(RAG_GRAPH_CROSS_SOURCE)" \
	RAG_AUTH_ENABLED="$(RAG_AUTH_ENABLED)" \
	RAG_FRONTEND_API_KEY="$(RAG_FRONTEND_API_KEY)" \
	AUTH_SESSION_SECRET="$(AUTH_SESSION_SECRET)" \
	ZITADEL_ISSUER="$(ZITADEL_ISSUER)" \
	ZITADEL_CLIENT_ID="$(ZITADEL_CLIENT_ID)" \
	ZITADEL_CLIENT_SECRET="$(ZITADEL_CLIENT_SECRET)" \
	ZITADEL_REDIRECT_URI="$(ZITADEL_REDIRECT_URI)" \
	ZITADEL_SCOPES="$(ZITADEL_SCOPES)" \
	RAG_OPENAI_API_BASE_URL="$(RAG_OPENAI_API_BASE_URL)" \
	RAG_OPENAI_API_KEY="$(RAG_OPENAI_API_KEY)" \
	RAG_OPENAI_MODEL="$(RAG_OPENAI_MODEL)" \
	RAG_OPENAI_TIMEOUT_SECS="$(RAG_OPENAI_TIMEOUT_SECS)" \
	RAG_ONTOLOGY_ENABLED="$(RAG_ONTOLOGY_ENABLED)" \
	RAG_ONTOLOGY_CONFIDENCE_THRESHOLD="$(RAG_ONTOLOGY_CONFIDENCE_THRESHOLD)" \
	RAG_ONTOLOGY_BATCH_SIZE="$(RAG_ONTOLOGY_BATCH_SIZE)" \
	RAG_ONTOLOGY_INTERVAL_SECS="$(RAG_ONTOLOGY_INTERVAL_SECS)" \
	RAG_ONTOLOGY_NEIGHBOR_COUNT="$(RAG_ONTOLOGY_NEIGHBOR_COUNT)" \
	RAG_ONTOLOGY_TARGET_PREVIEW_CHARS="$(RAG_ONTOLOGY_TARGET_PREVIEW_CHARS)" \
	RAG_ONTOLOGY_CANDIDATE_PREVIEW_CHARS="$(RAG_ONTOLOGY_CANDIDATE_PREVIEW_CHARS)" \
	RAG_CHUNK_MAX_CHARS="$(RAG_CHUNK_MAX_CHARS)" \
	RAG_CHUNK_OVERLAP_CHARS="$(RAG_CHUNK_OVERLAP_CHARS)" \
	RAG_LARGE_ITEM_THRESHOLD="$(RAG_LARGE_ITEM_THRESHOLD)" \
	RAG_MULTIMODAL_BASE_URL="$(RAG_MULTIMODAL_BASE_URL)" \
	RAG_MULTIMODAL_API_KEY="$(RAG_MULTIMODAL_API_KEY)" \
	RAG_MULTIMODAL_MODEL="$(RAG_MULTIMODAL_MODEL)" \
	RAG_MULTIMODAL_TIMEOUT_SECS="$(RAG_MULTIMODAL_TIMEOUT_SECS)" \
	RAG_UPLOAD_PATH="$(RAG_UPLOAD_PATH)" \
	RAG_MCP_ALLOWED_HOSTS="$(RAG_MCP_ALLOWED_HOSTS)" \
	cargo run 2>&1 | tee "$(LOG_FILE)"

# Local-dev: bge-m3 + remote Postgres. Mirrors `run` but points the embedder
# at the bge-m3 export and sets RAG_DATABASE_URL so migrations apply on boot.
# CLS pooling is required for bge-m3 — mean pooling produces valid 1024-d
# vectors but with degraded retrieval quality.
run-pg: export-bge-m3
	@mkdir -p "$(LOG_DIR)" "$(RAG_UPLOAD_PATH)"
	@printf 'Logging to %s — run "make tail-logs" in another terminal to follow\n' "$(LOG_FILE)"
	@printf 'Postgres: %s\n' "$(RAG_DATABASE_URL)"
	RUST_LOG="$(RUST_LOG)" \
	RAG_MODEL_PATH="$(M3_MODEL_PATH)" \
	RAG_TOKENIZER_PATH="$(M3_TOKENIZER_PATH)" \
	RAG_EMBEDDING_DIMENSION="1024" \
	RAG_EMBEDDING_POOLING="cls" \
	RAG_DATABASE_URL="$(RAG_DATABASE_URL)" \
	RAG_DB_PATH="$(CURDIR)/data/rag-m3.db" \
	RAG_PORT="$(RAG_PORT)" \
	RAG_GRAPH_ENABLED="$(RAG_GRAPH_ENABLED)" \
	RAG_GRAPH_BUILD_ON_STARTUP="$(RAG_GRAPH_BUILD_ON_STARTUP)" \
	RAG_GRAPH_K="$(RAG_GRAPH_K)" \
	RAG_GRAPH_MAX_DISTANCE="$(RAG_GRAPH_MAX_DISTANCE)" \
	RAG_GRAPH_CROSS_SOURCE="$(RAG_GRAPH_CROSS_SOURCE)" \
	RAG_AUTH_ENABLED="$(RAG_AUTH_ENABLED)" \
	RAG_FRONTEND_API_KEY="$(RAG_FRONTEND_API_KEY)" \
	AUTH_SESSION_SECRET="$(AUTH_SESSION_SECRET)" \
	ZITADEL_ISSUER="$(ZITADEL_ISSUER)" \
	ZITADEL_CLIENT_ID="$(ZITADEL_CLIENT_ID)" \
	ZITADEL_CLIENT_SECRET="$(ZITADEL_CLIENT_SECRET)" \
	ZITADEL_REDIRECT_URI="$(ZITADEL_REDIRECT_URI)" \
	ZITADEL_SCOPES="$(ZITADEL_SCOPES)" \
	RAG_OPENAI_API_BASE_URL="$(RAG_OPENAI_API_BASE_URL)" \
	RAG_OPENAI_API_KEY="$(RAG_OPENAI_API_KEY)" \
	RAG_OPENAI_MODEL="$(RAG_OPENAI_MODEL)" \
	RAG_OPENAI_TIMEOUT_SECS="$(RAG_OPENAI_TIMEOUT_SECS)" \
	RAG_ONTOLOGY_ENABLED="false" \
	RAG_MANAGER_ENABLED="false" \
	RAG_CHUNK_MAX_CHARS="$(RAG_CHUNK_MAX_CHARS)" \
	RAG_CHUNK_OVERLAP_CHARS="$(RAG_CHUNK_OVERLAP_CHARS)" \
	RAG_LARGE_ITEM_THRESHOLD="$(RAG_LARGE_ITEM_THRESHOLD)" \
	RAG_MULTIMODAL_BASE_URL="$(RAG_MULTIMODAL_BASE_URL)" \
	RAG_MULTIMODAL_API_KEY="$(RAG_MULTIMODAL_API_KEY)" \
	RAG_MULTIMODAL_MODEL="$(RAG_MULTIMODAL_MODEL)" \
	RAG_MULTIMODAL_TIMEOUT_SECS="$(RAG_MULTIMODAL_TIMEOUT_SECS)" \
	RAG_UPLOAD_PATH="$(RAG_UPLOAD_PATH)" \
	RAG_MCP_ALLOWED_HOSTS="$(RAG_MCP_ALLOWED_HOSTS)" \
	cargo run 2>&1 | tee "$(LOG_FILE)"

# Baseline server for eval comparisons: SQLite + bge-small + mean pooling
# (the legacy stack), pointed at a read-only copy of the prod snapshot so it
# has the same content as the Postgres/bge-m3 stack served by `run-pg`.
# Disables ontology + manager workers and similarity-graph rebuild on
# startup to keep the baseline deterministic and fast.
run-baseline: fetch-assets
	@test -f "$(PROD_SNAPSHOT_DB)" || { echo "missing $(PROD_SNAPSHOT_DB) — run \`make fetch-prod-snapshot\` first"; exit 1; }
	@mkdir -p "$(CURDIR)/data" "$(LOG_DIR)" "$(RAG_UPLOAD_PATH)"
	@cp -f "$(PROD_SNAPSHOT_DB)" "$(CURDIR)/data/rag-baseline.db"
	@rm -f "$(CURDIR)/data/rag-baseline.db-wal" "$(CURDIR)/data/rag-baseline.db-shm"
	@printf 'Baseline (SQLite + bge-small) on %s\n' "$(CURDIR)/data/rag-baseline.db"
	RUST_LOG="$(RUST_LOG)" \
	RAG_MODEL_PATH="$(RAG_MODEL_PATH)" \
	RAG_TOKENIZER_PATH="$(RAG_TOKENIZER_PATH)" \
	RAG_DB_PATH="$(CURDIR)/data/rag-baseline.db" \
	RAG_PORT="$(RAG_PORT)" \
	RAG_GRAPH_ENABLED="false" \
	RAG_GRAPH_BUILD_ON_STARTUP="false" \
	RAG_AUTH_ENABLED="false" \
	RAG_ONTOLOGY_ENABLED="false" \
	RAG_MANAGER_ENABLED="false" \
	RAG_UPLOAD_PATH="$(RAG_UPLOAD_PATH)" \
	cargo run 2>&1 | tee "$(LOG_FILE)"

# Run the eval harness against whichever server is up on $(RAG_PORT). Pass
# LABEL=... to tag the run; defaults derived from common values.
EVAL_LABEL ?= run
eval:
	@command -v python3 >/dev/null || { echo "python3 not found"; exit 1; }
	python3 eval/run_eval.py \
		--base-url "http://localhost:$(RAG_PORT)" \
		--label "$(EVAL_LABEL)" \
		--hybrid false

# Idempotent: skips if assets/bge-m3/model.onnx already exists.
export-bge-m3:
	@bash scripts/export_bge_m3.sh

# Phase 2 export: encoder + sparse_linear in one ONNX graph with two outputs
# (last_hidden_state, sparse_logits). Replaces the dense-only export above
# once the runtime backend is wired for sparse. Idempotent via marker file.
export-bge-m3-sparse:
	@bash scripts/export_bge_m3_sparse.sh

# Phase 3 export: bge-reranker-v2-m3 cross-encoder. Output goes to
# assets/bge-reranker-v2-m3/. Optional — only needed when running with
# RAG_RERANKER_ENABLED=true.
export-bge-reranker:
	@bash scripts/export_bge_reranker_v2_m3.sh

# Pull the live SQLite DB out of the rust-rag-cuda pod via kubectl-cp. WAL +
# SHM are copied alongside `rag.db` so SQLite can recover any in-flight
# transactions on first open. Requires `kubectl` access to the cluster.
fetch-prod-snapshot:
	@mkdir -p "$(PROD_SNAPSHOT_DIR)"
	@pod=$$(kubectl -n "$(PROD_POD_NAMESPACE)" get pod -l "$(PROD_POD_SELECTOR)" -o name | head -1); \
		test -n "$$pod" || { echo "no pod matching $(PROD_POD_SELECTOR) in namespace $(PROD_POD_NAMESPACE)"; exit 1; }; \
		echo "fetching from $$pod"; \
		kubectl -n "$(PROD_POD_NAMESPACE)" cp "$${pod#pod/}:/app/data/rag.db" "$(PROD_SNAPSHOT_DIR)/rag.db"; \
		kubectl -n "$(PROD_POD_NAMESPACE)" cp "$${pod#pod/}:/app/data/rag.db-wal" "$(PROD_SNAPSHOT_DIR)/rag.db-wal" || true; \
		kubectl -n "$(PROD_POD_NAMESPACE)" cp "$${pod#pod/}:/app/data/rag.db-shm" "$(PROD_SNAPSHOT_DIR)/rag.db-shm" || true
	@ls -lh "$(PROD_SNAPSHOT_DIR)"

# Re-embed the prod snapshot with bge-m3 (CLS pooling) and write documents +
# chunks to Postgres. Idempotent: ON CONFLICT updates documents in place;
# chunks are replaced per document. Requires `make export-bge-m3` and a
# fetched snapshot.
migrate-prod: export-bge-m3
	@test -f "$(PROD_SNAPSHOT_DB)" || { echo "missing $(PROD_SNAPSHOT_DB) — run \`make fetch-prod-snapshot\` first"; exit 1; }
	RAG_DATABASE_URL="$(RAG_DATABASE_URL)" \
	RAG_MODEL_PATH="$(M3_MODEL_PATH)" \
	RAG_TOKENIZER_PATH="$(M3_TOKENIZER_PATH)" \
	RAG_EMBEDDING_POOLING="cls" \
	RAG_INTRA_THREADS="4" \
	cargo run --release --bin migrate_sqlite_to_pg -- "$(PROD_SNAPSHOT_DB)"

# Regroup legacy `<id>:c:N` documents (artifact of pre-Postgres API-layer
# chunking) into single parents. Pass DRY=1 to log the plan without writes.
cleanup-legacy-chunks:
	@test -f "$(M3_MODEL_PATH)" || { echo "missing $(M3_MODEL_PATH) — run \`make export-bge-m3\` first"; exit 1; }
	RAG_DATABASE_URL="$(RAG_DATABASE_URL)" \
	RAG_MODEL_PATH="$(M3_MODEL_PATH)" \
	RAG_TOKENIZER_PATH="$(M3_TOKENIZER_PATH)" \
	RAG_EMBEDDING_POOLING="cls" \
	RAG_INTRA_THREADS="4" \
	cargo run --release --bin cleanup_legacy_chunks -- $(if $(DRY),--dry-run,)

# Recompute and write `chunks.section_path` for existing documents (no
# re-embedding). Used after the section-path tracking landed but data was
# already migrated.
backfill-section-paths:
	@test -f "$(M3_TOKENIZER_PATH)" || { echo "missing $(M3_TOKENIZER_PATH) — run \`make export-bge-m3\` first"; exit 1; }
	RAG_DATABASE_URL="$(RAG_DATABASE_URL)" \
	RAG_TOKENIZER_PATH="$(M3_TOKENIZER_PATH)" \
	cargo run --release --bin backfill_section_paths

run-cuda:
	@mkdir -p "$(LOG_DIR)" "$(RAG_UPLOAD_PATH)"
	@printf 'Logging to %s — run "make tail-logs" in another terminal to follow\n' "$(LOG_FILE)"
	ORT_STRATEGY="system" \
	ORT_LIB_LOCATION="/home/mats/github.com/matst80/rust-rag/test_ort_env/lib/python3.12/site-packages/onnxruntime/capi" \
	ORT_PREFER_DYNAMIC_LINK="1" \
	LD_LIBRARY_PATH="/home/mats/github.com/matst80/rust-rag/test_ort_env/lib/python3.12/site-packages/onnxruntime/capi:$$LD_LIBRARY_PATH" \
	RUST_LOG="$(RUST_LOG)" \
	RAG_MODEL_PATH="$(RAG_MODEL_PATH)" \
	RAG_TOKENIZER_PATH="$(RAG_TOKENIZER_PATH)" \
	RAG_DB_PATH="$(RAG_DB_PATH)" \
	RAG_PORT="$(RAG_PORT)" \
	RAG_GRAPH_ENABLED="$(RAG_GRAPH_ENABLED)" \
	RAG_GRAPH_BUILD_ON_STARTUP="$(RAG_GRAPH_BUILD_ON_STARTUP)" \
	RAG_GRAPH_K="$(RAG_GRAPH_K)" \
	RAG_GRAPH_MAX_DISTANCE="$(RAG_GRAPH_MAX_DISTANCE)" \
	RAG_GRAPH_CROSS_SOURCE="$(RAG_GRAPH_CROSS_SOURCE)" \
	RAG_AUTH_ENABLED="$(RAG_AUTH_ENABLED)" \
	RAG_FRONTEND_API_KEY="$(RAG_FRONTEND_API_KEY)" \
	AUTH_SESSION_SECRET="$(AUTH_SESSION_SECRET)" \
	ZITADEL_ISSUER="$(ZITADEL_ISSUER)" \
	ZITADEL_CLIENT_ID="$(ZITADEL_CLIENT_ID)" \
	ZITADEL_CLIENT_SECRET="$(ZITADEL_CLIENT_SECRET)" \
	ZITADEL_REDIRECT_URI="$(ZITADEL_REDIRECT_URI)" \
	ZITADEL_SCOPES="$(ZITADEL_SCOPES)" \
	RAG_OPENAI_API_BASE_URL="$(RAG_OPENAI_API_BASE_URL)" \
	RAG_OPENAI_API_KEY="$(RAG_OPENAI_API_KEY)" \
	RAG_OPENAI_MODEL="$(RAG_OPENAI_MODEL)" \
	RAG_OPENAI_TIMEOUT_SECS="$(RAG_OPENAI_TIMEOUT_SECS)" \
	RAG_ONTOLOGY_ENABLED="$(RAG_ONTOLOGY_ENABLED)" \
	RAG_ONTOLOGY_CONFIDENCE_THRESHOLD="$(RAG_ONTOLOGY_CONFIDENCE_THRESHOLD)" \
	RAG_ONTOLOGY_BATCH_SIZE="$(RAG_ONTOLOGY_BATCH_SIZE)" \
	RAG_ONTOLOGY_INTERVAL_SECS="$(RAG_ONTOLOGY_INTERVAL_SECS)" \
	RAG_ONTOLOGY_NEIGHBOR_COUNT="$(RAG_ONTOLOGY_NEIGHBOR_COUNT)" \
	RAG_ONTOLOGY_TARGET_PREVIEW_CHARS="$(RAG_ONTOLOGY_TARGET_PREVIEW_CHARS)" \
	RAG_ONTOLOGY_CANDIDATE_PREVIEW_CHARS="$(RAG_ONTOLOGY_CANDIDATE_PREVIEW_CHARS)" \
	RAG_MANAGER_ENABLED="$(RAG_MANAGER_ENABLED)" \
	RAG_MANAGER_CHANNEL="$(RAG_MANAGER_CHANNEL)" \
	RAG_MANAGER_MENTION="$(RAG_MANAGER_MENTION)" \
	RAG_MANAGER_INTERVAL_SECS="$(RAG_MANAGER_INTERVAL_SECS)" \
	RAG_MANAGER_API_BASE_URL="$(RAG_MANAGER_API_BASE_URL)" \
	RAG_MANAGER_API_KEY="$(RAG_MANAGER_API_KEY)" \
	RAG_MANAGER_MODEL="$(RAG_MANAGER_MODEL)" \
	RAG_MANAGER_TIMEOUT_SECS="$(RAG_MANAGER_TIMEOUT_SECS)" \
	RAG_MANAGER_MAX_ITERATIONS="$(RAG_MANAGER_MAX_ITERATIONS)" \
	RAG_MANAGER_MEMORY_SOURCE_ID="$(RAG_MANAGER_MEMORY_SOURCE_ID)" \
	RAG_MANAGER_SYSTEM_PROMPT="$$(cat $(RAG_MANAGER_SYSTEM_PROMPT_FILE) 2>/dev/null)" \
	RAG_ACP_WS_URL="$(RAG_ACP_WS_URL)" \
	RAG_ACP_WS_TOKEN="$(RAG_ACP_WS_TOKEN)" \
	RAG_CHUNK_MAX_CHARS="$(RAG_CHUNK_MAX_CHARS)" \
	RAG_CHUNK_OVERLAP_CHARS="$(RAG_CHUNK_OVERLAP_CHARS)" \
	RAG_LARGE_ITEM_THRESHOLD="$(RAG_LARGE_ITEM_THRESHOLD)" \
	RAG_CUDA_MEM_LIMIT_MB="$(RAG_CUDA_MEM_LIMIT_MB)" \
	RAG_CUDA_DEVICE_ID="$(RAG_CUDA_DEVICE_ID)" \
	RAG_MULTIMODAL_BASE_URL="$(RAG_MULTIMODAL_BASE_URL)" \
	RAG_MULTIMODAL_API_KEY="$(RAG_MULTIMODAL_API_KEY)" \
	RAG_MULTIMODAL_MODEL="$(RAG_MULTIMODAL_MODEL)" \
	RAG_MULTIMODAL_TIMEOUT_SECS="$(RAG_MULTIMODAL_TIMEOUT_SECS)" \
	RAG_UPLOAD_PATH="$(RAG_UPLOAD_PATH)" \
	RAG_MCP_ALLOWED_HOSTS="$(RAG_MCP_ALLOWED_HOSTS)" \
	cargo run --features cuda 2>&1 | tee "$(LOG_FILE)"



tail-logs:
	@test -f "$(LOG_FILE)" || { printf 'No log file at %s — run "make run" first\n' "$(LOG_FILE)"; exit 1; }
	tail -f "$(LOG_FILE)"

ontology-status:
	@sqlite3 -header -column "$(RAG_DB_PATH)" \
		"SELECT ontology_status, COUNT(*) AS count FROM items GROUP BY ontology_status ORDER BY count DESC;"

ontology-edges:
	@sqlite3 -header -column "$(RAG_DB_PATH)" \
		"SELECT relation, COUNT(*) AS count, ROUND(AVG(weight), 3) AS avg_confidence \
		 FROM graph_edges \
		 WHERE json_extract(metadata, '$$.source') = 'ontology_worker' \
		 GROUP BY relation ORDER BY count DESC;"

## CUDA image is amd64-only. On an amd64 host: plain `docker build` —
## no buildx container, no QEMU, hits the local layer cache directly.
## On non-amd64 hosts: fall back to buildx multiarch builder + registry
## cache so cross-builds stay reasonably fast.
HOST_ARCH := $(shell uname -m)
docker-build-cuda:
ifeq ($(HOST_ARCH),x86_64)
	docker build -f Dockerfile.cuda -t "$(CUDA_IMAGE_NAME)" .
else
	docker buildx build --builder multiarch --platform linux/amd64 \
		-f Dockerfile.cuda -t "$(CUDA_IMAGE_NAME)" \
		--cache-from type=registry,ref=$(CUDA_IMAGE_NAME)-cache .
endif

docker-push-cuda:
ifeq ($(HOST_ARCH),x86_64)
	docker build -f Dockerfile.cuda -t "$(CUDA_IMAGE_NAME)" .
	docker push "$(CUDA_IMAGE_NAME)"
else
	docker buildx build --builder multiarch --platform linux/amd64 \
		-f Dockerfile.cuda -t "$(CUDA_IMAGE_NAME)" --push \
		--cache-from type=registry,ref=$(CUDA_IMAGE_NAME)-cache \
		--cache-to   type=registry,ref=$(CUDA_IMAGE_NAME)-cache,mode=max .
endif

docker-run-cuda:
	mkdir -p "$(CURDIR)/data"
	docker run --rm \
		--gpus all \
		-p "$(RAG_PORT):4001" \
		-v "$(CURDIR)/data:/app/data" \
		-e RAG_AUTH_ENABLED="$(RAG_AUTH_ENABLED)" \
		-e RAG_FRONTEND_API_KEY="$(RAG_FRONTEND_API_KEY)" \
		-e AUTH_SESSION_SECRET="$(AUTH_SESSION_SECRET)" \
		-e ZITADEL_ISSUER="$(ZITADEL_ISSUER)" \
		-e ZITADEL_CLIENT_ID="$(ZITADEL_CLIENT_ID)" \
		-e ZITADEL_CLIENT_SECRET="$(ZITADEL_CLIENT_SECRET)" \
		-e ZITADEL_REDIRECT_URI="$(ZITADEL_REDIRECT_URI)" \
		-e ZITADEL_SCOPES="$(ZITADEL_SCOPES)" \
		"$(CUDA_IMAGE_NAME)"

frontend-docker-build:
	docker buildx build --platform linux/amd64,linux/arm64 -f "$(FRONTEND_DIR)/Dockerfile" -t "$(FRONTEND_IMAGE_NAME)" "$(CURDIR)"

frontend-docker-push:
	docker buildx build --platform linux/amd64,linux/arm64 -f "$(FRONTEND_DIR)/Dockerfile" -t "$(FRONTEND_IMAGE_NAME)" --push "$(CURDIR)"

frontend-docker-run:
	docker run --rm \
		-p 3000:3000 \
		-e RAG_API_URL="$(API_URL)" \
		-e RAG_AUTH_ENABLED="$(RAG_AUTH_ENABLED)" \
		-e RAG_FRONTEND_API_KEY="$(RAG_FRONTEND_API_KEY)" \
		-e APP_BASE_URL="$(APP_BASE_URL)" \
		-e AUTH_SESSION_SECRET="$(AUTH_SESSION_SECRET)" \
		-e ZITADEL_ISSUER="$(ZITADEL_ISSUER)" \
		-e ZITADEL_CLIENT_ID="$(ZITADEL_CLIENT_ID)" \
		-e ZITADEL_CLIENT_SECRET="$(ZITADEL_CLIENT_SECRET)" \
		-e ZITADEL_REDIRECT_URI="$(ZITADEL_REDIRECT_URI)" \
		-e ZITADEL_SCOPES="$(ZITADEL_SCOPES)" \
		"$(FRONTEND_IMAGE_NAME)"

frontend-install:
	cd "$(FRONTEND_DIR)" && npm install

# Run Next.js in dev mode pointed at the local backend on $(RAG_PORT).
# `RAG_AUTH_ENABLED` and `RAG_FRONTEND_API_KEY` are forwarded so the frontend
# matches the backend's auth state — set RAG_AUTH_ENABLED=false (the Makefile
# default) for local e2e runs against `make run-pg` without Zitadel; set it
# to true if you want the full prod auth flow against a Zitadel-backed run.
frontend-dev:
	cd "$(FRONTEND_DIR)" && \
		RAG_API_URL="http://127.0.0.1:$(RAG_PORT)" \
		RAG_AUTH_ENABLED="$(RAG_AUTH_ENABLED)" \
		RAG_FRONTEND_API_KEY="$(RAG_FRONTEND_API_KEY)" \
		APP_BASE_URL="$(APP_BASE_URL)" \
		AUTH_SESSION_SECRET="$(AUTH_SESSION_SECRET)" \
		ZITADEL_ISSUER="$(ZITADEL_ISSUER)" \
		ZITADEL_CLIENT_ID="$(ZITADEL_CLIENT_ID)" \
		ZITADEL_CLIENT_SECRET="$(ZITADEL_CLIENT_SECRET)" \
		ZITADEL_REDIRECT_URI="$(ZITADEL_REDIRECT_URI)" \
		ZITADEL_SCOPES="$(ZITADEL_SCOPES)" \
		npx next dev -H "$(FRONTEND_DEV_HOST)" -p "$(FRONTEND_DEV_PORT)"

# E2E shortcut: prints the two commands to run for a full local stack
# (backend + Postgres + bge-m3, frontend pointed at it, both with auth off).
e2e-local:
	@printf '%s\n' \
		'Run the backend in one terminal:' \
		'  make run-pg' \
		'' \
		'Run the frontend in another:' \
		'  make frontend-dev' \
		'' \
		'Then open http://localhost:$(FRONTEND_DEV_PORT)' \
		'(both default to RAG_AUTH_ENABLED=false; set =true if you want the Zitadel flow.)'

# Production-mode `next start` on the host (built first via `npm run build`).
frontend-prod:
	cd "$(FRONTEND_DIR)" && npm run build && \
		RAG_API_URL="http://127.0.0.1:$(RAG_PORT)" \
		APP_BASE_URL="$(APP_BASE_URL)" \
		AUTH_SESSION_SECRET="$(AUTH_SESSION_SECRET)" \
		ZITADEL_ISSUER="$(ZITADEL_ISSUER)" \
		ZITADEL_CLIENT_ID="$(ZITADEL_CLIENT_ID)" \
		ZITADEL_CLIENT_SECRET="$(ZITADEL_CLIENT_SECRET)" \
		ZITADEL_REDIRECT_URI="$(ZITADEL_REDIRECT_URI)" \
		ZITADEL_SCOPES="$(ZITADEL_SCOPES)" \
		npx next start -H "$(FRONTEND_DEV_HOST)" -p "$(FRONTEND_DEV_PORT)"

docker-push-all: docker-push-cuda frontend-docker-push

k8s-namespace:
	@test -n "$(strip $(K8S_NAMESPACE))" || { echo "K8S_NAMESPACE is empty"; exit 1; }
	kubectl get namespace "$(K8S_NAMESPACE)" >/dev/null 2>&1 || kubectl create namespace "$(K8S_NAMESPACE)"

k8s-apply-cuda:
	kubectl $(KUBECTL_NS) apply -f "$(K8S_CUDA_MANIFEST)"

k8s-delete-cuda:
	kubectl $(KUBECTL_NS) delete -f "$(K8S_CUDA_MANIFEST)"

k8s-apply-frontend:
	kubectl $(KUBECTL_NS) apply -f "$(K8S_FRONTEND_MANIFEST)"

k8s-delete-frontend:
	kubectl $(KUBECTL_NS) delete -f "$(K8S_FRONTEND_MANIFEST)"

k8s-apply-ingress:
	kubectl $(KUBECTL_NS) apply -f "$(K8S_INGRESS_MANIFEST)"
	kubectl $(KUBECTL_NS) apply -f "$(K8S_MCP_INGRESS_MANIFEST)"

k8s-delete-ingress:
	kubectl $(KUBECTL_NS) delete -f "$(K8S_MCP_INGRESS_MANIFEST)" --ignore-not-found
	kubectl $(KUBECTL_NS) delete -f "$(K8S_INGRESS_MANIFEST)"

# RuntimeClass is cluster-scoped — no namespace.
k8s-apply-runtimeclass:
	kubectl apply -f "$(K8S_RUNTIMECLASS_MANIFEST)"

k8s-delete-runtimeclass:
	kubectl delete -f "$(K8S_RUNTIMECLASS_MANIFEST)"

k8s-apply-nvidia-plugin:
	kubectl apply -f "$(K8S_NVIDIA_PLUGIN_MANIFEST)"

k8s-delete-nvidia-plugin:
	kubectl delete -f "$(K8S_NVIDIA_PLUGIN_MANIFEST)"

k8s-apply-all: k8s-apply-cuda k8s-apply-frontend

k8s-delete-all: k8s-delete-frontend k8s-delete-cuda

# Rolling restart — picks up the new `:cuda` / `:latest` image after a push.
rollout-cuda:
	kubectl $(KUBECTL_NS) rollout restart deployment/$(K8S_CUDA_DEPLOYMENT)
	kubectl $(KUBECTL_NS) rollout status  deployment/$(K8S_CUDA_DEPLOYMENT) --timeout=5m

rollout-frontend:
	kubectl $(KUBECTL_NS) rollout restart deployment/$(K8S_FRONTEND_DEPLOYMENT)
	kubectl $(KUBECTL_NS) rollout status  deployment/$(K8S_FRONTEND_DEPLOYMENT) --timeout=3m

rollout: rollout-cuda rollout-frontend

rollout-status:
	kubectl $(KUBECTL_NS) rollout status deployment/$(K8S_CUDA_DEPLOYMENT) --timeout=5m
	kubectl $(KUBECTL_NS) rollout status deployment/$(K8S_FRONTEND_DEPLOYMENT) --timeout=3m

push-and-rollout: docker-push-all rollout



store-knowledge:
	curl -sS -X POST "$(API_URL)/store" \
		-H 'content-type: application/json' \
		-d '{ \
			"id": "doc-knowledge-1", \
			"text": "Rust is a systems programming language focused on safety and performance.", \
			"metadata": { "topic": "rust", "kind": "reference" }, \
			"source_id": "knowledge" \
		}' | cat
	@printf '\n'

store-memory:
	curl -sS -X POST "$(API_URL)/store" \
		-H 'content-type: application/json' \
		-d '{ \
			"id": "doc-memory-1", \
			"text": "The user prefers concise responses when reviewing API changes.", \
			"metadata": { "user": "mats", "kind": "preference" }, \
			"source_id": "memory" \
		}' | cat
	@printf '\n'

search-knowledge:
	curl -sS -X POST "$(API_URL)/search" \
		-H 'content-type: application/json' \
		-d '{ \
			"query": "Rust safety", \
			"top_k": 5, \
			"source_id": "knowledge" \
		}' | cat
	@printf '\n'

search-memory:
	curl -sS -X POST "$(API_URL)/search" \
		-H 'content-type: application/json' \
		-d '{ \
			"query": "response preferences", \
			"top_k": 5, \
			"source_id": "memory" \
		}' | cat
	@printf '\n'

admin-categories:
	curl -sS "$(API_URL)/admin/categories" | cat
	@printf '\n'

admin-items:
	curl -sS "$(API_URL)/admin/items$(if $(SOURCE_ID),?source_id=$(SOURCE_ID),)" | cat
	@printf '\n'

graph-status:
	curl -sS "$(API_URL)/graph/status" | cat
	@printf '\n'

graph-rebuild:
	curl -sS -X POST "$(API_URL)/admin/graph/rebuild" | cat
	@printf '\n'

graph-neighborhood:
	curl -sS "$(API_URL)/graph/neighborhood/$(if $(ITEM_ID),$(ITEM_ID),doc-memory-1)?depth=1&limit=20" | cat
	@printf '\n'

smoke: store-knowledge store-memory search-knowledge search-memory admin-categories admin-items graph-status

http-files:
	@printf '%s\n' \
		'http/admin.http' \
		'http/graph.http' \
		'http/rag.http' \
		'http/memory.http'

mcp-inspector-local:
	npx -y @modelcontextprotocol/inspector --transport http --server-url "http://127.0.0.1:$(RAG_PORT)/mcp" $(if $(RAG_MCP_AUTH_BEARER),--header "Authorization: Bearer $(RAG_MCP_AUTH_BEARER)",)

mcp-inspector-hosted:
	npx -y @modelcontextprotocol/inspector --transport http --server-url "https://rag.k6n.net/mcp" $(if $(RAG_MCP_AUTH_BEARER),--header "Authorization: Bearer $(RAG_MCP_AUTH_BEARER)",)
