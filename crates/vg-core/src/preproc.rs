use std::{
    fs,
    fs::File,
    io::Read,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};

const DIRECT_TEXT_EXTENSIONS: &[&str] = &[
    "c", "cc", "cpp", "css", "csv", "go", "h", "hpp", "html", "java", "js", "json", "jsx", "kt",
    "md", "py", "rb", "rs", "scala", "sh", "sql", "svg", "toml", "ts", "tsx", "txt", "xml", "yaml",
    "yml",
];

pub fn extract_text(path: &Path) -> Result<Option<String>> {
    if should_read_directly(path) || looks_like_utf8(path)? {
        return read_utf8(path).map(Some);
    }

    let output = Command::new("rga-preproc")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("调用 rga-preproc 失败: {}", path.display()))?;

    if output.status.success() && !output.stdout.is_empty() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
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
