use std::{
    collections::{BTreeMap, BTreeSet},
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

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
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
    vg_only_hits: usize,
    rg_only_hits: usize,
    shared_hits: usize,
    net_file_gain: isize,
    vg_only_file_buckets: Vec<FileBucketCount>,
    rg_only_file_buckets: Vec<FileBucketCount>,
    vg_only_extensions: Vec<ExtensionCount>,
    rg_only_extensions: Vec<ExtensionCount>,
    core_vg_hits: usize,
    core_rg_hits: usize,
    core_vg_only_hits: usize,
    core_rg_only_hits: usize,
    core_shared_hits: usize,
    core_net_file_gain: isize,
    issues: Vec<QueryIssue>,
}

#[derive(Debug, Serialize)]
struct CoverageSummary {
    total_vg_hits: usize,
    total_rg_hits: usize,
    total_vg_only_hits: usize,
    total_rg_only_hits: usize,
    total_shared_hits: usize,
    total_net_gain: isize,
    query_type_summaries: Vec<QueryTypeSummary>,
    core_total_vg_hits: usize,
    core_total_rg_hits: usize,
    core_total_vg_only_hits: usize,
    core_total_rg_only_hits: usize,
    core_total_shared_hits: usize,
    core_total_net_gain: isize,
    core_query_type_summaries: Vec<QueryTypeSummary>,
    issue_summaries: Vec<IssueSummary>,
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

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum FileBucket {
    Document,
    Code,
    Config,
    Script,
    Other,
}

#[derive(Debug, Serialize)]
struct FileBucketCount {
    bucket: FileBucket,
    count: usize,
}

#[derive(Debug, Serialize)]
struct ExtensionCount {
    extension: String,
    count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum IssueCategory {
    MetricScopeBias,
    DatasetNoise,
    LexicalBridgeGap,
    SemanticDriftReview,
}

impl IssueCategory {
    fn as_label(self) -> &'static str {
        match self {
            Self::MetricScopeBias => "评估口径偏差",
            Self::DatasetNoise => "数据集噪音",
            Self::LexicalBridgeGap => "词汇桥接缺口",
            Self::SemanticDriftReview => "语义漂移待复核",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum IssueSeverity {
    Info,
    Warn,
    High,
}

impl IssueSeverity {
    fn as_label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Serialize)]
struct QueryIssue {
    category: IssueCategory,
    severity: IssueSeverity,
    summary: String,
}

#[derive(Debug, Serialize)]
struct QueryTypeSummary {
    query_type: QueryType,
    query_count: usize,
    total_vg_hits: usize,
    total_rg_hits: usize,
    total_vg_only_hits: usize,
    total_rg_only_hits: usize,
    total_shared_hits: usize,
    total_net_gain: isize,
    improvement_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
struct IssueSummary {
    category: IssueCategory,
    affected_queries: usize,
    highest_severity: IssueSeverity,
    queries: Vec<String>,
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

