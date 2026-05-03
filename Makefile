SHELL := /bin/sh
MODEL_DIR := $(CURDIR)/assets/bge-small-en-v1.5
MODEL_URL := https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/onnx/model.onnx
TOKENIZER_URL := https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/tokenizer.json

RAG_MODEL_PATH ?= $(MODEL_DIR)/model.onnx
RAG_TOKENIZER_PATH ?= $(MODEL_DIR)/tokenizer.json
RAG_DB_PATH ?= $(CURDIR)/data/rag.db
RAG_PORT ?= 4001
RAG_GRAPH_ENABLED ?= true
RAG_GRAPH_BUILD_ON_STARTUP ?= true
RAG_GRAPH_K ?= 5
RAG_GRAPH_MAX_DISTANCE ?= 0.75
RAG_GRAPH_CROSS_SOURCE ?= false
RAG_AUTH_ENABLED ?= false
RAG_FRONTEND_API_KEY ?= replace-with-shared-frontend-backend-key
RAG_MCP_AUTH_BEARER ?=
RAG_MCP_ALLOWED_HOSTS ?= localhost,127.0.0.1,::1,rag.k6n.net
AUTH_SESSION_SECRET ?= replace-with-a-long-random-secret
RAG_OPENAI_API_BASE_URL ?= http://127.0.0.1:8081/v1
RAG_OPENAI_API_KEY ?=
RAG_OPENAI_MODEL ?= current_model.gguf
RAG_OPENAI_TIMEOUT_SECS ?= 60
RAG_MULTIMODAL_BASE_URL ?= http://10.10.10.207:11434/v1
RAG_MULTIMODAL_API_KEY ?=
RAG_MULTIMODAL_MODEL ?= qwen3.5:9b
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

# Chunking — tune to your embedding model's context window.
# RAG_LARGE_ITEM_THRESHOLD defaults to RAG_CHUNK_MAX_CHARS when unset.
RAG_CHUNK_MAX_CHARS ?= 1536
RAG_CHUNK_OVERLAP_CHARS ?= 200
RAG_LARGE_ITEM_THRESHOLD ?= $(RAG_CHUNK_MAX_CHARS)

# Logging — set to rust_rag::ontology=debug to see per-item LLM calls and raw responses
RUST_LOG ?= rust_rag=info
LOG_DIR ?= $(CURDIR)/data/logs
LOG_FILE ?= $(LOG_DIR)/rust-rag.log
RAG_CUDA_MEM_LIMIT_MB ?= 2048
RAG_CUDA_DEVICE_ID ?= 0
API_URL ?= https://127.0.0.1:$(RAG_PORT)
RAG_CDP_URL ?= ws://127.0.0.1:9222
ZITADEL_ISSUER ?= https://auth.k6n.net
ZITADEL_CLIENT_ID ?= 369530153681881434@rag
ZITADEL_CLIENT_SECRET ?= U8opCVRn3hrFNcyXDJpb7DLQAa5aHEikjuQn2Rr5KwG7RiofvzKifxdTB3yEO0ID
ZITADEL_REDIRECT_URI ?= https://rag.k6n.net/auth/callback
ZITADEL_SCOPES ?= openid profile email
IMAGE_NAME ?= matst80/rust-rag:latest
FRONTEND_IMAGE_NAME ?= matst80/rust-rag-frontend:latest
FRONTEND_DIR ?= $(CURDIR)/frontend
APP_BASE_URL ?= http://localhost:3000
K8S_MANIFEST ?= deploy/kubernetes/rust-rag.yaml
K8S_FRONTEND_MANIFEST ?= deploy/kubernetes/rust-rag-frontend.yaml
K8S_FRONTEND_HOST_MANIFEST ?= deploy/kubernetes/rust-rag-frontend-host.yaml
K8S_INGRESS_MANIFEST ?= deploy/kubernetes/rust-rag-ingress.yaml
FRONTEND_DEV_PORT ?= 3000
FRONTEND_DEV_HOST ?= 0.0.0.0
K8S_NAMESPACE ?= home
KUBECTL_NS := $(if $(strip $(K8S_NAMESPACE)),-n $(K8S_NAMESPACE))
MCP_STDIO_TAG_PREFIX ?= mcp-stdio-v

