use std::collections::BTreeMap;

use anyhow::Result;

use super::config::RuntimeConfig;
use super::decide::{decidir, Decision};
use super::lexical::CategoryLexicon;
use super::nli::NliBackend;

const CONTEXT_SEP: &str = " ⋯ ";

pub struct Pipeline {
    pub cfg: RuntimeConfig,
    pub lex: BTreeMap<String, CategoryLexicon>,
    pub nli: NliBackend,
    /// Hipótesis por categoría aplanadas + neutral al final.
    pub all_hypotheses: Vec<String>,
    /// Para cada índice en all_hypotheses, su categoría (None = neutral).
    pub idx_to_cat: Vec<Option<String>>,
}

impl Pipeline {
    pub fn build(cfg: RuntimeConfig, nli: NliBackend) -> Result<Self> {
        let mut lex = BTreeMap::new();
        for cat in &cfg.category_keys {
            let patterns = cfg
                .lexical
                .get(cat)
                .ok_or_else(|| anyhow::anyhow!("falta lexical[{cat}]"))?;
            lex.insert(cat.clone(), CategoryLexicon::build(patterns)?);
        }

        let mut all_hypotheses = Vec::new();
        let mut idx_to_cat = Vec::new();
        for cat in &cfg.category_keys {
            let hyps = cfg
                .hypotheses
                .get(cat)
                .ok_or_else(|| anyhow::anyhow!("falta hypotheses[{cat}]"))?;
            for h in hyps {
                all_hypotheses.push(h.clone());
                idx_to_cat.push(Some(cat.clone()));
            }
        }
        all_hypotheses.push(cfg.neutral_hypothesis.clone());
        idx_to_cat.push(None);

        Ok(Self {
            cfg,
            lex,
            nli,
            all_hypotheses,
            idx_to_cat,
        })
    }

    pub fn classify(&self, text: &str, context: &[String]) -> Result<Decision> {
        let texto_eval = if context.is_empty() {
            text.to_string()
        } else {
            let n = context.len().min(self.cfg.max_context);
            let recientes = &context[context.len() - n..];
            let mut s = String::new();
            for c in recientes {
                s.push_str(c);
                s.push_str(CONTEXT_SEP);
            }
            s.push_str(text);
            s
        };

        // Capa 1: filtro léxico.
        let mut matches: BTreeMap<String, usize> = BTreeMap::new();
        for cat in &self.cfg.category_keys {
            let n = self.lex[cat].count_matches(&texto_eval);
            matches.insert(cat.clone(), n);
        }
        let (cat_atajo, n_atajo) = matches
            .iter()
            .max_by_key(|(_, n)| **n)
            .map(|(c, n)| (c.clone(), *n))
            .unwrap_or_default();

        if n_atajo >= 2 {
            let mut scores = BTreeMap::new();
            for cat in &self.cfg.category_keys {
                let s = if *cat == cat_atajo {
                    self.cfg.lexical_shortcut_score
                } else {
                    0.0
                };
                scores.insert(cat.clone(), s);
            }
            let (categories, action) = decidir(&scores, &self.cfg.thresholds);
            return Ok(Decision {
                action,
                categories,
                scores,
            });
        }

        // Capa 2: NLI batched (todas las hipótesis + neutral).
        let entail = self
            .nli
            .entailment_scores(&texto_eval, &self.all_hypotheses)?;
        let neutral_score = *entail.last().unwrap_or(&0.0);

        // Agregar por categoría: max - neutral, clamp a [0, ∞), boost si 1 patrón léxico.
        let mut scores: BTreeMap<String, f32> = BTreeMap::new();
        for cat in &self.cfg.category_keys {
            let mut max_score: f32 = 0.0;
            for (i, hyp_cat) in self.idx_to_cat.iter().enumerate() {
                if hyp_cat.as_deref() == Some(cat.as_str()) && entail[i] > max_score {
                    max_score = entail[i];
                }
            }
            let mut score = (max_score - neutral_score).max(0.0);
            if matches[cat] == 1 {
                score = score.max(self.cfg.lexical_boost_floor);
            }
            scores.insert(cat.clone(), score);
        }

        let (categories, action) = decidir(&scores, &self.cfg.thresholds);
        Ok(Decision {
            action,
            categories,
            scores,
        })
    }
}
