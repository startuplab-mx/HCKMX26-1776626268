"""Export MobileCLIP-S1 image encoder + zero-shot text anchors a ONNX.

Esto reproduce la pipeline V-NLI del notebook original (Notebook2.ipynb) pero
escribe los artefactos a `nsfw-py/mobileclip/`. El `build.rs` de la app los
hardlinks/copy a `app/src-tauri/resources/mobileclip/*` para que Tauri los
empaquete en el bundle final.

Uso:
    cd nsfw-py
    uv sync
    uv run python src/export.py

Outputs (gitignored vía *.onnx / *.npy en repo .gitignore):
    nsfw-py/mobileclip/
      ├── mobileclip_image.onnx     (image encoder, 1×3×224×224 → 512-d)
      ├── mobileclip_text.onnx      (text encoder, opcional con --with-text)
      └── text_features_anchors.npy (N_PROMPTS × 512, primeras N_RISK = riesgo)

Si modificas las listas RISK_PROMPTS / SAFE_PROMPTS, asegúrate de actualizar
`N_RISK_ANCHORS` en `crates/classifier-core/src/image_classifier.rs` para
que el slicing riesgo/seguro siga consistente.
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
import torch
from huggingface_hub import hf_hub_download

# ---------------------------------------------------------------------------
# Configuración. Mantener en sync con el runtime Rust.
# ---------------------------------------------------------------------------

# Variante MobileCLIP a exportar. S1 es buen balance latencia/accuracy en CPU
# y es lo que validó el compañero (~73% binario en weapons + nsfw).
MODEL_NAME = "mobileclip_s1"
HF_REPO = "apple/MobileCLIP-S1"
HF_FILE = "mobileclip_s1.pt"

# Tamaño de la imagen al ingresar al ONNX. Compromete con el preprocess Rust:
# si cambias esto, también cambia `INPUT_SIZE` en image_classifier.rs.
# 256 viene de mobileclip/configs/mobileclip_s1.json (image_cfg.image_size).
# Antes era 224 — el modelo se entrenó a 256, exportarlo a 224 le degradaba
# accuracy y disparaba false-positive blocks en imágenes benignas.
INPUT_SIZE = 256

# Categorías de riesgo — el orden importa: las primeras N_RISK filas del .npy
# se interpretan como riesgo en Rust. Si añades / quitas, actualiza
# `N_RISK_ANCHORS` en crates/classifier-core/src/image_classifier.rs.
RISK_PROMPTS = [
    "a photo of illegal drugs, syringes, or explicit drug consumption",
    "an explicit photo of nudity, pornography, or sexual acts",
    "a clear photograph of firearms, handguns, rifles, or weapons",
    "an image depicting narco-culture, drug cartels, and criminal activities",
    "a graphic image of physical violence, fighting, or assault",
    "a photo of a dead body, corpse, or explicit death",
    "a sensationalist image with blood, severe injuries, and gore",
]

# Anchors "neutrales / seguras" — compiten contra las de riesgo en el softmax.
# Sin éstas, casi cualquier imagen activaría algo en RISK_PROMPTS.
SAFE_PROMPTS = [
    "a photo of a dog, cat, or a pet in a normal setting",
    "a photo of fruit, food, or everyday household objects",
    "a beautiful landscape, nature, or outdoor environment",
    "a close-up of plants, flowers, or wildlife",
    "a photo of happy people in a safe, everyday social situation",
    "a clean, high-quality, safe, and normal photograph",
]

ALL_PROMPTS = RISK_PROMPTS + SAFE_PROMPTS


# ---------------------------------------------------------------------------
# Utilidades
# ---------------------------------------------------------------------------


def repo_root() -> Path:
    """Sube directorios hasta encontrar el Cargo.toml del workspace + app/."""
    here = Path(__file__).resolve().parent
    for d in [here, *here.parents]:
        if (d / "Cargo.toml").exists() and (d / "app").is_dir():
            return d
    raise FileNotFoundError(
        "No encontré la raíz del repo (busca Cargo.toml + carpeta app/)"
    )


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    # Output al lado de pyproject.toml para que build.rs encuentre los archivos
    # con `nsfw-py/mobileclip/*` (mismo patrón que classifier-py/onnx_model/).
    nsfw_root = Path(__file__).resolve().parent.parent
    out_dir = nsfw_root / "mobileclip"
    out_dir.mkdir(parents=True, exist_ok=True)

    print(f"Modelo:   {MODEL_NAME}")
    print(f"Repo HF:  {HF_REPO}")
    print(f"Salida:   {out_dir}")
    print(f"Anchors:  {len(RISK_PROMPTS)} riesgo + {len(SAFE_PROMPTS)} seguras = {len(ALL_PROMPTS)}")

    # 1. Descarga checkpoint
    print(f"\n[1/4] Descargando {HF_FILE} desde HuggingFace ...")
    ckpt_path = hf_hub_download(repo_id=HF_REPO, filename=HF_FILE)
    print(f"  -> {ckpt_path}")

    # 2. Carga modelo + tokenizer
    print(f"\n[2/4] Construyendo {MODEL_NAME} (reparametrizado para inferencia) ...")
    import mobileclip

    model, _, _ = mobileclip.create_model_and_transforms(
        MODEL_NAME, pretrained=ckpt_path
    )
    tokenizer = mobileclip.get_tokenizer(MODEL_NAME)
    model.eval()

    # 3. Exporta image encoder
    print(
        f"\n[3/4] Exportando image encoder a ONNX (1×3×{INPUT_SIZE}×{INPUT_SIZE}) ..."
    )
    dummy_image = torch.zeros(1, 3, INPUT_SIZE, INPUT_SIZE)
    image_path = out_dir / "mobileclip_image.onnx"
    # dynamo=False fuerza el exporter legacy (TorchScript-based). El nuevo
    # exporter dynamo (default en torch >=2.7) guarda los pesos en un
    # `<name>.onnx.data` aparte; al shippear sólo el .onnx, ORT revienta con
    # "Encountered unknown exception in Initialize()" y la app cae al modo
    # fail-closed (blur a todo). Inline los pesos para tener un único archivo.
    torch.onnx.export(
        model.image_encoder,
        dummy_image,
        str(image_path),
        export_params=True,
        opset_version=17,
        do_constant_folding=True,
        input_names=["image"],
        output_names=["image_features"],
        dynamo=False,
    )
    print(f"  -> {image_path} ({image_path.stat().st_size / 1e6:.1f} MB)")

    # 4. Pre-computa text features para los anchors
    print(f"\n[4/4] Computando text features para {len(ALL_PROMPTS)} prompts ...")
    text_tokens = tokenizer(ALL_PROMPTS)
    with torch.no_grad():
        text_features = model.encode_text(text_tokens)
        text_features = text_features / text_features.norm(dim=-1, keepdim=True)
    anchors_path = out_dir / "text_features_anchors.npy"
    np.save(anchors_path, text_features.cpu().numpy().astype(np.float32))
    print(f"  shape: {tuple(text_features.shape)} -> {anchors_path}")

    # 5. (opcional) text encoder — no se usa en runtime pero útil para debugging
    if "--with-text" in sys.argv:
        print("\n[+] Exportando text encoder (opcional) ...")
        dummy_text = text_tokens[:1]
        text_path = out_dir / "mobileclip_text.onnx"
        torch.onnx.export(
            model.text_encoder,
            dummy_text,
            str(text_path),
            export_params=True,
            opset_version=17,
            do_constant_folding=True,
            input_names=["text"],
            output_names=["text_features"],
            dynamo=False,
        )
        print(f"  -> {text_path} ({text_path.stat().st_size / 1e6:.1f} MB)")

    # Smoke test rápido — corre el image encoder sobre una imagen aleatoria y
    # verifica que el embedding tenga la dimensión esperada y que los anchors
    # produzcan un softmax sano.
    print("\n[smoke test] inferencia sobre tensor aleatorio ...")
    with torch.no_grad():
        feats = model.encode_image(torch.rand(1, 3, INPUT_SIZE, INPUT_SIZE))
        feats = feats / feats.norm(dim=-1, keepdim=True)
        sims = (100.0 * feats @ text_features.T).softmax(dim=-1).cpu().numpy()[0]
    risk_top = float(sims[: len(RISK_PROMPTS)].max())
    safe_top = float(sims[len(RISK_PROMPTS) :].max())
    print(f"  embed dim: {feats.shape[-1]} (esperado 512)")
    print(f"  best risk score: {risk_top:.3f}")
    print(f"  best safe score: {safe_top:.3f}")

    print(f"\n✓ Listo. Reconstruye el bundle Tauri: cd app && bun run tauri build")
    print(f"  (en dev, build.rs sincroniza desde {out_dir.relative_to(repo_root())})")


if __name__ == "__main__":
    main()
