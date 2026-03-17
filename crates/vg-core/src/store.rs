use std::{
    path::{Path, PathBuf},
    sync::Once,
};

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{SCHEMA_VERSION, chunk::Chunk, normalize_roots};

static SQLITE_VEC_REGISTER: Once = Once::new();

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub id: i64,
    pub file_path: String,
    pub blake3_hash: String,
    pub file_size: i64,
    pub mtime_ms: i64,
    pub chunk_count: i64,
}

#[derive(Debug, Clone)]
pub struct VectorHit {
    pub file_path: PathBuf,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub distance: f32,
}

#[derive(Debug, Clone)]
pub struct ScopeStats {
    pub files_total: usize,
    pub chunks_total: usize,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path, model_id: &str, dimensions: usize, rebuild: bool) -> Result<Self> {
        register_sqlite_vec();
        if rebuild && path.exists() {
            std::fs::remove_file(path)
                .with_context(|| format!("删除旧索引失败: {}", path.display()))?;
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建索引目录失败: {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("打开 SQLite 索引失败: {}", path.display()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        let mut store = Self { conn };
        store.ensure_schema(model_id, dimensions, rebuild)?;
        Ok(store)
    }

    pub fn get_file(&self, path: &Path) -> Result<Option<FileRecord>> {
        self.conn
            .query_row(
                "SELECT id, file_path, blake3_hash, file_size, mtime_ms, chunk_count
                 FROM files WHERE file_path = ?1",
                [path.to_string_lossy().to_string()],
                map_file_record,
            )
            .optional()
            .context("查询文件索引失败")
    }

    pub fn touch_file(&self, id: i64, file_size: i64, mtime_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET file_size = ?2, mtime_ms = ?3, indexed_at = datetime('now')
             WHERE id = ?1",
            params![id, file_size, mtime_ms],
        )?;
        Ok(())
    }

    pub fn replace_file(
        &mut self,
        path: &Path,
        blake3_hash: &str,
        file_size: i64,
        mtime_ms: i64,
        chunks: &[Chunk],
        embeddings: &[Vec<f32>],
    ) -> Result<()> {
        if chunks.len() != embeddings.len() {
            return Err(anyhow!("chunk 数量与 embedding 数量不一致"));
        }

        let transaction = self.conn.transaction()?;
        let path_text = path.to_string_lossy().to_string();
        let existing_id = transaction
            .query_row(
                "SELECT id FROM files WHERE file_path = ?1",
                [path_text.clone()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;

        let file_id = if let Some(file_id) = existing_id {
            transaction.execute("DELETE FROM vec_chunks WHERE file_id = ?1", [file_id])?;
            transaction.execute(
                "UPDATE files
                 SET blake3_hash = ?2, file_size = ?3, mtime_ms = ?4, chunk_count = ?5,
                     indexed_at = datetime('now')
                 WHERE id = ?1",
                params![
                    file_id,
                    blake3_hash,
                    file_size,
                    mtime_ms,
                    chunks.len() as i64
                ],
            )?;
            file_id
        } else {
            transaction.execute(
                "INSERT INTO files (file_path, blake3_hash, file_size, mtime_ms, chunk_count)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    path_text,
                    blake3_hash,
                    file_size,
                    mtime_ms,
                    chunks.len() as i64
                ],
            )?;
            transaction.last_insert_rowid()
        };

        let mut statement = transaction.prepare(
            "INSERT INTO vec_chunks (
                embedding,
                file_id,
                content,
                start_byte,
                end_byte,
                start_line,
                end_line,
                chunk_index
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            statement.execute(params![
                serialize_embedding(embedding)?,
                file_id,
                chunk.content,
                chunk.start_byte as i64,
                chunk.end_byte as i64,
                chunk.start_line as i64,
                chunk.end_line as i64,
                chunk.chunk_index as i64
            ])?;
        }

        drop(statement);
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_file(&mut self, path: &str) -> Result<()> {
        let transaction = self.conn.transaction()?;
        let record = transaction
            .query_row("SELECT id FROM files WHERE file_path = ?1", [path], |row| {
                row.get::<_, i64>(0)
            })
            .optional()?;
        if let Some(file_id) = record {
            transaction.execute("DELETE FROM vec_chunks WHERE file_id = ?1", [file_id])?;
            transaction.execute("DELETE FROM files WHERE id = ?1", [file_id])?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn vector_search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<VectorHit>> {
        let serialized = serialize_embedding(query_embedding)?;
        let mut statement = self.conn.prepare(
            "SELECT f.file_path, c.content, c.start_line, c.end_line, c.distance
             FROM (
                 SELECT file_id, content, start_line, end_line, distance
                 FROM vec_chunks
                 WHERE embedding MATCH ?1
                 ORDER BY distance
                 LIMIT ?2
             ) c
             JOIN files f ON c.file_id = f.id
             ORDER BY c.distance",
        )?;

        let rows = statement.query_map(params![serialized, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, f64>(4)? as f32,
            ))
        })?;

        let mut hits = Vec::new();
        for row in rows {
            let (file_path, content, start_line, end_line, distance) = row?;
            hits.push(VectorHit {
                file_path: PathBuf::from(file_path),
                content,
                start_line: start_line as usize,
                end_line: end_line as usize,
                distance,
            });
        }
        Ok(hits)
    }

    pub fn scope_stats(&self, roots: &[PathBuf]) -> Result<ScopeStats> {
        let normalized = normalize_roots(roots)?;
        let records = self.list_files_in_scope(&normalized)?;
        Ok(ScopeStats {
            files_total: records.len(),
            chunks_total: records.iter().map(|r| r.chunk_count as usize).sum(),
        })
    }

    pub fn list_files_in_scope(&self, roots: &[PathBuf]) -> Result<Vec<FileRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT id, file_path, blake3_hash, file_size, mtime_ms, chunk_count
             FROM files
             ORDER BY file_path",
        )?;
        let rows = statement.query_map([], map_file_record)?;
        let mut records = Vec::new();
        for row in rows {
            let record = row?;
            let candidate = PathBuf::from(&record.file_path);
            if roots.iter().any(|root| candidate.starts_with(root)) {
                records.push(record);
            }
        }
        Ok(records)
    }

    fn ensure_schema(&mut self, model_id: &str, dimensions: usize, rebuild: bool) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS index_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;

        let stored_model = self.meta("model_id")?;
        let stored_dimensions = self
            .meta("dimensions")?
            .and_then(|value| value.parse::<usize>().ok());
        let stored_schema = self
            .meta("schema_version")?
            .and_then(|value| value.parse::<u32>().ok());

        if rebuild
            || stored_model.as_deref() != Some(model_id)
            || stored_dimensions != Some(dimensions)
            || stored_schema != Some(SCHEMA_VERSION)
            || !self.has_vec_table()?
        {
            self.recreate_schema(dimensions)?;
        } else {
            self.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS files (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    file_path TEXT NOT NULL UNIQUE,
                    blake3_hash TEXT NOT NULL,
                    file_size INTEGER NOT NULL,
                    mtime_ms INTEGER NOT NULL,
                    chunk_count INTEGER NOT NULL DEFAULT 0,
                    indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
                );",
            )?;
        }

        self.set_meta("model_id", model_id)?;
        self.set_meta("dimensions", &dimensions.to_string())?;
        self.set_meta("schema_version", &SCHEMA_VERSION.to_string())?;
        self.set_meta("chunk_strategy", "sliding-window-v1")?;
        Ok(())
    }

    fn recreate_schema(&mut self, dimensions: usize) -> Result<()> {
        self.conn.execute_batch(
            "DROP TABLE IF EXISTS vec_chunks;
             DROP TABLE IF EXISTS files;
             DELETE FROM index_meta;",
        )?;
        self.conn.execute_batch(
            "CREATE TABLE files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL UNIQUE,
                blake3_hash TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                mtime_ms INTEGER NOT NULL,
                chunk_count INTEGER NOT NULL DEFAULT 0,
                indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )?;
        self.conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE vec_chunks USING vec0(
                embedding float[{dimensions}],
                +file_id INTEGER,
                +content TEXT,
                +start_byte INTEGER,
                +end_byte INTEGER,
                +start_line INTEGER,
                +end_line INTEGER,
                +chunk_index INTEGER
            );"
        ))?;
        Ok(())
    }

    fn meta(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM index_meta WHERE key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .context("读取索引元信息失败")
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO index_meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    fn has_vec_table(&self) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'vec_chunks'",
            [],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

fn serialize_embedding(values: &[f32]) -> Result<String> {
    serde_json::to_string(values).context("序列化向量失败")
}

fn register_sqlite_vec() {
    SQLITE_VEC_REGISTER.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

fn map_file_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileRecord> {
    Ok(FileRecord {
        id: row.get(0)?,
        file_path: row.get(1)?,
        blake3_hash: row.get(2)?,
        file_size: row.get(3)?,
        mtime_ms: row.get(4)?,
        chunk_count: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Store;
    use crate::chunk::Chunk;

    #[test]
    fn store_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("index.sqlite3");
        let mut store = Store::open(&path, "bge-small-zh", 3, true).expect("store");
        let chunks = vec![Chunk {
            content: "hello world".to_string(),
            start_byte: 0,
            end_byte: 11,
            start_line: 1,
            end_line: 1,
            chunk_index: 0,
        }];
        let embeddings = vec![vec![0.1_f32, 0.2_f32, 0.3_f32]];
        store
            .replace_file(
                Path::new("/tmp/example.txt"),
                "hash",
                11,
                1,
                &chunks,
                &embeddings,
            )
            .expect("replace");
        let results = store
            .vector_search(&[0.1_f32, 0.2_f32, 0.3_f32], 10)
            .expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].start_line, 1);
    }
}
