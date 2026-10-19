//! Does a cross-encoder recover the signal the bi-encoder compressed away?
//!
//! The bi-encoder puts every document in a 0.74-0.88 band regardless of
//! topic (measured: an unrelated mDNS doc scores 0.743 against a same-protocol
//! doc's 0.780). A cross-encoder is not subject to that limitation — it reads
//! the query and the passage *jointly*, so identifier-dense micro-notes still
//! match on their identifiers instead of being averaged into a direction.
//!
//! This measures the thing that actually matters in production: retrieval
//! quality at k, before and after reranking, over the same candidate pool.
//!
//! Run manually:
//!   cargo test --test rerank_bench -- --ignored --nocapture

use rust_rag::embedding::{Embedder, EmbeddingService, Pooling};
use rust_rag::reranker::Reranker;
use std::collections::HashSet;
use std::path::PathBuf;

const CORPUS_FIXTURE: &str = "tests/fixtures_memory.jsonl";
/// Candidate pool size, mirroring RAG_RERANKER_TOP_N in prod.
const TOP_N: usize = 25;

struct Note {
    id: String,
    text: String,
    label: String,
}

fn label_of(text: &str) -> Option<&'static str> {
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
        .filter(|(_, c)| *c > 0)
        .collect();
    if hits.len() != 1 {
        return None;
    }
    hits.sort_by(|a, b| b.1.cmp(&a.1));
    Some(hits[0].0)
}

fn load_notes() -> Vec<Note> {
    let raw = std::fs::read_to_string(CORPUS_FIXTURE).expect("fixture");
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            let text = v["text"].as_str().unwrap_or_default().to_owned();
            Some(Note {
                id: v["id"].as_str().unwrap_or_default().to_owned(),
                label: label_of(&text)?.to_owned(),
                text,
            })
        })
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Precision@k over anchors, for a candidate ordering.
fn precision_at(notes: &[Note], order: &[Vec<usize>], k: usize) -> f64 {
    let mut hits = 0usize;
    let mut total = 0usize;
    for (anchor, candidates) in order.iter().enumerate() {
        for &c in candidates.iter().take(k) {
            total += 1;
            if notes[c].label == notes[anchor].label {
                hits += 1;
            }
        }
    }
    hits as f64 / total.max(1) as f64
}

#[test]
#[ignore]
fn rerank_bench() {
    let notes = load_notes();
    let anchors: Vec<usize> = (0..notes.len()).step_by(7).take(40).collect();
    println!("corpus {} notes, {} anchors", notes.len(), anchors.len());

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let started = std::time::Instant::now();
    let embedder = Embedder::from_paths(
        &root.join("bge-m3/model_int8.onnx"),
        &root.join("bge-m3/tokenizer.json"),
        4,
        None,
    )
    .unwrap()
    .with_pooling(Pooling::Cls);
    let vectors: Vec<Vec<f32>> = notes.iter().map(|n| embedder.embed(&n.text).unwrap()).collect();
    println!("embedded in {:?}", started.elapsed());

    // Baseline ordering: dense top-N per anchor.
    let mut dense_top: Vec<Vec<usize>> = Vec::new();
    for &a in &anchors {
        let mut scored: Vec<(usize, f32)> = (0..notes.len())
            .filter(|&i| i != a)
            .map(|i| (i, cosine(&vectors[a], &vectors[i])))
            .collect();
        scored.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
        scored.truncate(TOP_N);
        dense_top.push(scored.into_iter().map(|(i, _)| i).collect());
    }

    println!("\nbefore rerank:");
    for k in [3usize, 5, 10, 25] {
        println!("  P@{k:<3} = {:.3}", precision_at(&notes, &dense_top, k));
    }

    println!("\nloading reranker (bge-reranker-v2-m3, ~2 GB)...");
    let started = std::time::Instant::now();
    let reranker = match rust_rag::reranker::OrtReranker::from_paths(
        &root.join("bge-reranker-v2-m3/model.onnx"),
        &root.join("bge-reranker-v2-m3/tokenizer.json"),
        4,
        512,
        None,
    ) {
        Ok(r) => r,
        Err(e) => {
            println!("  reranker load failed: {e}");
            return;
        }
    };
    println!("  loaded in {:?}", started.elapsed());

    let started = std::time::Instant::now();
    let mut reranked_top: Vec<Vec<usize>> = Vec::new();
    let mut score_gap_same = Vec::new();
    let mut score_gap_diff = Vec::new();
    for (pos, &a) in anchors.iter().enumerate() {
        let pool = &dense_top[pos];
        let passages: Vec<&str> = pool.iter().map(|&i| notes[i].text.as_str()).collect();
        let scores = reranker.rerank(notes[a].text.as_str(), &passages).unwrap_or_default();
        let mut pairs: Vec<(usize, f32)> = pool
            .iter()
            .copied()
            .zip(scores.iter().copied())
            .collect();
        for (i, sc) in &pairs {
            let i = *i;
            if notes[i].label == notes[a].label {
                score_gap_same.push(*sc);
            } else {
                score_gap_diff.push(*sc);
            }
        }
        pairs.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
        reranked_top.push(pairs.into_iter().map(|(i, _)| i).collect());
    }
    println!("  reranked {} anchors x {} passages in {:?}", anchors.len(), TOP_N, started.elapsed());

    println!("\nafter rerank:");
    for k in [3usize, 5, 10, 25] {
        println!("  P@{k:<3} = {:.3}", precision_at(&notes, &reranked_top, k));
    }

    // AUC over the same candidate pool.
    let auc = |same: &[f32], diff: &[f32]| -> f64 {
        if same.is_empty() || diff.is_empty() {
            return f64::NAN;
        }
        let mut d = diff.to_vec();
        d.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let hits: f64 = same.iter().map(|s| d.partition_point(|x| *x <= *s) as f64).sum();
        hits / (same.len() * d.len()) as f64
    };
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
    println!("\ncross-encoder AUC = {:.4}  same={:.4} other={:.4} gap={:+.4}",
        auc(&score_gap_same, &score_gap_diff),
        mean(&score_gap_same), mean(&score_gap_diff),
        mean(&score_gap_same) - mean(&score_gap_diff));

    // Sanity: are the reranked top-5 actually about the same project?
    let mut shown = 0;
    for (pos, &a) in anchors.iter().enumerate() {
        if shown >= 2 { break; }
        println!("\nanchor [{}] {}", notes[a].label, notes[a].text.chars().take(90).collect::<String>());
        for &i in reranked_top[pos].iter().take(3) {
            println!("   [{}] {}", notes[i].label, notes[i].text.chars().take(90).collect::<String>());
        }
        shown += 1;
    }
    let _ = HashSet::<usize>::new();
}