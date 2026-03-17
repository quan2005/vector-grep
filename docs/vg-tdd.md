# vg Technical Design Document (TDD)

## 1. 文档信息

- 项目名称: `vg` / `vector-grep`
- 文档类型: Technical Design Document
- 文档日期: 2026-03-16
- 当前状态: 已落地 MVP + Phase 2 核心优化
- 相关文档:
  - `README.md`
  - `tech-stack.md`
  - `implementation-plan.md`
  - `.context/attachments/plan.md`

---

## 2. 背景与问题定义

`vg` 的目标是在 `rga` (`ripgrep-all`) 的基础上增加向量语义检索能力，使用户可以用自然语言描述搜索意图，例如“如何处理用户认证”、“错误处理应该怎么做”，而不是只依赖精确关键词匹配。

### 2.1 现有痛点

纯 `rg` / `rga` 搜索存在以下限制：

1. 搜索质量依赖用户是否知道源码中的准确命名。
2. 跨语言、跨文档表达存在语义鸿沟。
3. 设计文档、Markdown、代码、Office/PDF 等异构内容无法统一靠关键词覆盖。

### 2.2 核心设计原则

1. `vg` 是 `rga` 的上层包装，不替代 `rga`。
2. 用户已有的 `rg` / `rga` 使用习惯应尽量保留。
3. 默认模式优先给出“文本命中 + 语义命中”的综合结果。
4. 索引构建应尽量隐式、增量、可缓存。
5. 依赖尽量本地化，不引入在线 embedding 服务。

---

## 3. 目标与非目标

### 3.1 目标

1. 提供三种搜索模式：
   - `--vg-text`: 纯文本搜索
   - `--vg-semantic`: 纯向量语义搜索
   - 默认模式: hybrid 混合搜索
2. 支持读时索引，按文件增量更新。
3. 支持文本、Markdown、代码，以及通过 `rga-preproc` 提取的多格式文档。
4. 将索引元数据与向量统一保存在单个 SQLite 数据库中。
5. 提供终端输出与 JSON 输出两种结果格式。
6. 保持单机 CLI 可用性，首次运行允许拉取模型并缓存。

### 3.2 非目标

1. 不实现独立 daemon / 服务端索引进程。
2. 不实现分布式索引或多机共享索引。
3. 不引入 ANN 近似检索引擎。
4. 不在当前阶段承诺与所有 `rg` 短参数组合完全 100% 兼容。
5. 不在当前阶段实现在线模型切换后的无缝热迁移。

---

## 4. 范围定义

### 4.1 当前已实现范围

1. Cargo workspace 三层结构：`vg-core`、`vg-cli`、`vg-indexer`
2. `--vg-*` 参数解析与透传拆分
3. 全局缓存目录、模型配置 `.config.json`、索引 DB 初始化
4. `rga-preproc` 文本提取 + UTF-8 文本直读回退
5. 滑动窗口分块
6. `fastembed` 本地 embedding
7. `sqlite-vec` 向量表与 `rusqlite` 元数据表
8. 纯语义搜索、纯文本搜索、hybrid RRF 融合
9. 终端输出、JSON 输出
10. 已实现但非基线要求的扩展项：上下文输出
11. Phase 2:
    - 文件预处理并行化
    - 跨文件批量 embedding
    - RRF 轻量调权
    - benchmark 骨架

### 4.2 当前未实现但预留的范围

1. 关键透传边界回归样例持续补充
2. 更细粒度的上下文回读策略
3. 完整 benchmark 报告与性能基线追踪

---

## 5. 总体架构

### 5.1 调用关系

```text
vg [VG OPTIONS] [RGA OPTIONS] [RG OPTIONS] PATTERN [PATH ...]

默认 hybrid:
  vg
   ├─ 文本侧: rga --json ...
   ├─ 语义侧: 读时索引 -> 向量检索
   └─ 融合: RRF

--vg-semantic:
  vg
   ├─ 索引同步
   ├─ query embedding
   └─ vector KNN

--vg-text:
  vg -> 直接透传 rga

vg-index:
  仅执行索引同步
```

### 5.2 二进制职责

| 二进制 | 职责 |
|---|---|
| `vg-cli` | 主入口，负责参数拆分、模式选择、输出渲染 |
| `vg-indexer` | 索引专用入口，负责显式重建/同步 |
| `vg-core` | 共享核心能力，包括索引、搜索、存储、输出 |

---

## 6. 模块设计

### 6.1 `config.rs`

职责：

1. 解析 `--vg-*` 参数
2. 保留其余参数供 `rga` / `rg` 透传
3. 管理 `.config.json`
4. 解析缓存路径

关键设计：

1. `vg` 自消费 `--vg-*`
2. 其余参数保留原顺序透传
3. `PATTERN` 与 `PATH ...` 从透传参数中提取

当前约束：

1. 对常见 `rg/rga` 参数已支持跳过取值
2. 对所有短参数簇未做完全语义级兼容，边角 case 异常直接向上传递

### 6.2 `preproc.rs`

职责：

1. 对常见文本文件直接 UTF-8 读取
2. 对其他文件调用 `rga-preproc`
3. 输出统一纯文本给分块器

