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
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(meta.max_length),
            direction: PaddingDirection::Right,
            pad_to_multiple_of: None,
            pad_id: 0,
            pad_type_id: 0,
            pad_token: "[PAD]".into(),
        }));

        let session = Session::builder()
            .map_err(ort_err)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(ort_err)?
            .with_intra_threads(4)
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

        let encodings = self
            .tokenizer
            .encode_batch(pairs, true)
            .map_err(|e| anyhow!("encode_batch: {e}"))?;

        let batch = encodings.len();
        let seq = self.max_length;
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
