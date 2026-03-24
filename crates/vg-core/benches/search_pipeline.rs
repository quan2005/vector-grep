use criterion::{Criterion, criterion_group, criterion_main};
use vg_core::{
    chunk::{ChunkConfig, ChunkSplitter},
    search::{SearchResult, SearchSource, hybrid::fuse_results},
};

fn bench_chunk_splitter(criterion: &mut Criterion) {
    let fixture = include_str!("../../../tests/fixtures/auth-guide.md").repeat(200);
    let splitter = ChunkSplitter::new(ChunkConfig {
        chunk_size_tokens: 128,
        chunk_overlap_tokens: 16,
        max_chunks_per_file: 5_000,
    });
    criterion.bench_function("chunk_splitter_auth_guide_x200", |bench| {
        bench.iter(|| splitter.split(&fixture))
    });
}

fn bench_hybrid_fusion(criterion: &mut Criterion) {
    let text = (0..500)
        .map(|index| SearchResult {
            file_path: format!("src/module_{index}.rs").into(),
            start_line: index + 1,
            end_line: index + 1,
            score: 0.0,
            content: format!("text result {index}"),
            source: SearchSource::Text,
            text_hit: true,
            vector_hit: false,
        })
        .collect::<Vec<_>>();
    let vector = (0..500)
        .map(|index| SearchResult {
            file_path: format!("src/module_{index}.rs").into(),
            start_line: index + 1,
            end_line: index + 1,
            score: 0.8,
            content: format!("vector result {index}"),
            source: SearchSource::Vector,
            text_hit: false,
            vector_hit: true,
        })
        .collect::<Vec<_>>();

    criterion.bench_function("hybrid_fusion_500x500", |bench| {
        bench.iter(|| fuse_results(text.clone(), vector.clone(), 50))
    });
}

criterion_group!(benches, bench_chunk_splitter, bench_hybrid_fusion);
criterion_main!(benches);
