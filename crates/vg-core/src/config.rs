use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

const DEFAULT_TOP_K: usize = 10;
const DEFAULT_THRESHOLD: f32 = 0.5;
const DEFAULT_CHUNK_SIZE: usize = 300;
const DEFAULT_CHUNK_OVERLAP: usize = 64;
const DEFAULT_CONTEXT: usize = 1;
const DEFAULT_MODEL_ID: &str = "bge-small-zh";
const DEFAULT_MODEL_DIMENSIONS: usize = 512;
const DEFAULT_POOLING: &str = "mean";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Hybrid,
    Semantic,
    Text,
}

#[derive(Debug, Clone)]
pub struct CliArgs {
    pub mode: SearchMode,
    pub top_k: usize,
    pub threshold: f32,
    pub no_cache: bool,
    pub rebuild: bool,
    pub cache_path: Option<PathBuf>,
    pub index_only: bool,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub list_models: bool,
    pub json: bool,
    pub show_score: bool,
    pub context: usize,
    pub index_stats: bool,
    pub help: bool,
    pub version: bool,
    pub passthrough_args: Vec<OsString>,
    pub query: Option<String>,
    pub paths: Vec<PathBuf>,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            mode: SearchMode::Hybrid,
            top_k: DEFAULT_TOP_K,
            threshold: DEFAULT_THRESHOLD,
            no_cache: false,
            rebuild: false,
            cache_path: None,
            index_only: false,
            chunk_size: DEFAULT_CHUNK_SIZE,
            chunk_overlap: DEFAULT_CHUNK_OVERLAP,
            list_models: false,
            json: false,
            show_score: false,
            context: DEFAULT_CONTEXT,
            index_stats: false,
            help: false,
            version: false,
            passthrough_args: Vec::new(),
            query: None,
            paths: vec![PathBuf::from(".")],
        }
    }
}

pub struct SplitArgs;

impl SplitArgs {
    pub fn parse(raw_args: &[OsString]) -> Result<CliArgs> {
        let mut cli = CliArgs::default();
        let mut mode_seen = None::<SearchMode>;
        let mut index = 0;

        while index < raw_args.len() {
            let arg = raw_args[index].to_string_lossy().to_string();
            match arg.as_str() {
                "-h" | "--help" => cli.help = true,
                "-V" | "--version" => cli.version = true,
                "--vg-semantic" => set_mode(&mut mode_seen, &mut cli, SearchMode::Semantic)?,
                "--vg-text" => set_mode(&mut mode_seen, &mut cli, SearchMode::Text)?,
                "--vg-no-cache" => cli.no_cache = true,
                "--vg-rebuild" => cli.rebuild = true,
                "--vg-index-only" => cli.index_only = true,
                "--vg-list-models" => cli.list_models = true,
                "--vg-json" => cli.json = true,
                "--vg-show-score" => cli.show_score = true,
                "--vg-index-stats" => cli.index_stats = true,
                "--vg-top-k" => {
                    index += 1;
                    cli.top_k = parse_usize(raw_args.get(index), "--vg-top-k")?;
                }
                "--vg-threshold" => {
                    index += 1;
                    cli.threshold = parse_f32(raw_args.get(index), "--vg-threshold")?;
                }
                "--vg-cache-path" => {
                    index += 1;
                    cli.cache_path = Some(parse_path(raw_args.get(index), "--vg-cache-path")?);
                }
                "--vg-chunk-size" => {
                    index += 1;
                    cli.chunk_size = parse_usize(raw_args.get(index), "--vg-chunk-size")?;
                }
                "--vg-chunk-overlap" => {
                    index += 1;
                    cli.chunk_overlap = parse_usize(raw_args.get(index), "--vg-chunk-overlap")?;
                }
                "--vg-model" => {
                    bail!("--vg-model 已移除，请改为编辑 .config.json");
                }
                "--vg-context" => {
                    index += 1;
                    cli.context = parse_usize(raw_args.get(index), "--vg-context")?;
                }
                _ if arg.starts_with("--vg-top-k=") => {
                    cli.top_k = parse_inline_usize(&arg, "--vg-top-k=")?;
                }
                _ if arg.starts_with("--vg-threshold=") => {
                    cli.threshold = parse_inline_f32(&arg, "--vg-threshold=")?;
                }
                _ if arg.starts_with("--vg-cache-path=") => {
                    cli.cache_path =
                        Some(PathBuf::from(arg.trim_start_matches("--vg-cache-path=")));
                }
                _ if arg.starts_with("--vg-chunk-size=") => {
                    cli.chunk_size = parse_inline_usize(&arg, "--vg-chunk-size=")?;
                }
                _ if arg.starts_with("--vg-chunk-overlap=") => {
                    cli.chunk_overlap = parse_inline_usize(&arg, "--vg-chunk-overlap=")?;
                }
                _ if arg.starts_with("--vg-model=") => {
                    bail!("--vg-model 已移除，请改为编辑 .config.json");
                }
                _ if arg.starts_with("--vg-context=") => {
                    cli.context = parse_inline_usize(&arg, "--vg-context=")?;
                }
                _ => cli.passthrough_args.push(raw_args[index].clone()),
            }
            index += 1;
        }

        cli.paths = vec![PathBuf::from(".")];
        let positionals = extract_positionals(&cli.passthrough_args);
        if cli.index_only || cli.index_stats {
            if !positionals.is_empty() {
                cli.paths = positionals.into_iter().map(PathBuf::from).collect();
            }
        } else if !positionals.is_empty() {
            cli.query = Some(positionals[0].clone());
            cli.paths = if positionals.len() > 1 {
                positionals[1..].iter().map(PathBuf::from).collect()
            } else {
                vec![PathBuf::from(".")]
            };
        }

        if cli.top_k == 0 {
            bail!("--vg-top-k 必须大于 0");
        }
        if cli.chunk_overlap >= cli.chunk_size {
            bail!("--vg-chunk-overlap 必须小于 --vg-chunk-size");
        }

        Ok(cli)
    }
}

