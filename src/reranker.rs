//! Cross-encoder reranker: scores `(query, passage)` pairs, used to
//! reorder the top-N candidates from the hybrid retriever.
//!
//! Default backend is `OrtReranker`, wrapping an ONNX export of
//! `BAAI/bge-reranker-v2-m3` (XLM-RoBERTa-based). The session is separate
//! from the embedder's session so retrieval and reranking can stream in
//! parallel; both share the same GPU via the ORT CUDA EP.
//!
//! VRAM: bge-m3 dense+sparse ~2.3 GB + reranker ~1.1 GB fp16 fits the
//! GTX 1660 SUPER's 6 GB on `sunk` with comfortable headroom for the CUDA
//! arena. Reranker stays optional via `RAG_RERANKER_ENABLED=true`.

use anyhow::{Context, Result, anyhow};
use ndarray::Array2;
use ort::{
    execution_providers::CPUExecutionProvider, inputs, session::Session,
    session::builder::GraphOptimizationLevel, value::TensorRef,
};
use std::{
    path::Path,
    sync::Mutex,
    time::Instant,
};
use tokenizers::{EncodeInput, PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

/// Score `(query, passage_i)` cross-encoder pairs in a single batched
/// forward pass. Returns one score per passage, in input order.
/// Higher = more relevant; range depends on the export but bge-reranker-v2-m3
/// post-sigmoid lands in `[0, 1]`.
pub trait Reranker: Send + Sync {
    fn rerank(&self, query: &str, passages: &[&str]) -> Result<Vec<f32>>;
}

pub struct OrtReranker {
    tokenizer: Tokenizer,
    session: Mutex<Session>,
    /// Most cross-encoders take 2 inputs (input_ids, attention_mask) on
    /// XLM-RoBERTa or 3 (+ token_type_ids) on BERT. Detected at load.
    accepts_token_type_ids: bool,
    max_length: usize,
}

impl OrtReranker {
    pub fn from_paths(
        model_path: &Path,
        tokenizer_path: &Path,
        intra_threads: usize,
        max_length: usize,
        ort_dylib_path: Option<&Path>,
    ) -> Result<Self> {
        let started = Instant::now();
        println!(
            "reranker: loading tokenizer from {}",
            tokenizer_path.display()
        );
        let mut tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|error| anyhow!(error.to_string()))
            .with_context(|| format!("loading tokenizer from {}", tokenizer_path.display()))?;
        // Pair encoding needs truncation on the longest segment so a
        // multi-paragraph passage doesn't blow past 512 tokens.
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length,
                strategy: tokenizers::TruncationStrategy::LongestFirst,
                stride: 0,
                direction: tokenizers::TruncationDirection::Right,
            }))
            .map_err(|error| anyhow!(error.to_string()))?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            direction: tokenizers::PaddingDirection::Right,
            pad_to_multiple_of: None,
            pad_id: 1, // XLM-R `<pad>`
            pad_type_id: 0,
            pad_token: "<pad>".to_owned(),
        }));
        println!(
            "reranker: tokenizer loaded in {:?}",
            started.elapsed()
        );

        crate::embedding::initialize_ort_for_reranker(ort_dylib_path)?;

        let mut builder = Session::builder()
            .map_err(ort_error)?
            .with_execution_providers(execution_providers())
            .map_err(ort_error)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(ort_error)?
            .with_intra_threads(intra_threads)
            .map_err(ort_error)?;
        if let Ok(level_str) = std::env::var("RAG_ORT_LOG_LEVEL") {
            if let Some(level) = match level_str.to_ascii_lowercase().as_str() {
                "verbose" | "v" | "0" => Some(ort::logging::LogLevel::Verbose),
                "info" | "1" => Some(ort::logging::LogLevel::Info),
                "warning" | "warn" | "2" => Some(ort::logging::LogLevel::Warning),
                "error" | "3" => Some(ort::logging::LogLevel::Error),
                "fatal" | "4" => Some(ort::logging::LogLevel::Fatal),
                _ => None,
            } {
                builder = builder.with_log_level(level).map_err(ort_error)?;
            }
        }

        println!(
            "reranker: committing model from {}",
            model_path.display()
        );
        let commit_started = Instant::now();
        let session = builder
            .commit_from_file(model_path)
            .map_err(ort_error)
            .with_context(|| {
                format!("failed to load reranker ONNX from {}", model_path.display())
            })?;
        println!(
            "reranker: model committed in {:?}",
            commit_started.elapsed()
        );

        let accepts_token_type_ids = session
            .inputs()
            .iter()
            .any(|i| i.name() == "token_type_ids");
        println!(
            "reranker: model inputs = [{}] (token_type_ids={})",
            session
                .inputs()
                .iter()
                .map(|i| i.name())
                .collect::<Vec<_>>()
                .join(", "),
            accepts_token_type_ids
        );

        Ok(Self {
            tokenizer,
            session: Mutex::new(session),
            accepts_token_type_ids,
            max_length,
        })
    }
}

