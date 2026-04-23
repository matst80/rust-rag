# RAG CLI

A simple Go CLI tool to interact with your `rust-rag` instance.

## Installation

```bash
cd go-cli
go build -o rag .
sudo mv rag /usr/local/bin/ # optional
```

## Usage

### Login

Uses the OIDC-like device flow. It will give you a link to visit in your browser to approve the CLI session.

```bash
rag login
```

### Store Data

Store text directly or pipe data from other tools.

```bash
# Direct text
rag store "This is a fact about RAG systems."

# Pipe data (useful for logs, exports, etc)
cat document.txt | rag store --source documents

# Advanced: metadata and source
rag store "Task for Mats" --source tasks --metadata '{"priority": "high", "todo": "mats"}'
```

### Search

```bash
rag search "What are the benefits of RAG?" --limit 3
```

### List Recent Items

```bash
rag list --n 20 --source tasks
```

## Configuration

Configuration is stored in `~/.config/rust-rag/config.json`. You can override the API URL:

```bash
rag --api-url https://my-rag-instance.com list
```