关键设计：

1. 优先走直接读取，减少外部进程开销
2. UTF-8 预判只读取首个 8 KiB 样本，避免全文件探测
3. 对 `rga-preproc` 失败场景返回 `None`，由上游决定是否移除索引

### 6.3 `chunk/`

职责：

1. 将提取文本切成可向量化的 chunk
2. 保留 byte range 与 line range 元信息

关键设计：

1. 滑动窗口分块
2. 默认 `chunk_size = 512 tokens`、`overlap = 64 tokens`
3. 按段落、句号、换行优先切分
4. 每文件最大 `1000` chunk

### 6.4 `embed.rs`

职责：

1. 管理支持的 embedding 模型
2. 统一 query / passage 的 embedding 调用
3. 使用模型缓存目录避免重复下载

当前策略：

1. 默认配置只指向 `bge-small-zh`
2. 运行时会将 `bge-small-zh` 归一化为 `fastembed` 的 `BGESmallZHV15`
3. 模型、维度与 pooling 由用户在 `.config.json` 中自行配置
4. `model_id` 先按 `fastembed` 内置模型解析，失败后按 HuggingFace repo id 加载
5. 模型或维度配置错误时异常直接向上传递

### 6.5 `store.rs`

职责：

1. 初始化 SQLite / sqlite-vec 扩展
2. 管理 `files`、`vec_chunks`、`index_meta`
3. 处理文件替换、删除、KNN 查询

关键设计：

1. 元数据与向量统一存储在单个 SQLite 文件
2. `vec_chunks` 使用 `vec0` 虚拟表
3. `index_meta` 记录 `model_id`、`dimensions`、`schema_version`
4. 打开 DB 时启用 WAL

### 6.6 `index/mod.rs`

职责：

1. 收集搜索路径下的文件集合
2. 对比现有索引，识别 `unchanged / touch / remove / upsert`
3. 批量调用 embedding
4. 串行写入 SQLite

Phase 2 优化：

1. 文件级预处理走并行
2. 多文件 chunk 合并成一次 embedding 批次
3. SQLite 写入保持串行，避免多线程写锁竞争

### 6.7 `search/`

子模块：

1. `text.rs`: 调用 `rga --json` 并解析文本命中结果
2. `vector.rs`: 纯向量搜索
3. `hybrid.rs`: 文本与向量结果融合

RRF 设计：

1. 基于 `file_path + start_line + end_line` 去重
2. 文本与向量结果分别按 rank 累加分数
3. 当前对向量结果设置轻微权重提升，用于改善默认 hybrid 排序

### 6.8 `output/`

职责：

1. 终端输出（rg 风格）
2. JSON 输出
3. 根据可选扩展参数回读上下文

关键设计：

1. 路径优先展示相对当前目录
2. 终端输出支持按需回读上下文，但这不是需求基线
3. JSON 输出保持结构化，不额外展开上下文块

---

## 7. 数据设计

### 7.1 缓存目录

```text
~/.cache/vg/
├── .config.json
├── models/
└── index.sqlite3
```

### 7.2 `.config.json`

```json
{
  "model_id": "bge-small-zh",
  "model_dimensions": 512,
  "pooling": "mean"
}
```

### 7.3 SQLite Schema

```sql
CREATE TABLE files (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path   TEXT NOT NULL UNIQUE,
    blake3_hash TEXT NOT NULL,
    file_size   INTEGER NOT NULL,
    mtime_ms    INTEGER NOT NULL,
    chunk_count INTEGER NOT NULL DEFAULT 0,
    indexed_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE VIRTUAL TABLE vec_chunks USING vec0(
    embedding float[DIMENSIONS],
    +file_id INTEGER,
    +content TEXT,
    +start_byte INTEGER,
    +end_byte INTEGER,
    +start_line INTEGER,
    +end_line INTEGER,
    +chunk_index INTEGER
);

CREATE TABLE index_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

### 7.4 数据一致性约束

1. `files.file_path` 绝对路径唯一
2. 文件重建索引前先删除旧 `vec_chunks`
3. 模型维度变化时必须重建 schema
4. `schema_version` 变更时必须重建 schema

---

## 8. 核心流程

### 8.1 索引同步流程

```text
输入 roots
 -> 遍历文件
 -> 读取现有索引快照
 -> 并行判断:
      unchanged / touch / remove / upsert
 -> 批量 embedding 所有 upsert chunk
 -> 串行写入 SQLite
 -> 删除 scope 内失效文件
```

### 8.2 纯语义搜索流程

```text
query
 -> 索引同步
 -> query embedding
 -> sqlite-vec KNN
 -> 按 roots 过滤
 -> score = 1 / (1 + distance)
 -> top_k
 -> 输出
```

### 8.3 纯文本搜索流程

```text
vg --vg-text ...
 -> 直接透传给 rga
 -> 继承 stdout/stderr
 -> 保持原行为
```

### 8.4 hybrid 搜索流程

```text
query
 -> 文本侧线程: rga --json
 -> 主线程: 索引同步 + 语义搜索
 -> 等待文本结果
 -> RRF 融合
 -> 输出
