use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_CORPUS: &str = "~/Projects/github/notebook/";
const DEFAULT_OUTPUT_FILE: &str = "coverage_report.json";
const DEFAULT_TOP_K: &str = "50";
const TEXT_GLOBS_TO_SKIP: [&str; 2] = ["!*.pdf", "!*.pptx"];

const QUERY_CASES: [QueryCase; 6] = [
    QueryCase {
        label: "AI工具对工作流程的影响",
        vg_query: "AI工具对工作流程的影响",
        rg_pattern: None,
        query_type: QueryType::Semantic,
    },
    QueryCase {
        label: "如何处理项目中的技术债务",
        vg_query: "如何处理项目中的技术债务",
        rg_pattern: None,
        query_type: QueryType::Semantic,
    },
    QueryCase {
        label: "团队协作中的沟通问题",
        vg_query: "团队协作中的沟通问题",
        rg_pattern: None,
        query_type: QueryType::Semantic,
    },
    QueryCase {
        label: "认证授权 vs auth",
        vg_query: "认证授权",
        rg_pattern: Some("auth"),
        query_type: QueryType::Synonym,
    },
    QueryCase {
        label: "错误处理 vs error",
        vg_query: "错误处理",
        rg_pattern: Some("error"),
        query_type: QueryType::Synonym,
    },
    QueryCase {
        label: "性能优化 vs performance",
        vg_query: "性能优化",
        rg_pattern: Some("performance"),
        query_type: QueryType::Synonym,
    },
];

#[derive(Debug, Clone, Copy)]
struct QueryCase {
    label: &'static str,
    vg_query: &'static str,
    rg_pattern: Option<&'static str>,
    query_type: QueryType,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum QueryType {
    Semantic,
    Synonym,
}

impl QueryType {
    fn as_label(self) -> &'static str {
        match self {
            Self::Semantic => "语义",
            Self::Synonym => "同义词",
        }
    }
}