        query_reports.push(build_query_report(case, &vg_files, &rg_files));
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

fn build_query_report(
    case: QueryCase,
    vg_files: &BTreeSet<PathBuf>,
    rg_files: &BTreeSet<PathBuf>,
) -> CoverageQueryReport {
    let vg_only_paths = vg_files.difference(rg_files).cloned().collect::<Vec<_>>();
    let rg_only_paths = rg_files.difference(vg_files).cloned().collect::<Vec<_>>();
    let shared_paths = vg_files.intersection(rg_files).cloned().collect::<Vec<_>>();
    let core_vg_paths = filter_core_scope(vg_files.iter().cloned()).collect::<BTreeSet<_>>();
    let core_rg_paths = filter_core_scope(rg_files.iter().cloned()).collect::<BTreeSet<_>>();
    let core_vg_only_paths = core_vg_paths
        .difference(&core_rg_paths)
        .cloned()
        .collect::<Vec<_>>();
    let core_rg_only_paths = core_rg_paths
        .difference(&core_vg_paths)
        .cloned()
        .collect::<Vec<_>>();
    let core_shared_paths = core_vg_paths
        .intersection(&core_rg_paths)
        .cloned()
        .collect::<Vec<_>>();
    let issues = classify_query_issues(
        case,
        &vg_only_paths,
        &rg_only_paths,
        &core_vg_only_paths,
        &core_rg_only_paths,
        core_shared_paths.len(),
    );

    CoverageQueryReport {
        label: case.label.to_string(),
        vg_query: case.vg_query.to_string(),
        rg_pattern: case.rg_pattern.map(str::to_string),
        query_type: case.query_type,
        vg_hits: vg_files.len(),
        rg_hits: rg_files.len(),
        vg_files: set_to_strings(vg_files.iter()),
        rg_files: set_to_strings(rg_files.iter()),
        vg_only_files: set_to_strings(vg_only_paths.iter()),
        rg_only_files: set_to_strings(rg_only_paths.iter()),
        shared_files: set_to_strings(shared_paths.iter()),
        vg_only_hits: vg_only_paths.len(),
        rg_only_hits: rg_only_paths.len(),
        shared_hits: shared_paths.len(),
        net_file_gain: vg_only_paths.len() as isize - rg_only_paths.len() as isize,
        vg_only_file_buckets: count_file_buckets(&vg_only_paths),
        rg_only_file_buckets: count_file_buckets(&rg_only_paths),
        vg_only_extensions: count_extensions(&vg_only_paths),
        rg_only_extensions: count_extensions(&rg_only_paths),
        core_vg_hits: core_vg_paths.len(),
        core_rg_hits: core_rg_paths.len(),
        core_vg_only_hits: core_vg_only_paths.len(),
        core_rg_only_hits: core_rg_only_paths.len(),
        core_shared_hits: core_shared_paths.len(),
        core_net_file_gain: core_vg_only_paths.len() as isize - core_rg_only_paths.len() as isize,
        issues,
    }
}

fn classify_query_issues(
    case: QueryCase,
    vg_only_paths: &[PathBuf],
    rg_only_paths: &[PathBuf],
    core_vg_only_paths: &[PathBuf],
    core_rg_only_paths: &[PathBuf],
    core_shared_hits: usize,
) -> Vec<QueryIssue> {
    let mut issues = Vec::new();

    if case.rg_pattern.is_none() {
        issues.push(QueryIssue {
            category: IssueCategory::MetricScopeBias,
            severity: IssueSeverity::Warn,
            summary: format!(
                "语义 query“{}”没有文本基线，只能评估新增召回，不能直接换算总体提升百分比。",
                case.vg_query
            ),
        });
    }

    let vg_noise = count_non_core_files(vg_only_paths);
    let rg_noise = count_non_core_files(rg_only_paths);
    if vg_noise + rg_noise > 0 {
        let severity = if vg_noise + rg_noise >= 5 {
            IssueSeverity::Warn
        } else {
            IssueSeverity::Info
        };
        issues.push(QueryIssue {
            category: IssueCategory::DatasetNoise,
            severity,
            summary: format!(
                "非核心文件混入对比结果：vg_only {} 个，rg_only {} 个，建议分桶统计或过滤。",
                vg_noise, rg_noise
            ),
        });
    }

    let core_rg_only_hits = core_rg_only_paths.len();
    let core_vg_only_hits = core_vg_only_paths.len();
    if matches!(case.query_type, QueryType::Synonym) && core_rg_only_hits >= 5 {
        let severity = if core_rg_only_hits > core_vg_only_hits {
            IssueSeverity::High
        } else {
            IssueSeverity::Warn
        };
        issues.push(QueryIssue {
            category: IssueCategory::LexicalBridgeGap,
            severity,
            summary: format!(
                "“{} -> {}”在核心评估范围内仍存在明显词汇桥接缺口：rg_only {} 个，vg_only {} 个。",
                case.vg_query,
                case.rg_pattern.unwrap_or_default(),
                core_rg_only_hits,
                core_vg_only_hits
            ),
        });
    }

    if core_vg_only_hits >= 10 && core_shared_hits <= 5 {
        let severity = if matches!(case.query_type, QueryType::Semantic) {
            IssueSeverity::Warn
        } else {
            IssueSeverity::Info
        };
        issues.push(QueryIssue {
            category: IssueCategory::SemanticDriftReview,
            severity,
            summary: format!(
                "核心评估范围内的语义独有结果较多：vg_only {} 个，shared {} 个，建议人工抽样复核相关性。",
                core_vg_only_hits, core_shared_hits
            ),
        });
    }

    issues
}

fn count_file_buckets(paths: &[PathBuf]) -> Vec<FileBucketCount> {
    let mut counts = BTreeMap::<FileBucket, usize>::new();
    for path in paths {
        *counts.entry(classify_file_bucket(path)).or_default() += 1;
    }

    counts
        .into_iter()
        .map(|(bucket, count)| FileBucketCount { bucket, count })
        .collect()
}

fn filter_core_scope<I>(paths: I) -> impl Iterator<Item = PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    paths.into_iter().filter(|path| is_core_scope(path))
}

fn count_extensions(paths: &[PathBuf]) -> Vec<ExtensionCount> {
    let mut counts = BTreeMap::<String, usize>::new();
    for path in paths {
        *counts.entry(extension_label(path)).or_default() += 1;
    }

    let mut entries = counts
        .into_iter()
        .map(|(extension, count)| ExtensionCount { extension, count })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.extension.cmp(&right.extension))
    });
    entries
}

fn is_core_scope(path: &Path) -> bool {
    matches!(
        classify_file_bucket(path),
        FileBucket::Document | FileBucket::Code
    )
}

fn count_non_core_files(paths: &[PathBuf]) -> usize {
    paths.iter().filter(|path| !is_core_scope(path)).count()
}

fn classify_file_bucket(path: &Path) -> FileBucket {
    let normalized = path.to_string_lossy().to_ascii_lowercase();
    if normalized.contains("/refs/scripts/") {
        return FileBucket::Script;
    }

    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("md" | "pdf" | "html" | "htm" | "txt" | "doc" | "docx") => FileBucket::Document,
        Some(
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "java" | "go" | "kt" | "scala" | "rb" | "c"
            | "cc" | "cpp" | "h" | "hpp",
        ) => FileBucket::Code,
        Some("sh" | "bash" | "zsh") => FileBucket::Script,
        Some("yml" | "yaml" | "json" | "toml" | "lock" | "zed" | "ini" | "conf" | "xml") => {
            FileBucket::Config
        }
        None => FileBucket::Other,
        Some(_) => FileBucket::Other,
    }
}

