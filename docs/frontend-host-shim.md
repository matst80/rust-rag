# Frontend host-shim deployment

Frontend now runs on the host (10.10.11.135:3000) for fast iteration. The
in-cluster Deployment + `rust-rag-frontend` Service stay in place as a
fallback. A new selector-less Service `rag-frontend` mirrors the backend's
`rag-service` pattern, and the Ingress routes `/` to it.

## Files

| Path                                                  | Role                                                                       |
|-------------------------------------------------------|----------------------------------------------------------------------------|
| `deploy/kubernetes/rust-rag-frontend.yaml`            | Original in-cluster Deployment + Service `rust-rag-frontend` (untouched).  |
| `deploy/kubernetes/rust-rag-frontend-host.yaml`       | Selector-less Service `rag-frontend` + Endpoints -> 10.10.11.135:3000.     |
| `deploy/kubernetes/rust-rag-ingress.yaml`             | Routes `/` and `/api/auth/device` to `rag-frontend` (was `rust-rag-frontend`). |

## One-time setup

```bash
make frontend-install         # npm install in frontend/
make k8s-apply-frontend-host  # apply host-shim Service + Endpoints
make k8s-apply-ingress        # re-apply Ingress so / -> rag-frontend
```

In-cluster Deployment is left running. It is no longer in the request path
but acts as a warm fallback; switch back by editing the Ingress to point
`/` at `rust-rag-frontend` again.

## Daily loop

```bash
make frontend-dev   # next dev -H 0.0.0.0 -p 3000   (hot reload)
# or
make frontend-prod  # next build && next start -H 0.0.0.0 -p 3000
```

Hit `https://rag.k6n.net/` — Ingress -> `rag-frontend` Service ->
host:3000. Code edits hot-reload, no Docker rebuild.

The dev server **must** bind `0.0.0.0`, not `127.0.0.1`, or the cluster
cannot reach it. The `frontend-dev` target enforces this via `-H 0.0.0.0`.

## Going back to in-cluster mode

```bash
# Edit deploy/kubernetes/rust-rag-ingress.yaml: change `rag-frontend` back
# to `rust-rag-frontend` for the `/` and `/api/auth/device` rules.
make k8s-apply-ingress
```

Or delete the host-shim entirely:
```bash
make k8s-delete-frontend-host
```
…then revert the Ingress.

## Why it works

`rag-frontend` is a Service with no `spec.selector`, so the endpoint
controller doesn't auto-populate it. We hand-maintain an `Endpoints`
resource of the same name pointing at `10.10.11.135:3000`. The Ingress
treats the Service as a normal upstream and forwards requests there.
Same pattern as `rag-service` for the Rust backend.

## Caveats

- Only one host can serve at a time; the home cluster has one anyway.
- If `next dev` is not running, requests 502.
- `kubectl get endpoints rag-frontend -n home` should show
  `10.10.11.135:3000`. If empty, re-apply `rust-rag-frontend-host.yaml`.
