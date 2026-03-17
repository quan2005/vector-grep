use std::{fmt::Write as FmtWrite, io::Write, path::Path};

use anyhow::Result;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    preproc,
    search::{SearchResponse, SearchResult},
};

pub fn render_rg_style(
    response: &SearchResponse,
    show_score: bool,
    context_lines: usize,
) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    for result in &response.results {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(&mut stdout, "{}", display_path(&result.file_path))?;
        stdout.reset()?;

        write!(&mut stdout, "{}:{}  ", result.start_line, result.end_line)?;
        if show_score {
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
            write!(&mut stdout, "[{:.4}] ", result.score)?;
            stdout.reset()?;
        }
        writeln!(
            &mut stdout,
            "{}",
            render_content(result, context_lines)
                .unwrap_or_else(|| compact_content(&result.content))
        )?;
        writeln!(&mut stdout)?;
    }
    Ok(())
}

fn render_content(result: &SearchResult, context_lines: usize) -> Option<String> {
    if context_lines == 0 || result.start_line == 0 {
        return None;
    }

    let text = preproc::extract_text(&result.file_path).ok().flatten()?;
    let lines = text.lines().collect::<Vec<_>>();
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
        let marker = if (start..=end).contains(&line_number) {
            '>'
        } else {
            ' '
        };
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

    use super::render_content;
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
        };
        let rendered = render_content(&result, 1).expect("context");
        assert!(rendered.contains("  2 | line2"));
        assert!(rendered.contains(">    3 | line3"));
        assert!(rendered.contains("  4 | line4"));
    }
}