#[derive(Debug, Serialize)]
struct CoverageQueryReport {
    label: String,
    vg_query: String,
    rg_pattern: Option<String>,
    query_type: QueryType,
    vg_hits: usize,
    rg_hits: usize,
    vg_files: Vec<String>,
    rg_files: Vec<String>,
    vg_only_files: Vec<String>,
    rg_only_files: Vec<String>,
    shared_files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CoverageSummary {
    total_vg_hits: usize,
    total_rg_hits: usize,
    vg_only_hits: usize,
    improvement_pct: f64,
}

#[derive(Debug, Serialize)]
struct CoverageReport {
    timestamp: u128,
    corpus: String,
    queries: Vec<CoverageQueryReport>,
    summary: CoverageSummary,
}

#[derive(Debug, Deserialize)]
struct VgResponse {
    results: Vec<VgResult>,
}

#[derive(Debug, Deserialize)]
struct VgResult {
    file_path: PathBuf,
}

#[test]
#[ignore = "需要外部语料与 rg，可用 cargo test -p vg-cli --test coverage_comparison -- --ignored --nocapture 显式运行"]
fn compare_coverage() -> Result<()> {
    ensure_command_available("rg")?;

    let corpus = resolve_corpus_path()?;
    let output_path = resolve_output_path()?;
    let cache_path = resolve_cache_path()?;
    let vg_binary = PathBuf::from(env!("CARGO_BIN_EXE_vg"));

    let mut query_reports = Vec::with_capacity(QUERY_CASES.len());
    for case in QUERY_CASES {
        let vg_files = run_vg(case, &corpus, &cache_path, &vg_binary)?;
        // 语义查询没有等价关键词匹配，直接记为 0 以体现文本搜索的下限。
        let rg_files = match case.rg_pattern {
            Some(pattern) => run_rg(pattern, &corpus)?,
            None => BTreeSet::new(),
        };

        let shared_files = set_to_strings(vg_files.intersection(&rg_files));
        let vg_only_files = set_to_strings(vg_files.difference(&rg_files));
        let rg_only_files = set_to_strings(rg_files.difference(&vg_files));

        query_reports.push(CoverageQueryReport {
            label: case.label.to_string(),
            vg_query: case.vg_query.to_string(),
            rg_pattern: case.rg_pattern.map(str::to_string),
            query_type: case.query_type,
            vg_hits: vg_files.len(),
            rg_hits: rg_files.len(),
            vg_files: set_to_strings(vg_files.iter()),
            rg_files: set_to_strings(rg_files.iter()),
            vg_only_files,
            rg_only_files,
            shared_files,
        });
    }

    let summary = build_summary(&query_reports);
    print_table(&query_reports, &summary);

    let report = CoverageReport {
        timestamp: current_unix_ms()?,
        corpus: corpus.display().to_string(),
        queries: query_reports,
        summary,
    };
    write_report(&report, &output_path)?;
    println!();
    println!("JSON 报告已写入: {}", output_path.display());
    Ok(())
}

fn run_vg(
    case: QueryCase,
    corpus: &Path,
    cache_path: &Path,
    vg_binary: &Path,
) -> Result<BTreeSet<PathBuf>> {
    let mut command = Command::new(vg_binary);
    command
        .arg("--vg-json")
        .arg("--vg-top-k")
        .arg(DEFAULT_TOP_K);
    if matches!(case.query_type, QueryType::Semantic) {
        command.arg("--vg-semantic");
    } else {
        for glob in TEXT_GLOBS_TO_SKIP {
            command.arg("--glob").arg(glob);
        }
    }
    command.arg("--vg-cache-path").arg(cache_path);
    command.arg(case.vg_query).arg(corpus);

    let output = command.output().context("执行 vg 覆盖度对比失败")?;
    if !output.status.success() {
        bail!(
            "vg 执行失败 (query: {}):\nstdout:\n{}\nstderr:\n{}",
            case.vg_query,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    let response: VgResponse =
        serde_json::from_slice(&output.stdout).context("解析 vg JSON 输出失败")?;
    Ok(response
        .results
        .into_iter()
        .map(|result| normalize_path(&result.file_path))
        .collect())
}

fn run_rg(pattern: &str, corpus: &Path) -> Result<BTreeSet<PathBuf>> {
    let output = Command::new("rg")
        .arg("--json")
        .arg("--line-number")
        .arg("--color=never")
        .args(TEXT_GLOBS_TO_SKIP.iter().flat_map(|glob| ["--glob", *glob]))
        .arg(pattern)
        .arg(corpus)
        .output()
        .context("执行 rg 覆盖度对比失败")?;

    if !output.status.success() && output.status.code() != Some(1) {
        bail!(
            "rg 执行失败 (pattern: {}):\nstdout:\n{}\nstderr:\n{}",
            pattern,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    let mut files = BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let event: Value = match serde_json::from_str(line) {
            Ok(event) => event,
            Err(_) => continue,
        };
        if event.get("type").and_then(Value::as_str) != Some("match") {
            continue;
        }

        let path = event
            .get("data")
            .and_then(|data| data.get("path"))
            .and_then(|path| path.get("text"))
            .and_then(Value::as_str);

        if let Some(path) = path {
            files.insert(normalize_path(Path::new(path)));
        }
    }

    Ok(files)
}

fn build_summary(queries: &[CoverageQueryReport]) -> CoverageSummary {
    let total_vg_hits = queries.iter().map(|query| query.vg_hits).sum::<usize>();
    let total_rg_hits = queries.iter().map(|query| query.rg_hits).sum::<usize>();
    let vg_only_hits = queries
        .iter()
        .map(|query| query.vg_only_files.len())
        .sum::<usize>();
    let improvement_pct = if total_rg_hits == 0 {
        0.0
    } else {
        (vg_only_hits as f64 / total_rg_hits as f64) * 100.0
    };

    CoverageSummary {
        total_vg_hits,
        total_rg_hits,
        vg_only_hits,
        improvement_pct,
    }
}

fn print_table(queries: &[CoverageQueryReport], summary: &CoverageSummary) {
    println!("| 查询 | 类型 | vg 命中 | rg 命中 | 差值 |");
    println!("| --- | --- | ---: | ---: | ---: |");
    for query in queries {
        println!(
            "| {} | {} | {} | {} | {} |",
            query.label,
            query.query_type.as_label(),
            query.vg_hits,
            query.rg_hits,
            query.vg_hits as isize - query.rg_hits as isize,
        );
    }
    println!(
        "| 合计 | - | {} | {} | {} |",
        summary.total_vg_hits, summary.total_rg_hits, summary.vg_only_hits
    );

    if summary.total_rg_hits == 0 {
        println!();
        println!("vg 覆盖优势：rg 总命中为 0，无法计算百分比提升");
    } else {
        println!();
        println!(
            "vg 覆盖优势：+{} 文件命中（+{:.1}%）",
            summary.vg_only_hits, summary.improvement_pct
        );
    }
}

fn write_report(report: &CoverageReport, output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建报告目录失败: {}", parent.display()))?;
    }
    let content = serde_json::to_vec_pretty(report).context("序列化覆盖度报告失败")?;
    fs::write(output_path, content)
        .with_context(|| format!("写入覆盖度报告失败: {}", output_path.display()))
}

fn resolve_corpus_path() -> Result<PathBuf> {
    let raw = env::var("VG_BENCH_CORPUS").unwrap_or_else(|_| DEFAULT_CORPUS.to_string());
    let path = resolve_input_path(&raw)?;
    let canonical = path
        .canonicalize()
        .with_context(|| format!("语料目录不存在或无法访问: {}", path.display()))?;
    if !canonical.is_dir() {
        bail!("VG_BENCH_CORPUS 不是目录: {}", canonical.display());
    }
    Ok(canonical)
}

fn resolve_output_path() -> Result<PathBuf> {
    match env::var("VG_BENCH_OUT") {
        Ok(path) => resolve_input_path(&path),
        Err(env::VarError::NotPresent) => Ok(workspace_root()?.join(DEFAULT_OUTPUT_FILE)),
        Err(err) => Err(anyhow!("读取 VG_BENCH_OUT 失败: {err}")),
    }
}

fn resolve_cache_path() -> Result<PathBuf> {
    match env::var("VG_BENCH_CACHE") {
        Ok(path) => Ok(resolve_input_path(&path)?),
        Err(env::VarError::NotPresent) => Ok(workspace_root()?.join(".context/vg-bench-cache")),
        Err(err) => Err(anyhow!("读取 VG_BENCH_CACHE 失败: {err}")),
    }
}

fn resolve_input_path(raw: &str) -> Result<PathBuf> {
    if let Some(rest) = raw.strip_prefix("~/") {
        let home = env::var("HOME").context("展开路径失败：缺少 HOME 环境变量")?;
        return Ok(PathBuf::from(home).join(rest));
    }

    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(workspace_root()?.join(path))
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn ensure_command_available(command: &str) -> Result<()> {
    let output = Command::new(command)
        .arg("--version")
        .output()
        .with_context(|| format!("命令不存在或不可执行: {command}"))?;
    if !output.status.success() {
        bail!("命令检查失败: {command} --version");
    }
    Ok(())
}

fn current_unix_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("系统时间早于 UNIX_EPOCH")?
        .as_millis())
}

fn set_to_strings<'a, I>(paths: I) -> Vec<String>
where
    I: Iterator<Item = &'a PathBuf>,
{
    paths.map(|path| path.display().to_string()).collect()
}

fn workspace_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("解析 workspace 根目录失败")
}
