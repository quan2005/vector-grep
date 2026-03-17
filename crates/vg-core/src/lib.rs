pub mod chunk;
pub mod config;
pub mod embed;
pub mod index;
pub mod output;
pub mod preproc;
pub mod progress;
pub mod search;
pub mod store;

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::Instant,
};

use anyhow::{Context, Result, anyhow, bail};
use config::{PersistentConfig, RuntimePaths, SearchMode, SplitArgs, build_indexer_args};
use embed::Embedder;
use index::{IndexManager, SyncReport};
use progress::default_reporter;
use search::{SearchResponse, SearchStats};
use store::Store;

pub const SCHEMA_VERSION: u32 = 1;

pub fn run_cli(raw_args: &[OsString]) -> Result<i32> {
    let args = SplitArgs::parse(raw_args)?;
    if args.help {
        println!("{}", config::usage("vg"));
        return Ok(0);
    }
    if args.version {
        println!("vg {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }

    if args.list_models {
        for model in embed::SupportedModel::supported() {
            println!("{}\t{}\t{}", model.id, model.dimensions, model.description);
        }
        return Ok(0);
    }

    if args.mode == SearchMode::Text {
        return passthrough_text_mode(&args.passthrough_args);
    }

    let reporter = default_reporter();
    let (mut embedder, mut store) = open_index_pipeline(
        args.cache_path.as_deref(),
        args.no_cache,
        args.rebuild,
        reporter.as_ref(),
    )?;
    let mut index_manager = IndexManager::new(
        &mut store,
        &mut embedder,
        args.chunk_size,
        args.chunk_overlap,
        reporter.as_ref(),
    );

    if args.index_only {
        let report = index_manager.sync(&args.paths)?;
        output::print_index_report(&report);
        return Ok(0);
    }

    if args.index_stats {
        let stats = store.scope_stats(&args.paths)?;
        output::print_index_stats(&stats);
        return Ok(0);
    }

    let query = args
        .query
        .as_deref()
        .ok_or_else(|| anyhow!("缺少查询语句"))?;

    match args.mode {
        SearchMode::Semantic => {
            let started = Instant::now();
            let report = index_manager.sync(&args.paths)?;
            let results = search::vector::semantic_search(
                &mut store,
                &mut embedder,
                query,
                &args.paths,
                args.top_k,
                args.threshold,
            )?;
            let response = SearchResponse {
                results,
                stats: build_stats(started.elapsed().as_millis(), &report),
            };
            output::render_response(&response, args.json, args.show_score, args.context)?;
        }
        SearchMode::Hybrid => {
            let passthrough_args = args.passthrough_args.clone();
            let text_handle =
                std::thread::spawn(move || search::text::search_json(&passthrough_args));

            let started = Instant::now();
            let report = index_manager.sync(&args.paths)?;
            let semantic = search::vector::semantic_search(
                &mut store,
                &mut embedder,
                query,
                &args.paths,
                args.top_k.max(25),
                args.threshold,
            )?;
            let text = text_handle
                .join()
                .map_err(|_| anyhow!("文本搜索线程异常退出"))??;
            let fused = search::hybrid::fuse_results(text, semantic, args.top_k);
            let response = SearchResponse {
                results: fused,
                stats: build_stats(started.elapsed().as_millis(), &report),
            };
            output::render_response(&response, args.json, args.show_score, args.context)?;
        }
        SearchMode::Text => unreachable!(),
    }

    Ok(0)
}

pub fn run_indexer(raw_args: &[OsString]) -> Result<i32> {
    let args = build_indexer_args(raw_args)?;
    if args.help {
        println!("{}", config::usage("vg-index"));
        return Ok(0);
    }

    let reporter = default_reporter();
    let (mut embedder, mut store) = open_index_pipeline(
        args.cache_path.as_deref(),
        args.no_cache,
        args.rebuild,
        reporter.as_ref(),
    )?;
    let mut index_manager = IndexManager::new(
        &mut store,
        &mut embedder,
        args.chunk_size,
        args.chunk_overlap,
        reporter.as_ref(),
    );
    let report = index_manager.sync(&args.paths)?;
    output::print_index_report(&report);
    Ok(0)
}

fn open_index_pipeline(
    cache_override: Option<&Path>,
    no_cache: bool,
    rebuild: bool,
    reporter: &dyn progress::ProgressReporter,
) -> Result<(Embedder, Store)> {
    let runtime_paths = RuntimePaths::resolve(cache_override, no_cache)?;
    let mut persistent = PersistentConfig::load_or_create(&runtime_paths.cache_dir)?;
    let embedder = Embedder::new(
        &persistent.model_id,
        &runtime_paths.models_dir,
        &persistent.pooling,
        reporter,
    )?;
    let resolved_dimensions = if embedder.model().dimensions > 0 {
        embedder.model().dimensions
    } else {
        persistent.model_dimensions
    };
    if resolved_dimensions == 0 {
        bail!("模型维度未知，请在 .config.json 中显式设置 model_dimensions");
    }
    if persistent.model_dimensions != resolved_dimensions {
        persistent.model_dimensions = resolved_dimensions;
        persistent.save(&runtime_paths.cache_dir.join(".config.json"))?;
    }
    let store = Store::open(
        &runtime_paths.index_path,
        persistent.model_id.as_str(),
        resolved_dimensions,
        rebuild || no_cache,
    )?;
    Ok((embedder, store))
}

fn build_stats(query_time_ms: u128, report: &SyncReport) -> SearchStats {
    SearchStats {
        query_time_ms,
        index_time_ms: report.index_time_ms,
        files_indexed: report.files_indexed,
        chunks_total: report.chunks_total,
    }
}

fn passthrough_text_mode(args: &[OsString]) -> Result<i32> {
    let status = Command::new("rga")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("调用 rga 失败")?;
    Ok(exit_status_code(status))
}

fn exit_status_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

pub fn make_absolute(path: &Path) -> Result<PathBuf> {
    if path == Path::new(".") {
        return std::env::current_dir().context("读取当前目录失败");
    }
    path.canonicalize()
        .with_context(|| format!("无法解析路径: {}", path.display()))
}

pub fn normalize_roots(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if paths.is_empty() {
        return Ok(vec![std::env::current_dir().context("读取当前目录失败")?]);
    }

    let mut normalized = Vec::with_capacity(paths.len());
    for path in paths {
        normalized.push(make_absolute(path)?);
    }
    normalized.sort();
    normalized.dedup();
    if normalized.is_empty() {
        bail!("至少需要一个搜索路径");
    }
    Ok(normalized)
}
