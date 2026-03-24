use std::mem;

const DEFAULT_WINDOW_STRIDE_DIVISOR: usize = 2;

pub(crate) fn approx_token_len(text: &str) -> usize {
    let mut tokens = 0;
    let mut ascii_run: usize = 0;

    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            ascii_run += 1;
            continue;
        }

        if ascii_run > 0 {
            tokens += ascii_run.div_ceil(4);
            ascii_run = 0;
        }

        if ch.is_whitespace() {
            continue;
        }

        tokens += usize::from(!is_low_signal_punctuation(ch)).max(is_cjk(ch) as usize);
    }

    if ascii_run > 0 {
        tokens += ascii_run.div_ceil(4);
    }

    tokens.max(1)
}

pub(crate) fn split_teaser_segments(text: &str, target_tokens: usize) -> Vec<String> {
    let normalized = normalize_text(text);
    if normalized.is_empty() {
        return Vec::new();
    }

    let candidates = split_candidates(&normalized);
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_tokens = 0;

    for candidate in candidates {
        let candidate_tokens = approx_token_len(&candidate);
        if candidate_tokens > target_tokens {
            if !current.is_empty() {
                segments.push(mem::take(&mut current));
                current_tokens = 0;
            }
            segments.extend(split_fixed_windows(&candidate, target_tokens));
            continue;
        }

        if current.is_empty() {
            current_tokens = candidate_tokens;
            current = candidate;
            continue;
        }

        if current_tokens + candidate_tokens <= target_tokens {
            current.push(' ');
            current.push_str(&candidate);
            current_tokens += candidate_tokens;
        } else {
            segments.push(mem::take(&mut current));
            current = candidate;
            current_tokens = candidate_tokens;
        }
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

pub(crate) fn truncate_segment(text: &str, preview_tokens: usize) -> String {
    let normalized = normalize_text(text);
    if normalized.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    let mut remaining = preview_tokens.max(1);
    let mut chars = normalized.chars().peekable();

    while let Some(ch) = chars.peek().copied() {
        if ch.is_ascii_alphanumeric() {
            let mut word = String::new();
            while let Some(next) = chars.peek().copied() {
                if next.is_ascii_alphanumeric() {
                    word.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            let word_tokens = word.len().div_ceil(4);
            if word_tokens <= remaining {
                output.push_str(&word);
                remaining = remaining.saturating_sub(word_tokens);
            } else {
                let take_chars = (remaining * 4).max(1).min(word.len());
                output.push_str(&word[..take_chars]);
                break;
            }
            continue;
        }

        chars.next();
        if ch.is_whitespace() {
            if !output.ends_with(' ') && !output.is_empty() {
                output.push(' ');
            }
            continue;
        }

        if is_cjk(ch) || !is_low_signal_punctuation(ch) {
            if remaining == 0 {
                break;
            }
            remaining -= 1;
        }
        output.push(ch);
        if remaining == 0 {
            break;
        }
    }

    let trimmed = output.trim();
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.ends_with("...") {
        trimmed.to_string()
    } else {
        format!("{trimmed}...")
    }
}

pub(crate) fn build_teaser_lines(
    text: &str,
    max_lines: usize,
    target_tokens: usize,
    preview_tokens: usize,
) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }

    let normalized = normalize_text(text);
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut segments = split_teaser_segments(&normalized, target_tokens.max(1));
    if segments.len() < max_lines {
        for fallback in split_fixed_windows(&normalized, target_tokens.max(1)) {
            if segments.len() >= max_lines {
                break;
            }
            if !segments.iter().any(|segment| segment == &fallback) {
                segments.push(fallback);
            }
        }
    }

    let mut lines = Vec::new();
    let mut seen = Vec::new();
    for segment in segments {
        let teaser = truncate_segment(&segment, preview_tokens.max(1));
        if teaser.is_empty() {
            continue;
        }
        let fingerprint = dedupe_key(&teaser);
        if seen.iter().any(|item| item == &fingerprint) {
            continue;
        }
        seen.push(fingerprint);
        lines.push(teaser);
        if lines.len() >= max_lines {
            break;
        }
    }

    if lines.is_empty() {
        vec![truncate_segment(&normalized, preview_tokens.max(1))]
            .into_iter()
            .filter(|line| !line.is_empty())
            .collect()
    } else {
        lines
    }
}

fn normalize_text(text: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_space = false;
    let mut last_was_break = false;

    for raw_line in text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            if !normalized.is_empty() && !last_was_break {
                normalized.push('\n');
                last_was_break = true;
            }
            last_was_space = false;
            continue;
        }

        if !normalized.is_empty() && !last_was_break && !last_was_space {
            normalized.push(' ');
        }

        for ch in trimmed.chars() {
            if ch.is_whitespace() {
                if !last_was_space && !normalized.ends_with('\n') {
                    normalized.push(' ');
                }
                last_was_space = true;
                continue;
            }
            normalized.push(ch);
            last_was_space = false;
            last_was_break = false;
        }
    }

    normalized.trim().to_string()
}

fn split_candidates(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut current = String::new();
    let mut prev_was_newline = false;

    for ch in text.chars() {
        if ch == '\n' {
            if prev_was_newline {
                push_candidate(&mut candidates, mem::take(&mut current));
                prev_was_newline = false;
                continue;
            }
            push_candidate(&mut candidates, mem::take(&mut current));
            prev_was_newline = true;
            continue;
        }

        prev_was_newline = false;
        current.push(ch);
        if is_sentence_boundary(ch) {
            push_candidate(&mut candidates, mem::take(&mut current));
        }
    }

    push_candidate(&mut candidates, current);
    candidates
}