```

---

## 9. CLI 设计

### 9.1 入口格式

```bash
vg [VG OPTIONS] [RGA OPTIONS] [RG OPTIONS] PATTERN [PATH ...]
```

### 9.2 已实现参数

#### 模式

- `--vg-semantic`
- `--vg-text`
- 默认 hybrid

#### 搜索与索引

- `--vg-top-k`
- `--vg-threshold`
- `--vg-no-cache`
- `--vg-rebuild`
- `--vg-cache-path`
- `--vg-index-only`
- `--vg-index-stats`
- `--vg-chunk-size`
- `--vg-chunk-overlap`

#### 模型

- `--vg-list-models`
- 模型、维度与 pooling 通过 `.config.json` 配置

#### 输出

- `--vg-json`
- `--vg-show-score`
- `--vg-context`（已实现扩展项，默认关闭，非需求基线）

### 9.3 兼容性说明

1. `--vg-*` 参数由 `vg` 自处理
2. 其余参数按原样透传
3. 当前对常见 `rg/rga` 参数的取值跳过已支持
4. 不维护显式兼容支持列表；解析或执行异常直接向上传递

---

## 10. 性能设计

### 10.1 当前策略

1. 文件变更采用 `mtime + size` 快速预判
2. 只有疑似变化文件才计算 BLAKE3
3. 文件级预处理与分块并行执行
4. 多文件 chunk 批量 embedding
5. SQLite 启用 WAL

### 10.2 复杂度预估

1. 文件扫描: `O(N)`
2. 哈希开销: 仅变化文件触发
3. 向量搜索: `O(M * D)` 暴力 KNN
   - `M`: chunk 数
   - `D`: 向量维度

### 10.3 适用规模

适合单项目目录、`1k - 50k` chunk 量级的本地检索场景。

---

## 11. 错误处理设计

### 11.1 失败场景

1. `rga` / `rga-preproc` 不存在
2. 模型首次下载失败
3. SQLite schema 维度不匹配
4. 查询或路径参数缺失
5. 文本提取失败

### 11.2 处理策略

1. 命令执行失败直接返回错误
2. 模型维度变化自动重建索引
3. 文本提取失败的文件不入索引
4. 上下文回读失败时回退到 chunk 内容输出

---

## 12. 测试策略

### 12.1 单元测试

当前已覆盖：

1. CLI 参数拆分
2. 分块边界与行号
3. sqlite-vec 存取
4. RRF 融合
5. 上下文输出

### 12.2 夹具

`tests/fixtures/` 提供：

1. Rust 代码样例
2. Markdown 文档样例
3. 中文错误处理样例

### 12.3 手工回归命令

```bash
cargo fmt
cargo test
cargo run -p vg-indexer -- --vg-rebuild ./tests/fixtures
cargo run -p vg-cli -- --vg-semantic "用户认证" ./tests/fixtures
cargo run -p vg-cli -- "OAuth2" ./tests/fixtures
cargo run -p vg-cli -- --vg-text "OAuth2" ./tests/fixtures
cargo run -p vg-cli -- --vg-index-stats ./tests/fixtures
```

### 12.4 Benchmark

已补 `criterion` bench 骨架：

1. `chunk_splitter_auth_guide_x200`
2. `hybrid_fusion_500x500`

编译验证命令：

```bash
cargo bench --bench search_pipeline --no-run
```

---

## 13. 安全与运维考虑

1. 本地索引默认写入用户缓存目录，不污染项目仓库
2. 不向外发送源码或文档内容
3. 模型下载依赖外部网络，仅首次触发
4. SQLite 文件是单机缓存，不承诺跨主机共享

---

## 14. 已知约束与风险

1. `rga` 参数兼容并非完整 parser 级别实现，边角 case 可能直接报错并向上传递。
2. `sqlite-vec` 仍处于 alpha 版本，后续 API 演进需要关注。
3. 不同文件格式的“行号”含义依赖提取文本，不等同于原始二进制文档的可视页码。
4. 大项目首次建索引时，模型下载和文本提取仍可能较慢。
5. benchmark 目前只提供骨架，尚未形成固定性能报表。

---

## 15. 后续演进建议

### 15.1 短期

1. 补充关键透传边界回归样例
2. 输出中增加来源标识（text/vector/hybrid）
3. 基于 benchmark 输出性能基线
4. 增加更多模型配置样例与校验工具

### 15.2 中期

1. 支持更细粒度增量策略（chunk 级重建）
2. 支持模型配置校验报告
3. 支持更细致的 ranking 特征融合

### 15.3 长期

1. 独立后台索引进程
2. 项目级索引快照
3. 多工作区共享模型与索引策略优化

---

## 16. 结论

`vg` 当前已经具备作为本地语义检索 CLI 的可用形态：

1. 架构简单，围绕 `rga + fastembed + sqlite-vec`
2. 交互符合 CLI 工具预期
3. 索引与搜索链路完整闭环
4. Phase 2 已完成关键性能优化与 benchmark 基础设施

后续重点不再是“从 0 到 1”，而是兼容性、性能基线、模型扩展与排序质量的持续迭代。
