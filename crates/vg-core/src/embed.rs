use std::{env, fs, path::Path, str::FromStr, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use fastembed::{
    EmbeddingModel, InitOptionsUserDefined, Pooling, TextEmbedding, TextInitOptions,
    TokenizerFiles, UserDefinedEmbeddingModel,
};
use hf_hub::{
    api::sync::ApiRepo,
    api::{RepoInfo, sync::ApiBuilder},
};
use serde::Deserialize;

use crate::progress::ProgressReporter;

const OLLAMA_DEFAULT_HOST: &str = "http://127.0.0.1:11434";

#[derive(Debug, Clone)]
pub struct SupportedModel {
    pub id: String,
    pub description: String,
    pub dimensions: usize,
    embedding_model: Option<EmbeddingModel>,
}

impl SupportedModel {
    pub fn supported() -> Vec<SupportedModel> {
        TextEmbedding::list_supported_models()
            .into_iter()
            .map(|info| SupportedModel {
                id: format!("{:?}", info.model),
                description: info.description,
                dimensions: info.dim,
                embedding_model: Some(info.model),
            })
            .collect()
    }

    pub fn resolve(id: &str) -> Result<SupportedModel> {
        let embedding_model = parse_model_id(id)?;
        let info = TextEmbedding::get_model_info(&embedding_model)
            .with_context(|| format!("不支持的模型: {}", id))?;
        Ok(SupportedModel {
            id: normalize_model_id(id),
            description: info.description.clone(),
            dimensions: info.dim,
            embedding_model: Some(embedding_model),
        })
    }
}

enum EmbedBackend {
    Fastembed(TextEmbedding),
    Ollama(OllamaEmbedder),
}

pub struct Embedder {
    spec: SupportedModel,
    backend: EmbedBackend,
}

impl Embedder {
    pub fn new(
        model_id: &str,
        cache_dir: &Path,
        pooling: &str,
        reporter: &dyn ProgressReporter,
    ) -> Result<Self> {
        if let Ok(spec) = SupportedModel::resolve(model_id) {
            reporter.on_model_loading(&spec.id, &spec.description);
            let embedding_model = spec
                .embedding_model
                .clone()
                .ok_or_else(|| anyhow!("内置模型解析失败: {}", model_id))?;
            let options = TextInitOptions::new(embedding_model)
                .with_cache_dir(cache_dir.to_path_buf())
                .with_show_download_progress(true);
            let model = TextEmbedding::try_new(options)?;
            reporter.on_model_loaded();
            return Ok(Self {
                spec,
                backend: EmbedBackend::Fastembed(model),
            });
        }

        if let Some(ollama_model) = parse_ollama_model_id(model_id) {
            reporter.on_model_loading(model_id, "通过 Ollama 本地服务加载模型");
            let ollama = OllamaEmbedder::new(&ollama_model)?;
            let dimensions = ollama.detect_dimensions()?;
            reporter.on_model_loaded();

            let spec = SupportedModel {
                id: model_id.to_string(),
                description: format!("Ollama 模型: {ollama_model}"),
                dimensions,
                embedding_model: None,
            };
            return Ok(Self {
                spec,
                backend: EmbedBackend::Ollama(ollama),
            });
        }

        if !is_hf_repo_id(model_id) {
            bail!(
                "不支持的模型: {}（不是 fastembed 内置模型、不是合法的 HuggingFace repo id，也不是 Ollama 模型名）",
                model_id
            );
        }

        reporter.on_model_loading(model_id, "从 HuggingFace Hub 下载自定义模型");
        let model = load_hf_model(model_id, cache_dir, pooling)?;
        reporter.on_model_loaded();

        let spec = SupportedModel {
            id: model_id.to_string(),
            description: format!("HuggingFace 模型: {model_id}"),
            dimensions: 0,
            embedding_model: None,
        };
        Ok(Self {
            spec,
            backend: EmbedBackend::Fastembed(model),
        })
    }

    pub fn model(&self) -> SupportedModel {
        self.spec.clone()
    }

    pub fn embed_passages(&mut self, passages: &[&str]) -> Result<Vec<Vec<f32>>> {
        if passages.is_empty() {
            return Ok(Vec::new());
        }

        match &mut self.backend {
            EmbedBackend::Fastembed(model) => {
                let docs = passages
                    .iter()
                    .map(|content| {
                        format_fastembed_passage(self.spec.embedding_model.as_ref(), content)
                    })
                    .collect::<Vec<_>>();
                model.embed(docs, None)
            }
            EmbedBackend::Ollama(ollama) => {
                let docs = passages.iter().map(|content| content.trim().to_string()).collect::<Vec<_>>();
                ollama.embed_texts(&docs)
            }
        }
    }

    pub fn embed_query(&mut self, query: &str) -> Result<Vec<f32>> {
        let query = query.trim();
        if query.is_empty() {
            bail!("查询不能为空");
        }

        match &mut self.backend {
            EmbedBackend::Fastembed(model) => {
                let mut outputs = model.embed(
                    vec![format_fastembed_query(
                        self.spec.embedding_model.as_ref(),
                        query,
                    )],
                    None,
                )?;
                outputs
                    .pop()
                    .ok_or_else(|| anyhow!("嵌入模型未返回查询向量"))
            }
            EmbedBackend::Ollama(ollama) => ollama.embed_one(query),
        }
    }
}

fn format_fastembed_passage(model: Option<&EmbeddingModel>, content: &str) -> String {
    let content = content.trim();
    match model {
        Some(EmbeddingModel::EmbeddingGemma300M) => {
            format!("title: none | text: {content}")
        }
        _ => format!("passage: {content}"),
    }
}

fn format_fastembed_query(model: Option<&EmbeddingModel>, query: &str) -> String {
    let query = query.trim();
    match model {
        Some(EmbeddingModel::EmbeddingGemma300M) => {
            format!("task: search result | query: {query}")
        }
        _ => format!("query: {query}"),
    }
}

struct OllamaEmbedder {
    model_name: String,
    endpoint: String,
    agent: ureq::Agent,
}

impl OllamaEmbedder {
    fn new(model_name: &str) -> Result<Self> {
        let endpoint = env::var("OLLAMA_HOST").unwrap_or_else(|_| OLLAMA_DEFAULT_HOST.to_string());
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(3))
            .timeout_read(Duration::from_secs(300))
            .timeout_write(Duration::from_secs(30))
            .build();
        Ok(Self {
            model_name: model_name.to_string(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            agent,
        })
    }

    fn detect_dimensions(&self) -> Result<usize> {
        let embedding = self.embed_one("dimension probe")?;
        if embedding.is_empty() {
            bail!("Ollama 未返回有效 embedding");
        }
        Ok(embedding.len())
    }

    fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let mut embeddings = self.embed_texts(&[text.to_string()])?;
        embeddings
            .pop()
            .ok_or_else(|| anyhow!("Ollama 未返回查询向量"))
    }

    fn embed_texts(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{}/api/embed", self.endpoint);
        let request = OllamaEmbedRequest {
            model: &self.model_name,
            input: inputs,
        };
        let body = serde_json::to_string(&request).context("序列化 Ollama 请求失败")?;
        let response = self
            .agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_string(&body);

        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => {
                let error_text = read_response_text(response);
                bail!(
                    "调用 Ollama embed API 失败: {}。请确认模型已安装（例如 `ollama pull {}`）且服务可用",
                    error_text,
                    self.model_name
                );
            }
            Err(ureq::Error::Transport(error)) => {
                bail!(
                    "连接 Ollama 失败: {}。请确认 Ollama 服务已启动，地址为 {}",
                    error,
                    self.endpoint
                );
            }
        };

        let payload: OllamaEmbedResponse = serde_json::from_reader(response.into_reader())
            .context("解析 Ollama embed 响应失败")?;
        if payload.embeddings.len() != inputs.len() {
            bail!(
                "Ollama 返回的 embedding 数量不匹配: 请求 {} 条，返回 {} 条",
                inputs.len(),
                payload.embeddings.len()
            );
        }
        Ok(payload.embeddings)
    }
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(serde::Serialize)]
struct OllamaEmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

