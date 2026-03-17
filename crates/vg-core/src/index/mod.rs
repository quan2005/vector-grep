pub mod file_walker;
pub mod hasher;

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    time::Instant,
};

use anyhow::Result;
use rayon::prelude::*;

use crate::{
    chunk::{ChunkConfig, ChunkSplitter},
    embed::Embedder,
    normalize_roots, preproc,
    progress::ProgressReporter,
    store::{ScopeStats, Store},
};

const EMBED_PASSAGE_BATCH_SIZE: usize = 16;

#[derive(Debug, Clone)]
pub struct SyncReport {
    pub files_indexed: usize,
    pub files_removed: usize,
    pub chunks_total: usize,
    pub index_time_ms: u128,
}

pub struct IndexManager<'a> {
    store: &'a mut Store,
    embedder: &'a mut Embedder,
    reporter: &'a dyn ProgressReporter,
    splitter: ChunkSplitter,
}

#[derive(Debug)]
struct PreparedFile {
    path: PathBuf,
    hash: String,
    file_size: i64,
    mtime_ms: i64,
    chunks: Vec<crate::chunk::Chunk>,
}

#[derive(Debug)]
enum PreparedAction {
    Unchanged,
    Touch {
        id: i64,
        file_size: i64,
        mtime_ms: i64,
    },
    Remove {
        path: String,
    },
    Upsert(PreparedFile),
}

impl<'a> IndexManager<'a> {
    pub fn new(
        store: &'a mut Store,
        embedder: &'a mut Embedder,
        chunk_size_tokens: usize,
        chunk_overlap_tokens: usize,
        reporter: &'a dyn ProgressReporter,
    ) -> Self {
        Self {
            store,
            embedder,
            reporter,
            splitter: ChunkSplitter::new(ChunkConfig {
                chunk_size_tokens,
                chunk_overlap_tokens,
                max_chunks_per_file: 1_000,
            }),
        }
    }

    pub fn sync(&mut self, roots: &[PathBuf]) -> Result<SyncReport> {
        let normalized_roots = normalize_roots(roots)?;
        let started = Instant::now();
        let files = file_walker::collect_files(&normalized_roots)?;
        self.reporter.on_scan_complete(files.len());
        let seen = files
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<HashSet<_>>();
        let existing = self
            .store
            .list_files_in_scope(&normalized_roots)?
            .into_iter()
            .map(|record| (record.file_path.clone(), record))
            .collect::<HashMap<_, _>>();
        let splitter_config = self.splitter.config();
        let reporter = self.reporter;

        self.reporter.on_index_start(files.len());
        let prepared = files
            .par_iter()
            .map(|path| {
                let key = path.to_string_lossy().to_string();
                let existing = existing.get(&key).cloned();
                let action = prepare_file(path.clone(), existing, splitter_config);
                reporter.on_index_tick();
                action
            })
            .collect::<Vec<_>>();
        self.reporter.on_index_done();

        let mut pending_upserts = Vec::new();
        let mut files_indexed = 0usize;
        let mut files_removed = 0usize;
        for action in prepared {
            apply_prepared_action(
                self.store,
                action?,
                &mut pending_upserts,
                &mut files_indexed,
                &mut files_removed,
            )?;
        }

        self.apply_upserts(pending_upserts)?;

        for record in existing.values() {
            if !seen.contains(&record.file_path) {
                self.store.remove_file(&record.file_path)?;
                files_removed += 1;
            }
        }

        let ScopeStats {
            files_total: _,
            chunks_total,
        } = self.store.scope_stats(&normalized_roots)?;

        Ok(SyncReport {
            files_indexed,
            files_removed,
            chunks_total,
            index_time_ms: started.elapsed().as_millis(),
        })
    }