.PHONY: help fetch-assets print-env fmt test verify check-env build build-cuda build-mcp run run-cuda run-mcp tail-logs ontology-status ontology-edges docker-build docker-push docker-run frontend-docker-build frontend-docker-push frontend-docker-run frontend-install frontend-dev frontend-prod docker-build-all docker-push-all k8s-namespace k8s-apply k8s-delete k8s-apply-frontend k8s-delete-frontend k8s-apply-frontend-host k8s-delete-frontend-host k8s-apply-ingress k8s-delete-ingress k8s-apply-all k8s-delete-all tag-mcp-stdio store-knowledge store-memory search-knowledge search-memory admin-categories admin-items graph-status graph-rebuild graph-neighborhood smoke http-files

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
		'  make build-mcp        Build the mcp-stdio binary in release mode' \
		'  make run              Start the service locally' \
		'  make run-cuda         Start the service locally with the cuda feature enabled' \
		'  make run-mcp          Start the stdio MCP bridge locally' \
		'  make docker-build     Build the server container image' \
		'  make docker-push      Push the server container image' \
		'  make docker-run       Run the server container with the local data directory mounted' \
		'  make frontend-install      Install frontend npm dependencies' \
		'  make frontend-dev          Run Next.js dev on $(FRONTEND_DEV_HOST):$(FRONTEND_DEV_PORT) (host-shim mode)' \
		'  make frontend-prod         Build and start Next.js on $(FRONTEND_DEV_HOST):$(FRONTEND_DEV_PORT)' \
		'  make frontend-docker-build Build the Next.js frontend container image' \
		'  make frontend-docker-push  Push the Next.js frontend container image' \
		'  make frontend-docker-run   Run the frontend container (override RAG_API_URL as needed)' \
		'  make docker-build-all Build both server and frontend images' \
		'  make docker-push-all  Push both server and frontend images' \
		'  make k8s-apply        Apply the Kubernetes manifest in deploy/kubernetes' \
		'  make k8s-delete       Delete the Kubernetes manifest in deploy/kubernetes' \
		'  make k8s-apply-frontend       Apply the in-cluster frontend Deployment' \
		'  make k8s-delete-frontend      Delete the in-cluster frontend Deployment' \
		'  make k8s-apply-frontend-host  Apply the host-shim Service `rag-frontend` -> 10.10.11.135:3000' \
		'  make k8s-delete-frontend-host Delete the host-shim Service' \
		'  make k8s-apply-ingress        Apply the Ingress (routes / to rag-frontend)' \
		'  make k8s-delete-ingress       Delete the Ingress' \
		'  make k8s-apply-all    Apply both server and frontend manifests' \
		'  make k8s-delete-all   Delete both server and frontend manifests' \
		'  make k8s-namespace    Create K8S_NAMESPACE (currently: $(K8S_NAMESPACE)) if it does not exist' \
		'  (override target namespace with K8S_NAMESPACE=<name>)' \
		'  make tag-mcp-stdio    Create an annotated release tag (set VERSION=0.1.0)' \
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
		'  make http-files       List the .http request files'

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

build-mcp:
	cargo build --release --manifest-path mcp-stdio/Cargo.toml

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

run-mcp:
	RAG_MCP_API_BASE_URL="$(API_URL)" \
	RAG_MCP_AUTH_BEARER="$(RAG_MCP_AUTH_BEARER)" \
	cargo run --manifest-path mcp-stdio/Cargo.toml

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

docker-build:
	docker build -t "$(IMAGE_NAME)" .

docker-push:
	docker push "$(IMAGE_NAME)"

docker-run:
	mkdir -p "$(CURDIR)/data"
	docker run --rm \
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
		"$(IMAGE_NAME)"

frontend-docker-build:
	docker build -t "$(FRONTEND_IMAGE_NAME)" "$(FRONTEND_DIR)"

frontend-docker-push:
	docker push "$(FRONTEND_IMAGE_NAME)"

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

