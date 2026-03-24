# Vector Teaser Rendering Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 `vg` 的向量检索结果引入紧凑 teaser 渲染，减少语义命中在终端中的噪声，同时保持纯文本 `rg/rga` 风格输出不变。

**Architecture:** 先在搜索结果中保留 text/vector provenance，再新增一个独立的 vector teaser 生成模块，由输出层按 provenance 分流渲染。语义 chunk 默认大小从 `512` 收到约 `300`，但 teaser 展示与 chunk 存储解耦，避免把大块原文直接打到终端。

**Tech Stack:** Rust workspace、`vg-core` 搜索/输出模块、现有单元测试、`cargo test`

---

### Task 1: 保留 Hybrid 结果的 text/vector provenance

**Files:**
- Modify: `crates/vg-core/src/search/mod.rs`
- Modify: `crates/vg-core/src/search/text.rs`
- Modify: `crates/vg-core/src/search/vector.rs`
- Modify: `crates/vg-core/src/search/hybrid.rs`

**Step 1: 先写失败测试，锁定 hybrid 不能丢 provenance**

在 `crates/vg-core/src/search/hybrid.rs` 现有测试旁新增一个断言：融合后的结果必须能区分 `text_hit` 和 `vector_hit`，不能只有单一 `Hybrid` 状态。

示例断言：

```rust
assert!(fused[0].text_hit);
assert!(fused[0].vector_hit);
```

**Step 2: 运行测试，确认当前实现失败**

Run:
```bash
cargo test -p vg-core fuse_keeps_best_ranked_results -- --nocapture
```

Expected: FAIL，提示 `SearchResult` 上没有 provenance 字段，或相关断言失败。

**Step 3: 在 `SearchResult` 增加 provenance 字段，并在 text/vector 搜索侧设置默认值**

在 `crates/vg-core/src/search/mod.rs` 的 `SearchResult` 中增加布尔字段：

```rust
pub text_hit: bool,
pub vector_hit: bool,
```

初始化规则：

```rust
// text.rs
text_hit: true,
vector_hit: false,

// vector.rs
text_hit: false,
vector_hit: true,
```

如果需要保持 `--vg-json` 兼容，给这两个字段加 `#[serde(skip_serializing)]`。

**Step 4: 修改 hybrid 融合逻辑，合并 provenance 而不是覆盖来源**

在 `crates/vg-core/src/search/hybrid.rs`：

- 文本结果进入 `merged` 时保留 `text_hit`
- 向量结果合并到已有结果时执行 provenance OR：

```rust
existing.text_hit |= result.text_hit;
existing.vector_hit |= result.vector_hit;
```

- `result.source` 仍可保留 `SearchSource::Hybrid`，但 renderer 之后不再只依赖它

**Step 5: 运行测试并提交**

Run:
```bash
cargo test -p vg-core search::hybrid::tests::fuse_keeps_best_ranked_results -- --nocapture
```

Expected: PASS，融合测试能证明 provenance 被保留。

Commit:
```bash
git add crates/vg-core/src/search/mod.rs crates/vg-core/src/search/text.rs crates/vg-core/src/search/vector.rs crates/vg-core/src/search/hybrid.rs
git commit -m "feat: 保留 hybrid 检索来源信息"
```

### Task 2: 新增 CJK-aware teaser 预算与切段模块

**Files:**
- Create: `crates/vg-core/src/output/vector_teaser.rs`
- Modify: `crates/vg-core/src/output/mod.rs`

**Step 1: 先写失败测试，固定 teaser 的中文切段规则**

在新文件 `crates/vg-core/src/output/vector_teaser.rs` 中先写单元测试，覆盖：

- 中文句子按 `。！？；` 优先切段
- 最多输出 `3` 行
- 每行目标约 `100` token
- 每行只露前 `30` token 左右并追加 `...`
- 句子不够时用固定窗口补齐

示例测试：

