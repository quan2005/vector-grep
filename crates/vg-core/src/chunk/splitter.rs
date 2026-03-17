use std::cmp::min;

const BOUNDARY_MARKERS: &[&str] = &["\n\n", "。\n", "。", ". ", "\n"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub content: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub chunk_index: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct ChunkConfig {
    pub chunk_size_tokens: usize,
    pub chunk_overlap_tokens: usize,
    pub max_chunks_per_file: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            chunk_size_tokens: 512,
            chunk_overlap_tokens: 64,
            max_chunks_per_file: 1_000,
        }
    }
}

impl ChunkConfig {
    fn chunk_char_size(self) -> usize {
        self.chunk_size_tokens * 4
    }

    fn overlap_char_size(self) -> usize {
        self.chunk_overlap_tokens * 4
    }
}

pub struct ChunkSplitter {
    config: ChunkConfig,
}

impl ChunkSplitter {
    pub fn new(config: ChunkConfig) -> Self {
        Self { config }
    }

    pub fn split(&self, text: &str) -> Vec<Chunk> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        let char_offsets = char_offsets(text);
        let line_starts = line_starts(text);
        let total_chars = char_offsets.len() - 1;
        let chunk_chars = self.config.chunk_char_size().max(1);
        let overlap_chars = self.config.overlap_char_size();

        let mut chunks = Vec::new();
        let mut start_char = 0usize;

        while start_char < total_chars && chunks.len() < self.config.max_chunks_per_file {
            let target_end_char = min(total_chars, start_char + chunk_chars);
            let hard_end_char = min(total_chars, target_end_char + overlap_chars / 2);
            let start_byte = char_offsets[start_char];
            let target_end_byte = char_offsets[target_end_char];
            let hard_end_byte = char_offsets[hard_end_char];
            let mut end_byte = find_boundary(text, start_byte, target_end_byte, hard_end_byte);
            if end_byte <= start_byte {
                end_byte = target_end_byte;
            }
            let end_char = byte_to_char_index(&char_offsets, end_byte);
            if end_char <= start_char {
                break;
            }

            let slice = text[start_byte..end_byte].trim();
            if !slice.is_empty() {
                chunks.push(Chunk {
                    content: slice.to_string(),
                    start_byte,
                    end_byte,
                    start_line: byte_to_line(&line_starts, start_byte),
                    end_line: byte_to_line(&line_starts, end_byte.saturating_sub(1)),
                    chunk_index: chunks.len(),
                });
            }

            if end_char >= total_chars {
                break;
            }

            let next_start = end_char.saturating_sub(overlap_chars);
            start_char = if next_start <= start_char {
                end_char
            } else {
                next_start
            };
        }

        chunks
    }

    pub fn config(&self) -> ChunkConfig {
        self.config
    }
}

fn char_offsets(text: &str) -> Vec<usize> {
    let mut offsets = text.char_indices().map(|(idx, _)| idx).collect::<Vec<_>>();
    offsets.push(text.len());
    offsets
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (idx, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(idx + 1);
        }
    }
    starts
}

fn byte_to_line(line_starts: &[usize], byte_index: usize) -> usize {
    line_starts.partition_point(|offset| *offset <= byte_index)
}

fn byte_to_char_index(offsets: &[usize], byte_index: usize) -> usize {
    offsets.partition_point(|offset| *offset < byte_index)
}

fn find_boundary(
    text: &str,
    start_byte: usize,
    target_end_byte: usize,
    hard_end_byte: usize,
) -> usize {
    let preferred = &text[start_byte..target_end_byte];
    for marker in BOUNDARY_MARKERS {
        if let Some(relative) = preferred.rfind(marker) {
            return start_byte + relative + marker.len();
        }
    }

    let extended = &text[start_byte..hard_end_byte];
    for marker in BOUNDARY_MARKERS {
        if let Some(relative) = extended.rfind(marker) {
            return start_byte + relative + marker.len();
        }
    }

    target_end_byte
}

#[cfg(test)]
mod tests {
    use super::{ChunkConfig, ChunkSplitter};

    #[test]
    fn split_preserves_lines() {
        let splitter = ChunkSplitter::new(ChunkConfig {
            chunk_size_tokens: 8,
            chunk_overlap_tokens: 2,
            max_chunks_per_file: 16,
        });
        let content = "第一段第一句。\n第一段第二句。\n\n第二段第一句。\n第二段第二句。";
        let chunks = splitter.split(content);
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].start_line, 1);
        assert!(chunks[0].end_line >= 2);
        assert!(chunks[1].start_line >= 2);
    }
}
