//! Helpers de obscuring + función one-shot para filtrar un texto vía Classifier.
//! Reusados por la app (Tauri command) y por el plugin (FFI Swift/Kotlin).

use std::sync::OnceLock;

use crate::{Action, Classifier};

/// Logging diagnóstico opcional. Activar con `CLASSIFIER_DEBUG=1` (ej. en el
/// scheme de Xcode) para inspeccionar `action`, scores por categoría y un
/// preview de cada texto. Cero overhead cuando el env var no está seteado.
fn debug_enabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| std::env::var("CLASSIFIER_DEBUG").is_ok())
}

/// Mínimo de caracteres no-blancos para correr el clasificador. Textos más
/// cortos pasan sin tocar (no hay señal suficiente).
pub const MIN_TEXT_LEN_FOR_CLASSIFY: usize = 20;

/// Sustituye alfanuméricos por █. Whitespace y puntuación se respetan.
pub fn obscure_full(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() { '█' } else { c })
        .collect()
}

/// Sustituye letras por '-'. Números y puntuación se respetan.
pub fn obscure_dashes(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphabetic() { '-' } else { c })
        .collect()
}

fn log_decision(text: &str, decision: &crate::Decision) {
    // En el hot path solo emitimos log para acciones disruptivas (Avisar /
    // Bloquear). Permitir es la mayoría — loggearla por texto (cada
    // `eprintln!` en iOS pasa por el log subsystem y agrega ~5-15 ms) era
    // un costo apreciable en batches grandes.
    if !debug_enabled() {
        return;
    }
    if matches!(decision.action, Action::Permitir) {
        return;
    }
    let preview: String = text.chars().take(80).collect();
    let scores: Vec<String> = decision
        .scores
        .iter()
        .map(|(c, s)| format!("{c}={s:.2}"))
        .collect();
    eprintln!(
        "[classifier-debug] action={:?} cats={:?} scores=[{}] text={:?}",
        decision.action,
        decision.categories,
        scores.join(","),
        preview
    );
}

fn obscure_for(action: Action, text: &str) -> String {
    match action {
        Action::Bloquear => obscure_full(text),
        Action::Avisar => obscure_dashes(text),
        Action::Permitir => text.to_string(),
    }
}

/// Aplica el clasificador a `text`. Devuelve el texto obscurecido según la acción.
/// Maneja short-text passthrough y errores de inferencia (passthrough).
pub fn apply_filter(classifier: &Classifier, text: &str) -> String {
    if text.trim().chars().count() < MIN_TEXT_LEN_FOR_CLASSIFY {
        return text.to_string();
    }
    match classifier.classify(text, &[]) {
        Ok(decision) => {
            log_decision(text, &decision);
            obscure_for(decision.action, text)
        }
        Err(e) => {
            eprintln!("[classifier] error: {e}");
            text.to_string()
        }
    }
}

/// Versión batched real: separa los textos demasiado cortos (passthrough) de
/// los que valen la pena clasificar y manda una sola pasada NLI agrupando
/// N premisas × H hipótesis (`Pipeline::classify_many`). Esto amortiza el
/// overhead de tokenización + ONNX y el lock de la sesión.
pub fn apply_filter_batch(classifier: &Classifier, texts: &[String]) -> Vec<String> {
    let mut results: Vec<String> = vec![String::new(); texts.len()];
    let mut to_classify: Vec<String> = Vec::with_capacity(texts.len());
    let mut idx_map: Vec<usize> = Vec::with_capacity(texts.len());

    for (i, t) in texts.iter().enumerate() {
        if t.trim().chars().count() < MIN_TEXT_LEN_FOR_CLASSIFY {
            results[i] = t.clone();
        } else {
            idx_map.push(i);
            to_classify.push(t.clone());
        }
    }

    if to_classify.is_empty() {
        return results;
    }

    match classifier.classify_many(&to_classify, &[]) {
        Ok(decisions) => {
            for (k, decision) in decisions.into_iter().enumerate() {
                let i = idx_map[k];
                let text = &to_classify[k];
                log_decision(text, &decision);
                results[i] = obscure_for(decision.action, text);
            }
        }
        Err(e) => {
            eprintln!("[classifier] error (batch): {e}");
            for (k, text) in to_classify.into_iter().enumerate() {
                results[idx_map[k]] = text;
            }
        }
    }

    results
}
