## [LRN-20260316-001] correction

**Logged**: 2026-03-16T18:03:17+0800
**Priority**: high
**Status**: completed
**Area**: docs

### Summary
不要把已实现扩展能力和固定模型矩阵误写成 `vg` 的基线需求

### Details
本次用户明确纠正了三点：

1. `--vg-context` 不在原始设计基线中，不应被当成必须项，只能视为已实现扩展。
2. 模型和维度应当是动态配置，代码只提供默认 `bge-small-zh`，其他值由用户在 `.config.json` 中配置。
3. `rg/rga` 参数兼容边界不需要额外形成支持列表，相关异常直接向上传递即可。

### Suggested Action

1. 文档和测试资产中把 `--vg-context` 从基线 smoke 要求中移除。
2. 代码层面不要维护固定模型白名单或自动覆盖用户配置。
3. 评审问题清单中不要把“参数兼容范围定义”当成阻塞项。

### Metadata
- Source: user_feedback
- Related Files: docs/vg-tdd.md, crates/vg-core/src/config.rs, crates/vg-core/src/embed.rs, test/260316_vg/test_cases/review-issues.md, test/260316_vg/test_cases/module-04-hybrid-output.md
- Tags: correction, docs, config

---

## [LRN-20260317-002] correction

**Logged**: 2026-03-17T15:40:00+0800
**Priority**: high
**Status**: completed
**Area**: backend

### Summary
索引阶段的进度条不能只按文件粒度设计，否则会被大文件或大 chunk 批次卡在 `0/N`

### Details
首次修复把 embedding 进度从“整批文件一次调用”降到了“单文件一次调用”，在小样本上可以看到进度推进，但用户在真实语料上仍然观察到 `索引构建 0/501` 长时间不动。说明文件级粒度仍然过粗，首个文件可能包含很多 chunk，或者单文件 embedding 调用本身足够慢。更稳妥的做法是按固定 chunk batch 推进进度，并让进度总数映射到 batch 数而不是文件数。

### Suggested Action

1. embedding 过程按固定 chunk batch 分批调用模型。
2. 进度条总量改成 embedding batch 总数。
3. 回归测试覆盖“单文件跨多个 batch 时仍会推进进度”的场景。

### Metadata
- Source: user_feedback
- Related Files: crates/vg-core/src/index/mod.rs
- Tags: correction, progress, indexing