pub fn build_indexer_args(raw_args: &[OsString]) -> Result<CliArgs> {
    let mut prefixed = Vec::with_capacity(raw_args.len() + 1);
    prefixed.push(OsString::from("--vg-index-only"));
    prefixed.extend(raw_args.iter().cloned());
    SplitArgs::parse(&prefixed)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentConfig {
    pub model_id: String,
    pub model_dimensions: usize,
    #[serde(default = "default_pooling")]
    pub pooling: String,
}

impl Default for PersistentConfig {
    fn default() -> Self {
        Self {
            model_id: DEFAULT_MODEL_ID.to_string(),
            model_dimensions: DEFAULT_MODEL_DIMENSIONS,
            pooling: default_pooling(),
        }
    }
}

fn default_pooling() -> String {
    DEFAULT_POOLING.to_string()
}

impl PersistentConfig {
    pub fn load_or_create(cache_dir: &Path) -> Result<Self> {
        let config_path = cache_dir.join(".config.json");
        match fs::read(&config_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("解析配置失败: {}", config_path.display())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let config = Self::default();
                config.save(&config_path)?;
                Ok(config)
            }
            Err(e) => Err(e).with_context(|| format!("读取配置失败: {}", config_path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("创建目录失败: {}", parent.display()))?;
        }
        let content = serde_json::to_vec_pretty(self).context("序列化配置失败")?;
        fs::write(path, content).with_context(|| format!("写入配置失败: {}", path.display()))
    }
}

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    pub cache_dir: PathBuf,
    pub index_path: PathBuf,
    pub models_dir: PathBuf,
}

impl RuntimePaths {
    pub fn resolve(cache_override: Option<&Path>, no_cache: bool) -> Result<Self> {
        let cache_dir = if let Some(cache_override) = cache_override {
            cache_override.to_path_buf()
        } else if no_cache {
            tempfile::Builder::new()
                .prefix("vg-no-cache-")
                .tempdir()
                .context("创建临时缓存目录失败")?
                .keep()
        } else {
            default_cache_dir()?
        };

        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("创建缓存目录失败: {}", cache_dir.display()))?;
        let models_dir = cache_dir.join("models");
        fs::create_dir_all(&models_dir)
            .with_context(|| format!("创建模型目录失败: {}", models_dir.display()))?;

        Ok(Self {
            index_path: cache_dir.join("index.sqlite3"),
            models_dir,
            cache_dir,
        })
    }
}

fn default_cache_dir() -> Result<PathBuf> {
    let home_dir = dirs::home_dir().ok_or_else(|| anyhow!("无法解析用户主目录"))?;
    Ok(home_dir.join(".cache").join("vg"))
}

