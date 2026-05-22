use rust_rag::code::chunker::analyze_file;
use rust_rag::code::lang::{detect_lang, Lang};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ignore::WalkBuilder;

fn main() -> anyhow::Result<()> {
    let root = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let max_bytes: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(32768);

    let mut files = 0usize;
    let mut chunks_total = 0usize;
    let mut size_buckets: BTreeMap<&str, usize> = BTreeMap::new();
    let mut kind_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut tiny_chunks: Vec<(PathBuf, String, usize)> = Vec::new();

    for entry in WalkBuilder::new(&root)
        .standard_filters(true)
        .build()
        .filter_map(Result::ok)
    {
        if !entry.file_type().map(|f| f.is_file()).unwrap_or(false) {
            continue;
        }
        let p = entry.path();
        let s = p.to_string_lossy();
        if s.contains("/.git/")
            || s.contains("/target/")
            || s.contains("/node_modules/")
            || s.contains("/.next/")
        {
            continue;
        }
        let Some(rel) = p.strip_prefix(&root).ok().map(|r| r.to_string_lossy().to_string()) else {
            continue;
        };
        let lang = detect_lang(Path::new(&rel));
        if matches!(lang, Lang::Other) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(p) else {
            continue;
        };
        let r = analyze_file(&rel, lang, &content, max_bytes);
        files += 1;
        chunks_total += r.chunks.len();
        for c in &r.chunks {
            *kind_counts.entry(c.kind.clone()).or_default() += 1;
            let n = c.byte_end - c.byte_start;
            let bucket = if n < 100 {
                "<100"
            } else if n < 500 {
                "100-500"
            } else if n < 2000 {
                "500-2k"
            } else if n < 8000 {
                "2k-8k"
            } else {
                ">=8k"
            };
            *size_buckets.entry(bucket).or_insert(0) += 1;
            if n < 100 {
                tiny_chunks.push((p.to_path_buf(), c.kind.clone(), n));
            }
        }
    }

    println!("files scanned: {files}");
    println!("chunks total : {chunks_total}");
    println!("avg/file     : {:.1}", chunks_total as f64 / files.max(1) as f64);
    println!("--- size buckets ---");
    for (k, v) in &size_buckets {
        println!("  {k:>8}: {v}");
    }
    println!("--- kinds (top 20) ---");
    let mut kinds: Vec<_> = kind_counts.into_iter().collect();
    kinds.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
    for (k, v) in kinds.iter().take(20) {
        println!("  {v:>5}  {k}");
    }
    println!("tiny chunks (<100 bytes): {}", tiny_chunks.len());
    for (p, k, n) in tiny_chunks.iter().take(10) {
        println!("  {n:>4}b  {k:>20}  {}", p.display());
    }
    Ok(())
}
