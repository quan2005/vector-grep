use std::{
    fs,
    fs::File,
    io::Read,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};

const DIRECT_TEXT_EXTENSIONS: &[&str] = &[
    "c", "cc", "cpp", "css", "csv", "go", "h", "hpp", "java", "js", "json", "jsx", "kt",
    "md", "py", "rb", "rs", "scala", "sh", "sql", "svg", "toml", "ts", "tsx", "txt", "xml", "yaml",
    "yml",
];

/// Extensions where rga-preproc produces cleaner text than reading the file directly.
/// These must not be short-circuited by the UTF-8 probe even though they are valid UTF-8.
const PREPROC_PREFERRED_EXTENSIONS: &[&str] = &["html", "htm"];

pub fn extract_text(path: &Path) -> Result<Option<String>> {
    if should_read_directly(path) {
        return read_utf8(path).map(Some);
    }

    if !prefers_preproc(path) && looks_like_utf8(path)? {
        return read_utf8(path).map(Some);
    }

    let output = Command::new("rga-preproc")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("调用 rga-preproc 失败: {}", path.display()))?;

    if output.status.success() && !output.stdout.is_empty() {
        let raw = String::from_utf8_lossy(&output.stdout);
        let text = filter_preproc_noise(&raw);
        if !text.is_empty() {
            return Ok(Some(text));
        }
    }

    Ok(None)
}

fn should_read_directly(path: &Path) -> bool {
    path.extension()
        .and_then(|item| item.to_str())
        .map(|item| DIRECT_TEXT_EXTENSIONS.contains(&item))
        .unwrap_or(false)
}

fn prefers_preproc(path: &Path) -> bool {
    path.extension()
        .and_then(|item| item.to_str())
        .map(|item| PREPROC_PREFERRED_EXTENSIONS.contains(&item))
        .unwrap_or(false)
}

fn read_utf8(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("读取文件失败: {}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn looks_like_utf8(path: &Path) -> Result<bool> {
    let mut file = File::open(path).with_context(|| format!("读取文件失败: {}", path.display()))?;
    let mut buffer = [0u8; 8192];
    let read = file
        .read(&mut buffer)
        .with_context(|| format!("读取文件失败: {}", path.display()))?;
    let sample = &buffer[..read];
    if sample.contains(&0) {
        return Ok(false);
    }
    Ok(std::str::from_utf8(sample).is_ok())
}

/// Strip the `Page N: ` prefixes produced by `rga-preproc` for PDF and similar formats.
/// Every line in rga-preproc output has the form `Page N: content` or `Page N: ` (empty).
/// We strip the prefix and discard empty lines so the indexed text is clean prose.
fn filter_preproc_noise(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter_map(strip_page_prefix)
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines.join("\n").trim().to_string()
}

/// If `line` starts with `Page N: `, return the content after the prefix.
/// If the line does not match the pattern, return it unchanged (Some(line)).
/// Returns None only for lines that are *only* a page marker with no real content.
fn strip_page_prefix(line: &str) -> Option<&str> {
    let Some(after_page) = line.strip_prefix("Page ") else {
        return Some(line);
    };

    // Find the `: ` separator after the page number
    let Some(colon_pos) = after_page.find(": ") else {
        // "Page N:" with nothing after — discard
        let trimmed = after_page.trim_end_matches(':').trim();
        if trimmed.parse::<u64>().is_ok() {
            return None;
        }
        return Some(line);
    };

    let number_part = &after_page[..colon_pos];
    if number_part.trim().parse::<u64>().is_ok() {
        // Valid "Page N: content" — return just the content
        Some(&after_page[colon_pos + 2..])
    } else {
        // Not a real page prefix (e.g. "Page views: 100") — keep as-is
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::{filter_preproc_noise, strip_page_prefix};

    #[test]
    fn strip_prefix_extracts_content() {
        assert_eq!(strip_page_prefix("Page 13: some content"), Some("some content"));
        assert_eq!(strip_page_prefix("Page 1: 比亚迪股份"), Some("比亚迪股份"));
    }

    #[test]
    fn strip_prefix_discards_empty_page_line() {
        assert_eq!(strip_page_prefix("Page 13:"), None);
        assert_eq!(strip_page_prefix("Page 5: "), Some(""));
    }

    #[test]
    fn strip_prefix_keeps_non_page_lines() {
        assert_eq!(strip_page_prefix("Some normal line"), Some("Some normal line"));
        assert_eq!(strip_page_prefix("Page views: 1000"), Some("Page views: 1000"));
    }

    #[test]
    fn filter_strips_prefixes_and_removes_empty() {
        let input = "Page 1: 比亚迪股份\nPage 1: \nPage 2: 年度报告\nPage 2: ";
        let output = filter_preproc_noise(input);
        assert_eq!(output, "比亚迪股份\n年度报告");
    }

    #[test]
    fn filter_keeps_non_pdf_lines_unchanged() {
        let input = "Normal markdown content\nAnother line";
        let output = filter_preproc_noise(input);
        assert_eq!(output, input);
    }

    #[test]
    fn filter_returns_empty_for_all_empty_pages() {
        let input = "Page 1:\nPage 2:\nPage 3:";
        assert!(filter_preproc_noise(input).is_empty());
    }
}