fn read_response_text(response: ureq::Response) -> String {
    let mut reader = response.into_reader();
    let mut body = String::new();
    match std::io::Read::read_to_string(&mut reader, &mut body) {
        Ok(_) if !body.trim().is_empty() => body.trim().to_string(),
        _ => "未知错误".to_string(),
    }
}

fn parse_model_id(model_id: &str) -> Result<EmbeddingModel> {
    let normalized = normalize_model_id(model_id);
    EmbeddingModel::from_str(&normalized).map_err(|error| anyhow!(error))
}

fn normalize_model_id(model_id: &str) -> String {
    match model_id {
        "bge-small-zh" => "BGESmallZHV15".to_string(),
        other => other.to_string(),
    }
}

fn parse_ollama_model_id(model_id: &str) -> Option<String> {
    if let Some(rest) = model_id.strip_prefix("ollama:") {
        return Some(rest.to_string());
    }

    if !model_id.contains('/') && model_id.contains(':') {
        return Some(model_id.to_string());
    }

    None
}

fn parse_pooling(pooling: &str) -> Result<Pooling> {
    match pooling.trim().to_ascii_lowercase().as_str() {
        "cls" => Ok(Pooling::Cls),
        "mean" => Ok(Pooling::Mean),
        other => bail!("不支持的 pooling: {other}（可选: cls / mean）"),
    }
}

