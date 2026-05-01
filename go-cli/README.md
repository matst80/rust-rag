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

You can also set the current message channel as a global flag for commands that operate on a channel:

```bash
rag --channel ops msg send "deploy started"
rag --channel ops msg history --limit 20
rag --channel ops acp gemini --name ops-bot
```

If `--channel` is not set, the CLI falls back to the configured `channel` value. If
that is also unset, it derives a default channel from the current directory name.

```bash
cd ~/src/rust-rag
rag msg send "working here"   # sends to #rust-rag
```
