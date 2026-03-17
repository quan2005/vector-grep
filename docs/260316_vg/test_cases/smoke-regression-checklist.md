# Smoke / Regression 清单

## 测试分层

| 层级 | 覆盖范围 |
|---|---|
| 单元测试 | 参数拆分、分块器、RRF 融合、sqlite-vec 存取、上下文输出 |
| CLI smoke | 命令可运行、主链路可用（text / semantic / hybrid / indexer）|
| CLI regression | 行为不回退：增量索引、参数透传、JSON 结构、路径渲染 |
| Manual | 网络/模型下载失败、权限异常、语义相关性主观验收 |

**环境要求**：本机安装 `rga`/`rga-preproc`；使用 `--vg-cache-path` 隔离缓存目录；首次运行允许模型下载。

---

## Smoke 清单

| ID | 命令 | 预期结果 |
| --- | --- | --- |
| SMK-001 | `cargo fmt && cargo test` | 所有单元测试通过 |
| SMK-002 | `cargo run -p vg-indexer -- --vg-rebuild ./tests/fixtures` | 索引成功，`indexed > 0` |
| SMK-003 | `cargo run -p vg-cli -- --vg-text OAuth2 ./tests/fixtures` | 返回 `rga` 文本命中 |
| SMK-004 | `cargo run -p vg-cli -- --vg-semantic 用户认证 ./tests/fixtures` | 返回语义命中结果 |
| SMK-005 | `cargo run -p vg-cli -- OAuth2 ./tests/fixtures` | 返回 hybrid 结果 |
| SMK-006 | `cargo run -p vg-cli -- --vg-json --vg-semantic 用户认证 ./tests/fixtures` | 合法 JSON，含 `results`/`stats` |
| SMK-007 | `cargo run -p vg-cli -- --vg-index-stats ./tests/fixtures` | 返回正确 stats |
| SMK-008 | `cargo bench --bench search_pipeline --no-run` | benchmark 骨架编译通过 |

---

## Regression 清单

### CLI 与参数

| ID | 检查项 | 步骤 | 预期结果 |
| --- | --- | --- | --- |
| REG-CLI-001 | 默认模式仍为 hybrid | `vg query path` | 融合结果，不退化成 text-only |
| REG-CLI-002 | `--vg-text` 保持透传 | 对比 `vg --vg-text` 与 `rga` | 输出一致或等价 |
| REG-CLI-003 | `--vg-semantic` 不受文本侧影响 | 执行语义模式 | 输出源仅来自向量侧 |
| REG-CLI-004 | 带值透传参数不误判 | 使用 `-g "*.rs"` 等 | query/path 提取正确 |
| REG-CLI-005 | `.config.json` 配置生效 | 预写 `model_id`/`model_dimensions` | 按配置加载，必要时触发重建 |

### 索引与缓存

| ID | 检查项 | 步骤 | 预期结果 |
| --- | --- | --- | --- |
| REG-IDX-001 | 重复执行不重复全量建索引 | 连续执行两次 `vg-index` | 第二次 `files_indexed` 接近 0 |
| REG-IDX-002 | 修改文件后重建索引 | 修改夹具内容再搜索 | 新内容可被命中 |
| REG-IDX-003 | 删除文件后索引清理 | 删除夹具文件再同步 | stats 下降，搜索不再返回该文件 |
| REG-IDX-004 | `--vg-rebuild` 强制重建 | 执行 rebuild | 结果与首次建索引一致 |
| REG-IDX-005 | `--vg-no-cache` 不污染默认缓存 | 指定 no-cache 执行 | 默认缓存不新增/不变更 |

### 语义检索

| ID | 检查项 | 步骤 | 预期结果 |
| --- | --- | --- | --- |
| REG-SEM-001 | `top_k` 生效 | `--vg-top-k=1` | 仅 1 条结果 |
| REG-SEM-002 | `threshold` 生效 | 提高阈值 | 结果集减少 |
| REG-SEM-003 | path scope 生效 | 搜索子目录 | 结果不越界 |
| REG-SEM-004 | 空结果稳定 | 使用无关 query | 返回空结果，不报错 |

### Hybrid 与输出

| ID | 检查项 | 步骤 | 预期结果 |
| --- | --- | --- | --- |
| REG-HYB-001 | 同一 file:line 正常去重 | 构造 text+vector 同时命中 | 最终结果不重复 |
| REG-HYB-002 | 相对路径输出稳定 | 在仓库根运行 | 输出不带绝对前缀 |
| REG-HYB-003 | `--vg-show-score` 稳定输出 | 打开该参数 | 每条结果带 score |
| REG-HYB-004 | `--vg-json` 结构不变 | 保存 JSON 快照 | 字段完整，路径与分值可解析 |
| REG-HYB-005 | 默认输出不展开 context | 不传 `--vg-context` | 输出紧凑，不自动回读上下文块 |
| REG-HYB-006 | `--vg-context` 向后兼容 | `--vg-context=1` | 输出包含上下文行和命中标记（已实现扩展项）|

---

## 建议执行顺序

- 每次提交前：`SMK-001` 到 `SMK-005`
- 涉及索引逻辑改动：加跑全部 `REG-IDX-*`
- 涉及排序/融合/输出改动：加跑全部 `REG-HYB-*`
- 涉及模型/向量/过滤改动：加跑全部 `REG-SEM-*`
