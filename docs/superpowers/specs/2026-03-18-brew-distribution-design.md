# Brew Distribution Design

## Overview

Enable `vg` and `vg-index` to be installed via Homebrew on macOS Apple Silicon using a custom tap. Pushing a `v*` tag triggers GitHub Actions to build binaries, create a GitHub Release, and update the tap Formula automatically.

## Scope

- Target platform: macOS Apple Silicon (`aarch64-apple-darwin`) only
- Model files: runtime download on first use (not bundled)
- Distribution: custom tap `quan2005/homebrew-vg` (not homebrew-core)
- Release trigger: `git tag v0.1.0 && git push --tags`

## Components

### 1. Main repo: `quan2005/vector-grep`

Adds `.github/workflows/release.yml` triggered on `push: tags: ['v*']`.

**Workflow steps:**
1. `actions/checkout`
2. Install Rust stable + `aarch64-apple-darwin` target
3. `cargo build --release --target aarch64-apple-darwin -p vg-cli -p vg-indexer`
4. Package: `tar czf vg-${{ github.ref_name }}-aarch64-apple-darwin.tar.gz -C target/aarch64-apple-darwin/release vg vg-index`
5. Compute `sha256sum` of the tarball
6. `gh release create ${{ github.ref_name }}` and upload the `.tar.gz`
7. Clone `quan2005/homebrew-vg`, update `url` and `sha256` in `Formula/vg.rb` via `sed`, commit and push

**Secret required:** `HOMEBREW_TAP_TOKEN` — a GitHub PAT with `contents: write` on the `homebrew-vg` repo.

Runner: `macos-latest` (GitHub-hosted Apple Silicon runner).

### 2. Tap repo: `quan2005/homebrew-vg`

Minimal structure:
```
Formula/
  vg.rb
README.md
```

**Formula skeleton (`Formula/vg.rb`):**
```ruby
class Vg < Formula
  desc "ripgrep-all + 本地向量语义搜索，hybrid/semantic/text 三模式"
  homepage "https://github.com/quan2005/vector-grep"
  url "https://github.com/quan2005/vector-grep/releases/download/VERSION/vg-VERSION-aarch64-apple-darwin.tar.gz"
  sha256 "PLACEHOLDER"
  license "MIT"

  depends_on "ripgrep-all"

  def install
    bin.install "vg"
    bin.install "vg-index"
  end

  test do
    system "#{bin}/vg", "--help"
  end
end
```

The CI workflow replaces `url` and `sha256` on each release using `sed`.

### 3. GitHub Release artifacts

Each tagged release contains:
- `vg-<version>-aarch64-apple-darwin.tar.gz` — contains `vg` and `vg-index` binaries

## User Experience

**Install:**
```bash
brew tap quan2005/vg
brew install vg
```

**Upgrade:**
```bash
brew upgrade vg
```

**First run:** `fastembed` downloads the ONNX model file to `~/.cache/vg/` on first search. No extra setup step required.

## CI Authentication

The workflow authenticates to `homebrew-vg` by rewriting the remote URL:
```bash
git remote set-url origin https://x-access-token:$HOMEBREW_TAP_TOKEN@github.com/quan2005/homebrew-vg
```

The PAT (`HOMEBREW_TAP_TOKEN`) is stored as a secret in `quan2005/vector-grep` and needs only `contents: write` permission scoped to `quan2005/homebrew-vg`.

## Out of Scope

- Linux support
- Bundled model files / offline install
- homebrew-core submission (future)
- Code signing / notarization