# Run Next.js in dev mode bound to 0.0.0.0 so the in-cluster `rust-rag-frontend`
# selector-less Service (Endpoints -> 10.10.11.135:3000) can reach it. Hot reload
# replaces the Docker rebuild + image push loop for frontend changes.
frontend-dev:
	cd "$(FRONTEND_DIR)" && \
		RAG_API_URL="http://127.0.0.1:$(RAG_PORT)" \
		APP_BASE_URL="$(APP_BASE_URL)" \
		AUTH_SESSION_SECRET="$(AUTH_SESSION_SECRET)" \
		ZITADEL_ISSUER="$(ZITADEL_ISSUER)" \
		ZITADEL_CLIENT_ID="$(ZITADEL_CLIENT_ID)" \
		ZITADEL_CLIENT_SECRET="$(ZITADEL_CLIENT_SECRET)" \
		ZITADEL_REDIRECT_URI="$(ZITADEL_REDIRECT_URI)" \
		ZITADEL_SCOPES="$(ZITADEL_SCOPES)" \
		npx next dev -H "$(FRONTEND_DEV_HOST)" -p "$(FRONTEND_DEV_PORT)"

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

docker-build-all: docker-build frontend-docker-build

docker-push-all: docker-push frontend-docker-push

k8s-namespace:
	@test -n "$(strip $(K8S_NAMESPACE))" || { echo "K8S_NAMESPACE is empty"; exit 1; }
	kubectl get namespace "$(K8S_NAMESPACE)" >/dev/null 2>&1 || kubectl create namespace "$(K8S_NAMESPACE)"

k8s-apply:
	kubectl $(KUBECTL_NS) apply -f "$(K8S_MANIFEST)"

k8s-delete:
	kubectl $(KUBECTL_NS) delete -f "$(K8S_MANIFEST)"

k8s-apply-frontend:
	kubectl $(KUBECTL_NS) apply -f "$(K8S_FRONTEND_MANIFEST)"

k8s-delete-frontend:
	kubectl $(KUBECTL_NS) delete -f "$(K8S_FRONTEND_MANIFEST)"

# Host-shim: selector-less Service `rag-frontend` + Endpoints -> 10.10.11.135:3000.
# Co-exists with the in-cluster Deployment (Service `rust-rag-frontend`). The
# Ingress routes `/` to `rag-frontend`; in-cluster Deployment stays as fallback.
k8s-apply-frontend-host:
	kubectl $(KUBECTL_NS) apply -f "$(K8S_FRONTEND_HOST_MANIFEST)"

k8s-delete-frontend-host:
	kubectl $(KUBECTL_NS) delete -f "$(K8S_FRONTEND_HOST_MANIFEST)"

k8s-apply-ingress:
	kubectl $(KUBECTL_NS) apply -f "$(K8S_INGRESS_MANIFEST)"

k8s-delete-ingress:
	kubectl $(KUBECTL_NS) delete -f "$(K8S_INGRESS_MANIFEST)"

k8s-apply-all: k8s-apply k8s-apply-frontend

k8s-delete-all: k8s-delete-frontend k8s-delete

tag-mcp-stdio:
	@test -n "$(VERSION)" || { echo "usage: make tag-mcp-stdio VERSION=0.1.0"; exit 1; }
	@git diff --quiet || { echo "working tree has unstaged changes"; exit 1; }
	@git diff --cached --quiet || { echo "working tree has staged but uncommitted changes"; exit 1; }
	@git rev-parse "$(MCP_STDIO_TAG_PREFIX)$(VERSION)" >/dev/null 2>&1 && { echo "tag $(MCP_STDIO_TAG_PREFIX)$(VERSION) already exists"; exit 1; } || true
	git tag -a "$(MCP_STDIO_TAG_PREFIX)$(VERSION)" -m "mcp-stdio $(VERSION)"
	@printf '%s\n' "created tag $(MCP_STDIO_TAG_PREFIX)$(VERSION)"
	@printf '%s\n' "push it with: git push origin $(MCP_STDIO_TAG_PREFIX)$(VERSION)"

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
