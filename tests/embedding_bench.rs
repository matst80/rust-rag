//! Embedding A/B harness: is the vector signal real, and does it beat a
//! deterministic lexical baseline?
//!
//! Motivation: on prod, retrieval measured at chance. Labelled 640 memory notes
//! by project using identifiers (independent of any embedding), then sampled 45
//! anchors and asked whether their top-10 neighbours shared a project:
//!
//!   P(same project) = 0.267, chance = 0.263, AUC = 0.409
//!
//! That is indistinguishable from random, and an unrelated document ("How to
//! use mDNS in Telegram-ACP") sat at distance 0.258 from the Telegram-ACP
//! WebSocket API spec while two documents of that *same* protocol sat at 0.144
//! and 0.211. Prod runs `RAG_MODEL_PATH=.../bge-m3/model_int8.onnx` with the
//! reranker disabled.
//!
//! So this harness answers four questions on real data, locally, on CPU:
//!   1. does fp32 differ materially from int8?
//!   2. does CLS pooling differ from mean?
//!   3. does embedding more than `text` (path/tags/title) help?
//!   4. does a deterministic TF-IDF baseline beat all of them?
//!
//! Run manually (loads ~2 GB models, several minutes):
//!   cargo test --test embedding_bench -- --ignored --nocapture
//!
//! Knobs: EMB_BENCH_MODELS="int8-cls,fp32-cls" EMB_BENCH_DOCS=200

use rust_rag::cluster::{ClusterParams, Corpus, Doc};
use rust_rag::embedding::{Embedder, EmbeddingService, Pooling};
use std::path::PathBuf;

mod helpers;
use helpers::{label_counts, load_notes, Note};

/// Project labels derived from identifier markers only — deliberately
/// independent of the embedding being evaluated, so the metric cannot be
/// All (same-label, different-label) pairs of cosine similarity, plus the AUC.
/// Sampling keeps this O(n^2) cheap.
fn score(sims_same: &[f32], sims_diff: &[f32]) -> f64 {
    if sims_same.is_empty() || sims_diff.is_empty() {
        return f64::NAN;
    }
    let mut diff = sims_diff.to_vec();
    diff.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let hits: f64 = sims_same
        .iter()
        .map(|s| diff.partition_point(|d| *d <= *s) as f64)
        .sum();
    hits / (sims_same.len() * diff.len()) as f64
}

fn summarise(name: &str, same: &[f32], diff: &[f32]) {
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
    println!(
        "  {name:<22} AUC={:.4}  same={:.4}  other={:.4}  gap={:+.4}",
        score(same, diff),
        mean(same),
        mean(diff),
        mean(same) - mean(diff),
    );
}

fn all_pairs<F>(notes: &[Note], mut sim: F) -> (Vec<f32>, Vec<f32>)
where
    F: FnMut(usize, usize) -> f32,
{
    let mut same = Vec::new();
    let mut diff = Vec::new();
    for i in 0..notes.len() {
        for j in (i + 1)..notes.len() {
            let s = sim(i, j);
            if notes[i].label == notes[j].label {
                same.push(s);
            } else {
                diff.push(s);
            }
        }
    }
    (same, diff)
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[test]
#[ignore]
fn embedding_bench() {
    let limit: usize = std::env::var("EMB_BENCH_DOCS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(160);
    let notes = load_notes(limit);
    println!("labelled notes: {}", notes.len());
    println!("labels: {:?}\n", label_counts(&notes));

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/bge-m3");
    let tokenizer = root.join("tokenizer.json");

    // ---- deterministic baseline -----------------------------------------
    {
        let docs: Vec<Doc> = notes
            .iter()
            .map(|n| Doc {
                id: n.id.clone(),
                text: n.text.clone(),
            })
            .collect();
        let corpus = Corpus::new(&docs, ClusterParams::default());
        let (same, diff) = all_pairs(&notes, |i, j| corpus.similarity_between(i, j).0);
        summarise("tfidf (deterministic)", &same, &diff);
    }

    // ---- dense variants --------------------------------------------------
    let default_models = || {
        vec![
            ("model_int8.onnx".to_owned(), "cls".to_owned()),
            ("model_fp16.onnx".to_owned(), "cls".to_owned()),
        ]
    };
    let models: Vec<(String, String)> = match std::env::var("EMB_BENCH_MODELS") {
        Ok(spec) => spec
            .split(',')
            .filter_map(|s| s.trim().split_once('-'))
            .map(|(m, p)| (m.to_owned(), p.to_owned()))
            .collect(),
        Err(_) => default_models(),
    };

    for (model, pooling) in &models {
        let path = root.join(model);
        if !path.exists() {
            println!("  {model}: missing, skipped");
            continue;
        }
        println!("embedding {model} / {pooling} ...");
        let started = std::time::Instant::now();
        let pool_name = pooling;
        let pooling = match pool_name.as_str() {
            "mean" => Pooling::Mean,
            _ => Pooling::Cls,
        };
        let embedder = match Embedder::from_paths(&path, &tokenizer, 4, None) {
            Ok(e) => e.with_pooling(pooling),
            Err(e) => {
                println!("  {model}: load failed: {e}");
                continue;
            }
        };
        let vectors: Vec<Vec<f32>> = notes
            .iter()
            .map(|n| embedder.embed(&n.text).unwrap_or_default())
            .collect();
        println!("  embedded in {:?}", started.elapsed());

        let (same, diff) = all_pairs(&notes, |i, j| cosine(&vectors[i], &vectors[j]));
        summarise(&format!("{model}/{pool_name}"), &same, &diff);

        // Does folding in the identifying metadata help? The notes carry no
        // path or tags in this corpus, so we simulate the enriched form by
        // prefixing the doc with its project label plus repeated rare tokens —
        // an upper bound on what metadata enrichment could buy.
        let enriched: Vec<Vec<f32>> = notes
            .iter()
            .map(|n| {
                let mut text = format!("[{}] {}", n.label, n.text);
                text.push_str(&format!(" {}", n.text));
                embedder.embed(&text).unwrap_or_default()
            })
            .collect();
        let (same, diff) = all_pairs(&notes, |i, j| cosine(&enriched[i], &enriched[j]));
        summarise(&format!("{model}/{pool_name}+meta"), &same, &diff);
    }
}