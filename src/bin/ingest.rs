//! `rust-rag-ingest` — CLI for uploading a local source repo into the
//! rust-rag code-search store.
//!
//! The CLI does all filesystem walking + hashing locally; the server only
//! receives file content for paths that actually need (re-)ingest. This
//! keeps the server stateless w.r.t. local paths and lets the user preview
//! before paying for embeddings.
//!
//! Subcommands:
//! - `preview`  walk + plan, print summary; no upload.
//! - `push`     walk + plan + upload + sweep.
//! - `watch`    push, then keep running and re-push on file changes.

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Parser, Debug)]
#[command(
    name = "rust-rag-ingest",
    about = "Upload a local source repo into rust-rag",
    version
)]
struct Cli {
    /// rust-rag base URL (e.g. http://localhost:4001).
    #[arg(long, env = "RAG_URL", default_value = "http://localhost:4001")]
    url: String,
    /// Bearer token. Read from $RAG_TOKEN if not passed.
    #[arg(long, env = "RAG_TOKEN")]
    token: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Walk repo and show what would be uploaded.
    Preview {
        /// Repo name (logical id used by server).
        #[arg(long)]
        name: String,
        /// Local root path.
        root: PathBuf,
        #[arg(long, value_delimiter = ',')]
        include: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        exclude: Vec<String>,
    },
    /// Walk repo, upload changed files, sweep stale entries.
    Push {
        #[arg(long)]
        name: String,
        root: PathBuf,
        #[arg(long, value_delimiter = ',')]
        include: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        exclude: Vec<String>,
        /// Re-upload everything even if hashes match.
        #[arg(long)]
        force: bool,
        /// Batch payload size in bytes.
        #[arg(long, default_value_t = 4 * 1024 * 1024)]
        batch_bytes: usize,
        /// Skip the post-upload stale sweep.
        #[arg(long)]
        no_sweep: bool,
    },
    /// Push once then re-push on file changes (debounced 500ms).
    Watch {
        #[arg(long)]
        name: String,
        root: PathBuf,
        #[arg(long, value_delimiter = ',')]
        include: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        exclude: Vec<String>,
        #[arg(long, default_value_t = 4 * 1024 * 1024)]
        batch_bytes: usize,
    },
}

// ---- shared types mirror the server JSON shape -----------------------------

#[derive(Serialize)]
struct UpsertRepoReq<'a> {
    name: &'a str,
    root_path: &'a str,
    include_globs: &'a [String],
    exclude_globs: &'a [String],
}

#[derive(Serialize)]
struct PlanReq {
    files: Vec<PlanEntry>,
    force: bool,
}

#[derive(Serialize)]
struct PlanEntry {
    path: String,
    hash: String,
    size_bytes: i64,
}

#[derive(Deserialize, Debug)]
struct PlanResp {
    upload: Vec<String>,
    stale: Vec<String>,
    total_local: usize,
    unchanged: usize,
}

#[derive(Serialize)]
struct BatchReq {
    files: Vec<FilePayload>,
    force: bool,
}

#[derive(Serialize)]
struct FilePayload {
    path: String,
    content: String,
    mtime: Option<i64>,
}

#[derive(Deserialize, Debug)]
struct BatchResp {
    repo: String,
    report: ReportJson,
    per_file: Vec<FileOutcome>,
}

#[derive(Deserialize, Debug)]
struct ReportJson {
    files_scanned: usize,
    files_changed: usize,
    chunks_inserted: usize,
    skipped_binary: usize,
    skipped_too_large: usize,
    errors: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct FileOutcome {
    path: String,
    status: String,
    error: Option<String>,
}

#[derive(Serialize)]
struct SweepReq {
    paths: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct SweepResp {
    deleted: Vec<String>,
}

// ---- walker ----------------------------------------------------------------

struct WalkResult {
    files: Vec<LocalFile>,
    skipped_too_large: usize,
    skipped_binary: usize,
}

struct LocalFile {
    rel_path: String,
    abs_path: PathBuf,
    bytes: Vec<u8>,
    hash: String,
    size: i64,
    mtime: Option<i64>,
}

const MAX_FILE_BYTES: u64 = 1_500_000;

fn build_globset(globs: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        b.add(Glob::new(g).with_context(|| format!("invalid glob: {g}"))?);
    }
    Ok(b.build()?)
}

fn default_excluded(rel: &str) -> bool {
    let p = rel.replace('\\', "/");
    p.starts_with("target/")
        || p.starts_with("node_modules/")
        || p.starts_with(".git/")
        || p.starts_with(".next/")
        || p.starts_with(".turbo/")
        || p.starts_with("dist/")
        || p.starts_with("build/")
        || p.starts_with(".venv/")
        || p.starts_with("venv/")
        || p.starts_with(".cache/")
        || p.ends_with(".lock")
        || p.ends_with("Cargo.lock")
        || p.ends_with("package-lock.json")
        || p.ends_with("pnpm-lock.yaml")
        || p.ends_with("yarn.lock")
        || p.ends_with("poetry.lock")
        || p.ends_with("go.sum")
        || p.ends_with(".min.js")
        || p.ends_with(".min.css")
        || p.ends_with(".map")
        || p.ends_with(".snap")
}

fn walk_repo(root: &Path, include: &[String], exclude: &[String]) -> Result<WalkResult> {
    let includes = build_globset(include)?;
    let excludes = build_globset(exclude)?;
    let mut walker = WalkBuilder::new(root);
    walker
        .follow_links(false)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false);

