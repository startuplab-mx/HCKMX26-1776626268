use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LexicalPatterns {
    #[serde(default)]
    pub frases: Vec<String>,
    #[serde(default)]
    pub emojis: Vec<String>,
    #[serde(default)]
    pub hashtags: Vec<String>,
    #[serde(default)]
    pub regex: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestCase {
    pub text: String,
    #[serde(default)]
    pub expected: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContextTestCase {
    pub messages: Vec<String>,
    #[serde(default)]
    pub expected: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeConfig {
    pub model_id: String,
    pub category_keys: Vec<String>,
    pub hypotheses: BTreeMap<String, Vec<String>>,
    pub lexical: BTreeMap<String, LexicalPatterns>,
    /// Single neutral hypothesis (legacy). Kept for backwards compat.
    /// Prefer `neutral_hypotheses`.
    #[serde(default)]
    pub neutral_hypothesis: Option<String>,
    /// Pool of competing neutral anchors. Empty list falls back to
    /// `neutral_hypothesis` if present.
    #[serde(default)]
    pub neutral_hypotheses: Vec<String>,
    pub thresholds: BTreeMap<String, f32>,
    #[serde(default)]
    pub test_cases: Vec<TestCase>,
    #[serde(default)]
    pub context_test_cases: Vec<ContextTestCase>,
    pub lexical_shortcut_score: f32,
    pub lexical_boost_floor: f32,
    pub max_context: usize,
}

impl RuntimeConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("leyendo runtime config: {}", path.display()))?;
        let cfg: RuntimeConfig = serde_json::from_slice(&bytes)?;
        Ok(cfg)
    }
}
