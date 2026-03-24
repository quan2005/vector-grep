use std::{fmt::Write as FmtWrite, io::Write, path::Path};

use anyhow::Result;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    preproc,
    search::{SearchResponse, SearchResult},
};

const VECTOR_TEASER_MAX_LINES: usize = 3;
const VECTOR_TEASER_TARGET_TOKENS: usize = 100;
const VECTOR_TEASER_PREVIEW_TOKENS: usize = 60;

pub fn render_rg_style(
    response: &SearchResponse,
    show_score: bool,
    context_lines: usize,
) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    for result in &response.results {
        let body = format_result_body(result, context_lines);
        let show_line_range = should_render_line_range_prefix(result, &body);

        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(&mut stdout, "{}", display_path(&result.file_path))?;
        stdout.reset()?;

        if show_line_range {
            write!(&mut stdout, "{}:{}  ", result.start_line, result.end_line)?;
        }
        if show_score {
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
            if show_line_range {
                write!(&mut stdout, "[{:.4}] ", result.score)?;
            } else {
                writeln!(&mut stdout, "[{:.4}]", result.score)?;
            }
            stdout.reset()?;
        }
        writeln!(&mut stdout, "{body}")?;
        writeln!(&mut stdout)?;
    }
    Ok(())
}

fn format_result_body(result: &SearchResult, context_lines: usize) -> String {
    if result.text_hit {
        return render_text_body(result, context_lines);
    }
    if result.vector_hit {
        return render_vector_teaser(result);
    }
    compact_content(&result.content)
}

fn render_text_body(result: &SearchResult, context_lines: usize) -> String {
    let body =
        render_content(result, context_lines).unwrap_or_else(|| compact_content(&result.content));
    if result.vector_hit {
        semantic_marker_body("+semantic", &body)
    } else {
        body
    }
}

fn render_content(result: &SearchResult, context_lines: usize) -> Option<String> {
    if context_lines == 0 || result.start_line == 0 {
        return None;
    }

    let lines = preproc::extract_display_lines(&result.file_path)
        .ok()
        .flatten()?;
    if lines.is_empty() {
        return None;
    }

    let start = result.start_line.max(1);
    let end = result.end_line.max(start);
    let from = start.saturating_sub(context_lines + 1);
    let to = (end + context_lines).min(lines.len());
    if from >= to {
        return None;
    }

    let mut output = String::new();
    for (offset, line) in lines[from..to].iter().enumerate() {
        let line_number = from + offset + 1;
        let is_match_line = (start..=end).contains(&line_number);
        if !is_match_line && line.trim().is_empty() {
            continue;
        }
        let marker = if is_match_line { '>' } else { ' ' };
        let _ = writeln!(
            output,
            "{} {:>4} | {}",
            marker,
            line_number,
            line.trim_end()
        );
    }

    if output.ends_with('\n') {
        output.pop();
    }
    Some(output)
}

fn render_vector_teaser(result: &SearchResult) -> String {
    let lines = super::vector_teaser::build_teaser_lines(
        &result.content,
        VECTOR_TEASER_MAX_LINES,
        VECTOR_TEASER_TARGET_TOKENS,
        VECTOR_TEASER_PREVIEW_TOKENS,
    );
    if lines.is_empty() {
        return format!("[semantic]\n> {}", compact_content(&result.content));
    }

    let body = lines
        .into_iter()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("[semantic]\n{body}")
}

fn should_render_line_range_prefix(result: &SearchResult, body: &str) -> bool {
    !(result.text_hit && body.contains(" | "))
}

fn semantic_marker_body(marker: &str, body: &str) -> String {
    if body.contains('\n') {
        format!("{marker}\n{body}")
    } else {
        format!("{marker} {body}")
    }
}

fn compact_content(content: &str) -> String {
    let mut compact = String::with_capacity(content.len().min(224));
    for word in content.split_whitespace() {
        if !compact.is_empty() {
            compact.push(' ');
        }
        if compact.len() + word.len() > 220 {
            compact.push_str("...");
            return compact;
        }
        compact.push_str(word);
    }
    compact
}

