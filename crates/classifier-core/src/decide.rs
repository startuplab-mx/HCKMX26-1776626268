use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    #[serde(rename = "BLOQUEAR")]
    Bloquear,
    #[serde(rename = "AVISAR")]
    Avisar,
    #[serde(rename = "PERMITIR")]
    Permitir,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Bloquear => "BLOQUEAR",
            Action::Avisar => "AVISAR",
            Action::Permitir => "PERMITIR",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub action: Action,
    pub categories: Vec<String>,
    pub scores: BTreeMap<String, f32>,
}

/// Multi-label thresholds: cada categoría se evalúa contra su umbral.
/// La acción es la severidad más alta entre las disparadas.
pub fn decidir(
    scores: &BTreeMap<String, f32>,
    thresholds: &BTreeMap<String, f32>,
) -> (Vec<String>, Action) {
    let mut bloqueadas: Vec<String> = Vec::new();
    let mut avisadas: Vec<String> = Vec::new();

    for (cat, score) in scores {
        let th = *thresholds.get(cat).unwrap_or(&0.70);
        if *score >= th + 0.10 {
            bloqueadas.push(cat.clone());
        } else if *score >= th {
            avisadas.push(cat.clone());
        }
    }

    if !bloqueadas.is_empty() {
        bloqueadas.extend(avisadas);
        (bloqueadas, Action::Bloquear)
    } else if !avisadas.is_empty() {
        (avisadas, Action::Avisar)
    } else {
        (Vec::new(), Action::Permitir)
    }
}
