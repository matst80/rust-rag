//! Shared helpers for offline embedding/retrieval benches.
//!
//! Labelled memory notes from `tests/fixtures_memory.jsonl`. Project labels
//! derive from identifier markers only — deliberately independent of any
//! embedding being evaluated, so metrics cannot be circular.
use std::collections::HashMap;

pub const CORPUS_FIXTURE: &str = "tests/fixtures_memory.jsonl";

pub struct Note {
    pub id: String,
    pub text: String,
    pub label: String,
}

/// Project labels derived from identifier markers only — deliberately
/// independent of the embedding being evaluated, so the metric cannot be
/// circular.
pub fn label_of(text: &str) -> Option<&'static str> {
    const MARKERS: &[(&str, &[&str])] = &[
        (
            "k8s-helper",
            &[
                "web/src/",
                "internal/adapters",
                "internal/orchestrator",
                "internal/circuit",
                "internal/kanban",
                "internal/blocklist",
                "qf_db_journal",
                "questfamily",
            ],
        ),
        (
            "eventsourced-ai",
            &[
                "services/",
                "routers/",
                "cmd/eval",
                "resourcesession",
                "loadsessions",
                "gitops",
                "evals/results",
                "agents.md",
            ],
        ),
        (
            "nextecom",
            &[
                "nextecom",
                "base-config-params",
                "3d configurator",
                "derivation.tsx",
                "derivation.html",
                "configuratorstate",
                "go-cart-actor",
            ],
        ),
        ("home-helper", &["home-helper", "vite"]),
        (
            "rust-rag",
            &[
                "src/api/dreaming.rs",
                "src/db/",
                "rag_dreaming",
                "rag_openai",
                "pgvector",
                "dreaming.rs",
            ],
        ),
        ("telegram-acp", &["telegram-acp", "telegram_acp", "_acp-ws"]),
    ];
    let lower = text.to_lowercase();
    let mut hits: Vec<(&str, usize)> = MARKERS
        .iter()
        .map(|(name, pats)| {
            (
                *name,
                pats.iter().map(|p| lower.matches(p).count()).sum::<usize>(),
            )
        })
        .filter(|(_, count)| *count > 0)
        .collect();
    if hits.len() != 1 {
        // Ambiguous or unlabelled: excluded so the metric stays clean.
        return None;
    }
    hits.sort_by(|a, b| b.1.cmp(&a.1));
    Some(hits[0].0)
}

pub fn load_notes(limit: usize) -> Vec<Note> {
    let raw = std::fs::read_to_string(CORPUS_FIXTURE).expect("fixture corpus");
    let mut notes = Vec::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let text = v["text"].as_str().unwrap_or_default().to_owned();
        let label = match label_of(&text) {
            Some(l) => l.to_owned(),
            None => continue,
        };
        notes.push(Note {
            id: v["id"].as_str().unwrap_or_default().to_owned(),
            text,
            label,
        });
        if notes.len() >= limit {
            break;
        }
    }
    notes
}

pub fn label_counts(notes: &[Note]) -> HashMap<&str, usize> {
    let mut by_label: HashMap<&str, usize> = HashMap::new();
    for n in notes {
        *by_label.entry(n.label.as_str()).or_default() += 1;
    }
    by_label
}