    fn apply_upserts(&mut self, pending_upserts: Vec<PreparedFile>) -> Result<()> {
        let store = &mut *self.store;
        let embedder = &mut *self.embedder;
        apply_upserts_with(store, self.reporter, pending_upserts, &mut |passages| {
            embedder.embed_passages(passages)
        })
    }
}

fn apply_upserts_with<F>(
    store: &mut Store,
    reporter: &dyn ProgressReporter,
    pending_upserts: Vec<PreparedFile>,
    embed_passages: &mut F,
) -> Result<()>
where
    F: FnMut(&[&str]) -> Result<Vec<Vec<f32>>>,
{
    if pending_upserts.is_empty() {
        return Ok(());
    }

    let total_batches = pending_upserts
        .iter()
        .map(|file| file.chunks.len().div_ceil(EMBED_PASSAGE_BATCH_SIZE))
        .sum();
    reporter.on_embed_start(total_batches);
    for file in pending_upserts {
        let mut embeddings = Vec::with_capacity(file.chunks.len());
        for chunk_batch in file.chunks.chunks(EMBED_PASSAGE_BATCH_SIZE) {
            let passages = chunk_batch
                .iter()
                .map(|chunk| chunk.content.as_str())
                .collect::<Vec<_>>();
            let mut batch_embeddings = embed_passages(&passages)?;
            embeddings.append(&mut batch_embeddings);
            reporter.on_embed_tick();
        }
        store.replace_file(
            &file.path,
            &file.hash,
            file.file_size,
            file.mtime_ms,
            &file.chunks,
            &embeddings,
        )?;
    }
    reporter.on_embed_done();
    Ok(())
}

fn apply_prepared_action(
    store: &mut Store,
    action: PreparedAction,
    pending_upserts: &mut Vec<PreparedFile>,
    files_indexed: &mut usize,
    files_removed: &mut usize,
) -> Result<()> {
    match action {
        PreparedAction::Unchanged => {}
        PreparedAction::Touch {
            id,
            file_size,
            mtime_ms,
        } => store.touch_file(id, file_size, mtime_ms)?,
        PreparedAction::Remove { path } => {
            store.remove_file(&path)?;
            *files_removed += 1;
        }
        PreparedAction::Upsert(file) => {
            *files_indexed += 1;
            pending_upserts.push(file);
        }
    }
    Ok(())
}

