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
API_URL ?= http://127.0.0.1:$(RAG_PORT)
IMAGE_NAME ?= rust-rag:latest
K8S_MANIFEST ?= deploy/kubernetes/rust-rag.yaml
MCP_STDIO_TAG_PREFIX ?= mcp-stdio-v

.PHONY: help fetch-assets print-env fmt test verify check-env run run-mcp docker-build docker-run k8s-apply k8s-delete tag-mcp-stdio store-knowledge store-memory search-knowledge search-memory admin-categories admin-items graph-status graph-rebuild graph-neighborhood smoke http-files

help:
	@printf '%s\n' \
		'Targets:' \
		'  make fetch-assets     Download the default ONNX model and tokenizer' \
		'  make print-env        Print exact export commands for local runs' \
		'  make fmt              Format the Rust codebase' \
		'  make test             Run the automated test suite' \
		'  make verify           Run formatting check and tests' \
		'  make check-env        Verify required runtime env vars are set' \
		'  make run              Start the service locally' \
		'  make run-mcp          Start the stdio MCP bridge locally' \
		'  make docker-build     Build the server container image' \
		'  make docker-run       Run the server container with the local data directory mounted' \
		'  make k8s-apply        Apply the Kubernetes manifest in deploy/kubernetes' \
		'  make k8s-delete       Delete the Kubernetes manifest in deploy/kubernetes' \
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
	mkdir -p "$(MODEL_DIR)" "$(CURDIR)/data"
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
		"export RAG_GRAPH_CROSS_SOURCE=$(RAG_GRAPH_CROSS_SOURCE)"

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

run: check-env
	RAG_MODEL_PATH="$(RAG_MODEL_PATH)" \
	RAG_TOKENIZER_PATH="$(RAG_TOKENIZER_PATH)" \
	RAG_DB_PATH="$(RAG_DB_PATH)" \
	RAG_PORT="$(RAG_PORT)" \
	RAG_GRAPH_ENABLED="$(RAG_GRAPH_ENABLED)" \
	RAG_GRAPH_BUILD_ON_STARTUP="$(RAG_GRAPH_BUILD_ON_STARTUP)" \
	RAG_GRAPH_K="$(RAG_GRAPH_K)" \
	RAG_GRAPH_MAX_DISTANCE="$(RAG_GRAPH_MAX_DISTANCE)" \
	RAG_GRAPH_CROSS_SOURCE="$(RAG_GRAPH_CROSS_SOURCE)" \
	cargo run

run-mcp:
	RAG_MCP_API_BASE_URL="$(API_URL)" \
	cargo run -p mcp-stdio

docker-build:
	docker build -t "$(IMAGE_NAME)" .

docker-run:
	mkdir -p "$(CURDIR)/data"
	docker run --rm \
		-p "$(RAG_PORT):4001" \
		-v "$(CURDIR)/data:/app/data" \
		"$(IMAGE_NAME)"

k8s-apply:
	kubectl apply -f "$(K8S_MANIFEST)"

k8s-delete:
	kubectl delete -f "$(K8S_MANIFEST)"

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
