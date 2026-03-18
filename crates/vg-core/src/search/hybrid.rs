use std::collections::HashMap;

use crate::search::{SearchResult, SearchSource};

const RRF_K: f32 = 60.0;
const TEXT_WEIGHT: f32 = 1.5;
const VECTOR_WEIGHT: f32 = 1.0;

pub fn fuse_results(
    text_results: Vec<SearchResult>,
    vector_results: Vec<SearchResult>,
    top_k: usize,
) -> Vec<SearchResult> {
    let mut merged = HashMap::<String, SearchResult>::new();
    let mut scores = HashMap::<String, f32>::new();

    for (rank, result) in text_results.into_iter().enumerate() {
        let key = key_for(&result);
        *scores.entry(key.clone()).or_default() += reciprocal_rank(rank, TEXT_WEIGHT);
        merged.entry(key).or_insert(result);
    }

    for (rank, result) in vector_results.into_iter().enumerate() {
        let key = key_for(&result);
        *scores.entry(key.clone()).or_default() += reciprocal_rank(rank, VECTOR_WEIGHT);
        merged
            .entry(key)
            .and_modify(|existing| {
                if existing.content.len() < result.content.len() {
                    *existing = result.clone();
                }
            })
            .or_insert(result);
    }

    let mut output = merged
        .into_iter()
        .filter_map(|(key, mut result)| {
            let score = scores.get(&key).copied()?;
            result.score = score;
            result.source = SearchSource::Hybrid;
            Some(result)
        })
        .collect::<Vec<_>>();
    output.sort_by(|left, right| right.score.total_cmp(&left.score));
    output.truncate(top_k);
    output
}

fn reciprocal_rank(rank: usize, weight: f32) -> f32 {
    weight / (RRF_K + rank as f32 + 1.0)
}

fn key_for(result: &SearchResult) -> String {
    format!(
        "{}:{}:{}",
        result.file_path.display(),
        result.start_line,
        result.end_line
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::fuse_results;
    use crate::search::{SearchResult, SearchSource};

    #[test]
    fn fuse_keeps_best_ranked_results() {
        let text = vec![SearchResult {
            file_path: PathBuf::from("a.rs"),
            start_line: 10,
            end_line: 10,
            score: 0.0,
            content: "auth".to_string(),
            source: SearchSource::Text,
        }];
        let vector = vec![SearchResult {
            file_path: PathBuf::from("a.rs"),
            start_line: 10,
            end_line: 10,
            score: 0.9,
            content: "auth implementation".to_string(),
            source: SearchSource::Vector,
        }];
        let fused = fuse_results(text, vector, 10);
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].source, SearchSource::Hybrid);
        assert!(fused[0].score > 0.0);
    }
}
