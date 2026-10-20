# Agent notes — `cs` code search in this repo

Prefer the `cs` CLI for code navigation (index covers the working tree,
including uncommitted edits):

- `cs query <2-4 keywords>` — find definitions by concept; rows are
  `path#Name` selectors for `cs show`.
- `cs show <path#Name|path:line>` — read a symbol (doc + body), not the file.
- `cs outline <path>` — symbol map of a file or directory.
- `cs callers <symbol>` — all AST call sites of a function/method.
- `cs grep <pattern>` — regex search, `path:line:text` rows (pipes to `cs show -`).
- `cs map [dir]` — per-package purpose/size/key symbols for orientation.

Known-weak for Rust repos right now (being improved — your feedback helps):
`tree` (callees bleed across languages), `def` (no `crate::`/`super::`
resolution), `tests` (misses `#[test]` call sites `callers` finds),
`related` (rejects `path#Name` selectors), `dead` (vendored JS noise),
`affected` (relative-path lookup fails). If one of these misfires on Rust
code, note the command + symbol in your session report so it becomes a
tool ticket.

# Self-improvement loop — `rag` CLI (go-cli/)

Build it (`cd go-cli && go build -o rag .`) and use it against the running
system to verify your own changes end to end — a green `cargo test` alone
is not proof the behavior works:

- `rag health`, `rag sources` — is the API up, what namespaces exist.
- `rag search <query> -s <source> -k 5` — does retrieval return the change.
- `rag entry list -s memory --min-importance 4` — what the system remembers.
- `rag entry dups -s memory` — near-duplicate scan (shingle + token
  fallback); merge keepers with `entry update`, remove subsumpions with
  `entry delete-many`, never anything unreviewed.
- `rag dream` / `rag dream apply` (plan first, `--apply` only after review),
  `rag dream clusters|actions` — consolidation behavior.

Document important changes as entries so future sessions inherit them:
store decisions/specs/facts that are not derivable from `git log` into
`project:rust-rag:knowledge` (use `--path` + `--tags`), and file follow-up
work as `dev-instance board` tickets on `matst80/rust-rag`. Calibrate
`importance` honestly (1-5): one-line notes are not 5s, test debris is not
a 4. The system learns from what you keep — curate it like the corpus it is.
