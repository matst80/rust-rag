# ACP frontend WebSocket implementation

This document describes how the Next.js ACP page connects to Telegram-ACP
daemons through the Rust backend, how messages are routed into the UI, and how
shared terminals and remote file previews are implemented.

## Scope and architecture

The ACP frontend is a browser client for the Telegram-ACP WebSocket protocol.
The browser does **not** connect directly to a discovered daemon. The path is:

```text
Browser (Next.js ACP page)
  │
  │ same-origin WebSocket: /api/acp/ws?instance=<instance_id>
  ▼
Rust Axum API: browser-facing ACP proxy
  │
  │ one long-lived upstream WebSocket per registered ACP instance
  ▼
Telegram-ACP daemon
```

The Rust server owns daemon discovery, daemon credentials, upstream
connections, cached state, and fan-out. One `AcpWsHandle` is maintained per ACP
instance. Multiple browser tabs can subscribe to the same handle, so terminal
output and session events are shared without opening one daemon connection per
browser tab.

The frontend implementation is primarily in:

- `frontend/components/acp/use-acp-socket.ts` — connection lifecycle, message
  dispatch, state, commands, reconnect, and file-browser requests.
- `frontend/components/acp/terminal-view.tsx` — xterm instance, terminal input,
  resize handling, and terminal output rendering.
- `frontend/components/acp/agent-terminal.tsx` — terminal tabs and terminal
  controls.
- `frontend/components/acp/file-browser.tsx` — remote directory/file results
  and the read-only CodeMirror preview.
- `frontend/components/acp/agent-chat.tsx` — wires socket commands to the ACP
  workspace UI.
- `src/acp_ws.rs` — upstream ACP worker, state cache, and broadcast fan-out.
- `src/api/mod.rs` — browser WebSocket proxy at `/api/acp/ws`.

## Authentication and instance selection

The browser uses the same origin as the frontend. It constructs the WebSocket
URL from `window.location`:

```text
https://app.example/api/acp/ws?instance=<url-encoded-instance-id>
```

or, when only one ACP instance is available:

```text
https://app.example/api/acp/ws
```

The scheme is translated automatically (`https` → `wss`, otherwise `ws`). The
browser does not receive or expose the daemon's bearer token. Authentication is
handled by the application/session and API middleware around the same-origin
route; the Rust proxy uses the server-side ACP handle selected by the
`instance` query parameter.

Instance discovery and selection use authenticated Next.js BFF routes:

| Browser route | Backend route | Purpose |
|---|---|---|
| `GET /bff/acp/instances` | `GET /api/acp/instances` | List discovered instances and worker health. |
| `POST /bff/acp/select` | `POST /api/acp/select` | Select the active daemon for server-side discovery/manager use. |
| `GET /bff/acp/config` | — | Legacy/configuration probe; returns the same-origin WebSocket route. |

When multiple instances are registered, the frontend requires an explicit
selection before it connects. The Rust proxy rejects an omitted `instance`
parameter in that case. Unknown instances and an empty registry produce a
client-visible API error instead of silently connecting to the wrong host.

## Connection lifecycle

`useAcpSocket` owns one browser WebSocket for the currently selected instance.
The lifecycle is:

1. Refresh `/bff/acp/instances` and inspect the registered workers.
2. If there is exactly one worker, use it implicitly; otherwise wait for an
   explicit instance selection.
3. Close the previous browser socket when changing instance.
4. Open `/api/acp/ws` with the selected instance query parameter.
5. On `open`, mark the connection open and flush terminal text buffered during
   the reconnect window.
6. Parse each text frame as JSON and normalize its envelope.
7. On `close`, mark the connection closed and reconnect with exponential
   backoff: 1 second initially, doubling up to 30 seconds.
8. Refresh the instance list every 10 seconds while the page is mounted.

The frontend accepts both protocol envelope forms:

```json
{"type":"terminal_output", "terminal_id":"t1", "data":"..."}
```

and the tagged-object form used by some ACP daemon versions:

```json
{"TerminalOutput":{"terminal_id":"t1", "data":"..."}}
```

`envelopeKind()` in `frontend/components/acp/utils.ts` converts both forms to
an internal `{ kind, payload }` pair. Event names are compared
case-insensitively in the socket hook.

The Rust proxy also sends a WebSocket ping approximately every 30 seconds.
The upstream ACP worker reconnects independently when its daemon connection
fails, using the configured ACP reconnect bounds.