fn display_path(path: &Path) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(relative) = path.strip_prefix(&cwd) {
            return relative.display().to_string();
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{format_result_body, render_content, should_render_line_range_prefix};
    use crate::search::{SearchResult, SearchSource};

    #[test]
    fn render_content_reads_context_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sample.txt");
        fs::write(&file, "line1\nline2\nline3\nline4\nline5\n").expect("write");
        let result = SearchResult {
            file_path: PathBuf::from(&file),
            start_line: 3,
            end_line: 3,
            score: 1.0,
            content: "line3".to_string(),
            source: SearchSource::Vector,
            text_hit: false,
            vector_hit: true,
        };
        let rendered = render_content(&result, 1).expect("context");
        assert!(rendered.contains("  2 | line2"));
        assert!(rendered.contains(">    3 | line3"));
        assert!(rendered.contains("  4 | line4"));
    }

    #[test]
    fn render_content_skips_blank_context_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sample.txt");
        fs::write(&file, "line1\n\nline3\n\nline5\n").expect("write");
        let result = SearchResult {
            file_path: PathBuf::from(&file),
            start_line: 3,
            end_line: 3,
            score: 1.0,
            content: "line3".to_string(),
            source: SearchSource::Text,
            text_hit: true,
            vector_hit: false,
        };

        let rendered = render_content(&result, 1).expect("context");
        assert!(!rendered.contains("  2 | "));
        assert!(rendered.contains(">    3 | line3"));
        assert!(!rendered.contains("  4 | "));
    }

    #[test]
    fn format_result_body_keeps_text_rendering_for_text_only_hits() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sample.txt");
        fs::write(&file, "line1\nline2\nline3\n").expect("write");
        let result = SearchResult {
            file_path: PathBuf::from(&file),
            start_line: 2,
            end_line: 2,
            score: 1.0,
            content: "line2".to_string(),
            source: SearchSource::Text,
            text_hit: true,
            vector_hit: false,
        };

        let body = format_result_body(&result, 1);
        assert!(body.contains(">    2 | line2"));
        assert!(!body.contains("[semantic]"));
        assert!(!body.contains("+semantic"));
    }

    #[test]
    fn format_result_body_uses_vector_teaser_for_vector_only_hits() {
        let result = SearchResult {
            file_path: PathBuf::from("notes.md"),
            start_line: 8,
            end_line: 14,
            score: 0.92,
            content: concat!(
                "第一段围绕营销管理展开说明，包含用户洞察、品牌定位、渠道策略与复盘节奏。",
                "第二段继续补充消费者需求、竞品分析、增长策略与执行路径。",
                "第三段描述组织协同、指标拆解和长期节奏。"
            )
            .to_string(),
            source: SearchSource::Vector,
            text_hit: false,
            vector_hit: true,
        };

        let body = format_result_body(&result, 2);
        assert!(body.starts_with("[semantic]"));
        assert!(body.contains("\n> "));
        assert!(!body.contains("|"));
    }

    #[test]
    fn format_result_body_expands_single_semantic_teaser_line() {
        let content = (1..=40)
            .map(|index| format!("phase{index:02}"))
            .collect::<Vec<_>>()
            .join(" ");
        let result = SearchResult {
            file_path: PathBuf::from("notes.md"),
            start_line: 8,
            end_line: 14,
            score: 0.92,
            content,
            source: SearchSource::Vector,
            text_hit: false,
            vector_hit: true,
        };

        let body = format_result_body(&result, 2);
        assert!(body.starts_with("[semantic]"));
        assert!(body.contains("phase20"));
    }

    #[test]
    fn format_result_body_marks_dual_hits_with_semantic_prefix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sample.txt");
        fs::write(&file, "line1\nline2\nline3\n").expect("write");
        let result = SearchResult {
            file_path: PathBuf::from(&file),
            start_line: 2,
            end_line: 2,
            score: 1.0,
            content: "line2".to_string(),
            source: SearchSource::Hybrid,
            text_hit: true,
            vector_hit: true,
        };

        let body = format_result_body(&result, 1);
        assert!(body.starts_with("+semantic"));
        assert!(body.contains(">    2 | line2"));
    }

    #[test]
    fn text_context_body_hides_outer_line_range_prefix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sample.txt");
        fs::write(&file, "line1\nline2\nline3\n").expect("write");
        let result = SearchResult {
            file_path: PathBuf::from(&file),
            start_line: 2,
            end_line: 2,
            score: 1.0,
            content: "line2".to_string(),
            source: SearchSource::Text,
            text_hit: true,
            vector_hit: false,
        };

        let body = format_result_body(&result, 1);
        assert!(!should_render_line_range_prefix(&result, &body));
    }
}
