use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use ndarray::{Array2, Axis};
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Tensor,
};
use serde::Deserialize;
use tokenizers::{
    PaddingDirection, PaddingParams, PaddingStrategy, Tokenizer, TruncationDirection,
    TruncationParams, TruncationStrategy,
};

#[derive(Debug, Deserialize)]
struct ModelMeta {
    entailment_idx: usize,
    label2id: BTreeMap<String, usize>,
    max_length: usize,
}

/// Wraps an ort Session + tokenizer.
/// Session behind a Mutex porque ort 2.x's `Session::run` requires `&mut self`.
pub struct NliBackend {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    entailment_idx: usize,
    contradiction_idx: usize,
    max_length: usize,
    use_token_type_ids: bool,
}

fn ort_err<E: std::fmt::Display>(e: E) -> anyhow::Error {
    anyhow!("ort: {e}")
}

impl NliBackend {
    pub fn new(model_path: &Path, tokenizer_path: &Path, meta_path: &Path) -> Result<Self> {
        let meta_bytes = std::fs::read(meta_path)
            .with_context(|| format!("leyendo {}", meta_path.display()))?;
        let meta: ModelMeta = serde_json::from_slice(&meta_bytes)?;

        let mut contradiction_idx = 0usize;
        for (label, idx) in &meta.label2id {
            if label.eq_ignore_ascii_case("contradiction") {
                contradiction_idx = *idx;
            }
        }

        let mut tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow!("tokenizer load: {e}"))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: meta.max_length,
                strategy: TruncationStrategy::LongestFirst,
                stride: 0,
                direction: TruncationDirection::Right,
            }))
            .map_err(|e| anyhow!("set truncation: {e}"))?;
        // BatchLongest + pad_to_multiple_of=8 en todas las plataformas.
        // En desktop la mayoría de candidatos del DOM son < 80 tokens;
        // pad dinámico recorta compute attention ~5-7× vs fijo a 256.
        // En iOS NLI ahora corre en CPU (ver `apply_dynamic_shape_eps`),
        // así que mismo razonamiento aplica — un fixed(max_length) sólo
        // forzaría 3-5× más compute por nada.
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            direction: PaddingDirection::Right,
            pad_to_multiple_of: Some(8),
            pad_id: 0,
            pad_type_id: 0,
            pad_token: "[PAD]".into(),
        }));

        // intra_threads = 4 siempre, incluso en iOS. En el primer deploy
        // probamos `1` con la teoría de que CoreML descargaría el cómputo
        // pesado al ANE/GPU. En la práctica una parte no trivial del grafo
        // NLI cae al CPU EP de fallback (ops no soportadas por CoreML EP,
        // o dim batch dinámica que rechaza), y `1` thread sobre un batch
        // ~156 × max_length tokens dejaba la sesión esencialmente colgada
        // en device. `4` da headroom suficiente para que el peor caso
        // (todo CPU) siga siendo de pocos segundos en vez de minutos.
        let intra_threads = 4;
        let mut builder = Session::builder()
            .map_err(ort_err)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(ort_err)?;
        builder = crate::apply_dynamic_shape_eps(builder).map_err(ort_err)?;
        let session = builder
            .with_intra_threads(intra_threads)
            .map_err(ort_err)?
            .commit_from_file(model_path)
            .map_err(ort_err)?;

        let use_token_type_ids = session
            .inputs
            .iter()
            .any(|i| i.name == "token_type_ids");

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            entailment_idx: meta.entailment_idx,
            contradiction_idx,
            max_length: meta.max_length,
            use_token_type_ids,
        })
    }

    /// P(entailment | premise, hypothesis_i) for each hypothesis.
    /// Softmax over [contradiction, entailment] — matches HF zero-shot pipeline
    /// con `multi_label=True`.
    pub fn entailment_scores(
        &self,
        premise: &str,
        hypotheses: &[String],
    ) -> Result<Vec<f32>> {
        let pairs: Vec<(String, String)> = hypotheses
            .iter()
            .map(|h| (premise.to_string(), h.clone()))
            .collect();
        let scores = self.run_pairs(pairs)?;
        Ok(scores)
    }

    /// Versión batched real de N premisas × H hipótesis. Devuelve una matriz
    /// `out[i][j] = P(entail | premise_i, hypothesis_j)` en una sola
    /// `session.run()` — amortiza el overhead de tokenización + ONNX cuando
    /// se clasifican muchos textos a la vez.
    pub fn entailment_scores_batch(
        &self,
        premises: &[String],
        hypotheses: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        if premises.is_empty() || hypotheses.is_empty() {
            return Ok(vec![Vec::new(); premises.len()]);
        }
        let mut pairs: Vec<(String, String)> =
            Vec::with_capacity(premises.len() * hypotheses.len());
        for p in premises {
            for h in hypotheses {
                pairs.push((p.clone(), h.clone()));
            }
        }
        let flat = self.run_pairs(pairs)?;
        let h = hypotheses.len();
        let mut out = Vec::with_capacity(premises.len());
        for chunk in flat.chunks(h) {
            out.push(chunk.to_vec());
        }
        Ok(out)
    }

    /// Ejecuta una inferencia dummy con un par premise/hypothesis del tamaño
    /// máximo (`max_length`). En iOS esto fuerza a CoreML EP a compilar el
    /// modelo a `.mlmodelc` durante el setup en vez de durante la primera
    /// petición real del usuario. En desktop es ~10ms desperdiciados que no
    /// afectan UX. Ignora cualquier error: el peor caso es que la primera
    /// inferencia real pague el costo de compilación.
    pub fn warmup(&self) -> Result<()> {
        let batch = 1usize;
        let seq = self.max_length;
        let mut input_ids = Array2::<i64>::zeros((batch, seq));
        // pad_id=0; insertamos un único token "1" en la posición 0 para que
        // attention_mask=1 ahí no parezca todo padding (algunos exporters
        // generan grafos que se atajan a cero si la mask es 0).
        input_ids[[0, 0]] = 1;
        let mut attention_mask = Array2::<i64>::zeros((batch, seq));
        attention_mask[[0, 0]] = 1;
        let token_type_ids = Array2::<i64>::zeros((batch, seq));

        let mut session = self.session.lock().map_err(|e| anyhow!("session lock: {e}"))?;
        let _ = if self.use_token_type_ids {
            session.run(ort::inputs![
                "input_ids" => Tensor::from_array(input_ids).map_err(ort_err)?,
                "attention_mask" => Tensor::from_array(attention_mask).map_err(ort_err)?,
                "token_type_ids" => Tensor::from_array(token_type_ids).map_err(ort_err)?,
            ]).map_err(ort_err)?
        } else {
            session.run(ort::inputs![
                "input_ids" => Tensor::from_array(input_ids).map_err(ort_err)?,
                "attention_mask" => Tensor::from_array(attention_mask).map_err(ort_err)?,
            ]).map_err(ort_err)?
        };
        Ok(())
    }

    fn run_pairs(&self, pairs: Vec<(String, String)>) -> Result<Vec<f32>> {
        let encodings = self
            .tokenizer
            .encode_batch(pairs, true)
            .map_err(|e| anyhow!("encode_batch: {e}"))?;

        let batch = encodings.len();
        // Con BatchLongest todas las encodings ya tienen el mismo length;
        // calculamos `seq` desde el primer encoding (capped por max_length).
        let seq = encodings
            .first()
            .map(|e| e.get_ids().len())
            .unwrap_or(0)
            .min(self.max_length);
        let mut input_ids = Array2::<i64>::zeros((batch, seq));
        let mut attention_mask = Array2::<i64>::zeros((batch, seq));
        let mut token_type_ids = Array2::<i64>::zeros((batch, seq));

        for (i, enc) in encodings.iter().enumerate() {
            for (j, id) in enc.get_ids().iter().take(seq).enumerate() {
                input_ids[[i, j]] = *id as i64;
            }
            for (j, m) in enc.get_attention_mask().iter().take(seq).enumerate() {
                attention_mask[[i, j]] = *m as i64;
            }
            for (j, t) in enc.get_type_ids().iter().take(seq).enumerate() {
                token_type_ids[[i, j]] = *t as i64;
            }
        }

        let mut session = self.session.lock().map_err(|e| anyhow!("session lock: {e}"))?;

        // Instrumentación iOS-only: si el primer batch tarda >>1s sospechamos
        // re-compile CoreML por shape (warmup compiló (12*N,max_length); este
        // batch puede ser otro). Sin estos logs el cuelgue es invisible.
        #[cfg(target_os = "ios")]
        let t_run = std::time::Instant::now();

        let outputs = if self.use_token_type_ids {
            session
                .run(ort::inputs![
                    "input_ids" => Tensor::from_array(input_ids).map_err(ort_err)?,
                    "attention_mask" => Tensor::from_array(attention_mask).map_err(ort_err)?,
                    "token_type_ids" => Tensor::from_array(token_type_ids).map_err(ort_err)?,
                ])
                .map_err(ort_err)?
        } else {
            session
                .run(ort::inputs![
                    "input_ids" => Tensor::from_array(input_ids).map_err(ort_err)?,
                    "attention_mask" => Tensor::from_array(attention_mask).map_err(ort_err)?,
                ])
                .map_err(ort_err)?
        };

        #[cfg(target_os = "ios")]
        eprintln!(
            "[nli] session.run batch={} seq={} → {} ms",
            batch,
            seq,
            t_run.elapsed().as_millis()
        );

        let (_shape, logits) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(ort_err)?;
        let logits_view = ndarray::ArrayView::from_shape((batch, 3), logits)?;

        let mut scores = Vec::with_capacity(batch);
        for row in logits_view.axis_iter(Axis(0)) {
            let lc = row[self.contradiction_idx];
            let le = row[self.entailment_idx];
            let mx = lc.max(le);
            let ec = (lc - mx).exp();
            let ee = (le - mx).exp();
            scores.push(ee / (ec + ee));
        }

        Ok(scores)
    }
}
