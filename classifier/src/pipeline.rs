use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use anyhow::Result;

use super::config::RuntimeConfig;
use super::decide::{decidir, Decision};
use super::lexical::CategoryLexicon;
use super::nli::NliBackend;

const CONTEXT_SEP: &str = " ⋯ ";
/// Capacidad máxima de la caché de decisiones por hash de (text, context).
/// 4096 entradas × ~80 B ≈ 320 KB. Cuando se excede se hace `clear()` total
/// (eviction amortizada O(1)) — bastantemente bueno para el caso de uso:
/// UI estática persiste durante toda una sesión y se vuelve a llenar al
/// instante en navegación SPA.
const DECISION_CACHE_CAP: usize = 4096;

/// Margen mínimo que la mejor categoría sospechosa debe superar a la mejor
/// hipótesis neutral para considerarse válida. Sin este margen la categoría
/// queda en 0 (no dispara) — pero su `max_score` original sí pasa al threshold
/// si el margen se cumple, manteniendo escalas comparables a las de antes.
const NEUTRAL_MARGIN: f32 = 0.10;

pub struct Pipeline {
    pub cfg: RuntimeConfig,
    pub lex: BTreeMap<String, CategoryLexicon>,
    pub nli: NliBackend,
    /// Hipótesis por categoría aplanadas + neutral al final.
    pub all_hypotheses: Vec<String>,
    /// Para cada índice en all_hypotheses, su categoría (None = neutral).
    pub idx_to_cat: Vec<Option<String>>,
    /// Caché de decisiones por hash(text + context). Evita reclasificar UI
    /// estática repetida (logs muestran "Pick a channel name…", "for more
    /// information.", etc. clasificados 5+ veces por scan).
    cache: Mutex<HashMap<u64, Decision>>,
}