fn split_fixed_windows(text: &str, target_tokens: usize) -> Vec<String> {
    let budget = target_tokens.max(1);
    let stride = (budget / DEFAULT_WINDOW_STRIDE_DIVISOR).max(1);
    let chars = text.chars().collect::<Vec<_>>();
    let mut windows = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let (end, consumed) = take_window(&chars, start, budget);
        if end <= start {
            break;
        }

        let segment = chars[start..end]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        if !segment.is_empty() {
            windows.push(segment);
        }
        if end == chars.len() {
            break;
        }

        start = advance_start(&chars, start, stride, consumed);
    }

    windows
}

fn take_window(chars: &[char], start: usize, budget: usize) -> (usize, usize) {
    let mut end = start;
    let mut remaining = budget.max(1);
    let mut consumed = 0;

    while end < chars.len() {
        let ch = chars[end];
        let token_cost = char_token_cost(ch, chars, end);
        if token_cost > 0 && token_cost > remaining && consumed > 0 {
            break;
        }
        end += 1;
        if token_cost > 0 {
            let applied = token_cost.min(remaining.max(token_cost));
            remaining = remaining.saturating_sub(applied);
            consumed += applied;
            if remaining == 0 {
                break;
            }
        }
    }

    (end, consumed.max(1))
}

fn advance_start(
    chars: &[char],
    start: usize,
    stride_budget: usize,
    consumed_budget: usize,
) -> usize {
    let mut index = start;
    let mut remaining = stride_budget.min(consumed_budget).max(1);

    while index < chars.len() {
        let token_cost = char_token_cost(chars[index], chars, index);
        index += 1;
        if token_cost == 0 {
            continue;
        }
        remaining = remaining.saturating_sub(token_cost.min(remaining.max(token_cost)));
        if remaining == 0 {
            break;
        }
    }

    index
}

fn char_token_cost(ch: char, chars: &[char], index: usize) -> usize {
    if ch.is_whitespace() || is_low_signal_punctuation(ch) {
        return 0;
    }
    if ch.is_ascii_alphanumeric() {
        let mut run_len: usize = 1;
        let mut cursor = index + 1;
        while cursor < chars.len() && chars[cursor].is_ascii_alphanumeric() {
            run_len += 1;
            cursor += 1;
        }
        return if index > 0 && chars[index - 1].is_ascii_alphanumeric() {
            0
        } else {
            run_len.div_ceil(4)
        };
    }
    if is_cjk(ch) {
        return 1;
    }
    1
}

fn push_candidate(candidates: &mut Vec<String>, candidate: String) {
    let trimmed = candidate.trim();
    if !trimmed.is_empty() {
        candidates.push(trimmed.to_string());
    }
}

fn dedupe_key(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '.' && *ch != '…')
        .collect()
}

fn is_sentence_boundary(ch: char) -> bool {
    matches!(ch, '。' | '！' | '？' | '；' | '.' | '!' | '?' | ';')
}

fn is_low_signal_punctuation(ch: char) -> bool {
    matches!(
        ch,
        ',' | '，'
            | '、'
            | ':'
            | '：'
            | '—'
            | '-'
            | '"'
            | '\''
            | '“'
            | '”'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
    )
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0x2CEB0..=0x2EBEF
    )
}

#[cfg(test)]
mod tests {
    use super::{approx_token_len, build_teaser_lines, split_teaser_segments, truncate_segment};

    #[test]
    fn builds_three_teaser_lines_for_chinese_chunk() {
        let chunk = concat!(
            "第一段很长，围绕营销管理展开说明，包含用户洞察、品牌定位、渠道策略与复盘节奏。",
            "为了让 teaser 更稳定，这里继续补充一些中文内容，确保切段预算足够长。",
            "第二段也很长，继续描述消费者需求、竞品分析、增长节奏与内容策略。",
            "这里再补充更多上下文，避免因为文本太短导致切段逻辑退化。",
            "第三段继续，补充执行路径、组织协同、指标拆解与长期目标。"
        );
        let lines = build_teaser_lines(chunk, 3, 35, 12);
        assert!(lines.len() <= 3);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|line| line.ends_with("...")));
    }

    #[test]
    fn short_chunk_keeps_single_teaser_line() {
        let lines = build_teaser_lines("短句说明语义命中。", 3, 100, 30);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "短句说明语义命中。...");
    }

    #[test]
    fn long_sentence_falls_back_to_fixed_windows() {
        let long_sentence = "营销管理".repeat(80);
        let segments = split_teaser_segments(&long_sentence, 20);
        assert!(segments.len() >= 2);
    }

    #[test]
    fn repeated_teasers_are_deduplicated() {
        let chunk = "品牌定位非常重要。品牌定位非常重要。品牌定位非常重要。";
        let lines = build_teaser_lines(chunk, 3, 10, 6);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn ascii_words_are_counted_in_compact_buckets() {
        assert_eq!(approx_token_len("market123"), 3);
    }

    #[test]
    fn truncate_segment_keeps_preview_budget() {
        let teaser = truncate_segment("消费者需求变化很快，需要持续观察。", 6);
        assert!(teaser.ends_with("..."));
        assert!(teaser.starts_with("消费者需求"));
        assert!(approx_token_len(teaser.trim_end_matches("...")) <= 6);
    }
}