    let mut files = Vec::new();
    let mut skipped_too_large = 0;
    let mut skipped_binary = 0;
    for entry in walker.build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let abs_path = entry.into_path();
        let rel = match abs_path.strip_prefix(root) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        if default_excluded(&rel) {
            continue;
        }
        if !include.is_empty() && !includes.is_match(&rel) {
            continue;
        }
        if !exclude.is_empty() && excludes.is_match(&rel) {
            continue;
        }
        let md = match std::fs::metadata(&abs_path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if md.len() > MAX_FILE_BYTES {
            skipped_too_large += 1;
            continue;
        }
        let bytes = match std::fs::read(&abs_path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if bytes.iter().take(8192).any(|&b| b == 0) {
            skipped_binary += 1;
            continue;
        }
        let mut h = Sha256::new();
        h.update(&bytes);
        let hash = format!("{:x}", h.finalize());
        let size = bytes.len() as i64;
        let mtime = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);
        files.push(LocalFile {
            rel_path: rel,
            abs_path,
            bytes,
            hash,
            size,
            mtime,
        });
    }
    Ok(WalkResult {
        files,
        skipped_too_large,
        skipped_binary,
    })
}

// ---- HTTP client -----------------------------------------------------------

struct Client {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl Client {
    fn new(base: String, token: Option<String>) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            token,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("build http client"),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn post<B: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R> {
        let mut req = self.http.post(self.url(path)).json(body);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.context("http send")?;
        let status = resp.status();
        let text = resp.text().await.context("read body")?;
        if !status.is_success() {
            return Err(anyhow!("server returned {status}: {text}"));
        }
        Ok(serde_json::from_str(&text).context("parse json")?)
    }
}

// ---- subcommands -----------------------------------------------------------

async fn cmd_preview(
    client: &Client,
    name: &str,
    root: &Path,
    include: Vec<String>,
    exclude: Vec<String>,
) -> Result<()> {
    upsert_repo(client, name, root, &include, &exclude).await?;
    let walk = walk_repo(root, &include, &exclude)?;
    let entries: Vec<PlanEntry> = walk
        .files
        .iter()
        .map(|f| PlanEntry {
            path: f.rel_path.clone(),
            hash: f.hash.clone(),
            size_bytes: f.size,
        })
        .collect();
    let plan: PlanResp = client
        .post(
            &format!("/api/code/repos/{name}/plan"),
            &PlanReq {
                files: entries,
                force: false,
            },
        )
        .await?;
    println!("repo:           {}", name);
    println!("local files:    {}", plan.total_local);
    println!("unchanged:      {}", plan.unchanged);
    println!("would upload:   {}", plan.upload.len());
    println!("would delete:   {}  (stale on server)", plan.stale.len());
    println!("skipped binary: {}", walk.skipped_binary);
    println!("skipped >1.5MB: {}", walk.skipped_too_large);
    println!();
    if !plan.upload.is_empty() {
        println!("upload targets (first 50):");
        for p in plan.upload.iter().take(50) {
            println!("  + {p}");
        }
        if plan.upload.len() > 50 {
            println!("  …and {} more", plan.upload.len() - 50);
        }
    }
    if !plan.stale.is_empty() {
        println!();
        println!("stale on server (first 50):");
        for p in plan.stale.iter().take(50) {
            println!("  - {p}");
        }
    }
    Ok(())
}

async fn cmd_push(
    client: &Client,
    name: &str,
    root: &Path,
    include: Vec<String>,
    exclude: Vec<String>,
    force: bool,
    batch_bytes: usize,
    no_sweep: bool,
) -> Result<()> {
    upsert_repo(client, name, root, &include, &exclude).await?;
    let walk = walk_repo(root, &include, &exclude)?;
    let entries: Vec<PlanEntry> = walk
        .files
        .iter()
        .map(|f| PlanEntry {
            path: f.rel_path.clone(),
            hash: f.hash.clone(),
            size_bytes: f.size,
        })
        .collect();
    let plan: PlanResp = client
        .post(
            &format!("/api/code/repos/{name}/plan"),
            &PlanReq {
                files: entries,
                force,
            },
        )
        .await?;
    println!(
        "plan: {} local, {} unchanged, {} to upload, {} stale",
        plan.total_local,
        plan.unchanged,
        plan.upload.len(),
        plan.stale.len()
    );
    let upload_set: HashSet<String> = plan.upload.iter().cloned().collect();
    let to_send: Vec<&LocalFile> = walk
        .files
        .iter()
        .filter(|f| upload_set.contains(&f.rel_path))
        .collect();
    let total = to_send.len();
    if total == 0 {
        println!("nothing to upload.");
    } else {
        let mut batch: Vec<FilePayload> = Vec::new();
        let mut batch_size = 0usize;
        let mut sent = 0usize;
        let mut totals = (0usize, 0usize, 0usize); // changed, chunks, errors
        for f in to_send {
            let content = match std::str::from_utf8(&f.bytes) {
                Ok(s) => s.to_string(),
                Err(_) => continue,
            };
            batch_size += content.len();
            batch.push(FilePayload {
                path: f.rel_path.clone(),
                content,
                mtime: f.mtime,
            });
            if batch_size >= batch_bytes {
                let n = batch.len();
                let resp = send_batch(client, name, std::mem::take(&mut batch), force).await?;
                totals.0 += resp.report.files_changed;
                totals.1 += resp.report.chunks_inserted;
                totals.2 += resp.report.errors.len();
                sent += n;
                batch_size = 0;
                println!("  batch: {sent}/{total} uploaded  (+{} chunks)", resp.report.chunks_inserted);
            }
        }
        if !batch.is_empty() {
            let n = batch.len();
            let resp = send_batch(client, name, batch, force).await?;
            totals.0 += resp.report.files_changed;
            totals.1 += resp.report.chunks_inserted;
            totals.2 += resp.report.errors.len();
            sent += n;
            println!("  batch: {sent}/{total} uploaded  (+{} chunks)", resp.report.chunks_inserted);
        }
        println!(
            "done: {} changed, {} chunks inserted, {} errors",
            totals.0, totals.1, totals.2
        );
    }

    if !no_sweep {
        let paths: Vec<String> = walk.files.iter().map(|f| f.rel_path.clone()).collect();
        let resp: SweepResp = client
            .post(
                &format!("/api/code/repos/{name}/sweep"),
                &SweepReq { paths },
            )
            .await?;
        if !resp.deleted.is_empty() {
            println!("swept {} stale files", resp.deleted.len());
        }
    }
    Ok(())
}

async fn send_batch(
    client: &Client,
    name: &str,
    files: Vec<FilePayload>,
    force: bool,
) -> Result<BatchResp> {
    client
        .post(
            &format!("/api/code/repos/{name}/files"),
            &BatchReq { files, force },
        )
        .await
}

async fn upsert_repo(
    client: &Client,
    name: &str,
    root: &Path,
    include: &[String],
    exclude: &[String],
) -> Result<()> {
    let root_str = root.canonicalize()?.to_string_lossy().to_string();
    let _: serde_json::Value = client
        .post(
            "/api/code/repos",
            &UpsertRepoReq {
                name,
                root_path: &root_str,
                include_globs: include,
                exclude_globs: exclude,
            },
        )
        .await?;
    Ok(())
}

async fn cmd_watch(
    client: &Client,
    name: &str,
    root: &Path,
    include: Vec<String>,
    exclude: Vec<String>,
    batch_bytes: usize,
) -> Result<()> {
    cmd_push(
        client,
        name,
        root,
        include.clone(),
        exclude.clone(),
        false,
        batch_bytes,
        false,
    )
    .await?;
    use notify::{RecursiveMode, Watcher};
    use notify_debouncer_full::{new_debouncer, DebounceEventResult};
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(8);
    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        None,
        move |res: DebounceEventResult| {
            if res.is_ok() {
                let _ = tx.blocking_send(());
            }
        },
    )?;
    debouncer
        .watcher()
        .watch(root, RecursiveMode::Recursive)?;
    println!("watching {} — press Ctrl-C to stop", root.display());
    loop {
        match rx.recv().await {
            Some(()) => {
                // Drain extra events that arrived during debounce window.
                while rx.try_recv().is_ok() {}
                println!("change detected, re-pushing…");
                if let Err(e) = cmd_push(
                    client,
                    name,
                    root,
                    include.clone(),
                    exclude.clone(),
                    false,
                    batch_bytes,
                    false,
                )
                .await
                {
                    eprintln!("push error: {e:#}");
                }
            }
            None => break,
        }
    }
    Ok(())
}

// ---- main ------------------------------------------------------------------

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::new(cli.url.clone(), cli.token.clone());
    let start = SystemTime::now();
    match cli.cmd {
        Cmd::Preview {
            name,
            root,
            include,
            exclude,
        } => cmd_preview(&client, &name, &root, include, exclude).await?,
        Cmd::Push {
            name,
            root,
            include,
            exclude,
            force,
            batch_bytes,
            no_sweep,
        } => {
            cmd_push(
                &client,
                &name,
                &root,
                include,
                exclude,
                force,
                batch_bytes,
                no_sweep,
            )
            .await?
        }
        Cmd::Watch {
            name,
            root,
            include,
            exclude,
            batch_bytes,
        } => cmd_watch(&client, &name, &root, include, exclude, batch_bytes).await?,
    }
    println!(
        "elapsed: {:.2}s",
        start.elapsed().unwrap_or_default().as_secs_f64()
    );
    Ok(())
}
