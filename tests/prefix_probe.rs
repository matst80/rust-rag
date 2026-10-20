//! Prefix-asymmetry probe: production embeds stored docs with a
//! `[repo:… path:…]` contextual prefix but embeds queries raw. For short
//! notes the template is a large share of the token sequence, which may
//! systematically offset query↔doc cosine.
//!
//! Experiment (self-retrieval, no invented relevance judgments — each note's
//! own leading words must retrieve the note itself):
//!   A. docs raw,               queries raw               (symmetric baseline)
//!   B. docs prefixed,          queries raw               (PRODUCTION)
//!   C. docs prefixed,          queries prefixed the same (symmetric upper bound)
//!
//! All docs share one repo tag so the measurement isolates the template
//! shift, not repo-clustering benefits. Metric: MRR + recall@1/@5 of the
//! source note, plus mean prefix displacement vs the ranking gap.
//!
//! Run manually (loads ~2 GB model):
//!   cargo test --test prefix_probe -- --ignored --nocapture
//! Knobs: PREFIX_PROBE_MODEL="model_int8.onnx" PREFIX_PROBE_DOCS=60
use rust_rag::api::store_search::build_contextual_embed_prefix;
use rust_rag::embedding::{Embedder, EmbeddingService, Pooling};
use std::path::PathBuf;

mod helpers;
use helpers::{label_counts, load_notes, Note};

fn head_query(text: &str) -> String {
    text.split_whitespace().take(12).collect::<Vec<_>>().join(" ")
}