fn is_hf_repo_id(model_id: &str) -> bool {
    let trimmed = model_id.trim();
    trimmed.contains('/') && !trimmed.starts_with('/') && !trimmed.ends_with('/')
}

fn load_hf_model(repo_id: &str, cache_dir: &Path, pooling: &str) -> Result<TextEmbedding> {
    let pool = parse_pooling(pooling)?;

    let api = ApiBuilder::from_env()
        .with_cache_dir(cache_dir.to_path_buf())
        .with_progress(true)
        .build()
        .context("初始化 HuggingFace Hub 客户端失败，请检查网络或 HF_ENDPOINT")?;
    let repo = api.model(repo_id.to_string());

    let onnx_path = repo
        .get("onnx/model.onnx")
        .with_context(|| format!("模型 {repo_id} 缺少 onnx/model.onnx"))?;
    let onnx_file = fs::read(&onnx_path)
        .with_context(|| format!("读取 ONNX 文件失败: {}", onnx_path.display()))?;

    let repo_info = repo
        .info()
        .with_context(|| format!("读取模型仓库信息失败: {repo_id}"))?;
    let external_initializer_files = collect_external_initializer_files(&repo_info);

    let tokenizer_file = read_repo_file(&repo, repo_id, "tokenizer.json")?;
    let config_file = read_repo_file(&repo, repo_id, "config.json")?;
    let special_tokens_map_file =
        read_repo_file_or_default(&repo, repo_id, "special_tokens_map.json", b"{}".to_vec())?;
    let tokenizer_config_file = read_repo_file(&repo, repo_id, "tokenizer_config.json")?;

    let mut user_model = UserDefinedEmbeddingModel::new(
        onnx_file,
        TokenizerFiles {
            tokenizer_file,
            config_file,
            special_tokens_map_file,
            tokenizer_config_file,
        },
    )
    .with_pooling(pool);

    for file_name in external_initializer_files {
        let buffer = read_repo_file(&repo, repo_id, &file_name)?;
        let initializer_name = Path::new(&file_name)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("无效的 external initializer 文件名: {file_name}"))?
            .to_string();
        user_model = user_model.with_external_initializer(initializer_name, buffer);
    }

    TextEmbedding::try_new_from_user_defined(user_model, InitOptionsUserDefined::new())
        .with_context(|| {
            format!("加载 HuggingFace 模型失败: {repo_id}，请检查模型结构、网络或 HF_ENDPOINT")
        })
}