## Server-to-browser message flow

The Rust upstream worker receives daemon text frames and:

1. Parses the JSON envelope.
2. Assigns a local monotonically increasing sequence number.
3. Updates cached session, project, terminal, and permission state where the
   event contains state that can be reconstructed.
4. Stores session events in bounded per-session buffers.
5. Publishes the original text frame through a Tokio broadcast channel.

A newly connected browser first receives a cached or synthesized snapshot, then
live frames from the broadcast channel. The worker requests `list_sessions`
when an upstream connection starts and when a browser proxy is attached.

The worker emits a `state_snapshot` approximately every 15 seconds when it has
cached sessions, projects, or terminals. This gives browser clients a way to
recover metadata after a reconnect or broadcast lag. The frontend replaces its
session/terminal maps from snapshots and dispatches
`acp:term:resync:<terminal_id>` events so mounted xterm views re-attach and
request a fresh terminal screen snapshot.

### Frontend state mapping

| Incoming event | Frontend behavior |
|---|---|
| `snapshot` / `state_snapshot` | Rebuild sessions, terminals, session-terminal associations, projects, and history. Re-attach mounted terminals. |
| `session_started` | Add/update the session and make it active. |
| `session_ended` / `session_removed` | Remove the session and its event history; clear related terminal selections. |
| `agent_update` | Update session status and append the event to the active session history. Chat blocks are built from this event stream. |
| `user_prompt` | Append a user message event to the session history. |
| `permission_request` | Add the request to the pending-permissions map. |
| `permission_resolved` | Remove the resolved request. |
| `terminal_created` | Add terminal metadata, associate it with a session when possible, and switch that session to terminal mode. |
| `terminal_closed` | Remove terminal metadata and clear any active selection pointing to it. |
| `terminal_output` | Dispatch a window event scoped to the terminal ID; the xterm view writes the decoded bytes directly. |
| `terminal_snapshot` | Dispatch a terminal-scoped snapshot event; the xterm resets and writes the decoded screen contents. |
| `terminal_resized` | Dispatch the new dimensions to the matching xterm view. |
| `directory_suggestions` | Replace directory results for the active host. |
| `find_files_result` | Replace file results for the active host. |
| `read_file_result` | Store the selected path and content for the active host's preview. |
| `error` | Clear the active file-browser request and show the daemon error in the file browser. |

The session event stream is bounded on the backend. The browser additionally
keeps terminal output out of React state, using terminal-scoped `CustomEvent`s
to avoid re-rendering the entire ACP page for every output frame.

## Client-to-server commands

The socket hook exposes a generic `send()` function for ACP commands. Commands
are JSON text frames and are forwarded unchanged by the Rust proxy to the
selected upstream daemon.

### Session and agent commands

The current UI sends these commands as needed:

- `send_prompt` — send text to a session.
- `cancel` — interrupt the active session.
- `spawn` — start a new ACP agent session.
- `end_session` — terminate a session.
- `permission_response` — answer a pending permission request.
- `bind_telegram_thread` — bind a session to a Telegram topic.
- `set_config_option`, `set_permission_mode`, and `execute_command` — available
  through the generic ACP command path when a session exposes them.

### Terminal commands

Terminal commands use the protocol's terminal identifiers and are shared by all
browser subscribers to the same ACP instance:

```json
{"type":"create_terminal","session_id":"...","cwd":"/work","cols":120,"rows":24}
{"type":"attach_terminal","terminal_id":"...","cols":120,"rows":24}
{"type":"terminal_input","terminal_id":"...","data":"<base64>"}
{"type":"terminal_resize","terminal_id":"...","cols":120,"rows":32}
{"type":"close_terminal","terminal_id":"..."}
```

`TerminalView` receives raw text from xterm's `onData` callback. The socket
hook batches text for 12 ms and base64-encodes the combined UTF-8 bytes before
sending one `terminal_input` frame. This reduces frame overhead for typing and
paste operations. Text entered while the socket is reconnecting remains in a
per-terminal buffer and is flushed on the next `open` event. Buffered text is
discarded when the user deliberately switches ACP instances so input cannot be
sent to a different daemon.

