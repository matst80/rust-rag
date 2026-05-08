use anyhow::{Context, Result, anyhow};
use ndarray::{Array2, Array3, Axis};
use ort::{
    execution_providers::CPUExecutionProvider, inputs, session::Session,
    session::builder::GraphOptimizationLevel, value::TensorRef,
};
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
    time::Instant,
};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

/// Pooling strategy applied to the model's last hidden state.
///
/// - `Mean`: average over non-padding tokens, then L2-normalize. The previous
///   default; works adequately for any encoder but isn't the reference
///   strategy for most BAAI checkpoints.
/// - `Cls`: take `last_hidden_state[:, 0]` (the CLS / `<s>` token), then
///   L2-normalize. Reference for bge-m3, bge-small/base/large, and most
///   `sentence-transformers` checkpoints derived from BERT/RoBERTa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pooling {
    Mean,
    Cls,
}

impl Default for Pooling {
    fn default() -> Self {
        Self::Mean
    }
}

impl std::str::FromStr for Pooling {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mean" | "avg" | "average" => Ok(Self::Mean),
            "cls" | "first" => Ok(Self::Cls),
            other => anyhow::bail!("unknown pooling strategy: {other}"),
        }
    }
}

/// Sparse embedding produced by the bge-m3 sparse head: a list of
/// `(vocab_id, weight)` pairs after the per-token aggregation
/// (max-pool by token id, special tokens dropped, sub-threshold
/// weights filtered). Maps onto Postgres `sparsevec(250002)`.
pub type SparseEmbedding = Vec<(u32, f32)>;

pub trait EmbeddingService: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Compute dense + sparse embeddings in a single forward pass.
    /// Default implementation returns the dense vector and an empty sparse
    /// vector — adequate for backends that don't (yet) export the sparse
    /// head. The Postgres write path treats an empty sparse as NULL.
    fn embed_both(&self, text: &str) -> Result<(Vec<f32>, SparseEmbedding)> {
        let dense = self.embed(text)?;
        Ok((dense, Vec::new()))
    }

    /// Count tokens for `text` without the truncation/padding the embedder
    /// applies during inference. Used by admin tools to display real token
    /// budgets per item.
    fn count_tokens(&self, text: &str) -> Result<usize>;
}

/// Result of one forward pass through the embedding model.
///
/// `dense` is `last_hidden_state` `(batch, seq, hidden)`.
/// `sparse_logits` is the post-ReLU per-token sparse output
/// `(batch, seq, 1)` from bge-m3's sparse head — `None` for models that
/// don't expose it (bge-small, dense-only ONNX exports). Aggregation into
/// `(vocab_id, weight)` pairs happens in `Embedder` so backends stay
/// model-agnostic.
#[derive(Debug)]
pub struct RunOutput {
    pub dense: Array3<f32>,
    pub sparse_logits: Option<Array3<f32>>,
}

pub trait InferenceBackend: Send + Sync {
    fn run(
        &self,
        input_ids: Array2<i64>,
        attention_mask: Array2<i64>,
        token_type_ids: Array2<i64>,
    ) -> Result<RunOutput>;

    /// Whether the underlying model produces sparse logits. Cheap; called
    /// at startup to log capability and let `embed_both` short-circuit.
    fn has_sparse(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub struct TokenizedInput {
    pub input_ids: Array2<i64>,
    pub attention_mask: Array2<i64>,
    pub attention_mask_values: Vec<i64>,
    pub token_type_ids: Array2<i64>,
}

pub struct Embedder<B = OrtBackend> {
    tokenizer: Tokenizer,
    /// Untruncated, unpadded clone of the same tokenizer used for accurate
    /// token-count reporting. Inference path keeps `tokenizer` (clamped to
    /// 512) so this avoids re-encoding twice in the hot path.
    count_tokenizer: Tokenizer,
    backend: B,
    pooling: Pooling,
}

impl Embedder<OrtBackend> {
    pub fn from_paths(
        model_path: &Path,
        tokenizer_path: &Path,
        intra_threads: usize,
        ort_dylib_path: Option<&Path>,
    ) -> Result<Self> {
        let started = Instant::now();
        println!(
            "embedder: loading tokenizer from {}",
            tokenizer_path.display()
        );
        let mut tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|error| anyhow!(error.to_string()))
            .with_context(|| {
                format!("failed to load tokenizer from {}", tokenizer_path.display())
            })?;
        // Snapshot the raw tokenizer (no truncation/padding) for token counting.
        let count_tokenizer = tokenizer.clone();
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(512),
            ..Default::default()
        }));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: 512,
                ..Default::default()
            }))
            .map_err(|error| anyhow!(error.to_string()))
            .context("failed to configure tokenizer truncation")?;
        println!("embedder: tokenizer loaded in {:?}", started.elapsed());

        let backend = OrtBackend::new(model_path, intra_threads, ort_dylib_path)?;
        Ok(Self {
            tokenizer,
            count_tokenizer,
            backend,
            pooling: Pooling::default(),
        })
    }
}