fn collect_external_initializer_files(repo_info: &RepoInfo) -> Vec<String> {
    let mut files = repo_info
        .siblings
        .iter()
        .map(|s| s.rfilename.as_str())
        .filter(|f| f.starts_with("onnx/") && f.ends_with(".onnx_data"))
        .map(str::to_string)
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    files
}

fn read_repo_file(repo: &ApiRepo, repo_id: &str, file: &str) -> Result<Vec<u8>> {
    let path = repo
        .get(file)
        .with_context(|| format!("模型 {repo_id} 缺少 {file}"))?;
    fs::read(&path).with_context(|| format!("读取文件失败: {}", path.display()))
}

fn read_repo_file_or_default(
    repo: &ApiRepo,
    repo_id: &str,
    file: &str,
    default: Vec<u8>,
) -> Result<Vec<u8>> {
    match repo.get(file) {
        Ok(path) => fs::read(&path).with_context(|| format!("读取文件失败: {}", path.display())),
        Err(error) if error.to_string().contains("status code 404") => Ok(default),
        Err(error) => {
            Err(anyhow!(error)).with_context(|| format!("模型 {repo_id} 读取 {file} 失败"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_external_initializer_files, format_fastembed_passage, format_fastembed_query,
        is_hf_repo_id, parse_ollama_model_id, parse_pooling,
    };
    use fastembed::{EmbeddingModel, Pooling};
    use hf_hub::api::{RepoInfo, Siblings};

    #[test]
    fn parse_pooling_accepts_supported_values() {
        assert!(matches!(
            parse_pooling("mean").expect("mean"),
            Pooling::Mean
        ));
        assert!(matches!(parse_pooling("CLS").expect("cls"), Pooling::Cls));
    }

    #[test]
    fn parse_pooling_rejects_invalid_value() {
        let error = parse_pooling("last-token").expect_err("invalid");
        assert!(error.to_string().contains("不支持的 pooling"));
    }

    #[test]
    fn hf_repo_id_requires_owner_and_repo() {
        assert!(is_hf_repo_id("jinaai/jina-embeddings-v5-text-nano"));
        assert!(!is_hf_repo_id("BGESmallZHV15"));
        assert!(!is_hf_repo_id("/invalid"));
    }

    #[test]
    fn parse_ollama_model_id_supports_tag_and_explicit_prefix() {
        assert_eq!(
            parse_ollama_model_id("qwen3-embedding:0.6b").as_deref(),
            Some("qwen3-embedding:0.6b")
        );
        assert_eq!(
            parse_ollama_model_id("ollama:qwen3-embedding:0.6b").as_deref(),
            Some("qwen3-embedding:0.6b")
        );
        assert_eq!(parse_ollama_model_id("jinaai/jina-embeddings-v5"), None);
    }

    #[test]
    fn collect_external_initializers_only_keeps_onnx_data_files() {
        let repo_info = RepoInfo {
            siblings: vec![
                Siblings {
                    rfilename: "onnx/model.onnx".to_string(),
                },
                Siblings {
                    rfilename: "onnx/model.onnx_data".to_string(),
                },
                Siblings {
                    rfilename: "onnx/model_q4.onnx_data".to_string(),
                },
                Siblings {
                    rfilename: "tokenizer.json".to_string(),
                },
            ],
            sha: "dummy".to_string(),
        };
        let files = collect_external_initializer_files(&repo_info);
        assert_eq!(
            files,
            vec![
                "onnx/model.onnx_data".to_string(),
                "onnx/model_q4.onnx_data".to_string()
            ]
        );
    }

    #[test]
    fn embedding_gemma_uses_search_specific_query_template() {
        assert_eq!(
            format_fastembed_query(Some(&EmbeddingModel::EmbeddingGemma300M), "  embedding  "),
            "task: search result | query: embedding"
        );
    }

    #[test]
    fn embedding_gemma_uses_title_text_passage_template() {
        assert_eq!(
            format_fastembed_passage(Some(&EmbeddingModel::EmbeddingGemma300M), "  document  "),
            "title: none | text: document"
        );
    }
}