```rust
#[test]
fn builds_three_teaser_lines_for_chinese_chunk() {
    let chunk = "第一段很长...。第二段也很长...。第三段继续...。";
    let lines = build_teaser_lines(chunk, 3, 100, 30);
    assert!(lines.len() <= 3);
    assert!(lines.iter().all(|line| line.ends_with("...")));
}
```

**Step 2: 运行测试，确认失败**

Run:
```bash
cargo test -p vg-core vector_teaser -- --nocapture
```

Expected: FAIL，因为模块和函数尚不存在。

**Step 3: 写最小实现，先把 teaser 生成做成纯函数**

在 `crates/vg-core/src/output/vector_teaser.rs` 中实现：

- `approx_token_len(text: &str) -> usize`
- `split_teaser_segments(text: &str, target_tokens: usize) -> Vec<String>`
- `truncate_segment(text: &str, preview_tokens: usize) -> String`
- `build_teaser_lines(text: &str, max_lines: usize, target_tokens: usize, preview_tokens: usize) -> Vec<String>`

实现约束：

- CJK 字符按约 `1 token`
- 连续 ASCII 字母/数字按长度折算
- 优先句子/换行切段，凑不够再补窗口

**Step 4: 在 `output/mod.rs` 暴露模块，并补齐边界测试**

增加 `mod vector_teaser;`，再补至少这些测试：

- 短 chunk 只输出 `1` 行
- 超长单句会在句内窗口切断
- 近似重复 teaser 行会被去重或合并

Run:
```bash
cargo test -p vg-core vector_teaser -- --nocapture
```

Expected: PASS，teaser 生成逻辑可单独稳定测试。

**Step 5: 提交**

```bash
git add crates/vg-core/src/output/vector_teaser.rs crates/vg-core/src/output/mod.rs
git commit -m "feat: 新增向量结果 teaser 生成模块"
```

### Task 3: 按 provenance 分流 renderer，只让向量结果走 teaser

**Files:**
- Modify: `crates/vg-core/src/output/rg_style.rs`
- Modify: `crates/vg-core/src/output/mod.rs`
- Modify: `crates/vg-core/src/search/mod.rs`

**Step 1: 先写失败测试，锁定 text-only / vector-only / dual-hit 的展示分支**

在 `crates/vg-core/src/output/rg_style.rs` 抽一个纯格式化辅助函数，优先测试该函数而不是直接测彩色 stdout。

建议新增类似：

```rust
fn format_result_body(result: &SearchResult, context_lines: usize) -> String
```

测试覆盖：

- text-only 结果仍使用 context/compact 路径
- vector-only 结果使用 `build_teaser_lines`
- dual-hit 结果仍用 text 风格，并包含 `+semantic`

**Step 2: 运行测试，确认当前实现失败**

Run:
```bash
cargo test -p vg-core output::rg_style -- --nocapture
```

Expected: FAIL，当前 renderer 还没有 provenance 分流。

**Step 3: 实现 renderer 分流**

在 `crates/vg-core/src/output/rg_style.rs`：

- 保留现有 text 渲染函数
- 新增 vector teaser 渲染函数，例如：

```rust
fn render_vector_teaser(result: &SearchResult) -> String
```

- `format_result_body` 规则：

```rust
if result.text_hit {
    // dual-hit 也走这里
    render_text_body(...)
} else if result.vector_hit {
    render_vector_teaser(...)
} else {
    compact_content(...)
}
```

dual-hit 的轻量标记可以先实现成 location header 或 body 前缀，避免改太多颜色逻辑。

**Step 4: 运行测试并做一次 CLI 回归**

Run:
```bash
cargo test -p vg-core output::rg_style -- --nocapture
cargo test -p vg-core
```

Expected:

- `rg_style` 分支测试 PASS
- `vg-core` 全量单测 PASS

**Step 5: 提交**