impl<B> Embedder<B>
where
    B: InferenceBackend,
{
    pub fn with_backend(tokenizer: Tokenizer, backend: B) -> Self {
        let count_tokenizer = tokenizer.clone();
        Self {
            tokenizer,
            count_tokenizer,
            backend,
            pooling: Pooling::default(),
        }
    }

    pub fn with_pooling(mut self, pooling: Pooling) -> Self {
        self.pooling = pooling;
        self
    }

    pub fn tokenize(&self, text: &str) -> Result<TokenizedInput> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|error| anyhow!(error.to_string()))
            .context("failed to tokenize input")?;

        let sequence_len = encoding.len();
        if sequence_len == 0 {
            anyhow::bail!("tokenizer returned an empty sequence");
        }

        let input_ids = encoding
            .get_ids()
            .iter()
            .map(|value| i64::from(*value))
            .collect::<Vec<_>>();
        let attention_mask_values = encoding
            .get_attention_mask()
            .iter()
            .map(|value| i64::from(*value))
            .collect::<Vec<_>>();
        let token_type_values = if encoding.get_type_ids().is_empty() {
            vec![0_i64; sequence_len]
        } else {
            encoding
                .get_type_ids()
                .iter()
                .map(|value| i64::from(*value))
                .collect::<Vec<_>>()
        };

        Ok(TokenizedInput {
            input_ids: Array2::from_shape_vec((1, sequence_len), input_ids)?,
            attention_mask: Array2::from_shape_vec(
                (1, sequence_len),
                attention_mask_values.clone(),
            )?,
            attention_mask_values,
            token_type_ids: Array2::from_shape_vec((1, sequence_len), token_type_values)?,
        })
    }
}

impl<B> EmbeddingService for Embedder<B>
where
    B: InferenceBackend,
{
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let tokenized = self.tokenize(text)?;
        let attention_mask_values = tokenized.attention_mask_values.clone();
        let output = self.backend.run(
            tokenized.input_ids,
            tokenized.attention_mask,
            tokenized.token_type_ids,
        )?;

        let hidden_state = output.dense.index_axis(Axis(0), 0);

        match self.pooling {
            Pooling::Mean => mean_pool_and_normalize(hidden_state, &attention_mask_values),
            Pooling::Cls => cls_pool_and_normalize(hidden_state),
        }
    }

    fn embed_both(&self, text: &str) -> Result<(Vec<f32>, SparseEmbedding)> {
        let tokenized = self.tokenize(text)?;
        let attention_mask_values = tokenized.attention_mask_values.clone();
        // Keep input_ids around for the sparse aggregator before they move
        // into `backend.run`.
        let input_ids_for_sparse = tokenized.input_ids.row(0).to_vec();
        let output = self.backend.run(
            tokenized.input_ids,
            tokenized.attention_mask,
            tokenized.token_type_ids,
        )?;

        let dense = {
            let hidden_state = output.dense.index_axis(Axis(0), 0);
            match self.pooling {
                Pooling::Mean => mean_pool_and_normalize(hidden_state, &attention_mask_values)?,
                Pooling::Cls => cls_pool_and_normalize(hidden_state)?,
            }
        };

        let sparse = match output.sparse_logits {
            Some(logits) => aggregate_sparse(
                logits.index_axis(Axis(0), 0),
                &input_ids_for_sparse,
                &attention_mask_values,
            ),
            None => Vec::new(),
        };

        Ok((dense, sparse))
    }

    fn count_tokens(&self, text: &str) -> Result<usize> {
        let encoding = self
            .count_tokenizer
            .encode(text, true)
            .map_err(|error| anyhow!(error.to_string()))
            .context("failed to tokenize input for count")?;
        Ok(encoding.len())
    }
}

