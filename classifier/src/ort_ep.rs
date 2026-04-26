//! Configuración de Execution Providers de ONNX Runtime por plataforma y
//! por perfil de shape del modelo.
//!
//! La premisa es que **CoreML EP en iOS sólo es viable para modelos con
//! shapes estáticas** (CNN, encoder visual). Para transformers con dim
//! batch dinámica (NLI), CoreML re-compila el grafo cada vez que ve una
//! `(batch, seq)` nueva — del orden de decenas de segundos por compile en
//! device para mDeBERTa-base, suficiente para parecer cuelgue. El ANE
//! además exige shapes 100 % estáticas, así que el upside ahí es ínfimo.
//!
//! Por eso exponemos dos funciones:
//! - [`apply_static_shape_eps`]: CoreML All (ANE + GPU + CPU) — usar en
//!   modelos como MobileCLIP (256×256 fixed input).
//! - [`apply_dynamic_shape_eps`]: en iOS no registra ningún EP (CPU
//!   implícito) — usar para NLI y otros transformers con batch variable.
//!
//! En desktop / Android ambas son no-op (CPU implícito). En `cfg(macos)`
//! desktop podríamos extender `static_shape` para usar CoreML pyke, pero
//! ese es un PR aparte.

use anyhow::Result;
#[cfg(target_os = "ios")]
use anyhow::anyhow;
use ort::session::builder::SessionBuilder;

/// EPs para modelos con shape de input estática (encoder visual, CNN).
/// **iOS**: CoreML con `MLProgram` + `ALL` compute units (ANE/GPU/CPU) +
/// subgraphs habilitados + `FastPrediction`. Seguido de CPU EP fallback.
/// **Otras plataformas**: identidad.
pub fn apply_static_shape_eps(
    #[cfg_attr(not(target_os = "ios"), allow(unused_mut))] mut builder: SessionBuilder,
) -> Result<SessionBuilder> {
    #[cfg(target_os = "ios")]
    {
        use ort::execution_providers::{
            coreml::{CoreMLComputeUnits, CoreMLModelFormat, CoreMLSpecializationStrategy},
            CPUExecutionProvider, CoreMLExecutionProvider,
        };

        let coreml = CoreMLExecutionProvider::default()
            .with_model_format(CoreMLModelFormat::MLProgram)
            .with_compute_units(CoreMLComputeUnits::All)
            .with_subgraphs(true)
            .with_static_input_shapes(false)
            .with_specialization_strategy(CoreMLSpecializationStrategy::FastPrediction)
            .build();
        let cpu = CPUExecutionProvider::default().build();

        builder = builder
            .with_execution_providers([coreml, cpu])
            .map_err(|e| anyhow!("ort EPs static (iOS): {e}"))?;
    }

    Ok(builder)
}

/// EPs para modelos con dim batch dinámica (transformers NLI).
///
/// **iOS**: NO toca el builder. Es decir, CPU EP por default. Se intentó
/// CoreML EP en este path y produjo cuelgues consistentes (compile per-shape
/// del orden de minutos en device para mDeBERTa-v3-base con batch variable
/// 1..216). El callsite (p.ej. `NliBackend`) compensa con `intra_threads=4`
/// para que el CPU iPhone no quede starved.
///
/// **Otras plataformas**: identidad.
///
/// Para desbloquear GPU/ANE en NLI haría falta re-exportar el modelo con
/// (a) batch fija, (b) opset ≤ 17 sin disentangled attention exótica, y
/// (c) quantización FP16 — fuera de scope de esta función, depende del
/// pipeline `classifier-py/src/export.py`.
pub fn apply_dynamic_shape_eps(builder: SessionBuilder) -> Result<SessionBuilder> {
    Ok(builder)
}
