pub mod hybrid;
pub mod text;
pub mod vector;

use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSource {
    Text,
    Vector,
    Hybrid,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub score: f32,
    pub content: String,
    pub source: SearchSource,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchStats {
    pub query_time_ms: u128,
    pub index_time_ms: u128,
    pub files_indexed: usize,
    pub chunks_total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub stats: SearchStats,
}
