//! Validation CLI for the Rust classifier port.
//!
//! Reproduces the Python prototype's TEST_CASES + CONTEXT_TEST_CASES output format.
//! Run from the app/src-tauri/ dir (or use --manifest-path).
//!
//!   cargo run --bin classify_test
//!
//! Defaults to loading from the repo's classifier/ output (../../classifier/onnx_model/)
//! and the runtime.json bundled in resources/.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use classifier_core::{Action, Classifier};

fn main() -> Result<()> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resources = manifest.join("resources");
    let classifier_dir = manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("classifier/onnx_model"))
        .ok_or_else(|| anyhow::anyhow!("no se pudo derivar la ruta de classifier/"))?;

    let runtime_path = resources.join("runtime.json");
    let model_path = classifier_dir.join("model.onnx");
    let tokenizer_path = classifier_dir.join("tokenizer.json");
    let meta_path = classifier_dir.join("meta.json");

    println!("runtime: {}", runtime_path.display());
    println!("model:   {}", model_path.display());

    if !model_path.exists() {
        anyhow::bail!(
            "{} no existe. Corre primero: cd classifier && uv run --extra export python src/export.py",
            model_path.display()
        );
    }

    let classifier = Classifier::new(&runtime_path, &model_path, &tokenizer_path, &meta_path)
        .context("Classifier::new")?;
    let cfg = classifier.cfg();

    println!("Categorías: {:?}", cfg.category_keys);
    let n_hyp: usize = cfg.hypotheses.values().map(|v| v.len()).sum();
    println!("Hipótesis: {n_hyp}  (+ neutral)");
    println!("Test cases: {}\n", cfg.test_cases.len());

    let test_cases = cfg.test_cases.clone();
    let context_cases = cfg.context_test_cases.clone();

    let mut latencias = Vec::new();
    let mut aciertos = 0usize;
    let mut total = 0usize;

    for case in &test_cases {
        let t0 = Instant::now();
        let decision = classifier.classify(&case.text, &[])?;
        let dt_ms = t0.elapsed().as_secs_f32() * 1000.0;
        latencias.push(dt_ms);

        let ok = match (&case.expected, &decision.action) {
            (None, Action::Permitir) => true,
            (Some(e), _) => decision.categories.iter().any(|c| c == e),
            _ => false,
        };
        total += 1;
        if ok {
            aciertos += 1;
        }

        let mark = if ok { "✓" } else { "✗" };
        let preview: String = case.text.chars().take(60).collect();
        let pred = if decision.categories.is_empty() {
            "—".to_string()
        } else {
            format!("[{}]", decision.categories.join(","))
        };
        let exp = case.expected.clone().unwrap_or_else(|| "None".into());
        let scores_str = decision
            .scores
            .iter()
            .map(|(c, s)| format!("{}: {:.3}", c, s))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{} [{:9}] pred={:<20} esp={:<5} {:>7.1}ms {{{}}}",
            mark,
            decision.action.as_str(),
            pred,
            exp,
            dt_ms,
            scores_str
        );
        println!("   texto: {}", preview);
    }

    if !latencias.is_empty() {
        let media = latencias.iter().sum::<f32>() / latencias.len() as f32;
        let mn = latencias.iter().cloned().fold(f32::INFINITY, f32::min);
        let mx = latencias.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        println!("\nLatencia media: {media:.1} ms  (min={mn:.1}, max={mx:.1})");
    }
    println!("Aciertos (solo): {aciertos}/{total}");

    if context_cases.is_empty() {
        return Ok(());
    }

    println!("\n=== Buffer de contexto ===\n");
    let mut ok_solo = 0usize;
    let mut ok_ctx = 0usize;
    let mut ctx_total = 0usize;

    for case in &context_cases {
        let ultimo = case
            .messages
            .last()
            .ok_or_else(|| anyhow::anyhow!("messages vacío"))?;
        let previos: Vec<String> = case.messages[..case.messages.len() - 1].to_vec();

        let solo = classifier.classify(ultimo, &[])?;
        let con = classifier.classify(ultimo, &previos)?;

        let was_ok = |d: &classifier_core::Decision| -> bool {
            match (&case.expected, &d.action) {
                (None, Action::Permitir) => true,
                (Some(e), _) => d.categories.iter().any(|c| c == e),
                _ => false,
            }
        };
        let solo_ok = was_ok(&solo);
        let con_ok = was_ok(&con);
        ctx_total += 1;
        if solo_ok {
            ok_solo += 1;
        }
        if con_ok {
            ok_ctx += 1;
        }

        let exp = case.expected.clone().unwrap_or_else(|| "None".into());
        println!("esperada={exp}");
        println!("  contexto: {}", previos.join(" | "));
        println!("  último:   {ultimo}");
        for (label, d, ok) in [("solo:   ", &solo, solo_ok), ("con ctx:", &con, con_ok)] {
            let mark = if ok { "✓" } else { "✗" };
            let pred = if d.categories.is_empty() {
                "—".to_string()
            } else {
                format!("[{}]", d.categories.join(","))
            };
            let scores = d
                .scores
                .iter()
                .map(|(c, s)| format!("{}: {:.3}", c, s))
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "  {} {} {:9} pred={:<20} {{{}}}",
                mark,
                label,
                d.action.as_str(),
                pred,
                scores
            );
        }
        println!();
    }

    println!("Aciertos sin contexto: {ok_solo}/{ctx_total}");
    println!("Aciertos con contexto: {ok_ctx}/{ctx_total}");

    Ok(())
}
