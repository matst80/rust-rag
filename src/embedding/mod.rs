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

pub trait EmbeddingService: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

pub trait InferenceBackend: Send + Sync {
    fn run(
        &self,
        input_ids: Array2<i64>,
        attention_mask: Array2<i64>,
        token_type_ids: Array2<i64>,
    ) -> Result<Array3<f32>>;
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
    backend: B,
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
        Ok(Self { tokenizer, backend })
    }
}

impl<B> Embedder<B>
where
    B: InferenceBackend,
{
    pub fn with_backend(tokenizer: Tokenizer, backend: B) -> Self {
        Self { tokenizer, backend }
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

        let hidden_state = output.index_axis(Axis(0), 0);

        mean_pool_and_normalize(hidden_state, &attention_mask_values)
    }
}

pub struct OrtBackend {
    session: Mutex<Session>,
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

        Ok(Self {
            session: Mutex::new(session),
        })
    }
}

impl InferenceBackend for OrtBackend {
    fn run(
        &self,
        input_ids: Array2<i64>,
        attention_mask: Array2<i64>,
        token_type_ids: Array2<i64>,
    ) -> Result<Array3<f32>> {
        let mut session = self.session.lock().expect("ort session mutex poisoned");
        let outputs = session
            .run(inputs![
                TensorRef::from_array_view(input_ids.view()).map_err(ort_error)?,
                TensorRef::from_array_view(attention_mask.view()).map_err(ort_error)?,
                TensorRef::from_array_view(token_type_ids.view()).map_err(ort_error)?
            ])
            .map_err(ort_error)?;

        Ok(outputs[0]
            .try_extract_array::<f32>()
            .map_err(ort_error)?
            .to_owned()
            .into_dimensionality::<ndarray::Ix3>()?)
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

    let cuda = CUDA::default()
        .with_device_id(device_id)
        .with_memory_limit(mem_limit_mb * 1024 * 1024)
        .with_arena_extend_strategy(ArenaExtendStrategy::SameAsRequested)
        .with_conv_algorithm_search(ConvAlgorithmSearch::Heuristic)
        .with_conv_max_workspace(false)
        .build();

    println!(
        "embedder: registering CUDA EP (device={device_id}, mem_limit={mem_limit_mb}MiB) with CPU fallback"
    );
    vec![cuda, CPUExecutionProvider::default().build()]
}

#[cfg(not(feature = "cuda"))]
fn execution_providers() -> Vec<ort::execution_providers::ExecutionProviderDispatch> {
    vec![CPUExecutionProvider::default().build()]
}

fn initialize_ort(ort_dylib_path: Option<&Path>) -> Result<()> {
    static ORT_INITIALIZED: OnceLock<()> = OnceLock::new();

    if ORT_INITIALIZED.get().is_some() {
        return Ok(());
    }

    if let Some(path) = ort_dylib_path {
        println!(
            "embedder: ignoring explicit ort dylib {} because this build uses ort download-binaries",
            path.display()
        );
    } else {
        println!("embedder: using ort bundled binary discovery");
    }
    ort::init().with_name("rust-rag").commit();

    let _ = ORT_INITIALIZED.set(());
    Ok(())
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
        ) -> Result<Array3<f32>> {
            Ok(self.output.clone())
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
}
