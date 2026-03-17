use std::{ffi::OsString, process::Command};

use anyhow::{Context, Result, anyhow};
use serde_json::Value;

use crate::search::{SearchResult, SearchSource};

pub fn search_json(args: &[OsString]) -> Result<Vec<SearchResult>> {
    let mut command = Command::new("rga");
    command
        .arg("--json")
        .arg("--line-number")
        .arg("--color=never");
    command.args(args);

    let output = command.output().context("执行 rga JSON 搜索失败")?;
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(anyhow!(
            "rga 搜索失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();
    for line in stdout.lines() {
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.get("type").and_then(Value::as_str) != Some("match") {
            continue;
        }

        let data = match value.get("data") {
            Some(data) => data,
            None => continue,
        };
        let path = data
            .get("path")
            .and_then(|value| value.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        let content = data
            .get("lines")
            .and_then(|value| value.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let line_number = data.get("line_number").and_then(Value::as_u64).unwrap_or(0) as usize;
        let file_path = std::path::PathBuf::from(path)
            .canonicalize()
            .unwrap_or_else(|_| path.into());
        results.push(SearchResult {
            file_path,
            start_line: line_number,
            end_line: line_number,
            score: 0.0,
            content,
            source: SearchSource::Text,
        });
    }

    Ok(results)
}
