use anyhow::Result;
use aho_corasick::AhoCorasick;
use deunicode::deunicode;
use regex::Regex;

use super::config::LexicalPatterns;

pub fn normalizar(s: &str) -> String {
    deunicode(&s.to_lowercase())
}

/// Compiled lexical index for one category. Built once at startup.
pub struct CategoryLexicon {
    ac: AhoCorasick,
    n_text_patterns: usize,
    emojis: Vec<String>,
    regex: Option<Regex>,
}

impl CategoryLexicon {
    pub fn build(patterns: &LexicalPatterns) -> Result<Self> {
        let mut text_patterns: Vec<String> = Vec::new();
        for f in &patterns.frases {
            text_patterns.push(normalizar(f));
        }
        for h in &patterns.hashtags {
            text_patterns.push(normalizar(h));
        }
        let n_text_patterns = text_patterns.len();
        let ac = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(&text_patterns)?;

        let regex = patterns
            .regex
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| Regex::new(&format!("(?i){s}")))
            .transpose()?;

        Ok(Self {
            ac,
            n_text_patterns,
            emojis: patterns.emojis.clone(),
            regex,
        })
    }

    /// Cuenta cuántos patrones únicos hacen match en `text`.
    /// (Múltiples ocurrencias del mismo patrón cuentan como 1.)
    pub fn count_matches(&self, text: &str) -> usize {
        let normalized = normalizar(text);
        let mut hits = 0usize;

        if self.n_text_patterns > 0 {
            let mut seen = vec![false; self.n_text_patterns];
            for m in self.ac.find_iter(&normalized) {
                let idx = m.pattern().as_usize();
                if let Some(slot) = seen.get_mut(idx) {
                    if !*slot {
                        *slot = true;
                        hits += 1;
                    }
                }
            }
        }

        // Emojis se comparan contra el texto raw (preservar codepoints).
        for emoji in &self.emojis {
            if text.contains(emoji.as_str()) {
                hits += 1;
            }
        }

        if let Some(rgx) = &self.regex {
            if rgx.is_match(&normalized) {
                hits += 1;
            }
        }

        hits
    }
}