pub fn usage(bin_name: &str) -> String {
    format!(
        "\
{bin_name} [VG OPTIONS] [RGA OPTIONS] [RG OPTIONS] PATTERN [PATH ...]

搜索模式:
  --vg-semantic          纯向量语义搜索
  --vg-text              纯文本搜索（完全透传 rga）
  默认无模式参数         hybrid 混合搜索

向量参数:
  --vg-top-k <N>         返回前 N 个结果（默认 10）
  --vg-threshold <F>     相似度阈值（默认 0.3）
  --vg-list-models       列出支持的模型

索引参数:
  --vg-index-only        仅建索引，不执行搜索
  --vg-index-stats       查看索引统计
  --vg-no-cache          使用临时缓存目录
  --vg-rebuild           强制重建索引
  --vg-cache-path <P>    自定义缓存目录
  --vg-chunk-size <N>    分块大小（默认 300）
  --vg-chunk-overlap <N> 分块重叠（默认 64）

输出:
  --vg-json              输出 JSON
  --vg-show-score        显示分数
  --vg-context <N>       可选扩展，按需输出上下文行（默认关闭）

其余参数将原样透传给 rga / rg。"
    )
}

fn set_mode(seen: &mut Option<SearchMode>, cli: &mut CliArgs, mode: SearchMode) -> Result<()> {
    if let Some(previous) = seen {
        if *previous != mode {
            bail!("--vg-semantic 与 --vg-text 不能同时使用");
        }
    }
    *seen = Some(mode);
    cli.mode = mode;
    Ok(())
}

fn parse_usize(value: Option<&OsString>, name: &str) -> Result<usize> {
    parse_string(value, name)?
        .parse::<usize>()
        .with_context(|| format!("{} 需要整数", name))
}

fn parse_f32(value: Option<&OsString>, name: &str) -> Result<f32> {
    parse_string(value, name)?
        .parse::<f32>()
        .with_context(|| format!("{} 需要浮点数", name))
}

fn parse_path(value: Option<&OsString>, name: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(parse_string(value, name)?))
}

fn parse_string(value: Option<&OsString>, name: &str) -> Result<String> {
    value
        .map(|item| item.to_string_lossy().to_string())
        .ok_or_else(|| anyhow!("{} 缺少参数", name))
}

fn parse_inline_usize(input: &str, prefix: &str) -> Result<usize> {
    input
        .trim_start_matches(prefix)
        .parse::<usize>()
        .with_context(|| format!("{} 需要整数", prefix.trim_end_matches('=')))
}

fn parse_inline_f32(input: &str, prefix: &str) -> Result<f32> {
    input
        .trim_start_matches(prefix)
        .parse::<f32>()
        .with_context(|| format!("{} 需要浮点数", prefix.trim_end_matches('=')))
}

fn extract_positionals(args: &[OsString]) -> Vec<String> {
    let mut positionals = Vec::new();
    let mut index = 0;
    let mut treat_rest_as_positionals = false;

    while index < args.len() {
        let arg = args[index].to_string_lossy().to_string();

        if treat_rest_as_positionals {
            positionals.push(arg);
            index += 1;
            continue;
        }

        if arg == "--" {
            treat_rest_as_positionals = true;
            index += 1;
            continue;
        }

        if let Some(option) = long_option_name(&arg) {
            let step = if passthrough_long_option_takes_value(option) && !arg.contains('=') {
                2
            } else {
                1
            };
            index += step;
            continue;
        }

        if arg.starts_with('-') && arg.len() > 1 {
            let step = if passthrough_short_option_takes_value(&arg)
                && !short_option_has_attached_value(&arg)
            {
                2
            } else {
                1
            };
            index += step;
            continue;
        }

        positionals.push(arg);
        index += 1;
    }

    positionals
}

fn long_option_name(arg: &str) -> Option<&str> {
    arg.strip_prefix("--")
        .map(|raw| raw.split('=').next().unwrap_or(raw))
}

fn passthrough_long_option_takes_value(option: &str) -> bool {
    matches!(
        option,
        "regexp"
            | "file"
            | "glob"
            | "iglob"
            | "type"
            | "type-not"
            | "type-add"
            | "type-clear"
            | "replace"
            | "context"
            | "after-context"
            | "before-context"
            | "max-count"
            | "max-depth"
            | "max-filesize"
            | "path-separator"
            | "sort"
            | "sortr"
            | "threads"
            | "encoding"
            | "pre-glob"
            | "engine"
            | "colors"
            | "hostname-bin"
            | "hyperlink-format"
            | "label"
            | "rga-adapters"
            | "rga-cache-path"
            | "rga-cache-compression-level"
            | "rga-cache-max-blob-len"
            | "rga-config-file"
            | "rga-max-archive-recursion"
    )
}

