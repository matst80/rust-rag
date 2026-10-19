//! Recall ceiling: does lexical or dense retrieval put the right documents in
//! the candidate pool at all? Reranking can only reorder what retrieval found.
//!
//! Lexical runs without any model, so this is fast.
use rust_rag::cluster::{ClusterParams, Corpus, Doc};
use std::path::PathBuf;

fn label_of(text: &str) -> Option<&'static str> {
    const M: &[(&str, &[&str])] = &[
        ("k8s-helper", &["web/src/","internal/adapters","internal/orchestrator","internal/circuit","internal/kanban","qf_db_journal","questfamily"]),
        ("eventsourced-ai", &["services/","routers/","cmd/eval","resourcesession","loadsessions","gitops","evals/results","agents.md"]),
        ("nextecom", &["nextecom","base-config-params","3d configurator","derivation.tsx","configuratorstate","go-cart-actor"]),
        ("home-helper", &["home-helper","vite"]),
        ("rust-rag", &["src/api/dreaming.rs","src/db/","rag_dreaming","rag_openai","pgvector","dreaming.rs"]),
    ];
    let lower = text.to_lowercase();
    let hits: Vec<(&str,usize)> = M.iter().map(|(n,ps)| (*n, ps.iter().map(|p| lower.matches(p).count()).sum::<usize>())).filter(|(_,c)| *c>0).collect();
    if hits.len() != 1 { return None; }
    hits.iter().max_by_key(|(_,c)| *c).map(|(n,_)| *n)
}

#[test]
#[ignore]
fn recall() {
    let raw = std::fs::read_to_string("tests/fixtures_memory.jsonl").unwrap();
    let mut notes = Vec::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = match serde_json::from_str(line) { Ok(v)=>v, Err(_)=>continue };
        let text = v["text"].as_str().unwrap_or_default().to_owned();
        if let Some(l) = label_of(&text) {
            notes.push((l.to_owned(), text));
        }
    }
    println!("labelled notes: {}", notes.len());
    let anchors: Vec<usize> = (0..notes.len()).step_by(7).take(40).collect();
    let docs: Vec<Doc> = notes.iter().map(|(_,t)| Doc{id:"x".into(), text:t.clone()}).collect();
    let corpus = Corpus::new(&docs, ClusterParams::default());

    for k in [5usize, 10, 25] {
        let mut hits = 0usize; let mut total = 0usize;
        for &a in &anchors {
            let mut scored: Vec<(usize,f32)> = (0..notes.len()).filter(|&i| i!=a)
                .map(|i| { let (sim,_,ratio) = corpus.similarity_between(a,i); (i, sim * ratio) }).collect();
            scored.sort_by(|x,y| y.1.partial_cmp(&x.1).unwrap());
            for &(i,_) in scored.iter().take(k) {
                total += 1;
                if notes[i].0 == notes[a].0 { hits += 1; }
            }
        }
        println!("  lexical  P@{k:<3} = {:.3}   (reranker can at best recover this)", hits as f64/total as f64);
    }
    let _ = PathBuf::new();
}
