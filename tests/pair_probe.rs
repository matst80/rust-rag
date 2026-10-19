//! Embeds a handful of documents whose true relationship is known by reading
//! them, and prints the local cosine — for direct comparison against the
//! distances the prod API returns for the same documents.
use rust_rag::embedding::{Embedder, EmbeddingService, Pooling};
use std::path::PathBuf;

fn cos(a: &[f32], b: &[f32]) -> f32 { a.iter().zip(b).map(|(x,y)| x*y).sum() }

#[test]
#[ignore]
fn probe() {
    let ids = ["telegram_acp_ws_protocol_v1", "telegram_acp_ws_api_spec_v1",
               "019def78-d39d-7cd0-b4b3-f66dde54b490", "how_to_use_and_verify_mdns"];
    // texts pulled from prod via the CLI, cached to /tmp/probe_docs.json
    let raw = std::fs::read_to_string("/tmp/probe_docs.json").expect("probe docs");
    let docs: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/bge-m3");
    for model in ["model_int8.onnx", "model_fp16.onnx"] {
        let e = Embedder::from_paths(&root.join(model), &root.join("tokenizer.json"), 4, None)
            .unwrap().with_pooling(Pooling::Cls);
        let v: Vec<Vec<f32>> = ids.iter().map(|i| e.embed(docs[i].as_str().unwrap()).unwrap()).collect();
        println!("== {model} (cls) ==");
        println!("  protocol_v1 vs spec_v1     {:.4}", cos(&v[0], &v[1]));
        println!("  protocol_v1 vs full_spec   {:.4}", cos(&v[0], &v[2]));
        println!("  spec_v1     vs full_spec   {:.4}", cos(&v[1], &v[2]));
        println!("  mDNS        vs full_spec   {:.4}  <-- should be clearly lower", cos(&v[3], &v[2]));
    }
}
