# 命令行入口与参数拆分

**覆盖目标**：`vg`/`vg-index` 的入口行为、`--vg-*` 自消费规则、非 `--vg-*` 参数透传规则、模型配置读取。

## 前置条件

- 本机已安装 `rga`、`rga-preproc`
- 存在可搜索夹具，缓存目录可写（或显式指定 `--vg-cache-path`）

## 约束

- 参数解析是 best-effort，不保证覆盖所有 `rg` 短参数簇
- 默认只指向 `bge-small-zh`，其余模型和维度由用户在 `.config.json` 中配置

## 测试矩阵

| 场景 | 用例 ID | 用例标题 | 类型 | 前置条件 | 预期结果 | 自动化建议 |
| --- | --- | --- | --- | --- | --- | --- |
| 模式选择 | CLI-001 | 无模式参数时默认走 hybrid | 主路径 | 存在 query 和 path | 返回融合结果而非纯文本结果 | CLI smoke |
| 模式选择 | CLI-002 | `--vg-semantic` 时只执行语义搜索 | 主路径 | 索引已建或可自动建 | 输出结果全部来自语义侧 | CLI smoke |
| 模式选择 | CLI-003 | `--vg-text` 时完全透传到 `rga` | 主路径 | 本机有 `rga` | 输出与 `rga` 直接执行一致 | CLI smoke |
| 模式选择 | CLI-004 | `--vg-semantic` 与 `--vg-text` 同时传入 | 异常 | 无 | 立即报错，退出码非 0 | CLI regression |
| 参数拆分 | CLI-005 | `--vg-top-k`/`--vg-threshold` 被本地消费 | 主路径 | 语义或 hybrid 模式 | 结果数量/阈值生效 | CLI regression |
| 参数拆分 | CLI-006 | `--rga-adapters`/`-i` 等参数继续透传 | 主路径 | 文本侧命令可执行 | `rga` 行为受透传参数影响 | CLI regression |
| 参数拆分 | CLI-007 | 带值参数不会被误识别成 query/path | 边界 | 使用 `-g "*.rs"` 等 | query/path 提取正确 | Unit / CLI regression |
| 参数拆分 | CLI-008 | 非法数值参数报错 | 异常 | `--vg-top-k=abc` | 返回错误信息，退出码非 0 | Unit / CLI regression |
| 模型与帮助 | CLI-009 | `--vg-list-models` 正常输出 | 主路径 | 无 | 返回当前 fastembed 可解析模型列表 | CLI smoke |
| 模型与帮助 | CLI-010 | `.config.json` 中的模型与维度配置生效 | 主路径 | 缓存目录已写入用户配置 | 按配置加载模型，维度匹配 | CLI regression |
| 模型与帮助 | CLI-011 | `--help` 输出完整 | 边界 | 无 | 输出 usage，退出码为 0 | CLI smoke |
| 模型与帮助 | CLI-012 | 配置目录不可写时初始化失败 | 权限 | 只读目录且无 `.config.json` | 返回明确错误 | Manual |
