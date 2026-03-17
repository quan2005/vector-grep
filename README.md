# vg

`vg` 是一个面向代码和文档目录的混合搜索 CLI。

- 默认模式：`rga` 文本搜索 + 向量语义搜索，使用 RRF 做融合
- `--vg-semantic`：只做向量搜索
- `--vg-text`：完全透传到 `rga`
- `vg-index`：只建索引

## 快速开始

```bash
cargo run -p vg-cli -- "OAuth2 token" ./tests/fixtures
cargo run -p vg-cli -- --vg-semantic "用户认证" ./tests/fixtures
cargo run -p vg-indexer -- ./tests/fixtures
```

## `.config.json` 配置示例

默认配置文件位置：

- `~/.cache/vg/.config.json`
- 如果使用 `--vg-cache-path <DIR>`，则配置文件位置为 `<DIR>/.config.json`

默认示例：

```json
{
  "model_id": "bge-small-zh",
  "model_dimensions": 512,
  "pooling": "mean"
}
```

自定义模型示例：

```json
{
  "model_id": "jinaai/jina-embeddings-v5-text-nano",
  "model_dimensions": 768,
  "pooling": "mean"
}
```

本地 `Ollama` 模型示例：

```json
{
  "model_id": "qwen3-embedding:0.6b",
  "model_dimensions": 0,
  "pooling": "mean"
}
```

说明：

- `model_id` / `model_dimensions` / `pooling` 由 `.config.json` 决定
- `model_id` 可以是 `fastembed` 内置模型、HuggingFace ONNX repo id，或本地 `Ollama` 模型名
- `Ollama` 模型会在初始化时自动探测维度，并回写 `model_dimensions`
- `pooling` 目前支持 `mean` / `cls`
- 可用模型可通过 `vg --vg-list-models` 查看
- `--vg-model` 已移除，不再作为配置入口
