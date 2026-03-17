use anyhow::{Context, Result};

use crate::search::SearchResponse;

pub fn render_json(response: &SearchResponse) -> Result<()> {
    let content = serde_json::to_string_pretty(response).context("序列化 JSON 输出失败")?;
    println!("{content}");
    Ok(())
}
