"""Export BAAI/bge-m3 with both dense (last_hidden_state) and sparse
(per-token sparse logits) outputs in a single ONNX graph.

The dense head is just the encoder's `last_hidden_state`; CLS-pooling +
L2-normalize happens in Rust. The sparse head is a `Linear(hidden, 1) +
ReLU` lifted from `BGEM3FlagModel.sparse_linear`. Token-id max-pool
aggregation (dropping CLS/SEP/PAD/special tokens) also happens in Rust —
keeping it out of ONNX avoids encoding tokenizer-specific special-token
ids into the graph.

Outputs:
    last_hidden_state : (batch, seq_len, 1024)   float32
    sparse_logits     : (batch, seq_len, 1)      float32  (post-ReLU)

Usage: invoked by scripts/export_bge_m3_sparse.sh, which manages the venv.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import torch
import torch.nn as nn
from transformers import AutoModel, AutoTokenizer


class BgeM3DenseSparseWrapper(nn.Module):
    """Encoder + sparse_linear in one graph, two outputs."""

    def __init__(self, encoder: nn.Module, sparse_linear: nn.Linear) -> None:
        super().__init__()
        self.encoder = encoder
        self.sparse_linear = sparse_linear

    def forward(
        self,
        input_ids: torch.Tensor,
        attention_mask: torch.Tensor,
    ) -> tuple[torch.Tensor, torch.Tensor]:
        outputs = self.encoder(
            input_ids=input_ids,
            attention_mask=attention_mask,
            return_dict=True,
        )
        last_hidden_state = outputs.last_hidden_state
        sparse_logits = torch.relu(self.sparse_linear(last_hidden_state))
        return last_hidden_state, sparse_logits


def load_sparse_linear(model_dir: Path, hidden_size: int) -> nn.Linear:
    """Load the sparse_linear weight matrix shipped with bge-m3.

    BAAI/bge-m3 ships the sparse head as a separate `sparse_linear.pt`
    state dict alongside the encoder weights. We instantiate
    `nn.Linear(hidden, 1)` and load it.
    """
    weight_path = model_dir / "sparse_linear.pt"
    if not weight_path.exists():
        raise FileNotFoundError(
            f"missing {weight_path}: HF snapshot of BAAI/bge-m3 must include "
            "sparse_linear.pt — re-download with allow_patterns including "
            "'sparse_linear.pt'"
        )
    state_dict = torch.load(weight_path, map_location="cpu", weights_only=True)
    sparse_linear = nn.Linear(hidden_size, 1)
    # Keys in the file are `weight` and `bias` (matching nn.Linear directly).
    sparse_linear.load_state_dict(state_dict)
    sparse_linear.eval()
    return sparse_linear


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="BAAI/bge-m3")
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--opset", type=int, default=17)
    args = parser.parse_args()

    out_dir: Path = args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)

    print(f"loading encoder + tokenizer from {args.model}", file=sys.stderr)
    encoder = AutoModel.from_pretrained(args.model)
    encoder.eval()
    tokenizer = AutoTokenizer.from_pretrained(args.model)

    # `huggingface_hub.snapshot_download` would be cleaner, but `from_pretrained`
    # already cached every file we need. Resolve the local cache dir.
    from huggingface_hub import snapshot_download
    cache_dir = Path(snapshot_download(args.model, allow_patterns=["sparse_linear.pt"]))
    sparse_linear = load_sparse_linear(cache_dir, encoder.config.hidden_size)

    wrapper = BgeM3DenseSparseWrapper(encoder, sparse_linear).eval()

    # Dummy inputs: 2 sequences of 16 tokens each, so dynamic_axes can shape
    # both batch and seq dimensions.
    dummy_ids = torch.zeros(2, 16, dtype=torch.long)
    dummy_mask = torch.ones(2, 16, dtype=torch.long)

    onnx_path = out_dir / "model.onnx"
    print(f"exporting → {onnx_path}", file=sys.stderr)
    torch.onnx.export(
        wrapper,
        (dummy_ids, dummy_mask),
        str(onnx_path),
        input_names=["input_ids", "attention_mask"],
        output_names=["last_hidden_state", "sparse_logits"],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "seq"},
            "attention_mask": {0: "batch", 1: "seq"},
            "last_hidden_state": {0: "batch", 1: "seq"},
            "sparse_logits": {0: "batch", 1: "seq"},
        },
        opset_version=args.opset,
        do_constant_folding=True,
    )

    # Save tokenizer artifacts alongside, mirroring optimum-cli's layout.
    tokenizer.save_pretrained(str(out_dir))
    print("done.", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
