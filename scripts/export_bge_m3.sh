#!/usr/bin/env bash
# Export BAAI/bge-m3 to ONNX (dense head, feature-extraction task).
# Output: assets/bge-m3/{model.onnx,tokenizer.json,...}
#
# Idempotent: skips if assets/bge-m3/model.onnx already exists.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VENV="$ROOT/.venv-export"
OUT="$ROOT/assets/bge-m3"

if [ -f "$OUT/model.onnx" ]; then
    echo "bge-m3 ONNX already at $OUT/model.onnx — skipping export."
    exit 0
fi

mkdir -p "$OUT"

if [ ! -x "$VENV/bin/optimum-cli" ]; then
    echo "creating venv at $VENV"
    python3 -m venv "$VENV"
    "$VENV/bin/pip" install --upgrade pip
    # optimum 2.x split exporters into separate packages; optimum-onnx is the
    # one we need for `optimum-cli export onnx`.
    "$VENV/bin/pip" install optimum optimum-onnx onnx sentencepiece
fi

echo "exporting BAAI/bge-m3 → $OUT"
"$VENV/bin/optimum-cli" export onnx \
    --model BAAI/bge-m3 \
    --task feature-extraction \
    --opset 17 \
    "$OUT"

echo "done. files:"
ls -lh "$OUT"
