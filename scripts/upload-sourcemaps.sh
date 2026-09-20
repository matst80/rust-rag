#!/usr/bin/env bash
# Upload browser source maps for the frontend to the RUM backend.
#
# Builds the frontend Docker image (builder stage) with the release baked in,
# copies the generated *.js.map files out of that exact build, uploads them
# with `release` + `source` set so error stacks de-obfuscate, and then the
# caller (Makefile) pushes the same image — maps in the served image are
# stripped by the Dockerfile runner stage.
#
# Turbopack stores each chunk's map under its own hash and links it from the
# chunk's `//# sourceMappingURL=` comment, so we walk the .js files, resolve
# each one's map, and upload under the .js URL that browser stacks will show.
#
# Usage: scripts/upload-sourcemaps.sh [RELEASE] [IMAGE]
#   RELEASE defaults to git short SHA, suffixed -dirty on an unclean tree
#   IMAGE defaults to $FRONTEND_IMAGE_NAME (matst80/rust-rag-frontend:latest)
#
# Env:
#   RUM_BASE_URL   RUM backend base URL (default https://otel.k6n.net)
set -euo pipefail

cd "$(dirname "$0")/.."

RELEASE="${1:-$(git rev-parse --short HEAD)$(git diff --quiet || echo -dirty)}"
IMAGE="${2:-${FRONTEND_IMAGE_NAME:-matst80/rust-rag-frontend:latest}}"
BASE_URL="${RUM_BASE_URL:-https://otel.k6n.net}"
FRONTEND_DIR="frontend"

echo "release: $RELEASE"
echo "image:   $IMAGE"
echo "rum:     $BASE_URL"

# Build only up to the sourcemaps stage so the extracted maps come from the
# exact bundle that will be deployed (no local rebuild drift).
if [[ "$(uname -m)" == "x86_64" ]]; then
  docker build --target sourcemaps \
    --build-arg "NEXT_PUBLIC_APP_RELEASE=$RELEASE" \
    -f "$FRONTEND_DIR/Dockerfile" -t "$IMAGE-sourcemaps" .
else
  docker buildx build --platform linux/amd64 --load --target sourcemaps \
    --build-arg "NEXT_PUBLIC_APP_RELEASE=$RELEASE" \
    -f "$FRONTEND_DIR/Dockerfile" -t "$IMAGE-sourcemaps" .
fi

CID=$(docker create "$IMAGE-sourcemaps")
trap 'docker rm "$CID" >/dev/null 2>&1' EXIT

WORKDIR="$(mktemp -d)"
trap 'docker rm "$CID" >/dev/null 2>&1; rm -rf "$WORKDIR"' EXIT
docker cp "$CID:/app/frontend/.next/static" "$WORKDIR/static"

# Every client chunk needs its own upload; fail the deploy if the build
# produced no bundles at all.
shopt -s nullglob globstar
JS_FILES=("$WORKDIR"/static/**/*.js)
if [[ ${#JS_FILES[@]} -eq 0 ]]; then
  echo "ERROR: no *.js bundles found in build output — deploy aborted" >&2
  exit 1
fi

echo "uploading source maps for release $RELEASE"
uploaded=0
skipped=0
failed=0
for js in "$WORKDIR"/static/**/*.js; do
  rel="${js#"$WORKDIR"/static/}"          # e.g. chunks/main-app-abc123.js
  source_path="/_next/static/${rel}"      # browser stack path of the bundle
  map_url="$(grep -ao '//# sourceMappingURL=[^ ]*' "$js" | tail -1 | cut -d= -f2- || true)"
  if [[ -z "$map_url" ]]; then
    echo "  skipped (no sourceMappingURL) $source_path"
    skipped=$((skipped + 1))
    continue
  fi
  map="$WORKDIR/static/$(dirname "$rel")/$map_url"
  if [[ ! -f "$map" ]]; then
    echo "  WARN: map $map_url referenced by $rel not found" >&2
    failed=$((failed + 1))
    continue
  fi
  # Turbopack emits empty stub maps for some chunks; the ingest rejects them.
  if grep -q '"mappings": *""' "$map"; then
    echo "  skipped (empty mappings) $source_path"
    skipped=$((skipped + 1))
    continue
  fi
  if curl --fail-with-body --silent --show-error \
    -X POST "$BASE_URL/api/sourcemaps" \
    -F "release=$RELEASE" \
    -F "source=$source_path" \
    -F "file=@$map;type=application/json" >/dev/null; then
    echo "  uploaded $source_path <- $map_url"
    uploaded=$((uploaded + 1))
  else
    echo "  WARN: upload failed for $source_path" >&2
    failed=$((failed + 1))
  fi
done

echo "done: $uploaded uploaded, $skipped skipped, $failed failed (release $RELEASE)"
if [[ $uploaded -eq 0 ]]; then
  echo "ERROR: no source maps were uploaded — deploy aborted" >&2
  exit 1
fi
