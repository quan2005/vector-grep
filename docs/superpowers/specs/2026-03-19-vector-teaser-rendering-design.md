# Vector Result Teaser Rendering Design

## Overview

Improve `vg` result readability for Chinese semantic search by changing only the rendering path for vector-backed results. The goal is to stop large semantic chunks from flooding the terminal while preserving the current `rg/rga`-style display for pure text matches.

This design intentionally separates:

- retrieval granularity: smaller semantic chunks for better vector recall
- presentation granularity: compact teaser lines for vector-backed output

No code implementation is included in this document. This is the agreed design baseline for a later implementation plan.

## Problem Statement

Current semantic and hybrid output has two stacked issues:

1. semantic chunks are too large by default
   - current default chunk size is `512` tokens with `64` overlap
   - for Chinese notes and PDFs, one returned chunk often contains too much unrelated text
2. vector-backed results reuse a display path that is optimized for text matches
   - text-mode rendering is good for `rg/rga` line hits
   - vector hits are concept-level matches, so dumping large chunk content or re-reading context creates noisy output

For queries like `vg "营销管理"`, the user experience problem is less about "no retrieval signal" and more about "the displayed semantic hit is too large and visually noisy".

## Goals

- Reduce the visual footprint of semantic results in terminal output
- Preserve the current text result experience
- Keep hybrid mode readable by rendering text-backed and vector-backed hits differently
- Make the teaser generation work well for Chinese-first content, especially markdown notes and preprocessed PDF text

## Non-Goals

- Do not redesign ranking in this phase
- Do not change pure text search behavior
- Do not introduce model-specific exact tokenizer logic into rendering
- Do not change JSON output shape in this phase unless required by provenance changes

## User Experience

### Semantic Mode

`--vg-semantic` results always render as vector teasers:

- one file header
- one location header
- up to `3` teaser lines
- each line targets about `100` tokens of source material
- each line displays only the leading `30` tokens approximately, then appends `...`

This makes semantic results scannable without dumping a full chunk.

### Hybrid Mode

Hybrid results render according to match provenance:

- text-only hit: preserve current `rg/rga`-style output
- vector-only hit: render as vector teaser
- text + vector hit: preserve text-style output and add a lightweight semantic marker such as `+semantic`

This rule avoids penalizing exact text matches while still exposing vector-only discoveries cleanly.

## Retrieval Granularity

Default semantic chunk size should move from `512` to approximately `300` tokens.

Rationale:

- `512` is too large for note-like Chinese content
- `300` is a better compromise between topic cohesion and display locality
- smaller chunks reduce the chance that one vector hit carries multiple unrelated subtopics

Chunk overlap can remain conservative initially. The design does not require changing overlap semantics yet.

### Token Approximation

Chunking and teaser splitting both use approximate token counting rather than model-exact tokenization.

Recommended approximation:

- each CJK character counts as about `1` token
- contiguous ASCII letters and digits are counted proportionally rather than character-for-character inflation
- punctuation contributes minimally

Reasoning:

- this is stable across model swaps
- terminal rendering should not be tightly coupled to embedding tokenizer internals
- the approximation is sufficient for display budgeting

## Provenance Preservation

The current hybrid fusion collapses result provenance into a single `Hybrid` source. That is insufficient for selective rendering because the output layer can no longer tell whether a result came from text, vector, or both.

The design therefore requires preserving two concepts:

- ranking source: the result can still be ranked in hybrid mode
- match provenance: the renderer must know whether the hit is text-backed, vector-backed, or dual-backed

Minimum required states:

- `text_hit`
- `vector_hit`

Derived rendering states:

- text-only
- vector-only
- text+vector

This provenance is the key enabler for "only change vector rendering".

## Teaser Generation Algorithm

Vector teaser generation should be implemented as a pure content-to-lines transformation over the matched chunk content. It should not depend on re-reading file context.

### Input

- matched semantic chunk text
- optional line metadata already carried by the chunk

### Output

- `1..=3` teaser lines

### Steps

1. Normalize whitespace
   - trim leading and trailing whitespace
   - collapse excessive spaces
   - preserve meaningful line boundaries where possible
2. Split by high-value boundaries first
   - blank lines
   - line breaks
   - Chinese sentence punctuation: `。！？；`
   - fallback English punctuation when present
3. Build teaser segments with a target budget of about `100` tokens each
4. If boundary-based splitting yields fewer than `3` useful segments, fill the remainder using fixed windows
5. For each teaser segment, show only the leading `30` tokens approximately, then append `...`
6. Drop or merge near-duplicate adjacent teaser lines

## Rendering Rules

### Text Results

Keep the current renderer unchanged for text-backed matches.

### Vector Results

Vector teaser rendering should:

- not call the existing context re-read path
- not print full chunk bodies
- not rely on whitespace tokenization alone

Preferred shape:

```text
path/to/file.md
11:20  [semantic]
> 第一行 teaser...
> 第二行 teaser...
> 第三行 teaser...
```

Exact marker text is implementation detail, but the semantic rendering should be visibly distinct from text-mode output.

### Dual Hits in Hybrid

If a result is matched by both text and vector:

- preserve text-style rendering
- optionally append a lightweight marker such as `+semantic`
- do not additionally expand the vector teaser

Reason:

- text-backed lines already have strong locality
- showing both render styles for one result would duplicate noise

## Edge Cases

### Very Short Chunks

If the matched chunk is short, render only one teaser line. Do not force three lines.

### Very Long Sentences

If a single sentence exceeds the target teaser window, split inside the sentence using the fallback fixed-window rule.

### Markdown Notes

Prefer retaining headings, list prefixes, and lead sentences because they often carry most of the semantic signal.

### Preprocessed PDF Text

PDF-derived text often contains unnatural line breaks. The teaser builder should lightly normalize those breaks before sentence segmentation, otherwise every teaser line becomes fragmented.

### Chinese Text Without Spaces

The existing whitespace-based compaction strategy is not sufficient. Chinese teaser generation must operate on CJK-aware token approximation rather than `split_whitespace()` semantics.

## Implementation Boundaries

Expected impact areas for a later implementation:

- chunk default configuration
- hybrid result metadata / provenance retention
- vector-specific teaser builder
- renderer dispatch based on provenance
- tests for Chinese markdown and PDF-derived content

This phase should avoid mixing ranking changes with rendering changes. Keep the rollout narrow and observable.

## Validation Plan

Success should be validated with queries that previously produced visually noisy semantic output, especially short Chinese intent queries such as:

- `营销管理`
- `品牌定位`
- `消费者需求`

Expected validation outcome:

- pure text hits still look like current `rg/rga`
- vector-only hits shrink to short teaser lines
- hybrid output remains readable and no longer dumps large semantic blocks by default

## Open Questions

- whether the semantic marker text should be always shown or only under `--vg-show-score`
- whether JSON output should later expose provenance flags explicitly
- whether overlap should also be tuned when the chunk size drops to `300`
