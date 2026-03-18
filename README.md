# vg

`vg` 之于 `rga`，正如 `rga` 之于 `rg`——上层包装，不是替代。`vg` 在 `rga` 基础上增加本地向量语义检索能力，让用户可以用自然语言描述搜索意图，而不只依赖精确关键词。`--vg-*` 之外的所有参数原样透传给 `rga`/`rg`，已有使用习惯完全保留。

支持三种搜索模式：

- **hybrid（默认）**：`rga` 文本搜索 + 向量语义搜索，RRF 融合排序
- **`--vg-semantic`**：仅向量语义搜索
- **`--vg-text`**：完全透传 `rga`，行为与直接调用 `rga` 一致

索引按需构建，增量更新，缓存在本地 `~/.cache/vg/`。

## 依赖

- `rga` / `rga-preproc`：文本搜索与多格式内容提取（PDF、Office、代码等）
- `fastembed`：本地 embedding 推理，无需外部服务
- `rusqlite` + `sqlite-vec`：向量与元数据统一存储
- `ignore`：遵循 `.gitignore` 的文件遍历

## Homebrew 安装（macOS Apple Silicon）

```bash
brew tap quan2005/vg
brew install vg
```

如需显式指定 tap，可使用：

```bash
brew install quan2005/vg/vg
```

升级：

```bash
brew upgrade vg
```

首次运行时，`fastembed` 会自动把 ONNX 模型下载到 `~/.cache/vg/`，无需额外初始化步骤。

维护者发版方式：

```bash
git tag v0.1.0
git push --tags
```

## 快速开始

```bash
# hybrid 搜索
cargo run -p vg-cli -- "OAuth2 token" ./tests/fixtures

# 纯语义搜索
cargo run -p vg-cli -- --vg-semantic "用户认证" ./tests/fixtures

# 仅建索引
cargo run -p vg-indexer -- ./tests/fixtures
```

## 参数

```
vg [VG OPTIONS] [RGA OPTIONS] [RG OPTIONS] PATTERN [PATH ...]
```

| 参数 | 说明 |
|---|---|
| `--vg-semantic` | 纯向量语义搜索 |
| `--vg-text` | 纯文本搜索，透传 rga |
| `--vg-top-k <N>` | 返回前 N 条结果（默认 10） |
| `--vg-threshold <F>` | 相似度阈值（默认 0.3） |
| `--vg-index-only` | 仅建索引，不执行搜索 |
| `--vg-index-stats` | 查看索引统计 |
| `--vg-rebuild` | 强制重建索引 |
| `--vg-no-cache` | 使用临时缓存目录 |
| `--vg-cache-path <P>` | 自定义缓存目录 |
| `--vg-chunk-size <N>` | 分块大小，单位 token（默认 512） |
| `--vg-chunk-overlap <N>` | 分块重叠（默认 64） |
| `--vg-list-models` | 列出 fastembed 内置模型 |
| `--vg-json` | 输出 JSON |
| `--vg-show-score` | 显示分数 |
| `--vg-context <N>` | 输出命中行前后 N 行上下文 |

`--vg-*` 之外的参数原样透传给 `rga` / `rg`。

## 模型配置

配置文件：`~/.cache/vg/.config.json`（或 `--vg-cache-path` 指定目录下的 `.config.json`）

```json
{
  "model_id": "bge-small-zh",
  "model_dimensions": 512,
  "pooling": "mean"
}
```

`model_id` 支持三类来源：

- `fastembed` 内置模型（通过 `vg --vg-list-models` 查看）
- HuggingFace ONNX repo，如 `jinaai/jina-embeddings-v5-text-nano`
- 本地 Ollama 模型，如 `qwen3-embedding:0.6b`（维度自动探测，`model_dimensions` 填 0）

切换模型或维度后，索引会在下次运行时自动重建。

## 工程结构

```
crates/
  vg-core/      # 共享核心：索引、搜索、存储、输出
  vg-cli/       # vg 主入口
  vg-indexer/   # vg-index 索引专用入口
tests/
  fixtures/     # 搜索与索引测试夹具
  integration/  # 端到端集成测试
docs/           # 设计文档与测试资产
```

## 开发命令

```bash
cargo test                                        # 全量单元测试
cargo fmt --check                                 # 格式检查
cargo bench --bench search_pipeline --no-run      # 确认 benchmark 可编译
```
