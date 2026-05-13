# rag — rust-rag CLI

Entry/attachment tool for a `rust-rag` instance. Scope: store/search/list/browse entries (free-text + typed), URL ingestion, multimodal image ingest, attachments, analysis, graph discovery, schema discovery.

ACP/messaging surface was removed — use the MCP server or HTTP API directly for those.

## Build

```bash
cd go-cli
go build -o rag .
sudo mv rag /usr/local/bin/   # optional
```

## Auth

Device-code flow:

```bash
rag login
```

Token stored in `~/.config/rust-rag/config.json`.

## Command tree

```
rag login
rag entry store [text]            # -s source, -m metadata, --type, --data, --path, --tags, --title
rag entry smart [text]            # LLM splits text + assigns source_id/metadata (--url, --title, --model)
rag entry get <id>
rag entry update <id>             # --text/--metadata/--source/--path/--type/--data
rag entry delete <id>
rag entry list                    # -s source, --path-prefix, --type, -n limit
rag entry browse                  # -s source, --prefix
rag entry related <id>            # graph neighborhood (--depth, --limit, --edge-type)
rag entry analyze <id|->          # LLM analysis (id, or `-` for stdin text)
rag entry image <path>            # multimodal image ingest

rag search <query>                # -k, -s, --type, --rerank/--no-rerank, --show-related
rag ingest <url>                  # -s, --path, --type, --cdp, --llm-clean
rag sources

rag schema list
rag schema get <type>

rag edge list                     # --item, --type
rag edge add <from> <to> <predicate>  # --weight, --directed

rag attach add <entry_id> <url>
rag attach list <entry_id>
rag attach rm <attachment_id>

rag dream
rag health
```

Global flags:

- `--api-url`  base URL (default `http://localhost:4001`)
- `--config`   alt config file
- `--json`     emit raw JSON instead of pretty output

## Examples

```bash
rag entry store "fact about caching" -s knowledge --tags rag,caching
cat doc.md | rag entry store -s documents --path docs/intro --title "Intro"
rag entry list -s project:rust-rag:todos -n 5
rag search "vector store sqlite" -k 3 --rerank --show-related
rag ingest https://example.com/article -s web --llm-clean
rag entry image ./screenshot.png -s screenshots
rag entry analyze go_cli_roadmap_v1
rag edge add a_id b_id refines --weight 0.8
rag attach add entry_id https://example.com/file.pdf
```