fn cache_key(text: &str, context: &[String]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    // Separador para evitar colisiones text||ctx vs textc||tx.
    0u8.hash(&mut h);
    for c in context {
        c.hash(&mut h);
        0u8.hash(&mut h);
    }
    h.finish()
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
        // Pool de hipótesis neutrales: prefiere la lista; cae al string legado.
        // Si no hay ninguna, falla — sin ancla competitiva la pipeline degrada.
        let mut neutrals: Vec<String> = cfg.neutral_hypotheses.clone();
        if neutrals.is_empty() {
            if let Some(h) = &cfg.neutral_hypothesis {
                neutrals.push(h.clone());
            }
        }
        if neutrals.is_empty() {
            return Err(anyhow::anyhow!(
                "runtime config: se requiere `neutral_hypotheses` (lista) o `neutral_hypothesis` (string)"
            ));
        }
        for h in neutrals {
            all_hypotheses.push(h);
            idx_to_cat.push(None);
        }

        Ok(Self {
            cfg,
            lex,
            nli,
            all_hypotheses,
            idx_to_cat,
            cache: Mutex::new(HashMap::with_capacity(DECISION_CACHE_CAP)),
        })
    }

    pub fn classify(&self, text: &str, context: &[String]) -> Result<Decision> {
        let key = cache_key(text, context);
        if let Some(cached) = self.cache_get(key) {
            return Ok(cached);
        }
        let decision = self.classify_uncached(text, context)?;
        self.cache_put(key, &decision);
        Ok(decision)
    }

    fn cache_get(&self, key: u64) -> Option<Decision> {
        self.cache.lock().ok().and_then(|c| c.get(&key).cloned())
    }

    fn cache_put(&self, key: u64, decision: &Decision) {
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= DECISION_CACHE_CAP {
                cache.clear();
            }
            cache.insert(key, decision.clone());
        }
    }

    /// Versión batched: clasifica N textos contra el mismo `context`. Resuelve
    /// vía caché y atajo léxico cuanto se pueda; el resto se batchea en una
    /// sola pasada NLI (N × H pares en una `session.run`).
    pub fn classify_many(
        &self,
        texts: &[String],
        context: &[String],
    ) -> Result<Vec<Decision>> {
        let mut results: Vec<Option<Decision>> = vec![None; texts.len()];
        let mut keys: Vec<u64> = Vec::with_capacity(texts.len());
        let mut nli_premises: Vec<String> = Vec::new();
        let mut nli_indices: Vec<usize> = Vec::new();
        // matches[cat] precomputado por cada índice que va al NLI (para
        // reaplicar el lexical_boost_floor sin recomputar).
        let mut nli_matches: Vec<BTreeMap<String, usize>> = Vec::new();

        for (i, text) in texts.iter().enumerate() {
            let key = cache_key(text, context);
            keys.push(key);

            if let Some(cached) = self.cache_get(key) {
                results[i] = Some(cached);
                continue;
            }

            let texto_eval = self.compose_premise(text, context);

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
                let decision = self.shortcut_decision(&cat_atajo);
                self.cache_put(key, &decision);
                results[i] = Some(decision);
                continue;
            }

            nli_premises.push(texto_eval);
            nli_indices.push(i);
            nli_matches.push(matches);
        }

        if !nli_premises.is_empty() {
            let entail_batch = self
                .nli
                .entailment_scores_batch(&nli_premises, &self.all_hypotheses)?;

            for (k, entail) in entail_batch.into_iter().enumerate() {
                let i = nli_indices[k];
                let matches = &nli_matches[k];
                let decision = self.aggregate(&entail, matches);
                self.cache_put(keys[i], &decision);
                results[i] = Some(decision);
            }
        }

        Ok(results
            .into_iter()
            .map(|d| d.expect("classify_many: missing slot (bug)"))
            .collect())
    }

    fn compose_premise(&self, text: &str, context: &[String]) -> String {
        if context.is_empty() {
            return text.to_string();
        }
        let n = context.len().min(self.cfg.max_context);
        let recientes = &context[context.len() - n..];
        let mut s = String::new();
        for c in recientes {
            s.push_str(c);
            s.push_str(CONTEXT_SEP);
        }
        s.push_str(text);
        s
    }

    fn shortcut_decision(&self, cat_atajo: &str) -> Decision {
        let mut scores = BTreeMap::new();
        for cat in &self.cfg.category_keys {
            let s = if cat == cat_atajo {
                self.cfg.lexical_shortcut_score
            } else {
                0.0
            };
            scores.insert(cat.clone(), s);
        }
        let (categories, action) = decidir(&scores, &self.cfg.thresholds);
        Decision {
            action,
            categories,
            scores,
        }
    }

    fn aggregate(
        &self,
        entail: &[f32],
        matches: &BTreeMap<String, usize>,
    ) -> Decision {
        // Mejor ancla inocua: máximo entailment entre las hipótesis neutrales.
        // Funciona como GATE, no como resta — restar el max de un pool de 6
        // neutrales colapsaba demasiada señal real (texto malicioso sin
        // léxico se quedaba en ~0.15 y nunca cruzaba el umbral).
        let mut neutral_max: f32 = 0.0;
        for (i, hyp_cat) in self.idx_to_cat.iter().enumerate() {
            if hyp_cat.is_none() && entail[i] > neutral_max {
                neutral_max = entail[i];
            }
        }

        let mut scores: BTreeMap<String, f32> = BTreeMap::new();
        for cat in &self.cfg.category_keys {
            let mut max_score: f32 = 0.0;
            for (i, hyp_cat) in self.idx_to_cat.iter().enumerate() {
                if hyp_cat.as_deref() == Some(cat.as_str()) && entail[i] > max_score {
                    max_score = entail[i];
                }
            }
            // Gate: la categoría sólo entra si vence a la mejor neutral por al
            // menos NEUTRAL_MARGIN. Sin gate aplicado, score = cat_max puro
            // (que es lo que el threshold compara). Si la neutral gana, score=0.
            let passes_gate = max_score >= neutral_max + NEUTRAL_MARGIN;
            let mut score = if passes_gate { max_score } else { 0.0 };
            if matches.get(cat).copied().unwrap_or(0) == 1 {
                score = score.max(self.cfg.lexical_boost_floor);
            }
            scores.insert(cat.clone(), score);
        }
        let (categories, action) = decidir(&scores, &self.cfg.thresholds);
        Decision {
            action,
            categories,
            scores,
        }
    }

    fn classify_uncached(&self, text: &str, context: &[String]) -> Result<Decision> {
        let texto_eval = self.compose_premise(text, context);

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
            return Ok(self.shortcut_decision(&cat_atajo));
        }

        let entail = self
            .nli
            .entailment_scores(&texto_eval, &self.all_hypotheses)?;

        Ok(self.aggregate(&entail, &matches))
    }
}