pub struct OrtBackend {
    session: Mutex<Session>,
    /// XLM-RoBERTa-family models (bge-m3) don't expose token_type_ids; BERT
    /// (bge-small) does. Detected from the session's declared inputs at load.
    accepts_token_type_ids: bool,
    /// True when the model graph declares a `sparse_logits` output
    /// (bge-m3 sparse-export). When false, only dense is produced and
    /// `embed_both` returns empty sparse.
    has_sparse_output: bool,
}

impl OrtBackend {
    pub fn new(
        model_path: &Path,
        intra_threads: usize,
        ort_dylib_path: Option<&Path>,
    ) -> Result<Self> {
        let started = Instant::now();
        println!("embedder: initializing ort runtime");
        initialize_ort(ort_dylib_path)?;
        println!(
            "embedder: ort runtime initialized in {:?}",
            started.elapsed()
        );

        let builder_started = Instant::now();
        println!("embedder: creating session builder");
        let mut builder = Session::builder()
            .map_err(ort_error)?
            .with_execution_providers(execution_providers())
            .map_err(ort_error)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(ort_error)?
            .with_intra_threads(intra_threads)
            .map_err(ort_error)?;
        if let Ok(level_str) = std::env::var("RAG_ORT_LOG_LEVEL") {
            let level = match level_str.to_ascii_lowercase().as_str() {
                "verbose" | "v" | "0" => Some(ort::logging::LogLevel::Verbose),
                "info" | "1" => Some(ort::logging::LogLevel::Info),
                "warning" | "warn" | "2" => Some(ort::logging::LogLevel::Warning),
                "error" | "3" => Some(ort::logging::LogLevel::Error),
                "fatal" | "4" => Some(ort::logging::LogLevel::Fatal),
                _ => None,
            };
            if let Some(level) = level {
                println!("embedder: setting ORT log level to {level:?}");
                builder = builder.with_log_level(level).map_err(ort_error)?;
            }
        }
        println!(
            "embedder: session builder configured in {:?}",
            builder_started.elapsed()
        );
        println!("embedder: committing model from {}", model_path.display());
        let commit_started = Instant::now();
        let session = builder
            .commit_from_file(model_path)
            .map_err(ort_error)
            .with_context(|| format!("failed to load ONNX model from {}", model_path.display()))?;
        println!(
            "embedder: model committed in {:?}",
            commit_started.elapsed()
        );

        let accepts_token_type_ids = session
            .inputs()
            .iter()
            .any(|i| i.name() == "token_type_ids");
        let has_sparse_output = session
            .outputs()
            .iter()
            .any(|o| o.name() == "sparse_logits");
        println!(
            "embedder: model inputs = [{}] (token_type_ids={})",
            session
                .inputs()
                .iter()
                .map(|i| i.name())
                .collect::<Vec<_>>()
                .join(", "),
            accepts_token_type_ids
        );
        println!(
            "embedder: model outputs = [{}] (sparse_logits={})",
            session
                .outputs()
                .iter()
                .map(|o| o.name())
                .collect::<Vec<_>>()
                .join(", "),
            has_sparse_output
        );

        Ok(Self {
            session: Mutex::new(session),
            accepts_token_type_ids,
            has_sparse_output,
        })
    }
}

