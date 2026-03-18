# Brew Distribution Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 `vg` / `vg-index` 增加基于 GitHub Release + 自定义 Homebrew tap 的自动分发能力，支持通过打 `v*` tag 发布 Apple Silicon macOS 二进制。

**Architecture:** 在 Rust workspace 中显式声明两个二进制产物名，保证 release 产物和 Homebrew Formula 安装名分别为 `vg` 与 `vg-index`。新增 GitHub Actions workflow，在 tag 推送时构建 `aarch64-apple-darwin` release 包、创建 GitHub Release，并自动更新 `quan2005/homebrew-vg` 仓库里的 `Formula/vg.rb`。同时补充 README 中的安装与升级说明，保证用户分发路径和仓库文档一致。

**Tech Stack:** Rust 2024 workspace、Cargo bin targets、GitHub Actions、GitHub Release、Homebrew custom tap、macOS `tar`/`shasum`/`perl`

---

### Task 1: 声明稳定的二进制产物名

**Files:**
- Modify: `crates/vg-cli/Cargo.toml`
- Modify: `crates/vg-indexer/Cargo.toml`

**Step 1: 先验证当前 manifest 还没有产物别名**

Run:
```bash
cargo metadata --no-deps --format-version 1 | rg '"name":"vg"|'"name":"vg-index"'
```

Expected: 无输出，说明当前 workspace target 里还没有 `vg` / `vg-index`。

**Step 2: 为两个入口补充显式 `[[bin]]` 定义**

Add to `crates/vg-cli/Cargo.toml`:
```toml
[[bin]]
name = "vg"
path = "src/main.rs"
```

Add to `crates/vg-indexer/Cargo.toml`:
```toml
[[bin]]
name = "vg-index"
path = "src/main.rs"
```

**Step 3: 再次验证 Cargo target 名称已经切到预期值**

Run:
```bash
cargo metadata --no-deps --format-version 1 | rg '"name":"vg"|"name":"vg-index"'
```

Expected: 输出里能看到 `vg` 与 `vg-index` 两个 bin target。

**Step 4: 构建两个显式 bin，确认命名可被 Cargo 正常解析**

Run:
```bash
cargo build -p vg-cli --bin vg
cargo build -p vg-indexer --bin vg-index
```

Expected: 两条命令都成功完成。

**Step 5: Commit**

```bash
git add crates/vg-cli/Cargo.toml crates/vg-indexer/Cargo.toml
git commit -m "feat: 声明发布二进制名称"
```

### Task 2: 增加 tag 驱动的 Release 与 Homebrew tap 更新 workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Step 1: 验证 workflow 文件当前不存在**

Run:
```bash
test -f .github/workflows/release.yml
```

Expected: 命令返回非 0，说明文件尚未创建。

**Step 2: 编写 release workflow**

Create `.github/workflows/release.yml` with:
- `on.push.tags: ['v*']`
- `jobs.release.runs-on: macos-15`
- `permissions.contents: write`
- `actions/checkout@v4`
- `dtolnay/rust-toolchain@stable` with `targets: aarch64-apple-darwin`
- `cargo build --locked --release --target aarch64-apple-darwin -p vg-cli -p vg-indexer`
- `tar czf vg-${{ github.ref_name }}-aarch64-apple-darwin.tar.gz -C target/aarch64-apple-darwin/release vg vg-index`
- `shasum -a 256 ...`
- `gh release create ... --title "${{ github.ref_name }}" --generate-notes`
- clone `quan2005/homebrew-vg`
- patch `Formula/vg.rb` 的 `url` / `sha256` / `version`
- 使用 `secrets.HOMEBREW_TAP_TOKEN` 推送回 tap repo

**Step 3: 做一遍 YAML 结构校验**

Run:
```bash
ruby -e 'require "yaml"; YAML.load_file(ARGV[0])' .github/workflows/release.yml
```

Expected: 命令成功退出，没有 YAML 语法错误。

**Step 4: 本地验证 release 构建命令与归档命名**

Run:
```bash
rustup target add aarch64-apple-darwin
cargo build --locked --release --target aarch64-apple-darwin -p vg-cli -p vg-indexer
VERSION=v0.1.0
tar czf vg-${VERSION}-aarch64-apple-darwin.tar.gz -C target/aarch64-apple-darwin/release vg vg-index
tar tzf vg-${VERSION}-aarch64-apple-darwin.tar.gz
```

Expected:
- `cargo build` 成功
- `tar tzf` 输出只有 `vg` 和 `vg-index`

**Step 5: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "feat: 增加 brew 发布工作流"
```

### Task 3: 补充分发说明，保证仓库文档与发布方式一致

**Files:**
- Modify: `README.md`

**Step 1: 在 README 中新增 Homebrew 安装与升级说明**

Add a section covering:
- `brew tap quan2005/vg`
- `brew install vg`
- `brew install quan2005/vg/vg`
- `brew upgrade vg`
- 首次运行会下载模型到 `~/.cache/vg/`
- 发版触发方式 `git tag vX.Y.Z && git push --tags`

**Step 2: 验证关键命令和说明文字已出现在 README**

Run:
```bash
rg 'brew tap quan2005/vg|brew install vg|brew upgrade vg|git tag vX.Y.Z' README.md
```

Expected: 四类关键文案都能匹配到。

**Step 3: Commit**

```bash
git add README.md
git commit -m "docs: 补充 brew 安装说明"
```

### Task 4: 全量回归并整理交付说明

**Files:**
- Modify: `README.md`
- Modify: `crates/vg-cli/Cargo.toml`
- Modify: `crates/vg-indexer/Cargo.toml`
- Modify: `.github/workflows/release.yml`

**Step 1: 运行格式与测试回归**

Run:
```bash
cargo fmt --check
cargo test
```

Expected: 两条命令都成功完成。

**Step 2: 再次确认 release 产物名与 README 安装文案**

Run:
```bash
cargo metadata --no-deps --format-version 1 | rg '"name":"vg"|"name":"vg-index"'
rg 'brew tap quan2005/vg|brew install vg|brew upgrade vg' README.md
```

Expected:
- bin target 名称正确
- README 含安装和升级命令

**Step 3: 汇总需要在 GitHub 上预先配置的外部条件**

Document in final handoff:
- `quan2005/homebrew-vg` 必须是 public repo
- 需要配置 fine-grained PAT 到 `HOMEBREW_TAP_TOKEN`
- tag 发布命令与预期 release artifact 名称

**Step 4: Commit**

```bash
git add .github/workflows/release.yml crates/vg-cli/Cargo.toml crates/vg-indexer/Cargo.toml README.md
git commit -m "feat: 完成 brew 分发链路"
```
