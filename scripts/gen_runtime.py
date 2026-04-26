#!/usr/bin/env python3
"""Convert classifier/.env into a runtime.json bundled with the Rust app.

Reads classifier/.env and emits app/src-tauri/resources/runtime.json with the same
structure the Python prototype uses, suitable for serde_json deserialization in Rust.

Run: uv run --directory classifier python ../scripts/gen_runtime.py

Or directly: python scripts/gen_runtime.py
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

try:
    from dotenv import dotenv_values
except ImportError:
    print("ERROR: pip install python-dotenv", file=sys.stderr)
    sys.exit(1)


REPO_ROOT = Path(__file__).resolve().parent.parent
ENV_PATH = REPO_ROOT / "classifier" / ".env"
OUT_PATH = REPO_ROOT / "app" / "src-tauri" / "resources" / "runtime.json"

# Constantes que el prototipo Python tiene como literales en main.py
LEXICAL_SHORTCUT_SCORE = 0.95
LEXICAL_BOOST_FLOOR = 0.70
MAX_CONTEXT = 4


def require(values: dict, key: str) -> str:
    v = values.get(key)
    if v is None:
        raise KeyError(f"Falta {key} en {ENV_PATH}")
    return v


def main() -> None:
    if not ENV_PATH.exists():
        sys.exit(f"No existe {ENV_PATH}")

    values = dotenv_values(ENV_PATH)
    keys = json.loads(require(values, "CATEGORY_KEYS"))

    runtime = {
        "model_id": require(values, "NLI_MODEL"),
        "category_keys": keys,
        "hypotheses": {
            k: json.loads(require(values, f"HYPOTHESES_{k.upper()}")) for k in keys
        },
        "lexical": {
            k: json.loads(require(values, f"LEXICAL_{k.upper()}")) for k in keys
        },
        "neutral_hypothesis": require(values, "NEUTRAL_HYPOTHESIS"),
        "thresholds": json.loads(require(values, "THRESHOLDS")),
        "test_cases": json.loads(require(values, "TEST_CASES")),
        "context_test_cases": json.loads(values.get("CONTEXT_TEST_CASES", "[]")),
        "lexical_shortcut_score": LEXICAL_SHORTCUT_SCORE,
        "lexical_boost_floor": LEXICAL_BOOST_FLOOR,
        "max_context": MAX_CONTEXT,
    }

    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUT_PATH.write_text(json.dumps(runtime, indent=2, ensure_ascii=False))
    n_hyps = sum(len(v) for v in runtime["hypotheses"].values())
    print(f"✓ {OUT_PATH}  ({len(keys)} cats, {n_hyps} hypotheses, "
          f"{len(runtime['test_cases'])} test cases)")


if __name__ == "__main__":
    main()