fn extension_label(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_else(|| "(no_extension)".to_string())
}

fn build_summary(queries: &[CoverageQueryReport]) -> CoverageSummary {
    let total_vg_hits = queries.iter().map(|query| query.vg_hits).sum::<usize>();
    let total_rg_hits = queries.iter().map(|query| query.rg_hits).sum::<usize>();
    let total_vg_only_hits = queries
        .iter()
        .map(|query| query.vg_only_hits)
        .sum::<usize>();
    let total_rg_only_hits = queries
        .iter()
        .map(|query| query.rg_only_hits)
        .sum::<usize>();
    let total_shared_hits = queries.iter().map(|query| query.shared_hits).sum::<usize>();
    let total_net_gain = total_vg_only_hits as isize - total_rg_only_hits as isize;
    let core_total_vg_hits = queries
        .iter()
        .map(|query| query.core_vg_hits)
        .sum::<usize>();
    let core_total_rg_hits = queries
        .iter()
        .map(|query| query.core_rg_hits)
        .sum::<usize>();
    let core_total_vg_only_hits = queries
        .iter()
        .map(|query| query.core_vg_only_hits)
        .sum::<usize>();
    let core_total_rg_only_hits = queries
        .iter()
        .map(|query| query.core_rg_only_hits)
        .sum::<usize>();
    let core_total_shared_hits = queries
        .iter()
        .map(|query| query.core_shared_hits)
        .sum::<usize>();
    let core_total_net_gain = core_total_vg_only_hits as isize - core_total_rg_only_hits as isize;

    CoverageSummary {
        total_vg_hits,
        total_rg_hits,
        total_vg_only_hits,
        total_rg_only_hits,
        total_shared_hits,
        total_net_gain,
        query_type_summaries: build_query_type_summaries(queries),
        core_total_vg_hits,
        core_total_rg_hits,
        core_total_vg_only_hits,
        core_total_rg_only_hits,
        core_total_shared_hits,
        core_total_net_gain,
        core_query_type_summaries: build_query_type_summaries_with_scope(queries, true),
        issue_summaries: build_issue_summaries(queries),
    }
}

fn build_query_type_summaries(queries: &[CoverageQueryReport]) -> Vec<QueryTypeSummary> {
    build_query_type_summaries_with_scope(queries, false)
}

