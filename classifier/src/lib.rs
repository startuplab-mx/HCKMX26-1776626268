//! Zero-shot multi-hypothesis NLI classifier — Rust port of classifier/src/main.py.
//!
//! All sensitive content (hypotheses, lexical patterns, thresholds) lives in
//! `runtime.json`, generated from `classifier/.env` via `scripts/gen_runtime.py`.

mod config;
mod decide;
mod filter;
mod image_classifier;
mod lexical;
mod nli;
mod pipeline;

use std::path::Path;
use std::sync::{Arc, OnceLock};

use anyhow::Result;

/// Instancia global compartida entre app y plugin nativo. La app la setea
/// en su setup hook; el plugin la lee en su FFI handler. Esto evita
/// duplicar el modelo en memoria y resolver paths distintos en cada lado
/// (Tauri `resource_dir()` vs Swift `Bundle.main.resourcePath`).
pub static SHARED_CLASSIFIER: OnceLock<Arc<Classifier>> = OnceLock::new();

pub use config::RuntimeConfig;
pub use decide::{Action, Decision};
pub use filter::{
    apply_filter, apply_filter_batch, obscure_dashes, obscure_full,
    MIN_TEXT_LEN_FOR_CLASSIFY,
};
pub use image_classifier::{ImageClassifier, ImageDecision};
pub use pipeline::Pipeline;

/// Instancia global del clasificador de imágenes (mismo patrón que
/// `SHARED_CLASSIFIER`): la app la setea en setup; el plugin nativo la
/// puede leer si en algún momento se quiere filtrado en mobile.
pub static SHARED_IMAGE_CLASSIFIER: OnceLock<Arc<ImageClassifier>> = OnceLock::new();

/// Inicializa la `OrtApi` global vía `OrtGetApiBase` cuando se enlaza
/// estáticamente contra libonnxruntime.a (feature `alternative-backend`).
/// Debe llamarse antes de cualquier uso de `ort` (incluyendo `Classifier::new`).
/// Idempotente — `ort::set_api` no sobreescribe si ya hay una instancia.
#[cfg(target_os = "ios")]
pub fn init_ort_api() {
    use ort::sys;
    unsafe {
        let base = sys::OrtGetApiBase();
        assert!(!base.is_null(), "OrtGetApiBase devolvió null");
        let api_ptr = ((*base).GetApi)(sys::ORT_API_VERSION);
        assert!(!api_ptr.is_null(), "GetApi devolvió null");
        ort::set_api(*api_ptr);
    }
}

/// Public façade: load once at app startup, call `classify` per text.
pub struct Classifier {
    pipeline: Pipeline,
}

impl Classifier {
    pub fn new(
        runtime_path: &Path,
        model_path: &Path,
        tokenizer_path: &Path,
        meta_path: &Path,
    ) -> Result<Self> {
        let cfg = RuntimeConfig::load(runtime_path)?;
        let nli = nli::NliBackend::new(model_path, tokenizer_path, meta_path)?;
        let pipeline = Pipeline::build(cfg, nli)?;
        Ok(Self { pipeline })
    }

    pub fn classify(&self, text: &str, context: &[String]) -> Result<Decision> {
        self.pipeline.classify(text, context)
    }

    pub fn classify_many(
        &self,
        texts: &[String],
        context: &[String],
    ) -> Result<Vec<Decision>> {
        self.pipeline.classify_many(texts, context)
    }

    pub fn cfg(&self) -> &RuntimeConfig {
        &self.pipeline.cfg
    }
}
