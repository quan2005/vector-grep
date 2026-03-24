# vg 技术设计文档

更新日期：2026-03-17 | 相关文档：`README.md`、`docs/260316_vg/test_cases/`

## 1. 背景与设计原则

`vg` 在 `rga` 基础上增加本地向量语义检索能力，让用户可以用自然语言描述意图，而不只依赖精确关键词。

设计约束：

1. `vg` 是 `rga` 的上层包装，不替代 `rga`。
2. 保留用户已有的 `rg`/`rga` 使用习惯。
3. 索引增量、可缓存、尽量隐式触发。
4. 不依赖在线 embedding 服务，全部本地推理。

## 2. 架构

### 2.1 调用关系

```
vg [VG OPTIONS] [RGA OPTIONS] [RG OPTIONS] PATTERN [PATH ...]

hybrid（默认）:
  vg
   ├─ 文本侧: rga --json
   ├─ 语义侧: 读时索引 -> 向量检索
   └─ 融合: RRF

--vg-semantic:
  vg -> 索引同步 -> query embedding -> sqlite-vec KNN

--vg-text:
  vg -> 直接透传 rga

vg-index:
  仅执行索引同步
```

### 2.2 模块职责

| 模块 | 职责 |
|---|---|
| `config.rs` | 解析 `--vg-*` 参数，拆分透传参数，读写 `.config.json` |
| `preproc.rs` | 文本文件直读（UTF-8），其余调用 `rga-preproc`，统一输出纯文本 |
| `chunk/` | 滑动窗口分块，保留 byte range 与 line range 元信息 |
| `embed.rs` | 管理 fastembed 模型，统一 query/passage embedding 调用 |
| `store.rs` | SQLite 初始化、文件替换/删除、KNN 查询 |
| `index/mod.rs` | 文件集合扫描、变更分类、并行预处理、批量 embedding、串行写库 |
| `search/` | 文本搜索（`rga --json`）、向量搜索、RRF 融合 |
| `output/` | 终端（rg 风格）与 JSON 两种输出格式 |

### 2.3 参数解析说明

- `--vg-*` 由 `vg` 自消费，其余参数原样透传。
- 透传参数中提取 `PATTERN` 与 `PATH ...`。
- 对常见 `rg/rga` 带值参数已支持跳过取值；边角 case 异常直接向上传递。

## 3. 数据设计

### 3.1 缓存目录结构

```
~/.cache/vg/
├── .config.json
├── models/
└── index.sqlite3
```

### 3.2 SQLite Schema

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
    +file_id   INTEGER,
    +content   TEXT,
    +start_byte  INTEGER,
    +end_byte    INTEGER,
    +start_line  INTEGER,
    +end_line    INTEGER,
    +chunk_index INTEGER
);

CREATE TABLE index_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

一致性约束：

- 重建文件索引前先删除旧 `vec_chunks`。
- `model_id` 或 `model_dimensions` 变化时自动重建 schema。
- `schema_version` 变更时强制重建。

## 4. 核心流程

### 4.1 索引同步

```
遍历 roots
 -> 读取现有索引快照
 -> 并行分类: unchanged / touch / remove / upsert
 -> 批量 embedding（按固定 chunk batch，进度按 batch 推进）
 -> 串行写入 SQLite
 -> 删除 scope 内失效文件
```

### 4.2 语义搜索

```
query -> embedding -> sqlite-vec KNN
 -> 按 roots 过滤
 -> score = 1 / (1 + distance)
 -> top_k -> 输出
```

- 默认语义分块大小为 `300` token，`chunk_overlap` 保持 `64`。
- 向量命中的终端展示不直接复用整段 chunk，而是由 teaser 渲染进一步压缩到更短的预览行。

### 4.3 hybrid 搜索

```
文本侧线程: rga --json
主线程: 索引同步 + 语义搜索
 -> 等待文本结果
 -> RRF 融合（基于 file_path + start_line + end_line 去重）
 -> 输出
```

## 5. 性能设计

| 策略 | 说明 |
|---|---|
| mtime + size 快速预判 | 只对疑似变化文件计算 BLAKE3 |
| 并行预处理 | 文件级预处理与分块并行执行 |
| 批量 embedding | 多文件 chunk 合并成一次调用 |
| WAL | SQLite 写入开启 WAL |
| 暴力 KNN | O(M × D)，适合 1k–50k chunk 的单项目目录场景 |

## 6. 错误处理

| 场景 | 策略 |
|---|---|
| `rga`/`rga-preproc` 不存在 | 直接返回错误 |
| 文本提取失败 | 该文件不入索引，继续处理其余文件 |
| model 维度变化 | 自动重建 schema |
| context 回读失败 | 回退到 chunk 内容输出 |
| 参数/路径缺失 | 立即报错，非 0 退出 |

## 7. 非目标

- 不实现 daemon 或服务端索引进程。
- 不实现分布式/多机索引。
- 不引入 ANN 近似检索引擎。
- 不维护 `rg` 短参数组合的完整兼容列表。

## 8. 已知约束

- `rga` 参数兼容是 best-effort，边角 case 可能直接报错。
- `sqlite-vec` 仍处于 alpha 版本，后续 API 演进需要关注。
- 行号含义依赖文本提取结果，不等同于原始二进制文档的可视页码。
- 大项目首次建索引时，模型下载和文本提取可能较慢。
