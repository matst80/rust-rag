#!/usr/bin/env bash
# Export BAAI/bge-reranker-v2-m3 cross-encoder to ONNX. Output goes to
# assets/bge-reranker-v2-m3/ (mirroring the bge-m3 layout).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VENV="$ROOT/.venv-export-sparse"
OUT="$ROOT/assets/bge-reranker-v2-m3"

if [ -f "$OUT/model.onnx" ] && [ -f "$OUT/.export-marker" ]; then
    echo "bge-reranker-v2-m3 ONNX already present — skipping."
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
        onnxscript \
        protobuf
fi

echo "exporting BAAI/bge-reranker-v2-m3 → $OUT"
"$VENV/bin/python" "$ROOT/scripts/export_bge_reranker_v2_m3.py" \
    --model BAAI/bge-reranker-v2-m3 \
    --out-dir "$OUT" \
    --opset 17

touch "$OUT/.export-marker"

echo "done. files:"
ls -lh "$OUT"
