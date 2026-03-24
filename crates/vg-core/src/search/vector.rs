use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::Result;

use crate::{
    embed::Embedder,
    normalize_roots,
    search::{SearchResult, SearchSource, query_bridge},
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
    let mut variants = query_bridge::build_query_variants(query).into_iter();
    let Some(primary_query) = variants.next() else {
        return Ok(Vec::new());
    };

    let mut results = collect_variant_results(
        store,
        embedder,
        primary_query.as_ref(),
        &roots,
        top_k,
        threshold,
    )?;
    if results.len() >= top_k {
        return Ok(results);
    }

    let mut seen = results
        .iter()
        .map(result_key)
        .collect::<BTreeSet<(PathBuf, usize, usize)>>();

    for variant in variants {
        let bridge_results =
            collect_variant_results(store, embedder, variant.as_ref(), &roots, top_k, threshold)?;
        for result in bridge_results {
            if seen.insert(result_key(&result)) {
                results.push(result);
            }
            if results.len() >= top_k {
                return Ok(results);
            }
        }
    }

    Ok(results)
}

fn collect_variant_results(
    store: &mut Store,
    embedder: &mut Embedder,
    query: &str,
    roots: &[PathBuf],
    top_k: usize,
    threshold: f32,
) -> Result<Vec<SearchResult>> {
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
            text_hit: false,
            vector_hit: true,
        });
        if results.len() >= top_k {
            break;
        }
    }

    Ok(results)
}

fn result_key(result: &SearchResult) -> (PathBuf, usize, usize) {
    (result.file_path.clone(), result.start_line, result.end_line)
}

fn distance_to_score(distance: f32) -> f32 {
    1.0 / (1.0 + distance.max(0.0))
}
