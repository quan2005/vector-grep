## [LRN-001] 不要把已实现扩展能力误写成基线需求

**来源**：用户反馈 2026-03-16

1. `--vg-context` 不在原始设计基线中，只能视为已实现扩展。
2. 模型和维度应当是动态配置，代码只提供默认 `bge-small-zh`，其他值由用户在 `.config.json` 中配置。
3. `rg/rga` 参数兼容边界不需要形成支持列表，相关异常直接向上传递即可。

**涉及文件**：`docs/vg-tdd.md`、`crates/vg-core/src/config.rs`、`crates/vg-core/src/embed.rs`

---

## [LRN-002] 索引进度条粒度应按 embedding batch，而非文件数

**来源**：用户反馈 2026-03-17

文件级进度过粗：首个文件可能包含大量 chunk，导致进度在 `0/N` 长时间停滞。应按固定 chunk batch 推进进度，总量映射到 batch 数。

**后续操作**：

1. embedding 按固定 chunk batch 分批调用模型。
2. 进度条总量改为 embedding batch 总数。
3. 补充"单文件跨多个 batch 时进度仍推进"的回归测试。

**涉及文件**：`crates/vg-core/src/index/mod.rs`
