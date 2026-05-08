//! Token-aware, markdown-structure-aware chunking for parent/child retrieval.
//!
//! Uses `text-splitter`'s `MarkdownSplitter` configured with the embedder's
//! tokenizer (XLM-RoBERTa for bge-m3) so chunk size is measured in real
//! model tokens, not characters. Markdown structure (headings, code fences,
//! tables, lists, paragraphs) is respected — a chunk never breaks inside a
//! code block, and section boundaries are preferred over arbitrary cuts.
//!
//! Section-path tracking (header breadcrumb per chunk) is **not** done here;
//! it requires walking the parsed markdown AST. Chunks land with `section_path
//! = NULL` for now and the parent-section reassembly path will own that work.

use anyhow::Result;
use text_splitter::{ChunkConfig, MarkdownSplitter};
use tokenizers::Tokenizer;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub position: i32,
    pub content: String,
}

pub struct MarkdownChunker {
    splitter: MarkdownSplitter<Tokenizer>,
}

impl MarkdownChunker {
    /// `max_tokens` is the upper bound for any single chunk (in tokenizer
    /// tokens). `overlap_tokens` is added to the *end* of one chunk and the
    /// *start* of the next so retrieval doesn't lose context at boundaries.
    /// Per the migration plan: ~500 / ~50 for bge-m3 (8k context window).
    pub fn new(tokenizer: Tokenizer, max_tokens: usize, overlap_tokens: usize) -> Result<Self> {
        let config = ChunkConfig::new(max_tokens)
            .with_sizer(tokenizer)
            .with_overlap(overlap_tokens)?;
        Ok(Self {
            splitter: MarkdownSplitter::new(config),
        })
    }

    pub fn chunks(&self, text: &str) -> Vec<Chunk> {
        self.splitter
            .chunks(text)
            .enumerate()
            .map(|(i, content)| Chunk {
                position: i as i32,
                content: content.to_owned(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::AHashMap;
    use tokenizers::{models::wordlevel::WordLevel, pre_tokenizers::whitespace::Whitespace};

    fn whitespace_tokenizer() -> Tokenizer {
        // Word-level tokenizer with whitespace pre-tokenization. Token count
        // == word count, which makes test assertions trivially predictable
        // without pulling the bge-m3 SentencePiece tokenizer into the test.
        let model = WordLevel::builder()
            .vocab(AHashMap::from([("[UNK]".to_owned(), 0)]))
            .unk_token("[UNK]".to_owned())
            .build()
            .unwrap();
        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Some(Whitespace::default()));
        tokenizer
    }

    #[test]
    fn short_text_yields_one_chunk() {
        let chunker = MarkdownChunker::new(whitespace_tokenizer(), 50, 5).unwrap();
        let chunks = chunker.chunks("Hello world. This is a short paragraph.");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].position, 0);
    }

    #[test]
    fn long_text_yields_multiple_chunks() {
        let chunker = MarkdownChunker::new(whitespace_tokenizer(), 10, 2).unwrap();
        // ~30 words → at least 3 chunks at max=10.
        let text = "one two three four five six seven eight nine ten \
                    eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty \
                    alpha beta gamma delta epsilon zeta eta theta iota kappa";
        let chunks = chunker.chunks(text);
        assert!(chunks.len() >= 3, "got {} chunks", chunks.len());
        assert!(chunks.iter().enumerate().all(|(i, c)| c.position == i as i32));
    }
}