impl InferenceBackend for OrtBackend {
    fn run(
        &self,
        input_ids: Array2<i64>,
        attention_mask: Array2<i64>,
        token_type_ids: Array2<i64>,
    ) -> Result<RunOutput> {
        let mut session = self.session.lock().expect("ort session mutex poisoned");
        let input_ids_t = TensorRef::from_array_view(input_ids.view()).map_err(ort_error)?;
        let attn_t = TensorRef::from_array_view(attention_mask.view()).map_err(ort_error)?;
        let outputs = if self.accepts_token_type_ids {
            let tti_t = TensorRef::from_array_view(token_type_ids.view()).map_err(ort_error)?;
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

        // Pull dense by name when present, fall back to first output for the
        // legacy single-output dense-only export.
        let dense_output = outputs
            .get("last_hidden_state")
            .unwrap_or(&outputs[0]);
        let dense = dense_output
            .try_extract_array::<f32>()
            .map_err(ort_error)?
            .to_owned()
            .into_dimensionality::<ndarray::Ix3>()?;

        let sparse_logits = if self.has_sparse_output {
            let s = outputs
                .get("sparse_logits")
                .ok_or_else(|| anyhow!("declared sparse_logits output missing at run time"))?
                .try_extract_array::<f32>()
                .map_err(ort_error)?
                .to_owned()
                .into_dimensionality::<ndarray::Ix3>()?;
            Some(s)
        } else {
            None
        };

        Ok(RunOutput { dense, sparse_logits })
    }

    fn has_sparse(&self) -> bool {
        self.has_sparse_output
    }
}

#[cfg(feature = "cuda")]
fn execution_providers() -> Vec<ort::execution_providers::ExecutionProviderDispatch> {
    use ort::ep::{ArenaExtendStrategy, CUDA, cuda::ConvAlgorithmSearch};

    let mem_limit_mb: usize = std::env::var("RAG_CUDA_MEM_LIMIT_MB")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2048);
    let device_id: i32 = std::env::var("RAG_CUDA_DEVICE_ID")
        .ok()
        .and_then(|value| value.parse().ok())
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
        "embedder: registering CUDA EP (device={device_id}, mem_limit={mem_limit_mb}MiB, strict={strict}) with CPU fallback"
    );
    vec![cuda, CPUExecutionProvider::default().build()]
}

#[cfg(not(feature = "cuda"))]
fn execution_providers() -> Vec<ort::execution_providers::ExecutionProviderDispatch> {
    vec![CPUExecutionProvider::default().build()]
}

/// Reranker shares the embedder's process-wide `ort::init()` call. Exposed
/// so `src/reranker.rs` can lazily lazily ensure the runtime is up before
/// committing its own session.
pub fn initialize_ort_for_reranker(ort_dylib_path: Option<&Path>) -> Result<()> {
    initialize_ort(ort_dylib_path)
}

fn initialize_ort(ort_dylib_path: Option<&Path>) -> Result<()> {
    static ORT_INITIALIZED: OnceLock<()> = OnceLock::new();

    if ORT_INITIALIZED.get().is_some() {
        return Ok(());
    }

    #[cfg(feature = "cuda")]
    {
        if let Some(path) = ort_dylib_path {
            // SAFETY: writing the env var before `ort::init()` reads it.
            unsafe { std::env::set_var("ORT_DYLIB_PATH", path); }
        }
        let resolved = std::env::var("ORT_DYLIB_PATH").unwrap_or_default();
        println!("embedder: load-dynamic ORT_DYLIB_PATH={resolved}");
    }
    #[cfg(not(feature = "cuda"))]
    {
        if let Some(path) = ort_dylib_path {
            println!(
                "embedder: ignoring explicit ort dylib {} (build uses ort download-binaries)",
                path.display()
            );
        } else {
            println!("embedder: using ort bundled binary discovery");
        }
    }
    ort::init().with_name("rust-rag").commit();

    let _ = ORT_INITIALIZED.set(());
    Ok(())
}

fn cls_pool_and_normalize(
    hidden_state: ndarray::ArrayView2<'_, f32>,
) -> Result<Vec<f32>> {
    if hidden_state.nrows() == 0 {
        anyhow::bail!("hidden state had zero rows; cannot read CLS token");
    }
    let cls = hidden_state.row(0);
    let mut pooled: Vec<f32> = cls.iter().copied().collect();
    let norm = pooled.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm == 0.0 {
        anyhow::bail!("CLS embedding norm was zero before normalization");
    }
    for v in &mut pooled {
        *v /= norm;
    }
    Ok(pooled)
}

