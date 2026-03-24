use std::{
    fs,
    fs::File,
    io::Read,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};

const DIRECT_TEXT_EXTENSIONS: &[&str] = &[
    "c", "cc", "cpp", "css", "csv", "go", "h", "hpp", "java", "js", "json", "jsx", "kt", "md",
    "py", "rb", "rs", "scala", "sh", "sql", "svg", "toml", "ts", "tsx", "txt", "xml", "yaml",
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

pub fn extract_display_lines(path: &Path) -> Result<Option<Vec<String>>> {
    if should_read_directly(path) {
        return read_utf8(path).map(|text| Some(text.lines().map(str::to_string).collect()));
    }

    if !prefers_preproc(path) && looks_like_utf8(path)? {
        return read_utf8(path).map(|text| Some(text.lines().map(str::to_string).collect()));
    }

    let output = Command::new("rga-preproc")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("调用 rga-preproc 失败: {}", path.display()))?;

    if output.status.success() && !output.stdout.is_empty() {
        let raw = String::from_utf8_lossy(&output.stdout);
        let lines = raw
            .lines()
            .map(strip_page_prefix_keep_empty)
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !lines.is_empty() {
            return Ok(Some(lines));
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

fn strip_page_prefix_keep_empty(line: &str) -> &str {
    parse_page_prefixed_content(line).unwrap_or(line)
}

/// If `line` starts with `Page N: `, return the content after the prefix.
/// If the line does not match the pattern, return it unchanged (Some(line)).
/// Returns None only for lines that are *only* a page marker with no real content.
fn strip_page_prefix(line: &str) -> Option<&str> {
    match parse_page_prefixed_content(line) {
        Some("") => None,
        Some(content) => Some(content),
        None => Some(line),
    }
}

fn parse_page_prefixed_content(line: &str) -> Option<&str> {
    let after_page = line.strip_prefix("Page ")?;
    let (number_part, rest) = after_page.split_once(':')?;
    if number_part.trim().parse::<u64>().is_ok() {
        Some(rest.trim_start_matches(' '))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{filter_preproc_noise, strip_page_prefix, strip_page_prefix_keep_empty};

    #[test]
    fn strip_prefix_extracts_content() {
        assert_eq!(
            strip_page_prefix("Page 13: some content"),
            Some("some content")
        );
        assert_eq!(strip_page_prefix("Page 1: 比亚迪股份"), Some("比亚迪股份"));
    }

    #[test]
    fn strip_prefix_discards_empty_page_line() {
        assert_eq!(strip_page_prefix("Page 13:"), None);
        assert_eq!(strip_page_prefix("Page 5: "), None);
    }

    #[test]
    fn strip_prefix_keep_empty_preserves_alignment() {
        assert_eq!(strip_page_prefix_keep_empty("Page 13:"), "");
        assert_eq!(strip_page_prefix_keep_empty("Page 5: "), "");
        assert_eq!(
            strip_page_prefix_keep_empty("Page 7: line content"),
            "line content"
        );
        assert_eq!(
            strip_page_prefix_keep_empty("Page views: 1000"),
            "Page views: 1000"
        );
    }

    #[test]
    fn strip_prefix_keeps_non_page_lines() {
        assert_eq!(
            strip_page_prefix("Some normal line"),
            Some("Some normal line")
        );
        assert_eq!(
            strip_page_prefix("Page views: 1000"),
            Some("Page views: 1000")
        );
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
