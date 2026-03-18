# Brew Distribution Design

## Overview

Enable `vg` and `vg-index` to be installed via Homebrew on macOS Apple Silicon using a custom tap. Pushing a `v*` tag triggers GitHub Actions to build binaries, create a GitHub Release, and update the tap Formula automatically.

## Scope

- Target platform: macOS Apple Silicon (`aarch64-apple-darwin`) only
- Model files: runtime download on first use (not bundled)
- Distribution: custom tap `quan2005/homebrew-vg` (not homebrew-core)
- Release trigger: `git tag v0.1.0 && git push --tags`

## Binary Names

Two binaries are produced:
- `vg` — from `crates/vg-cli`; requires adding `[[bin]] name = "vg"` to `crates/vg-cli/Cargo.toml` because the package name (`vg-cli`) would otherwise produce a binary named `vg-cli`
- `vg-index` — from `crates/vg-indexer`; requires adding `[[bin]] name = "vg-index"` to `crates/vg-indexer/Cargo.toml` because the package name (`vg-indexer`) would otherwise produce a binary named `vg-indexer`

## Components

### 1. Main repo: `quan2005/vector-grep`

Adds `.github/workflows/release.yml` triggered on `push: tags: ['v*']`.

**Permissions block (job level):**
```yaml
permissions:
  contents: write
```
Required so `GITHUB_TOKEN` can create GitHub Releases.

**Workflow steps:**
1. `actions/checkout`
2. Install Rust stable + `aarch64-apple-darwin` target via `rustup target add aarch64-apple-darwin`
3. `cargo build --release --target aarch64-apple-darwin -p vg-cli -p vg-indexer`
4. Package:
   ```bash
   tar czf vg-${{ github.ref_name }}-aarch64-apple-darwin.tar.gz \
     -C target/aarch64-apple-darwin/release vg vg-index
   ```
5. Compute SHA256 (macOS runner uses `shasum`, not `sha256sum`):
   ```bash
   SHA=$(shasum -a 256 vg-${{ github.ref_name }}-aarch64-apple-darwin.tar.gz | awk '{print $1}')
   ```
6. Create release and upload artifact:
   ```bash
   gh release create ${{ github.ref_name }} \
     vg-${{ github.ref_name }}-aarch64-apple-darwin.tar.gz \
     --title "${{ github.ref_name }}" --generate-notes
   ```
7. Clone tap repo, patch Formula, commit and push:
   ```bash
   git clone https://github.com/quan2005/homebrew-vg
   cd homebrew-vg
   git config user.email "github-actions[bot]@users.noreply.github.com"
   git config user.name "github-actions[bot]"
   VERSION=${{ github.ref_name }}
   perl -pi -e "s|url \".*\"|url \"https://github.com/quan2005/vector-grep/releases/download/${VERSION}/vg-${VERSION}-aarch64-apple-darwin.tar.gz\"|" Formula/vg.rb
   perl -pi -e "s|sha256 \".*\"|sha256 \"${SHA}\"|" Formula/vg.rb
   perl -pi -e "s|version \".*\"|version \"${VERSION#v}\"|" Formula/vg.rb
   git add Formula/vg.rb
   git commit -m "vg ${VERSION}"
   git remote set-url origin https://x-access-token:${{ secrets.HOMEBREW_TAP_TOKEN }}@github.com/quan2005/homebrew-vg
   git push
   ```
   A fresh clone is used on each run to avoid non-fast-forward conflicts.

**Runner:** `macos-15` (pinned, not `macos-latest`, to ensure Apple Silicon and stability).

**Rust toolchain:** Use `dtolnay/rust-toolchain@stable` action to install Rust and add the `aarch64-apple-darwin` target.

**Secret required:** `HOMEBREW_TAP_TOKEN` — a GitHub **fine-grained PAT** (not classic PAT) with `contents: write` scoped to the `quan2005/homebrew-vg` repository only. Set an expiry (max 1 year) and rotate before expiry.

### 2. Tap repo: `quan2005/homebrew-vg`

**The tap repo must be public.** Homebrew taps are cloned without authentication; a private repo cannot be tapped by end users. The CI workflow also performs an unauthenticated clone before injecting the PAT — if the repo is private, the initial clone will fail.

Minimal structure:
```
Formula/
  vg.rb
README.md
```

**Formula (`Formula/vg.rb`):**
```ruby
class Vg < Formula
  desc "ripgrep-all + 本地向量语义搜索，hybrid/semantic/text 三模式"
  homepage "https://github.com/quan2005/vector-grep"
  url "https://github.com/quan2005/vector-grep/releases/download/v0.1.0/vg-v0.1.0-aarch64-apple-darwin.tar.gz"
  sha256 "PLACEHOLDER"
  license "MIT"
  version "0.1.0"

  depends_on "ripgrep-all"
  # ripgrep-all installs the `rga` and `rga-preproc` binaries into PATH;
  # vg locates them by name at runtime

  def install
    bin.install "vg"
    bin.install "vg-index"
  end

  test do
    system "#{bin}/vg", "--help"
  end
end
```

### 3. GitHub Release artifacts

Each tagged release contains:
- `vg-<version>-aarch64-apple-darwin.tar.gz` — contains `vg` and `vg-index` binaries

## User Experience

**Install:**
```bash
brew tap quan2005/vg
brew install vg
```

For the unambiguous form (in case of formula name conflicts):
```bash
brew install quan2005/vg/vg
```

**Upgrade:**
```bash
brew upgrade vg
```

**First run:** `fastembed` downloads the ONNX model file to `~/.cache/vg/` on first search. No extra setup step required.

## Cargo.toml Changes Required

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

## Out of Scope

- Linux support
- Bundled model files / offline install
- homebrew-core submission (future)
- Code signing / notarization