fn mean_pool_and_normalize(
    hidden_state: ndarray::ArrayView2<'_, f32>,
    attention_mask: &[i64],
) -> Result<Vec<f32>> {
    if hidden_state.nrows() != attention_mask.len() {
        anyhow::bail!(
            "hidden state rows ({}) do not match attention mask length ({})",
            hidden_state.nrows(),
            attention_mask.len()
        );
    }

    let mut pooled = vec![0.0_f32; hidden_state.ncols()];
    let mut token_count = 0.0_f32;

    for (token_index, token_embedding) in hidden_state.outer_iter().enumerate() {
        if attention_mask[token_index] == 0 {
            continue;
        }

        for (index, value) in token_embedding.iter().enumerate() {
            pooled[index] += *value;
        }
        token_count += 1.0;
    }

    if token_count == 0.0 {
        anyhow::bail!("attention mask did not include any tokens");
    }

    for value in &mut pooled {
        *value /= token_count;
    }

    let norm = pooled.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm == 0.0 {
        anyhow::bail!("embedding norm was zero after pooling");
    }

    for value in &mut pooled {
        *value /= norm;
    }

    Ok(pooled)
}

fn ort_error<E>(error: E) -> anyhow::Error
where
    E: std::fmt::Display,
{
    anyhow!(error.to_string())
}

/// XLM-RoBERTa (bge-m3 tokenizer) reserved ids that must not contribute to
/// the sparse signal: `<s>`(0), `<pad>`(1), `</s>`(2), `<unk>`(3), and
/// `<mask>`(250001 — last vocab slot). FlagEmbedding's
/// `_process_token_weights` filters these via the tokenizer's special-tokens
/// set; we hard-code them because the set is invariant across bge-m3
/// checkpoints and the alternative is plumbing the tokenizer into the
/// aggregator just to recompute the same five ids.
const XLMR_SPECIAL_IDS: &[i64] = &[0, 1, 2, 3, 250001];
const SPARSE_WEIGHT_THRESHOLD: f32 = 1e-6;

