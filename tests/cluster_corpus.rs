//! Validates the deterministic clustering against a real corpus dump.
//! Run manually: cargo test --test cluster_corpus -- --ignored --nocapture
use rust_rag::cluster::{near_duplicates, ClusterParams, Corpus, Doc};
use std::collections::BTreeSet;

fn load(path: &str) -> Vec<Doc> {
    let raw = std::fs::read_to_string(path).expect("fixture");
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).expect("json");
            Doc {
                id: v["id"].as_str().unwrap_or_default().to_owned(),
                text: v["text"].as_str().unwrap_or_default().to_owned(),
            }
        })
        .collect()
}

#[test]
#[ignore]
fn cluster_real_memory_corpus() {
    let docs = load("tests/fixtures_memory.jsonl");
    eprintln!("corpus: {} docs", docs.len());
    let f = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let params = ClusterParams {
        min_similarity: f("T_SIM", 0.28),
        min_shared_ratio: f("T_RATIO", 0.35),
        min_cohesion: f("T_COH", 0.45),
        max_df_ratio: f("T_DF", 0.10),
        max_df_min_count: f("T_DFN", 8.0) as usize,
        min_chars: f("T_CHARS", 40.0) as usize,
        ..ClusterParams::default()
    };
    let dups = near_duplicates(&docs, &params);
    eprintln!("near-duplicate pairs: {}", dups.len());

    let corpus = Corpus::new(&docs, params.clone());
    let p = corpus.partition(&docs);
    eprintln!(
        "clusters={} singletons={} junk={}",
        p.clusters.len(),
        p.singletons.len(),
        p.junk.len()
    );
    for c in p.clusters_by_size().iter().take(25) {
        eprintln!("  {:5}  {}", c.members.len(), c.label);
    }
    // coverage
    let clustered: usize = p.clusters.iter().map(|c| c.members.len()).sum();
    eprintln!(
        "coverage: clustered={clustered} ({:.1}%) singleton={} junk={}",
        100.0 * clustered as f32 / docs.len() as f32,
        p.singletons.len(),
        p.junk.len()
    );
    assert!(!p.clusters.is_empty());
}