fn build_query_type_summaries_with_scope(
    queries: &[CoverageQueryReport],
    core_only: bool,
) -> Vec<QueryTypeSummary> {
    let mut grouped = BTreeMap::<QueryType, Vec<&CoverageQueryReport>>::new();
    for query in queries {
        grouped.entry(query.query_type).or_default().push(query);
    }

    grouped
        .into_iter()
        .map(|(query_type, reports)| {
            let total_vg_hits = reports
                .iter()
                .map(|query| {
                    if core_only {
                        query.core_vg_hits
                    } else {
                        query.vg_hits
                    }
                })
                .sum::<usize>();
            let total_rg_hits = reports
                .iter()
                .map(|query| {
                    if core_only {
                        query.core_rg_hits
                    } else {
                        query.rg_hits
                    }
                })
                .sum::<usize>();
            let total_vg_only_hits = reports
                .iter()
                .map(|query| {
                    if core_only {
                        query.core_vg_only_hits
                    } else {
                        query.vg_only_hits
                    }
                })
                .sum::<usize>();
            let total_rg_only_hits = reports
                .iter()
                .map(|query| {
                    if core_only {
                        query.core_rg_only_hits
                    } else {
                        query.rg_only_hits
                    }
                })
                .sum::<usize>();
            let total_shared_hits = reports
                .iter()
                .map(|query| {
                    if core_only {
                        query.core_shared_hits
                    } else {
                        query.shared_hits
                    }
                })
                .sum::<usize>();
            let total_net_gain = total_vg_only_hits as isize - total_rg_only_hits as isize;
            let improvement_pct = if total_rg_hits == 0 {
                None
            } else {
                Some((total_net_gain as f64 / total_rg_hits as f64) * 100.0)
            };

            QueryTypeSummary {
                query_type,
                query_count: reports.len(),
                total_vg_hits,
                total_rg_hits,
                total_vg_only_hits,
                total_rg_only_hits,
                total_shared_hits,
                total_net_gain,
                improvement_pct,
            }
        })
        .collect()
}

fn build_issue_summaries(queries: &[CoverageQueryReport]) -> Vec<IssueSummary> {
    let mut grouped = BTreeMap::<IssueCategory, (IssueSeverity, Vec<String>)>::new();
    for query in queries {
        for issue in &query.issues {
            let entry = grouped
                .entry(issue.category)
                .or_insert((issue.severity, Vec::new()));
            if issue.severity > entry.0 {
                entry.0 = issue.severity;
            }
            entry.1.push(query.label.clone());
        }
    }

    grouped
        .into_iter()
        .map(|(category, (highest_severity, mut labels))| {
            labels.sort();
            labels.dedup();
            IssueSummary {
                category,
                affected_queries: labels.len(),
                highest_severity,
                queries: labels,
            }
        })
        .collect()
}

fn print_table(queries: &[CoverageQueryReport], summary: &CoverageSummary) {
    println!("| 查询 | 类型 | vg 命中 | rg 命中 | vg_only | rg_only | shared | 净增 |");
    println!("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |");
    for query in queries {
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            query.label,
            query.query_type.as_label(),
            query.vg_hits,
            query.rg_hits,
            query.vg_only_hits,
            query.rg_only_hits,
            query.shared_hits,
            query.net_file_gain,
        );
    }
    println!(
        "| 合计 | - | {} | {} | {} | {} | {} | {} |",
        summary.total_vg_hits,
        summary.total_rg_hits,
        summary.total_vg_only_hits,
        summary.total_rg_only_hits,
        summary.total_shared_hits,
        summary.total_net_gain
    );

    print_grouped_summary(
        "原始口径分组",
        &summary.query_type_summaries,
        summary.total_vg_hits,
        summary.total_rg_hits,
        summary.total_vg_only_hits,
        summary.total_rg_only_hits,
        summary.total_shared_hits,
        summary.total_net_gain,
    );
    print_grouped_summary(
        "核心评估范围分组（Document + Code）",
        &summary.core_query_type_summaries,
        summary.core_total_vg_hits,
        summary.core_total_rg_hits,
        summary.core_total_vg_only_hits,
        summary.core_total_rg_only_hits,
        summary.core_total_shared_hits,
        summary.core_total_net_gain,
    );

    if !summary.issue_summaries.is_empty() {
        println!();
        println!("| 问题分类 | 影响查询数 | 最高级别 | 查询 |");
        println!("| --- | ---: | --- | --- |");
        for item in &summary.issue_summaries {
            println!(
                "| {} | {} | {} | {} |",
                item.category.as_label(),
                item.affected_queries,
                item.highest_severity.as_label(),
                item.queries.join("；"),
            );
        }
    }

    for query in queries {
        if query.issues.is_empty() {
            continue;
        }
        println!();
        println!("## {}", query.label);
        for issue in &query.issues {
            println!(
                "- [{}] {}：{}",
                issue.severity.as_label(),
                issue.category.as_label(),
                issue.summary
            );
        }
    }
}