/// Keyword-style query: long tokens only, order kept. Simulates a user
/// typing distinctive terms ("enduro last service") instead of pasting a
/// sentence — no verbatim 12-gram for the encoder to latch onto.
fn keyword_query(text: &str) -> String {
    text.split_whitespace()
        .filter(|w| w.chars().filter(|c| c.is_alphanumeric()).count() >= 5)
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Rank of `target` in `docs` by cosine to `query` (0-based). None when empty.
fn rank_of(query: &[f32], docs: &[Vec<f32>], target: usize) -> Option<usize> {
    if docs.is_empty() || query.is_empty() {
        return None;
    }
    let tq = cosine(query, &docs[target]);
    Some(docs.iter().filter(|d| cosine(query, d) > tq).count())
}

fn report(name: &str, ranks: &[usize]) {
    let n = ranks.len() as f64;
    let mrr: f64 = ranks.iter().map(|r| 1.0 / (*r as f64 + 1.0)).sum::<f64>() / n;
    let r1 = ranks.iter().filter(|r| **r == 0).count() as f64 / n;
    let r5 = ranks.iter().filter(|r| **r < 5).count() as f64 / n;
    println!("  {name:<28} n={:<4} MRR={:.4}  R@1={:.3}  R@5={:.3}", ranks.len(), mrr, r1, r5);
}

#[test]
#[ignore]
fn prefix_probe() {
    let limit: usize = std::env::var("PREFIX_PROBE_DOCS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let model: String =
        std::env::var("PREFIX_PROBE_MODEL").unwrap_or_else(|_| "model_int8.onnx".to_owned());
    let notes: Vec<Note> = load_notes(usize::MAX)
        .into_iter()
        .filter(|n| n.text.split_whitespace().count() >= 25)
        .take(limit)
        .collect();
    println!("notes: {} labels: {:?}", notes.len(), label_counts(&notes));
    let texts: Vec<&str> = notes.iter().map(|n| n.text.as_str()).collect();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/bge-m3");
    let embedder = Embedder::from_paths(&root.join(&model), &root.join("tokenizer.json"), 4, None)
        .expect("load embedder")
        .with_pooling(Pooling::Cls); // production pooling

    // Production prefix form: uniform repo tag for every doc.
    let meta = serde_json::json!({"repo": "notes"});
    let prefix = build_contextual_embed_prefix(&meta, None, None, None).unwrap_or_default();
    println!("production prefix: {prefix:?}");

    let embed = |t: &str| embedder.embed(t).unwrap_or_default();
    let docs_raw: Vec<Vec<f32>> = texts.iter().map(|t| embed(t)).collect();
    let docs_fx: Vec<Vec<f32>> = texts.iter().map(|t| embed(&format!("{prefix}{t}"))).collect();
    let queries: Vec<String> = texts.iter().map(|t| head_query(t)).collect();
    let q_raw: Vec<Vec<f32>> = queries.iter().map(|q| embed(q)).collect();
    let q_fx: Vec<Vec<f32>> = queries
        .iter()
        .map(|q| embed(&format!("{prefix}{q}")))
        .collect();

    let kw_queries: Vec<String> = texts.iter().map(|t| keyword_query(t)).collect();
    let kq_raw: Vec<Vec<f32>> = kw_queries.iter().map(|q| embed(q)).collect();
    let kq_fx: Vec<Vec<f32>> = kw_queries
        .iter()
        .map(|q| embed(&format!("{prefix}{q}")))
        .collect();

    // Displacement: how far does the template move a doc, relative to the
    // ranking scale? Compare against the self-retrieval margin.
    let displ: f32 = docs_raw
        .iter()
        .zip(&docs_fx)
        .map(|(r, f)| 1.0 - cosine(r, f))
        .sum::<f32>()
        / docs_raw.len() as f32;
    println!("  mean prefix displacement (cosine distance): {displ:.4}");

    let ranks = |qs: &[Vec<f32>], ds: &[Vec<f32>]| -> Vec<usize> {
        qs.iter()
            .zip(0..)
            .filter_map(|(q, i)| rank_of(q, ds, i))
            .collect()
    };
    report("A docs-raw / q-raw", &ranks(&q_raw, &docs_raw));
    report("B docs-prefix / q-raw (PROD)", &ranks(&q_raw, &docs_fx));
    report("C docs-prefix / q-prefix", &ranks(&q_fx, &docs_fx));
    println!("  --- keyword-style queries (no verbatim overlap) ---");
    report("A-kw docs-raw / q-raw", &ranks(&kq_raw, &docs_raw));
    report("B-kw docs-prefix / q-raw (PROD)", &ranks(&kq_raw, &docs_fx));
    report("C-kw docs-prefix / q-prefix", &ranks(&kq_fx, &docs_fx));

    // Topical retrieval: keyword query of note i must surface a SAME-LABEL
    // *other* note — the actual use case (self-match excluded). Rank of the
    // first same-label hit.
    println!("  --- topical: first same-label other note ---");
    let topical = |qs: &[Vec<f32>], ds: &[Vec<f32>]| -> Vec<usize> {
        qs.iter()
            .zip(0..)
            .filter_map(|(q, i)| {
                if q.is_empty() || notes.iter().filter(|n| n.label == notes[i].label).count() < 2 {
                    return None;
                }
                let mut scored: Vec<(f32, usize)> = ds
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(j, d)| (cosine(q, d), j))
                    .collect();
                scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
                scored
                    .iter()
                    .position(|(_, j)| notes[*j].label == notes[i].label)
            })
            .collect()
    };
    report("A-top docs-raw / q-raw", &topical(&kq_raw, &docs_raw));
    report("B-top docs-prefix / q-raw (PROD)", &topical(&kq_raw, &docs_fx));
    report("C-top docs-prefix / q-prefix", &topical(&kq_fx, &docs_fx));

    // Production-realistic shape: docs carry their TRUE per-doc repo tag
    // (heterogeneous prefixes), queries stay raw. If label signal bleeding
    // into geometry helps topical retrieval, B2 beats A; if per-doc template
    // variance scrambles neighborhoods, B2 loses.
    println!("  --- topical: heterogeneous per-doc repo prefix, raw queries ---");
    let docs_fx2: Vec<Vec<f32>> = notes
        .iter()
        .map(|n| {
            let m = serde_json::json!({"repo": n.label});
            let p = build_contextual_embed_prefix(&m, None, None, None).unwrap_or_default();
            embed(&format!("{p}{}", n.text))
        })
        .collect();
    report("B2-top docs-labelprefix / q-raw", &topical(&kq_raw, &docs_fx2));
}
