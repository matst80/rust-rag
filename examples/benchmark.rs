use anyhow::{Context, Result, anyhow};
use ndarray::Array2;
use ort::{
    inputs, session::Session,
    session::builder::GraphOptimizationLevel, value::TensorRef,
};
use std::{path::Path, time::Instant};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing to see ONNX Runtime diagnostic output if RUST_LOG=ort=debug is set
    let _ = tracing_subscriber::fmt::try_init();

    println!("=== CUDA Embedding Model Benchmark ===");
    
    // Configure model paths
    let bge_small_path = Path::new("assets/bge-small-en-v1.5/model.onnx");
    let bge_m3_path = Path::new("assets/bge-m3/model_fp16.onnx");
    
    // 1. Download BGE-M3 if it does not exist
    if !bge_m3_path.exists() {
        println!("BGE-M3 model not found at {}", bge_m3_path.display());
        println!("Downloading BGE-M3 (FP16 ONNX) from Hugging Face...");
        std::fs::create_dir_all("assets/bge-m3")?;
        
        let url = "https://huggingface.co/Xenova/bge-m3/resolve/main/onnx/model_fp16.onnx";
        let resp = reqwest::get(url)
            .await
            .context("Failed to download BGE-M3 FP16 model from Hugging Face")?;
            
        let bytes = resp.bytes().await.context("Failed to read response bytes")?;
        std::fs::write(bge_m3_path, bytes)?;
        println!("BGE-M3 model downloaded successfully.");
    }

    // Initialize the ONNX Runtime
    ort::init().with_name("rust-rag-benchmark").commit();
    
    #[cfg(feature = "cuda")]
    {
        use ort::ep::ExecutionProvider;
        match ort::ep::CUDA::default().is_available() {
            Ok(true) => println!("CUDA Execution Provider is supported by this ONNX Runtime build."),
            Ok(false) => println!("CUDA Execution Provider is NOT supported by this ONNX Runtime build! (Fallback to CPU may occur silently)"),
            Err(e) => println!("Error checking CUDA availability: {e}"),
        }
    }
    
    println!("Benchmarking BGE-Small (33.4M parameters)...");
    benchmark_model(bge_small_path, "BGE-Small")?;
    
    println!("\nBenchmarking BGE-M3 (568M parameters)...");
    benchmark_model(bge_m3_path, "BGE-M3")?;
    
    Ok(())
}

fn execution_providers() -> Vec<ort::execution_providers::ExecutionProviderDispatch> {
    #[cfg(feature = "cuda")]
    {
        use ort::ep::{ArenaExtendStrategy, CUDA, cuda::ConvAlgorithmSearch, CPUExecutionProvider};
        let mem_limit_mb: usize = std::env::var("RAG_CUDA_MEM_LIMIT_MB")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let device_id: i32 = std::env::var("RAG_CUDA_DEVICE_ID")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);

        println!("Initializing with CUDA Execution Provider (Device ID: {device_id}, Memory Limit: {mem_limit_mb} MiB)...");
        let mut cuda = CUDA::default().with_device_id(device_id);
        if mem_limit_mb > 0 {
            cuda = cuda.with_memory_limit(mem_limit_mb * 1024 * 1024);
        }
        
        vec![
            cuda
                .with_arena_extend_strategy(ArenaExtendStrategy::SameAsRequested)
                .with_conv_algorithm_search(ConvAlgorithmSearch::Heuristic)
                .with_conv_max_workspace(false)
                .build(),
            CPUExecutionProvider::default().build(),
        ]
    }
    #[cfg(not(feature = "cuda"))]
    {
        use ort::execution_providers::CPUExecutionProvider;
        println!("Initializing with CPU Execution Provider (cuda feature not enabled)...");
        vec![CPUExecutionProvider::default().build()]
    }
}

fn ort_error<E: std::fmt::Display>(error: E) -> anyhow::Error {
    anyhow!("{}", error)
}

fn benchmark_model(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("Model path does not exist: {}", path.display()));
    }

    let started = Instant::now();
    let intra_threads: usize = std::env::var("RAG_INTRA_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);

    let mut builder = Session::builder().map_err(ort_error)?;
    builder = builder.with_execution_providers(execution_providers()).map_err(ort_error)?;
    builder = builder.with_optimization_level(GraphOptimizationLevel::Level3).map_err(ort_error)?;
    builder = builder.with_intra_threads(intra_threads).map_err(ort_error)?;

    let mut session = builder.commit_from_file(path)
        .map_err(ort_error)
        .context("Failed to load ONNX model")?;
        
    println!("{} model loaded into memory in {:?}", name, started.elapsed());

    // Generate dummy input matching typical transformer input
    let seq_length = 512;
    let input_ids = Array2::<i64>::zeros((1, seq_length));
    let attention_mask = Array2::<i64>::ones((1, seq_length));
    
    // Test if token_type_ids is required by checking input names
    let has_token_type = session.inputs().iter().any(|i| i.name() == "token_type_ids");
    let token_type_ids = Array2::<i64>::zeros((1, seq_length));
    
    // Warmup
    println!("Warming up model with 3 iterations...");
    for _ in 0..3 {
        let _ = run_inference(&mut session, &input_ids, &attention_mask, has_token_type.then_some(&token_type_ids))?;
    }
    
    // Benchmark
    let iters = 20;
    println!("Benchmarking inference across {} iterations...", iters);
    let mut total_duration = std::time::Duration::default();
    
    for _ in 0..iters {
        let start = Instant::now();
        let _ = run_inference(&mut session, &input_ids, &attention_mask, has_token_type.then_some(&token_type_ids))?;
        total_duration += start.elapsed();
    }
    
    let avg_ms = total_duration.as_millis() as f64 / iters as f64;
    println!("Average inference time for {}: {:.2} ms", name, avg_ms);
    
    Ok(())
}

fn run_inference(
    session: &mut Session,
    input_ids: &Array2<i64>,
    attention_mask: &Array2<i64>,
    token_type_ids: Option<&Array2<i64>>,
) -> Result<()> {
    let outputs = if let Some(t_ids) = token_type_ids {
        session.run(inputs![
            TensorRef::from_array_view(input_ids.view()).map_err(ort_error)?,
            TensorRef::from_array_view(attention_mask.view()).map_err(ort_error)?,
            TensorRef::from_array_view(t_ids.view()).map_err(ort_error)?
        ]).map_err(ort_error)?
    } else {
        session.run(inputs![
            TensorRef::from_array_view(input_ids.view()).map_err(ort_error)?,
            TensorRef::from_array_view(attention_mask.view()).map_err(ort_error)?
        ]).map_err(ort_error)?
    };

    let _ = outputs[0].try_extract_array::<f32>().map_err(ort_error)?;
    Ok(())
}