fn print_grouped_summary(
    title: &str,
    items: &[QueryTypeSummary],
    total_vg_hits: usize,
    total_rg_hits: usize,
    total_vg_only_hits: usize,
    total_rg_only_hits: usize,
    total_shared_hits: usize,
    total_net_gain: isize,
) {
    println!();
    println!("{title}");
    println!("| 分组 | 查询数 | vg 命中 | rg 命中 | vg_only | rg_only | shared | 净增 | 提升 |");
    println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for item in items {
        let improvement = item
            .improvement_pct
            .map(|value| format!("{value:+.1}%"))
            .unwrap_or_else(|| "n/a".to_string());
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            item.query_type.as_label(),
            item.query_count,
            item.total_vg_hits,
            item.total_rg_hits,
            item.total_vg_only_hits,
            item.total_rg_only_hits,
            item.total_shared_hits,
            item.total_net_gain,
            improvement,
        );
    }
    println!(
        "| 合计 | {} | {} | {} | {} | {} | {} | {} | {} |",
        items.iter().map(|item| item.query_count).sum::<usize>(),
        total_vg_hits,
        total_rg_hits,
        total_vg_only_hits,
        total_rg_only_hits,
        total_shared_hits,
        total_net_gain,
        if total_rg_hits == 0 {
            "n/a".to_string()
        } else {
            format!(
                "{:+.1}%",
                (total_net_gain as f64 / total_rg_hits as f64) * 100.0
            )
        },
    );
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

#[cfg(test)]
mod tests {
    use super::{
        CoverageQueryReport, FileBucket, IssueCategory, IssueSeverity, QueryIssue, QueryType,
        build_query_type_summaries, build_summary, classify_file_bucket, classify_query_issues,
        count_extensions,
    };
    use std::path::PathBuf;

    use super::QueryCase;

    #[test]
    fn classify_file_bucket_separates_documents_and_noise() {
        assert_eq!(
            classify_file_bucket(&PathBuf::from("/tmp/notes/design.md")),
            FileBucket::Document
        );
        assert_eq!(
            classify_file_bucket(&PathBuf::from("/tmp/config/alert.yml")),
            FileBucket::Config
        );
        assert_eq!(
            classify_file_bucket(&PathBuf::from("/tmp/refs/scripts/journal-audit")),
            FileBucket::Script
        );
        assert_eq!(
            classify_file_bucket(&PathBuf::from("/tmp/app/server.py")),
            FileBucket::Code
        );
    }

    #[test]
    fn classify_query_issues_flags_metric_scope_and_lexical_gap() {
        let semantic_case = QueryCase {
            label: "语义",
            vg_query: "技术债务",
            rg_pattern: None,
            query_type: QueryType::Semantic,
        };
        let semantic_vg_only = vec![PathBuf::from("/tmp/a.md"); 12];
        let semantic_issues = classify_query_issues(
            semantic_case,
            &semantic_vg_only,
            &[],
            &semantic_vg_only,
            &[],
            0,
        );
        assert!(semantic_issues.iter().any(|issue| {
            issue.category == IssueCategory::MetricScopeBias
                && issue.severity == IssueSeverity::Warn
        }));

        let synonym_case = QueryCase {
            label: "同义词",
            vg_query: "错误处理",
            rg_pattern: Some("error"),
            query_type: QueryType::Synonym,
        };
        let synonym_vg_only = vec![PathBuf::from("/tmp/one.md"); 2];
        let synonym_rg_only = vec![PathBuf::from("/tmp/two.md"); 7];
        let lexical_issues = classify_query_issues(
            synonym_case,
            &synonym_vg_only,
            &synonym_rg_only,
            &synonym_vg_only,
            &synonym_rg_only,
            1,
        );
        assert!(lexical_issues.iter().any(|issue| {
            issue.category == IssueCategory::LexicalBridgeGap
                && issue.severity == IssueSeverity::High
        }));
    }

