"""Export and quantize the NLI model for ONNX Runtime deployment.

Reads NLI_MODEL from .env, exports to ONNX (fp32) via optimum, runs dynamic int8
quantization, and writes everything the Rust runtime needs to classifier/onnx_model/.

Run with the export deps installed:
    uv sync --extra export && uv run python src/export.py

Output (gitignored via *.onnx in repo root):
    classifier/onnx_model/
      ├── model.onnx              (~140 MB int8)
      ├── tokenizer.json          (HF fast tokenizer; pure-Rust loadable)
      ├── tokenizer_config.json
      ├── special_tokens_map.json
      ├── spm.model               (sentencepiece, if present)
      ├── config.json
      └── meta.json               (entailment_idx, max_length, model_id)
"""

from __future__ import annotations

import json
import os
import shutil
import sys
from pathlib import Path

from dotenv import load_dotenv


def find_env() -> Path:
    here = Path(__file__).resolve().parent
    for d in [here, *here.parents]:
        candidate = d / ".env"
        if candidate.exists():
            return candidate
    raise FileNotFoundError("No .env encontrado subiendo desde el script.")


def main() -> None:
    env_path = find_env()
    load_dotenv(env_path, override=True)

    model_id = os.environ.get("NLI_MODEL")
    if not model_id:
        raise KeyError("Falta variable de entorno NLI_MODEL en .env")

    classifier_root = env_path.parent
    out_dir = classifier_root / "onnx_model"
    fp32_dir = out_dir / "_fp32"
    out_dir.mkdir(exist_ok=True)
    fp32_dir.mkdir(parents=True, exist_ok=True)

    print(f"Modelo: {model_id}")
    print(f"Salida: {out_dir}\n")

    # ---- 1) Export fp32 ONNX ----
    print("[1/5] Exportando a ONNX fp32 (optimum)...")
    from optimum.exporters.onnx import main_export

    main_export(
        model_name_or_path=model_id,
        output=fp32_dir,
        task="zero-shot-classification",
        opset=17,
        do_validation=True,
    )
    fp32_model = fp32_dir / "model.onnx"
    print(f"  fp32 size: {fp32_model.stat().st_size / 1e6:.1f} MB")

    # ---- 1.5) Simplificar con onnx-simplifier ----
    # Pliega constantes y elimina patrones dinámicos (Shape→Squeeze, Reshape
    # con shapes computados, etc.) que burn-onnx / tract no soportan.
    print("\n[1.5/5] Simplificando ONNX (onnx-simplifier)...")
    import onnx
    import onnxsim

    model = onnx.load(str(fp32_model))
    simplified, ok = onnxsim.simplify(model)
    if ok:
        onnx.save(simplified, str(fp32_model))
        print(f"  simplificado size: {fp32_model.stat().st_size / 1e6:.1f} MB")
    else:
        print("  WARNING: onnxsim falló validación; sigo con modelo original")

    # ---- 2) Quantización opcional ----
    final_model = out_dir / "model.onnx"
    if "--int8" in sys.argv:
        print("\n[2/5] Cuantización dinámica int8 (--int8)...")
        from onnxruntime.quantization import QuantType, quantize_dynamic

        quantize_dynamic(
            model_input=str(fp32_model),
            model_output=str(final_model),
            weight_type=QuantType.QInt8,
        )
        print(f"  int8 size: {final_model.stat().st_size / 1e6:.1f} MB")
        print(
            "  ⚠ INT8 introduce ops (MatMulInteger, DynamicQuantizeLinear) que tract \n"
            "    no implementa. Si target = mobile + ort-tract, omite --int8."
        )
    else:
        print("\n[2/5] Skip quantización (fp32). Pasa --int8 si quieres int8 + ort+ONNXRuntime nativo.")
        shutil.copy(fp32_model, final_model)
        print(f"  fp32 model: {final_model.stat().st_size / 1e6:.1f} MB")

    # ---- 3) Copia tokenizer + config ----
    print("\n[3/5] Copiando tokenizer y config...")
    artifacts = [
        "tokenizer.json",
        "tokenizer_config.json",
        "special_tokens_map.json",
        "config.json",
        "spm.model",
        "sentencepiece.bpe.model",
        "added_tokens.json",
    ]
    for fname in artifacts:
        src = fp32_dir / fname
        if src.exists():
            shutil.copy(src, out_dir / fname)
            print(f"  + {fname}")

    # ---- 4) meta.json (label2id + entailment idx) ----
    print("\n[4/5] Generando meta.json...")
    cfg = json.loads((out_dir / "config.json").read_text())
    label2id = cfg.get("label2id") or {}
    label2id_norm = {str(k).lower(): int(v) for k, v in label2id.items()}
    if "entailment" not in label2id_norm:
        raise ValueError(f"label2id no contiene 'entailment': {label2id}")

    meta = {
        "model_id": model_id,
        "entailment_idx": label2id_norm["entailment"],
        "label2id": label2id,
        "max_length": 256,
        "expected_input_names": ["input_ids", "attention_mask", "token_type_ids"],
    }
    (out_dir / "meta.json").write_text(
        json.dumps(meta, indent=2, ensure_ascii=False)
    )
    print(f"  entailment_idx = {meta['entailment_idx']}, label2id = {label2id}")

    # ---- 5) Smoke test: int8 ONNX produce scores válidos ----
    print("\n[5/5] Smoke test (int8 ONNX)...")
    from optimum.onnxruntime import ORTModelForSequenceClassification
    from transformers import AutoTokenizer, pipeline

    tok = AutoTokenizer.from_pretrained(out_dir)
    onnx_model = ORTModelForSequenceClassification.from_pretrained(out_dir)
    int8_clf = pipeline(
        "zero-shot-classification",
        model=onnx_model,
        tokenizer=tok,
    )

    sample_text = "El gato duerme tranquilo en el sofá."
    sample_label = "Este texto describe un animal en una situación cotidiana"
    out = int8_clf(
        sample_text,
        candidate_labels=[sample_label],
        multi_label=True,
        hypothesis_template="{}",
    )
    int8_score = float(out["scores"][0])
    print(f"  int8 score: {int8_score:.4f}")

    if not (0.0 <= int8_score <= 1.0):
        sys.exit(f"ERROR: score fuera de [0,1]: {int8_score}")

    if "--validate-against-pytorch" in sys.argv:
        print("  comparando contra PyTorch fp32...")
        pt_clf = pipeline("zero-shot-classification", model=model_id)
        pt_score = float(
            pt_clf(
                sample_text,
                candidate_labels=[sample_label],
                multi_label=True,
                hypothesis_template="{}",
            )["scores"][0]
        )
        diff = abs(int8_score - pt_score)
        print(f"  pytorch fp32 score: {pt_score:.4f}, |diff|: {diff:.4f}")
        if diff > 0.15:
            print(
                f"  WARNING: |diff|={diff:.3f} > 0.15 — la cuantización afectó accuracy."
            )

    # ---- cleanup ----
    if "--keep-fp32" not in sys.argv:
        shutil.rmtree(fp32_dir)
        print(f"\nLimpiado {fp32_dir.name}/. Usa --keep-fp32 para conservarlo.")

    print(f"\n✓ Listo. {final_model} → {final_model.stat().st_size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