```bash
git add crates/vg-core/src/output/rg_style.rs crates/vg-core/src/output/mod.rs crates/vg-core/src/search/mod.rs
git commit -m "feat: 按检索来源分流向量结果渲染"
```

### Task 4: 收紧 chunk 默认值并补 CLI / 文档回归

**Files:**
- Modify: `crates/vg-core/src/config.rs`
- Modify: `docs/vg-tdd.md`

**Step 1: 先写失败测试，锁定新的默认 chunk 大小**

在 `crates/vg-core/src/config.rs` 现有参数解析测试旁新增断言，确认默认 `chunk_size == 300`。

示例：

```rust
#[test]
fn parse_uses_new_default_chunk_size() {
    let parsed = SplitArgs::parse(&[OsString::from("query")]).expect("parse");
    assert_eq!(parsed.chunk_size, 300);
}
```

**Step 2: 运行测试，确认失败**

Run:
```bash
cargo test -p vg-core parse_uses_new_default_chunk_size -- --nocapture
```

Expected: FAIL，当前默认仍是 `512`。

**Step 3: 修改默认值与帮助文案**

在 `crates/vg-core/src/config.rs`：

- 把 `DEFAULT_CHUNK_SIZE` 改为 `300`
- 更新 `usage()` 里的默认值说明

同步更新 `docs/vg-tdd.md` 中关于 semantic chunk 的描述，避免文档漂移。

**Step 4: 运行全量回归**

Run:
```bash
cargo test -p vg-core
cargo test -p vg-cli --test coverage_comparison -- --ignored --nocapture
```

Expected:

- `vg-core` 单测全部通过
- 覆盖率对比测试如本地语料存在，可人工观察语义输出没有明显退化

**Step 5: 提交**

```bash
git add crates/vg-core/src/config.rs docs/vg-tdd.md
git commit -m "docs: 收紧向量 chunk 默认值并同步设计文档"
```

### Task 5: 终态验收与人工体验检查

**Files:**
- Modify: `crates/vg-core/src/output/vector_teaser.rs`
- Modify: `crates/vg-core/src/output/rg_style.rs`
- Modify: `crates/vg-core/src/search/hybrid.rs`
- Modify: `crates/vg-core/src/config.rs`

**Step 1: 用最小中文语料做一次手工验收**

准备一个小型 markdown 语料目录，至少包含：

- 纯文本命中案例
- 纯向量命中案例
- 文本 + 向量双命中案例

**Step 2: 跑三条命令看终端观感**

Run:
```bash
cargo run -p vg-cli -- --vg-semantic "营销管理" /path/to/corpus
cargo run -p vg-cli -- "营销管理" /path/to/corpus
cargo run -p vg-cli -- --vg-json "营销管理" /path/to/corpus
```

Expected:

- `--vg-semantic`：看到 1..=3 行 teaser，而不是大段原文
- 默认 hybrid：text-only 结果保持现有风格；vector-only 结果显示 teaser；dual-hit 保留 text 风格并带语义标记
- `--vg-json`：输出结构不因 teaser 渲染而变得难以消费

**Step 3: 必要时微调 teaser 参数，但不要扩大范围**

允许微调的仅限：

- teaser 目标行数
- 目标 token 预算
- preview token 预算
- dual-hit 标记文案

不要在这个任务里顺手改 ranking、threshold、RRF 权重。

**Step 4: 最终全量测试**

Run:
```bash
cargo fmt --check
cargo test
```

Expected: 全部 PASS。

**Step 5: 最终提交**

```bash
git add crates/vg-core/src/search/mod.rs crates/vg-core/src/search/text.rs crates/vg-core/src/search/vector.rs crates/vg-core/src/search/hybrid.rs crates/vg-core/src/output/mod.rs crates/vg-core/src/output/rg_style.rs crates/vg-core/src/output/vector_teaser.rs crates/vg-core/src/config.rs docs/vg-tdd.md
git commit -m "feat: 压缩向量检索结果展示"
```