impl Reranker for OrtReranker {
    fn rerank(&self, query: &str, passages: &[&str]) -> Result<Vec<f32>> {
        if passages.is_empty() {
            return Ok(Vec::new());
        }

        let pairs: Vec<EncodeInput<'_>> = passages
            .iter()
            .map(|p| EncodeInput::Dual(query.into(), (*p).into()))
            .collect();
        let encodings = self
            .tokenizer
            .encode_batch_fast(pairs, true)
            .map_err(|e| anyhow!(e.to_string()))
            .context("tokenizing reranker pairs")?;

        let batch = encodings.len();
        let seq_len = encodings.first().map(|e| e.len()).unwrap_or(0).max(1);

        let mut input_ids: Vec<i64> = Vec::with_capacity(batch * seq_len);
        let mut attn_mask: Vec<i64> = Vec::with_capacity(batch * seq_len);
        let mut tti: Vec<i64> = if self.accepts_token_type_ids {
            Vec::with_capacity(batch * seq_len)
        } else {
            Vec::new()
        };

        for enc in &encodings {
            for &id in enc.get_ids() {
                input_ids.push(id as i64);
            }
            for &m in enc.get_attention_mask() {
                attn_mask.push(m as i64);
            }
            if self.accepts_token_type_ids {
                for &t in enc.get_type_ids() {
                    tti.push(t as i64);
                }
            }
            // Defensive — encode_batch_fast pads to longest, so all rows
            // should already match seq_len. Truncate or pad if not.
            let actual = enc.len();
            if actual < seq_len {
                input_ids.resize(input_ids.len() + (seq_len - actual), 1); // <pad>
                attn_mask.resize(attn_mask.len() + (seq_len - actual), 0);
                if self.accepts_token_type_ids {
                    tti.resize(tti.len() + (seq_len - actual), 0);
                }
            }
        }

        let input_ids_arr = Array2::from_shape_vec((batch, seq_len), input_ids)?;
        let attn_arr = Array2::from_shape_vec((batch, seq_len), attn_mask)?;
        let tti_arr = if self.accepts_token_type_ids {
            Array2::from_shape_vec((batch, seq_len), tti)?
        } else {
            Array2::zeros((batch, seq_len))
        };

        let mut session = self.session.lock().expect("reranker session mutex poisoned");
        let input_ids_t = TensorRef::from_array_view(input_ids_arr.view()).map_err(ort_error)?;
        let attn_t = TensorRef::from_array_view(attn_arr.view()).map_err(ort_error)?;
        let outputs = if self.accepts_token_type_ids {
            let tti_t = TensorRef::from_array_view(tti_arr.view()).map_err(ort_error)?;
            session
                .run(inputs![
                    "input_ids" => input_ids_t,
                    "attention_mask" => attn_t,
                    "token_type_ids" => tti_t,
                ])
                .map_err(ort_error)?
        } else {
            session
                .run(inputs![
                    "input_ids" => input_ids_t,
                    "attention_mask" => attn_t,
                ])
                .map_err(ort_error)?
        };

        // Reranker outputs `logits` shape (batch, 1) (or (batch,) collapsed).
        // Pull the first output by name when present, fall back to index 0.
        let logits_view = outputs
            .get("logits")
            .unwrap_or(&outputs[0])
            .try_extract_array::<f32>()
            .map_err(ort_error)?;

        // Squeeze trailing 1-dim and apply sigmoid for [0, 1] scores.
        let scores: Vec<f32> = logits_view
            .iter()
            .copied()
            .map(|v| 1.0 / (1.0 + (-v).exp()))
            .collect();

        if scores.len() != batch {
            anyhow::bail!(
                "reranker output had {} scores for batch of {}",
                scores.len(),
                batch
            );
        }

        let _ = self.max_length; // silence unused warning when max_length is only consumed via tokenizer config
        Ok(scores)
    }
}

#[cfg(feature = "cuda")]
fn execution_providers() -> Vec<ort::execution_providers::ExecutionProviderDispatch> {
    use ort::ep::{ArenaExtendStrategy, CUDA, cuda::ConvAlgorithmSearch};

    let mem_limit_mb: usize = std::env::var("RAG_RERANKER_CUDA_MEM_LIMIT_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2048);
    let device_id: i32 = std::env::var("RAG_CUDA_DEVICE_ID")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let strict = std::env::var("RAG_CUDA_STRICT")
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    let cuda = CUDA::default()
        .with_device_id(device_id)
        .with_memory_limit(mem_limit_mb * 1024 * 1024)
        .with_arena_extend_strategy(ArenaExtendStrategy::SameAsRequested)
        .with_conv_algorithm_search(ConvAlgorithmSearch::Heuristic)
        .with_conv_max_workspace(false)
        .build();
    let cuda = if strict { cuda.error_on_failure() } else { cuda };

    println!(
        "reranker: registering CUDA EP (device={device_id}, mem_limit={mem_limit_mb}MiB, strict={strict}) with CPU fallback"
    );
    vec![cuda, CPUExecutionProvider::default().build()]
}

#[cfg(not(feature = "cuda"))]
fn execution_providers() -> Vec<ort::execution_providers::ExecutionProviderDispatch> {
    vec![CPUExecutionProvider::default().build()]
}

fn ort_error<E: std::fmt::Display>(error: E) -> anyhow::Error {
    anyhow!(error.to_string())
}