fn passthrough_short_option_takes_value(arg: &str) -> bool {
    matches!(
        arg,
        "-e" | "-f" | "-g" | "-j" | "-m" | "-A" | "-B" | "-C" | "-M" | "-t" | "-T"
    )
}

fn short_option_has_attached_value(arg: &str) -> bool {
    if arg.len() <= 2 {
        return false;
    }
    matches!(
        arg.chars().nth(1),
        Some('e' | 'f' | 'g' | 'j' | 'm' | 'A' | 'B' | 'C' | 'M' | 't' | 'T')
    )
}

#[cfg(test)]
mod tests {
    use super::{PersistentConfig, RuntimePaths, SearchMode, SplitArgs};
    use serde_json::json;
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn split_args_keeps_passthrough() {
        let args = vec![
            OsString::from("--vg-semantic"),
            OsString::from("--vg-top-k=5"),
            OsString::from("--rga-adapters=+sqlite"),
            OsString::from("-i"),
            OsString::from("用户认证"),
            OsString::from("./docs"),
        ];
        let parsed = SplitArgs::parse(&args).expect("args");
        assert_eq!(parsed.mode, SearchMode::Semantic);
        assert_eq!(parsed.top_k, 5);
        assert_eq!(parsed.query.as_deref(), Some("用户认证"));
        assert_eq!(parsed.paths, vec![PathBuf::from("./docs")]);
        assert_eq!(
            parsed.passthrough_args,
            vec![
                OsString::from("--rga-adapters=+sqlite"),
                OsString::from("-i"),
                OsString::from("用户认证"),
                OsString::from("./docs"),
            ]
        );
    }

    #[test]
    fn index_only_uses_positionals_as_paths() {
        let args = vec![
            OsString::from("--vg-index-only"),
            OsString::from("./src"),
            OsString::from("./docs"),
        ];
        let parsed = SplitArgs::parse(&args).expect("args");
        assert!(parsed.query.is_none());
        assert_eq!(
            parsed.paths,
            vec![PathBuf::from("./src"), PathBuf::from("./docs")]
        );
    }

    #[test]
    fn parser_skips_option_values() {
        let args = vec![
            OsString::from("-g"),
            OsString::from("*.rs"),
            OsString::from("auth"),
            OsString::from("./src"),
        ];
        let parsed = SplitArgs::parse(&args).expect("args");
        assert_eq!(parsed.query.as_deref(), Some("auth"));
        assert_eq!(parsed.paths, vec![PathBuf::from("./src")]);
    }

    #[test]
    fn removed_model_flag_returns_error() {
        let args = vec![OsString::from("--vg-model=bge-m3")];
        let error = SplitArgs::parse(&args).expect_err("error");
        assert!(error.to_string().contains("--vg-model 已移除"));
    }

    #[test]
    fn parse_uses_new_default_chunk_size() {
        let parsed = SplitArgs::parse(&[OsString::from("query")]).expect("parse");
        assert_eq!(parsed.chunk_size, 300);
    }

    #[test]
    fn runtime_paths_default_to_home_cache_vg() {
        let paths = RuntimePaths::resolve(None, false).expect("runtime paths");
        let home_dir = dirs::home_dir().expect("home dir");
        assert_eq!(paths.cache_dir, home_dir.join(".cache").join("vg"));
        assert_eq!(paths.models_dir, paths.cache_dir.join("models"));
        assert_eq!(paths.index_path, paths.cache_dir.join("index.sqlite3"));
    }

    #[test]
    fn persistent_config_default_uses_mean_pooling() {
        let config = PersistentConfig::default();
        assert_eq!(config.pooling, "mean");
    }

    #[test]
    fn persistent_config_deserialize_missing_pooling_uses_default() {
        let value = json!({
            "model_id": "bge-small-zh",
            "model_dimensions": 512
        });
        let config: PersistentConfig = serde_json::from_value(value).expect("config");
        assert_eq!(config.pooling, "mean");
    }
}