Terminal resize is driven by xterm's `FitAddon`, a `ResizeObserver`, the
browser `resize` event, and `visualViewport` changes. This covers desktop
resizing, mobile orientation changes, and soft-keyboard viewport changes. A
terminal attach is sent with the current proposed dimensions; the daemon then
returns a `terminal_snapshot` that restores the remote screen.

On macOS, xterm is configured with `macOptionIsMeta: false`. The custom key
handler also maps the US-layout `Option+2` chord to a literal `@` before it can
be interpreted as an Alt/Meta escape sequence.

Closing a terminal tab passes the tab's terminal ID to `onClose`, so a tab
cannot accidentally close the currently active terminal when another tab's
close button is pressed. Note that `close_terminal` is still a daemon-level
termination command, not a detach-only operation; closing it ends the shared
PTY for other subscribers as well.

## Host-scoped remote files and CodeMirror

The ACP protocol supports lightweight remote browsing with three request types:

```json
{"type":"list_directories","query":"/work/src","session_id":null}
{"type":"find_files","query":"*.rs","start_directory":"/work","session_id":null}
{"type":"read_file","path":"/work/src/main.rs","start_line":1,"line_count":400,"session_id":null}
```

The frontend exposes these through the **Remote files** button in the ACP
header. Results are deliberately scoped to the selected ACP host. The host key
is the selected instance ID, or the sole worker ID when only one worker exists.
This means switching hosts does not mix directory suggestions, file search
results, or an open preview from another machine.

The file browser is a right-side drawer with:

1. A host label and close control.
2. A directory path field and `Browse` action.
3. A file query field and `Find` action.
4. A scrollable directory/file result list.
5. A read-only CodeMirror pane for the selected file.

The initial directory comes from the active standalone terminal's `cwd`, the
active session's project path, or `/` as a fallback. Clicking a directory
issues another `list_directories` request. Clicking a file issues `read_file`
and displays the returned slice with its line metadata.

CodeMirror language extensions currently cover JavaScript, JSX, TypeScript,
TSX, JSON, Markdown/MDX, HTML, CSS, Python, and Rust. Unknown extensions are
shown as plain text. The preview is read-only; editing and writing files are
not part of this WebSocket integration.

File browsing sends `session_id: null`, so it is host-scoped rather than tied
to a particular ACP agent session. The daemon remains responsible for path
validation, access control, fuzzy matching, and file-size/line-slice limits.

## Failure and recovery behavior

- **No workers:** the UI enters a disabled state and displays that no ACP
  instances are registered.
- **Several workers without a selection:** the UI remains disabled until a
  target is selected.
- **Unknown instance:** the backend rejects the WebSocket request instead of
  falling back to another host.
- **Socket close/error:** the browser reconnects with bounded exponential
  backoff. Terminal text is retained during this period.
- **Missed broadcast frames:** the proxy logs a lagged subscriber; periodic
  state snapshots and reconnect snapshots restore session/terminal metadata.
  The current protocol does not provide a browser-side replay cursor for every
  terminal output byte, so a terminal should re-attach and rely on the daemon's
  `terminal_snapshot` rather than attempting to replay raw output frames.
- **Malformed server frame:** the frontend logs and ignores it.
- **Malformed client JSON:** the Rust proxy ignores it and keeps the connection
  alive.
- **Remote command error:** the file browser displays the daemon's error for
  the active request.

## Security and operational notes

- Keep daemon WebSocket URLs and bearer tokens on the server side. The browser
  should use the same-origin proxy, not a discovered daemon URL.
- Protect `/api/acp/ws`, `/api/acp/instances`, and `/api/acp/select` with the
  same authentication policy as the rest of the ACP surface.
- ACP instance authorization is currently at the authenticated route/registry
  level; deployments serving multiple users should add per-user authorization
  for instance and terminal access before treating terminals as tenant-isolated.
- `close_terminal` terminates shared state. A future detach/terminate split is
  needed if closing one browser view must not affect other viewers.
- Keep the backend ring-buffer and snapshot limits bounded. Terminal output is
  intentionally streamed outside React state, but the daemon and browser still
  need sensible limits for large snapshots and paste operations.

## Related documentation

- [ACP discovery and registration](usage.md#ACP-discovery--registration)
- [Frontend host-shim deployment](frontend-host-shim.md)
- `src/acp_ws.rs` for the upstream worker and cached state.
- `frontend/components/acp/use-acp-socket.ts` for the browser protocol adapter.
