---
name: rag-usage
description: Guidelines for code agents on how to use the rust-rag retrieval store for semantic search and long-term memory. Use this skill when you need to decide whether to search for existing context or store new information in the RAG system.
---

# RAG Usage Guidelines

## Overview
The `rust-rag` system serves as a long-term memory and knowledge base for code agents. Efficient usage involves proactive retrieval to avoid redundant work and strategic storage to preserve high-value insights. 

**Mandatory Tooling**: Always use the connected MCP tools (`store_entry`, `search_entries`, `get_entry`, etc.) to interact with the RAG. Do not attempt to make direct HTTP calls or store project knowledge in local files unless explicitly requested.

## When to Search
Search the RAG whenever you encounter:
- **Project Context**: Vague requests about "how things work" or "existing patterns".
- **Previous Decisions**: Questions about why a specific architecture or library was chosen.
- **Redundant Tasks**: Before implementing a common utility or fixing a bug that might have been addressed before.
- **Onboarding**: When starting a task in an unfamiliar part of the codebase.

### Search Patterns
- Use `search_entries` with a broad query first.
- If you know the category, filter by `source_id` (e.g., `knowledge` for docs, `memory` for project history).
- Use `get_entry` to retrieve full text once a relevant item is identified.

## When to Store
Store information when you produce high-value output that should be remembered:
- **Architectural Decisions**: Summary of why a specific design was chosen.
- **Bug Root Causes**: Deep dives into complex bugs and their eventual fixes.
- **Completed Task Summaries**: A concise record of what was done in a multi-step session.
- **Reusable Snippets**: Complex configuration or scripts that might be useful later.

### Storage Conventions
Use consistent `source_id` namespaces:
- `knowledge`: Permanent documentation, architecture diagrams, domain knowledge.
- `memory`: Project-specific history, task summaries, bug reports.
- `notes`: Temporary snippets, personal observations, raw research data.

## Best Practices
- **Summarize Before Storing**: Don't dump raw logs or entire files. Store concise, semantic summaries that are easy to retrieve.
- **Use Metadata**: Include relevant context like file paths, ticket IDs, or related component names in the metadata.
- **Link Related Items**: Use graph tools (`create_manual_edge`) to link related entries (e.g., a bug report linked to its fix).
- **Quality Over Quantity**: Focus on "searchable" content. If it's not something you'd search for later, it probably doesn't need to be in the RAG.