    #[test]
    fn classify_query_issues_treats_code_as_core_scope() {
        let case = QueryCase {
            label: "同义词",
            vg_query: "认证授权",
            rg_pattern: Some("auth"),
            query_type: QueryType::Synonym,
        };
        let vg_only = vec![PathBuf::from("/tmp/service/auth.rs")];
        let rg_only = vec![PathBuf::from("/tmp/config/permify.yml")];

        let issues = classify_query_issues(case, &vg_only, &rg_only, &vg_only, &[], 0);
        let noise_issue = issues
            .iter()
            .find(|issue| issue.category == IssueCategory::DatasetNoise)
            .expect("dataset noise issue");
        assert_eq!(noise_issue.severity, IssueSeverity::Info);
        assert!(noise_issue.summary.contains("vg_only 0 个"));
        assert!(noise_issue.summary.contains("rg_only 1 个"));
    }

    #[test]
    fn count_extensions_uses_no_extension_label() {
        let counts = count_extensions(&[
            PathBuf::from("/tmp/a.md"),
            PathBuf::from("/tmp/b"),
            PathBuf::from("/tmp/c.md"),
        ]);
        assert_eq!(counts[0].extension, "md");
        assert_eq!(counts[0].count, 2);
        assert_eq!(counts[1].extension, "(no_extension)");
        assert_eq!(counts[1].count, 1);
    }

    #[test]
    fn build_summary_separates_query_type_metrics() {
        let queries = vec![
            mock_report(
                "语义-1",
                QueryType::Semantic,
                (10, 0, 10, 0, 0),
                (8, 0, 8, 0, 0),
                vec![],
            ),
            mock_report(
                "同义词-1",
                QueryType::Synonym,
                (12, 10, 7, 5, 5),
                (9, 6, 4, 1, 5),
                vec![QueryIssue {
                    category: IssueCategory::LexicalBridgeGap,
                    severity: IssueSeverity::Warn,
                    summary: "bridge".to_string(),
                }],
            ),
        ];

        let summary = build_summary(&queries);
        assert_eq!(summary.total_vg_only_hits, 17);
        assert_eq!(summary.total_rg_only_hits, 5);
        assert_eq!(summary.total_net_gain, 12);
        assert_eq!(summary.core_total_vg_only_hits, 12);
        assert_eq!(summary.core_total_rg_only_hits, 1);
        assert_eq!(summary.core_total_net_gain, 11);
        assert_eq!(summary.core_query_type_summaries.len(), 2);

        let type_summaries = build_query_type_summaries(&queries);
        assert_eq!(type_summaries.len(), 2);
        let synonym = type_summaries
            .iter()
            .find(|item| item.query_type == QueryType::Synonym)
            .expect("synonym summary");
        assert_eq!(synonym.total_net_gain, 2);
        assert_eq!(synonym.improvement_pct, Some(20.0));

        let core_synonym = summary
            .core_query_type_summaries
            .iter()
            .find(|item| item.query_type == QueryType::Synonym)
            .expect("core synonym summary");
        assert_eq!(core_synonym.total_net_gain, 3);
        assert_eq!(core_synonym.improvement_pct, Some(50.0));
    }

    fn mock_report(
        label: &str,
        query_type: QueryType,
        raw: (usize, usize, usize, usize, usize),
        core: (usize, usize, usize, usize, usize),
        issues: Vec<QueryIssue>,
    ) -> CoverageQueryReport {
        let (vg_hits, rg_hits, vg_only_hits, rg_only_hits, shared_hits) = raw;
        let (core_vg_hits, core_rg_hits, core_vg_only_hits, core_rg_only_hits, core_shared_hits) =
            core;
        CoverageQueryReport {
            label: label.to_string(),
            vg_query: label.to_string(),
            rg_pattern: None,
            query_type,
            vg_hits,
            rg_hits,
            vg_files: Vec::new(),
            rg_files: Vec::new(),
            vg_only_files: Vec::new(),
            rg_only_files: Vec::new(),
            shared_files: Vec::new(),
            vg_only_hits,
            rg_only_hits,
            shared_hits,
            net_file_gain: vg_only_hits as isize - rg_only_hits as isize,
            vg_only_file_buckets: Vec::new(),
            rg_only_file_buckets: Vec::new(),
            vg_only_extensions: Vec::new(),
            rg_only_extensions: Vec::new(),
            core_vg_hits,
            core_rg_hits,
            core_vg_only_hits,
            core_rg_only_hits,
            core_shared_hits,
            core_net_file_gain: core_vg_only_hits as isize - core_rg_only_hits as isize,
            issues,
        }
    }
}