/// Aggregate per-token sparse logits into `(vocab_id, weight)` pairs by
/// max-pooling over positions sharing the same `input_id`. Mirrors
/// `BGEM3FlagModel._process_token_weights`: drop padding (via attention
/// mask), drop reserved tokens, take the per-token max of the relu'd
/// logits, threshold tiny values, return one entry per unique vocab id.
///
/// `sparse_logits` shape: `(seq_len, 1)`.
/// `input_ids`     shape: `(seq_len,)`.
/// `attention_mask` shape: `(seq_len,)`.
fn aggregate_sparse(
    sparse_logits: ndarray::ArrayView2<'_, f32>,
    input_ids: &[i64],
    attention_mask: &[i64],
) -> SparseEmbedding {
    debug_assert_eq!(sparse_logits.nrows(), input_ids.len());
    debug_assert_eq!(input_ids.len(), attention_mask.len());

    use std::collections::HashMap;
    let mut by_id: HashMap<u32, f32> = HashMap::new();

    for (pos, &token_id) in input_ids.iter().enumerate() {
        if attention_mask.get(pos).copied().unwrap_or(0) == 0 {
            continue;
        }
        if XLMR_SPECIAL_IDS.contains(&token_id) {
            continue;
        }
        if token_id < 0 {
            continue;
        }
        let weight = sparse_logits[(pos, 0)];
        if !weight.is_finite() || weight < SPARSE_WEIGHT_THRESHOLD {
            continue;
        }
        let key = token_id as u32;
        let entry = by_id.entry(key).or_insert(0.0);
        if weight > *entry {
            *entry = weight;
        }
    }

    let mut out: Vec<(u32, f32)> = by_id.into_iter().collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::AHashMap;
    use tokenizers::{
        Tokenizer, models::wordlevel::WordLevel, pre_tokenizers::whitespace::Whitespace,
    };

    struct MockBackend {
        output: Array3<f32>,
    }

    impl InferenceBackend for MockBackend {
        fn run(
            &self,
            _input_ids: Array2<i64>,
            _attention_mask: Array2<i64>,
            _token_type_ids: Array2<i64>,
        ) -> Result<RunOutput> {
            Ok(RunOutput {
                dense: self.output.clone(),
                sparse_logits: None,
            })
        }
    }

    fn tokenizer() -> Tokenizer {
        let model = WordLevel::builder()
            .vocab(AHashMap::from([
                ("[UNK]".to_owned(), 0),
                ("hello".to_owned(), 1),
                ("world".to_owned(), 2),
            ]))
            .unk_token("[UNK]".to_owned())
            .build()
            .unwrap();

        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Some(Whitespace::default()));
        tokenizer
    }

    #[test]
    fn aggregate_sparse_drops_specials_and_max_pools() {
        // 6 token positions: [<s>, hello, world, hello, <pad>, </s>]
        // input ids               [   0,     5,    99,    5,      1,     2]
        // attention_mask          [   1,     1,     1,    1,      0,     1]
        // sparse_logits           [ 0.9,   0.5,   0.2,  0.7,    0.0,   0.6]
        //
        // Expected: 5 -> max(0.5, 0.7) = 0.7, 99 -> 0.2; specials (0,1,2)
        // dropped, padding (pos 4) dropped via attention mask.
        let logits = ndarray::arr2(&[
            [0.9_f32], [0.5], [0.2], [0.7], [0.0], [0.6],
        ]);
        let input_ids = vec![0_i64, 5, 99, 5, 1, 2];
        let mask = vec![1_i64, 1, 1, 1, 0, 1];

        let out = aggregate_sparse(logits.view(), &input_ids, &mask);
        assert_eq!(out, vec![(5, 0.7), (99, 0.2)]);
    }

    #[test]
    fn aggregate_sparse_thresholds_tiny_weights() {
        let logits = ndarray::arr2(&[[1e-9_f32], [0.4]]);
        let input_ids = vec![10_i64, 11];
        let mask = vec![1_i64, 1];

        let out = aggregate_sparse(logits.view(), &input_ids, &mask);
        assert_eq!(out, vec![(11, 0.4)]);
    }

    #[test]
    fn tokenization_builds_expected_model_inputs() {
        let embedder = Embedder::with_backend(
            tokenizer(),
            MockBackend {
                output: Array3::zeros((1, 2, 2)),
            },
        );

        let tokenized = embedder.tokenize("hello world").unwrap();

        assert_eq!(tokenized.input_ids.shape(), &[1, 2]);
        assert_eq!(tokenized.attention_mask.shape(), &[1, 2]);
        assert_eq!(tokenized.token_type_ids.shape(), &[1, 2]);
        assert_eq!(tokenized.attention_mask_values, vec![1, 1]);
    }

    #[test]
    fn embedder_mean_pools_and_normalizes_output() {
        let backend = MockBackend {
            output: Array3::from_shape_vec((1, 2, 3), vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0]).unwrap(),
        };
        let embedder = Embedder::with_backend(tokenizer(), backend);

        let embedding = embedder.embed("hello world").unwrap();

        let expected = std::f32::consts::FRAC_1_SQRT_2;
        assert!((embedding[0] - expected).abs() < 1e-6);
        assert!((embedding[1] - expected).abs() < 1e-6);
        assert!(embedding[2].abs() < 1e-6);
    }

    #[test]
    fn embedder_cls_pools_only_first_token() {
        // First token vector [3,4,0] (norm 5); second token [9,9,9] is ignored
        // by CLS pooling. After L2 normalize: [0.6, 0.8, 0].
        let backend = MockBackend {
            output: Array3::from_shape_vec((1, 2, 3), vec![3.0, 4.0, 0.0, 9.0, 9.0, 9.0]).unwrap(),
        };
        let embedder = Embedder::with_backend(tokenizer(), backend).with_pooling(Pooling::Cls);

        let embedding = embedder.embed("hello world").unwrap();

        assert!((embedding[0] - 0.6).abs() < 1e-6);
        assert!((embedding[1] - 0.8).abs() < 1e-6);
        assert!(embedding[2].abs() < 1e-6);
    }
}
