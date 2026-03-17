use std::path::PathBuf;

use anyhow::Result;

use crate::{
    embed::Embedder,
    normalize_roots,
    search::{SearchResult, SearchSource},
    store::Store,
};

pub fn semantic_search(
    store: &mut Store,
    embedder: &mut Embedder,
    query: &str,
    roots: &[PathBuf],
    top_k: usize,
    threshold: f32,
) -> Result<Vec<SearchResult>> {
    let roots = normalize_roots(roots)?;
    let query_embedding = embedder.embed_query(query)?;
    let raw = store.vector_search(&query_embedding, top_k.max(20) * 5)?;
    let mut results = Vec::new();

    for hit in raw {
        if !roots.iter().any(|root| hit.file_path.starts_with(root)) {
            continue;
        }

        let score = distance_to_score(hit.distance);
        if score < threshold {
            continue;
        }

        results.push(SearchResult {
            file_path: hit.file_path,
            start_line: hit.start_line,
            end_line: hit.end_line,
            score,
            content: hit.content,
            source: SearchSource::Vector,
        });
        if results.len() >= top_k {
            break;
        }
    }

    Ok(results)
}

fn distance_to_score(distance: f32) -> f32 {
    1.0 / (1.0 + distance.max(0.0))
}