fn prepare_file(
    path: PathBuf,
    existing: Option<crate::store::FileRecord>,
    splitter_config: ChunkConfig,
) -> Result<PreparedAction> {
    let metadata = fs::metadata(&path)?;
    if !metadata.is_file() {
        return Ok(PreparedAction::Unchanged);
    }

    let mtime_ms = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    let file_size = metadata.len() as i64;

    if let Some(record) = existing.as_ref() {
        if record.mtime_ms == mtime_ms && record.file_size == file_size {
            return Ok(PreparedAction::Unchanged);
        }
    }

    let hash = hasher::hash_file(&path)?;
    if let Some(record) = existing.as_ref() {
        if record.blake3_hash == hash {
            return Ok(PreparedAction::Touch {
                id: record.id,
                file_size,
                mtime_ms,
            });
        }
    }

    let Some(text) = preproc::extract_text(&path)? else {
        return Ok(PreparedAction::Remove {
            path: path.to_string_lossy().to_string(),
        });
    };

    let splitter = ChunkSplitter::new(splitter_config);
    let chunks = splitter.split(&text);
    if chunks.is_empty() {
        return Ok(PreparedAction::Remove {
            path: path.to_string_lossy().to_string(),
        });
    }

    Ok(PreparedAction::Upsert(PreparedFile {
        path,
        hash,
        file_size,
        mtime_ms,
        chunks,
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use super::{PreparedAction, PreparedFile, apply_prepared_action, apply_upserts_with};
    use crate::{chunk::Chunk, progress::ProgressReporter, store::Store};

    #[test]
    fn remove_action_counts_as_removed_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("index.sqlite3");
        let file_path = PathBuf::from("/tmp/example.txt");
        let mut store = Store::open(&db_path, "bge-small-zh", 3, true).expect("store");
        store
            .replace_file(
                Path::new(&file_path),
                "hash",
                5,
                1,
                &[Chunk {
                    content: "hello".to_string(),
                    start_byte: 0,
                    end_byte: 5,
                    start_line: 1,
                    end_line: 1,
                    chunk_index: 0,
                }],
                &[vec![0.1_f32, 0.2_f32, 0.3_f32]],
            )
            .expect("replace");

        let mut pending_upserts = Vec::new();
        let mut files_indexed = 0usize;
        let mut files_removed = 0usize;
        apply_prepared_action(
            &mut store,
            PreparedAction::Remove {
                path: file_path.to_string_lossy().to_string(),
            },
            &mut pending_upserts,
            &mut files_indexed,
            &mut files_removed,
        )
        .expect("apply");

        assert_eq!(files_indexed, 0);
        assert_eq!(files_removed, 1);
        assert!(store.get_file(&file_path).expect("get file").is_none());
    }

    #[derive(Default)]
    struct RecordingReporter {
        embed_start_total: AtomicUsize,
        embed_ticks: AtomicUsize,
        embed_done: AtomicBool,
    }

    impl ProgressReporter for RecordingReporter {
        fn on_model_loading(&self, _model_id: &str, _description: &str) {}

        fn on_model_loaded(&self) {}

        fn on_scan_complete(&self, _total_files: usize) {}

        fn on_index_start(&self, _total: usize) {}

        fn on_index_tick(&self) {}

        fn on_index_done(&self) {}

        fn on_embed_start(&self, total: usize) {
            self.embed_start_total.store(total, Ordering::SeqCst);
        }

        fn on_embed_tick(&self) {
            self.embed_ticks.fetch_add(1, Ordering::SeqCst);
        }

        fn on_embed_done(&self) {
            self.embed_done.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn apply_upserts_embeds_and_ticks_per_batch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("index.sqlite3");
        let mut store = Store::open(&db_path, "bge-small-zh", 3, true).expect("store");
        let reporter = RecordingReporter::default();
        let mut embed_calls = Vec::new();

        apply_upserts_with(
            &mut store,
            &reporter,
            vec![
                prepared_file("/tmp/first.txt", 17),
                prepared_file("/tmp/second.txt", 1),
            ],
            &mut |passages| {
                embed_calls.push(passages.len());
                Ok(passages
                    .iter()
                    .map(|_| vec![0.1_f32, 0.2_f32, 0.3_f32])
                    .collect())
            },
        )
        .expect("apply upserts");

        assert_eq!(embed_calls, vec![16, 1, 1]);
        assert_eq!(reporter.embed_start_total.load(Ordering::SeqCst), 3);
        assert_eq!(reporter.embed_ticks.load(Ordering::SeqCst), 3);
        assert!(reporter.embed_done.load(Ordering::SeqCst));
        assert_eq!(
            store
                .get_file(Path::new("/tmp/first.txt"))
                .expect("get first")
                .expect("first present")
                .chunk_count,
            17
        );
        assert_eq!(
            store
                .get_file(Path::new("/tmp/second.txt"))
                .expect("get second")
                .expect("second present")
                .chunk_count,
            1
        );
    }

    fn prepared_file(path: &str, chunk_count: usize) -> PreparedFile {
        PreparedFile {
            path: PathBuf::from(path),
            hash: format!("hash-{path}"),
            file_size: chunk_count as i64 * 8,
            mtime_ms: 1,
            chunks: (0..chunk_count)
                .map(|index| {
                    let content = format!("chunk-{index}");
                    Chunk {
                        content,
                        start_byte: index * 10,
                        end_byte: index * 10 + 8,
                        start_line: index + 1,
                        end_line: index + 1,
                        chunk_index: index,
                    }
                })
                .collect(),
        }
    }
}
