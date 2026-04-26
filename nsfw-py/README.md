# nsfw-py

Exporta el clasificador zero-shot de imágenes (MobileCLIP-S1) a ONNX para que
el runtime Rust de Shield pueda decidir si una imagen es benigna o de riesgo
sin volver a tocar PyTorch.

## ¿Qué genera?

```
nsfw-py/mobileclip/
├── mobileclip_image.onnx        # image encoder (1×3×224×224 → 512-d)
├── text_features_anchors.npy    # 13 × 512  (7 anchors riesgo + 6 seguros)
└── mobileclip_text.onnx         # opcional con --with-text
```

`app/src-tauri/build.rs` hardlinks/copy estos archivos a
`app/src-tauri/resources/mobileclip/` para que Tauri los meta al bundle.

## Cómo correrlo

```bash
cd nsfw-py
uv sync
uv run python src/export.py
```

Opcional:

```bash
# También exporta el text encoder (~8 MB) por si quieres re-computar anchors
# en runtime más adelante.
uv run python src/export.py --with-text
```

La primera corrida descarga `apple/MobileCLIP-S1` (~140 MB) desde
HuggingFace al cache de tu usuario (`~/.cache/huggingface/`). Las siguientes
corridas reutilizan el cache.

## Modificar las categorías

Las listas `RISK_PROMPTS` y `SAFE_PROMPTS` viven en `src/export.py`. Si
añades o quitas anchors, **también** actualiza `N_RISK_ANCHORS` en
`classifier/src/image_classifier.rs` para que el slicing
riesgo/seguro siga consistente.

## Por qué no commiteamos los pesos

Los `.onnx` y `.npy` están gitignored (políticas globales `*.onnx`/`*.npy`).
Cualquier integrante del equipo regenera localmente con un `uv run`. Esto
mantiene el repo livianito y obliga a que la pipeline de export sea
reproducible.
