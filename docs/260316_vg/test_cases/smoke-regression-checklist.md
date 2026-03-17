# Smoke / Regression 清单

## 1. 测试策略

### 1.1 分层策略

1. 单元测试：
   - 参数拆分
   - 分块器
   - RRF 融合
   - sqlite-vec 存取
   - 上下文输出
2. CLI smoke：
   - 面向“命令可运行、主链路可用”
   - 覆盖 `text / semantic / hybrid / indexer`
3. CLI regression：
   - 面向“行为不回退”
   - 重点覆盖增量索引、参数透传、JSON 结构、路径/上下文渲染
4. Manual：
   - 网络/模型下载失败
   - 权限异常
   - 语义相关性主观验收

### 1.2 环境要求

1. 本机安装 `rga`、`rga-preproc`
2. 允许模型首次下载
3. 使用隔离 cache path，避免互相污染
4. 需要可修改的临时夹具目录

---

## 2. Smoke 清单

| ID | 命令/步骤 | 预期结果 |
| --- | --- | --- |
| SMK-001 | `cargo fmt && cargo test` | 所有单元测试通过 |
| SMK-002 | `cargo run -p vg-indexer -- --vg-rebuild ./tests/fixtures` | 索引成功，输出 `indexed>0` |
| SMK-003 | `cargo run -p vg-cli -- --vg-text OAuth2 ./tests/fixtures` | 返回 `rga` 文本命中 |
| SMK-004 | `cargo run -p vg-cli -- --vg-semantic 用户认证 ./tests/fixtures` | 返回语义命中结果 |
| SMK-005 | `cargo run -p vg-cli -- OAuth2 ./tests/fixtures` | 返回 hybrid 结果 |
| SMK-006 | `cargo run -p vg-cli -- --vg-json --vg-semantic 用户认证 ./tests/fixtures` | 返回合法 JSON，含 `results`/`stats` |
| SMK-007 | `cargo run -p vg-cli -- --vg-index-stats ./tests/fixtures` | 返回正确 stats |
| SMK-008 | `cargo bench --bench search_pipeline --no-run` | benchmark 骨架编译通过 |

---

## 3. Regression 清单

### 3.1 CLI 与参数

| ID | 检查项 | 步骤 | 预期结果 |
| --- | --- | --- | --- |
| REG-CLI-001 | 默认模式仍为 hybrid | 执行 `vg query path` | 返回融合结果，不退化成 text-only |
| REG-CLI-002 | `--vg-text` 仍保持透传 | 对比 `vg --vg-text` 与 `rga` | 输出一致或等价 |
| REG-CLI-003 | `--vg-semantic` 不受文本侧影响 | 执行语义模式 | 输出源仅来自向量侧 |
| REG-CLI-004 | 带值透传参数不误判 | 使用 `-g "*.rs"` 等 | query/path 提取正确 |
| REG-CLI-005 | `.config.json` 用户配置生效 | 预写 `model_id` / `model_dimensions` 后执行命令 | CLI 按配置加载并在需要时触发重建 |

### 3.2 索引与缓存

| ID | 检查项 | 步骤 | 预期结果 |
| --- | --- | --- | --- |
| REG-IDX-001 | 重复执行索引命令不会重复全量建索引 | 连续执行两次 `vg-index` | 第二次 `files_indexed` 接近 0 |
| REG-IDX-002 | 修改文件后会重新建索引 | 修改夹具内容再执行搜索 | 新内容能被命中 |
| REG-IDX-003 | 删除文件后索引会清理 | 删除夹具文件再同步 | stats 下降，搜索不再返回该文件 |
| REG-IDX-004 | `--vg-rebuild` 可强制重建 | 执行 rebuild | 结果与首次建索引一致 |
| REG-IDX-005 | `--vg-no-cache` 不污染默认缓存 | 指定 no-cache 执行 | 默认 cache 不新增/不变更 |

### 3.3 语义检索

| ID | 检查项 | 步骤 | 预期结果 |
| --- | --- | --- | --- |
| REG-SEM-001 | `top_k` 生效 | `--vg-top-k=1` | 仅 1 条结果 |
| REG-SEM-002 | `threshold` 生效 | 提高阈值 | 结果集减少 |
| REG-SEM-003 | path scope 生效 | 搜索子目录 | 结果不越界 |
| REG-SEM-004 | 空结果稳定 | 使用无关 query | 返回空结果，不报错 |

### 3.4 Hybrid 与输出

| ID | 检查项 | 步骤 | 预期结果 |
| --- | --- | --- | --- |
| REG-HYB-001 | 同一 file:line 正常去重 | 构造 text+vector 同时命中 | 最终结果不重复 |
| REG-HYB-002 | 相对路径输出保持稳定 | 在仓库根运行 | 输出不带绝对前缀 |
| REG-HYB-003 | `--vg-show-score` 稳定输出 | 打开该参数 | 每条结果带 score |
| REG-HYB-004 | `--vg-json` 结构不变 | 保存 JSON 快照 | 字段完整，路径与分值可解析 |
| REG-HYB-005 | 默认输出不自动展开 context | 不传 `--vg-context` 执行查询 | 输出保持紧凑，不回读上下文块 |
| REG-HYB-006 | 可选 `--vg-context` 向后兼容 | `--vg-context=1` | 输出包含上下文行和命中标记，不影响主链路 | 已实现扩展项 |

---

## 4. 建议执行顺序

1. 每次提交前至少跑 `SMK-001` 到 `SMK-005`
2. 涉及索引逻辑改动时加跑全部 `REG-IDX-*`
3. 涉及排序、融合、输出改动时加跑全部 `REG-HYB-*`
4. 涉及模型、向量、过滤改动时加跑全部 `REG-SEM-*`

---

## 5. 评审建议

- 当前测试资产足以支撑：
  - 本地 smoke 回归
  - 后续 benchmark 资产细化
  - 语义样例集扩展
- 当前仍建议补齐：
  - 固定 query 样例库
  - 关键透传边界回归样例
