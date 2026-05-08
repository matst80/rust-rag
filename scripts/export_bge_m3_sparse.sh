#!/usr/bin/env bash
# Export BAAI/bge-m3 with both dense (last_hidden_state) and sparse
# (sparse_logits) outputs in one ONNX graph. Output overwrites
# assets/bge-m3/. Use this in place of export_bge_m3.sh once phase 2 sparse
# is wired up.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VENV="$ROOT/.venv-export-sparse"
OUT="$ROOT/assets/bge-m3"

if [ -f "$OUT/model.onnx" ] && [ -f "$OUT/.sparse-export-marker" ]; then
    echo "bge-m3 sparse-export ONNX already present — skipping."
    exit 0
fi

mkdir -p "$OUT"

if [ ! -x "$VENV/bin/python" ]; then
    echo "creating venv at $VENV"
    python3 -m venv "$VENV"
    "$VENV/bin/pip" install --upgrade pip
    "$VENV/bin/pip" install \
        "torch>=2.1,<3" \
        "transformers>=4.40,<5" \
        "huggingface_hub>=0.23" \
        sentencepiece \
        onnx \
        protobuf
fi

echo "exporting BAAI/bge-m3 (dense + sparse) → $OUT"
"$VENV/bin/python" "$ROOT/scripts/export_bge_m3_sparse.py" \
    --model BAAI/bge-m3 \
    --out-dir "$OUT" \
    --opset 17

# Marker so future runs of this script skip — and so future runs of the
# legacy dense-only export_bge_m3.sh don't overwrite our two-output graph.
touch "$OUT/.sparse-export-marker"

echo "done. files:"
ls -lh "$OUT"
