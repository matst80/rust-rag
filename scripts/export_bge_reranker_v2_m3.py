"""Export BAAI/bge-reranker-v2-m3 (XLM-RoBERTa-based cross-encoder) to ONNX.

Two inputs (input_ids, attention_mask), one output (logits). At runtime
the Rust side applies sigmoid to convert raw logits to a [0, 1] relevance
score; that's a deliberate split — keeping sigmoid out of the graph
matches what `AutoModelForSequenceClassification` does and lets us swap
the reranker for a different cross-encoder later without re-exporting.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import torch
import torch.nn as nn
from transformers import AutoModelForSequenceClassification, AutoTokenizer


class RerankerWrapper(nn.Module):
    def __init__(self, model: nn.Module) -> None:
        super().__init__()
        self.model = model

    def forward(
        self,
        input_ids: torch.Tensor,
        attention_mask: torch.Tensor,
    ) -> torch.Tensor:
        out = self.model(
            input_ids=input_ids,
            attention_mask=attention_mask,
            return_dict=True,
        )
        # Squeeze trailing class dim (always 1 for bge-reranker-v2-m3).
        return out.logits


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default="BAAI/bge-reranker-v2-m3")
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--opset", type=int, default=17)
    args = parser.parse_args()

    out_dir: Path = args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)

    print(f"loading reranker + tokenizer from {args.model}", file=sys.stderr)
    model = AutoModelForSequenceClassification.from_pretrained(args.model)
    model.eval()
    tokenizer = AutoTokenizer.from_pretrained(args.model)

    wrapper = RerankerWrapper(model).eval()

    # Dummy: 2 pairs, 16 tokens each. dynamic_axes covers both batch+seq.
    dummy_ids = torch.zeros(2, 16, dtype=torch.long)
    dummy_mask = torch.ones(2, 16, dtype=torch.long)

    onnx_path = out_dir / "model.onnx"
    print(f"exporting → {onnx_path}", file=sys.stderr)
    torch.onnx.export(
        wrapper,
        (dummy_ids, dummy_mask),
        str(onnx_path),
        input_names=["input_ids", "attention_mask"],
        output_names=["logits"],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "seq"},
            "attention_mask": {0: "batch", 1: "seq"},
            "logits": {0: "batch"},
        },
        opset_version=args.opset,
        do_constant_folding=True,
    )

    tokenizer.save_pretrained(str(out_dir))
    print("done.", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
